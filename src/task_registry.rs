use dashmap::DashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast::{self, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

/// The type of each output line produced by a managed child process.
pub type Output = String;

/// A message emitted by the managed process lifecycle.
#[derive(Clone)]
pub enum TaskMessage {
    /// A single line of stdout data from the child process.
    Data(Output),
    /// An error or diagnostic string (e.g., spawn failure, read error).
    Error(Output),
    /// The process has exited and its stdout pipe has been closed.
    Eof,
}

type TaskKey = String;

/// A registry of named long-running subprocesses with broadcast output.
///
/// Multiple callers can subscribe to the same key; the underlying process is
/// started on the first subscription and stopped when the last subscriber
/// leaves. Output lines are forwarded to all subscribers via a
/// [`broadcast::Sender`].
///
/// If the process exits naturally while subscribers remain, the next
/// subscriber (including late ones) automatically spawns a replacement.
///
/// # Clone semantics
///
/// Cloning is cheap (an `Arc` clone) and produces another handle to the
/// **same** underlying registry.
#[derive(Clone)]
pub struct TaskRegistry {
    tasks: Arc<DashMap<TaskKey, Arc<TaskHandle>>>,
}

impl TaskRegistry {
    /// Creates a new, empty [`TaskRegistry`].
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
        }
    }

    /// Subscribes to the task identified by `key`, spawning it if necessary.
    ///
    /// Returns a broadcast receiver and an RAII deregistration guard.
    /// When the guard is dropped, this subscriber is removed; the underlying
    /// process is stopped only when all subscribers have left.
    pub async fn subscribe(
        self: &Arc<Self>,
        key: TaskKey,
        cmd: Vec<String>,
    ) -> (broadcast::Receiver<TaskMessage>, TaskDeregister) {
        let entry = self
            .tasks
            .entry(key.clone())
            .or_insert_with(|| {
                debug!("Created task entry for key: {:?}", key);
                Arc::new(TaskHandle::new())
            })
            .clone();

        let rx = entry.subscribe(cmd).await;

        let weak_registry = Arc::downgrade(self);
        let dereg_key = key.clone();
        let dereg = TaskDeregister::new(move || {
            trace!("Task deregistered: {:?}", dereg_key);
            if let Some(registry) = weak_registry.upgrade() {
                if let Some(handle) = registry.tasks.get(&dereg_key) {
                    let last = handle.unsubscribe();
                    // Clean up the map entry if no one is listening any more.
                    if last {
                        drop(handle);
                        registry.tasks.remove(&dereg_key);
                    }
                } else {
                    warn!("Tried to deregister nonexistent key: {:?}", dereg_key);
                }
            }
        });

        (rx, dereg)
    }
}

// ─── TaskHandle ────────────────────────────────────────────────────────────

/// Internal handle for a single task.
///
/// State machine:
///
/// ```text
///                  subscribe()
///     ┌──────┐   ──────────────▶   ┌────────┐
///     │ Idle │                      │ Active │
///     │      │◀───────────────────  │  { N   │
///     └──────┘   last unsubscribe   │   subscribers }   │
///                   or process end  └────────┘
/// ```
struct TaskHandle {
    tx: Sender<TaskMessage>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Active {
        cancel_token: CancellationToken,
        subscriber_count: usize,
    },
}

impl TaskHandle {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            tx,
            state: Arc::new(Mutex::new(State::Idle)),
        }
    }

    /// Subscribe to this task. Spawns the process if not already running.
    async fn subscribe(&self, cmd: Vec<String>) -> broadcast::Receiver<TaskMessage> {
        let mut need_spawn: Option<(Vec<String>, CancellationToken)> = None;

        {
            let mut st = self.state.lock().expect("state mutex poisoned");
            match &mut *st {
                State::Idle => {
                    let token = CancellationToken::new();
                    need_spawn = Some((cmd.clone(), token.clone()));
                    *st = State::Active {
                        cancel_token: token,
                        subscriber_count: 1,
                    };
                }
                State::Active {
                    subscriber_count, ..
                } => {
                    *subscriber_count += 1;
                }
            }
        }

        if let Some((cmd, token)) = need_spawn {
            if let Err(e) = Self::spawn_process(&self.tx, &self.state, &cmd, token) {
                error!("Spawn task failed! Error: {}", e);
                let _ = self
                    .tx
                    .send(TaskMessage::Error("Spawn task failed".to_string()));
                // Roll back to Idle so the next subscribe can retry.
                let mut st = self.state.lock().expect("state mutex poisoned");
                *st = State::Idle;
            } else {
                debug!("Process spawned successfully");
            }
        }

        self.tx.subscribe()
    }

    /// Unsubscribe one caller. Returns `true` if this was the last subscriber
    /// (i.e. the handle should be removed from the registry).
    fn unsubscribe(&self) -> bool {
        let mut need_cancel: Option<CancellationToken> = None;

        {
            let mut st = self.state.lock().expect("state mutex poisoned");
            match &mut *st {
                State::Active {
                    subscriber_count,
                    cancel_token,
                } => {
                    *subscriber_count = subscriber_count.saturating_sub(1);
                    if *subscriber_count == 0 {
                        need_cancel = Some(cancel_token.clone());
                        *st = State::Idle;
                    }
                }
                _ => {
                    warn!("unsubscribe called while state is not Active");
                }
            }
        }

        if let Some(token) = need_cancel {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Spawn the child process and start a background reader task.
    ///
    /// The caller must have already transitioned the state to
    /// `Active { cancel_token, .. }` before calling this.
    fn spawn_process(
        tx: &Sender<TaskMessage>,
        state: &Arc<Mutex<State>>,
        cmd: &[String],
        cancel_token: CancellationToken,
    ) -> anyhow::Result<()> {
        if cmd.is_empty() {
            anyhow::bail!("spawn_process called with empty cmd");
        }

        let program = cmd[0].clone();
        let args = cmd[1..].to_vec();
        trace!("Spawning process: {} {:?}", &program, &args);

        let mut child = Command::new(&program)
            .args(&args)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to spawn child process: {} {:?}: {}",
                    program,
                    args,
                    e
                )
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout of process: {:?}", cmd))?;

        let tx = tx.clone();
        let state = Arc::clone(state);
        let mut reader = BufReader::new(stdout).lines();
        let cmd_debug = cmd.to_vec();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        debug!("Cancellation received, killing process: {:?}", cmd_debug);
                        if let Err(e) = child.kill().await {
                            let _ = tx.send(TaskMessage::Error(
                                format!("Failed to kill child: {e}"),
                            ));
                        }
                        break;
                    }
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(data)) => { let _ = tx.send(TaskMessage::Data(data)); }
                            Ok(None) => {
                                warn!("Process finished (EOF): {:?}", cmd_debug);
                                let _ = tx.send(TaskMessage::Eof);
                                break;
                            }
                            Err(e) => {
                                error!("Read error from process {:?}: {}", cmd_debug, e);
                                let _ = tx.send(TaskMessage::Error(
                                    format!("Failed to read line from process: {:?}, err: {}", cmd_debug, e),
                                ));
                                break;
                            }
                        }
                    }
                }
            }

            // Process has exited; if there are still subscribers, go back to
            // Idle so the next subscribe call spawns a fresh process.
            let mut st = state.lock().expect("state mutex poisoned");
            if matches!(&*st, State::Active { subscriber_count, .. } if *subscriber_count > 0) {
                *st = State::Idle;
            }
            // If subscriber_count is 0, the state is already Idle
            // (set by unsubscribe). No need to touch it.
        });

        Ok(())
    }
}

// ─── TaskDeregister ────────────────────────────────────────────────────────

/// An RAII guard that runs a cleanup closure when dropped.
///
/// Dropping this guard deregisters the associated subscriber from the
/// [`TaskRegistry`], stopping the underlying process if it was the last
/// subscriber.
pub struct TaskDeregister {
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl TaskDeregister {
    fn new<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(cleanup)),
        }
    }
}

impl Drop for TaskDeregister {
    fn drop(&mut self) {
        if let Some(f) = self.cleanup.take() {
            f();
        }
    }
}
