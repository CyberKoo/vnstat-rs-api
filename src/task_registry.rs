use dashmap::DashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

/// The type of each output line produced by a managed child process.
pub type Output = String;

/// A message emitted by the managed process lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskMessage {
    /// A single line of stdout data from the child process.
    Data(Output),
    /// An error or diagnostic string (e.g., spawn failure, read error).
    Error(Output),
    /// The process has exited and its stdout pipe has been closed.
    Eof,
}

type TaskKey = String;

/// A registry of named long-running subprocesses with per-subscriber output.
///
/// Multiple callers can subscribe to the same key; the underlying process is
/// started on the first subscription and stopped when the last subscriber
/// leaves. Each subscriber receives its own bounded channel. If a subscriber
/// cannot keep up, its messages are dropped (`try_send`) instead of blocking
/// the process pipeline — there is no backpressure.
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
    /// Buffer capacity (in messages) of each subscriber channel.
    subscriber_buffer: usize,
}

impl TaskRegistry {
    /// Creates a new, empty [`TaskRegistry`].
    ///
    /// `subscriber_buffer` is the per-subscriber channel capacity; see
    /// `[sse] subscriber_buffer` in the configuration file.
    pub fn new(subscriber_buffer: usize) -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
            subscriber_buffer,
        }
    }

    /// Subscribes to the task identified by `key`, spawning it if necessary.
    ///
    /// Returns a per-subscriber receiver and an RAII deregistration guard.
    /// When the guard is dropped, this subscriber is removed; the underlying
    /// process is stopped only when all subscribers have left.
    pub async fn subscribe(
        self: &Arc<Self>,
        key: TaskKey,
        cmd: Vec<String>,
    ) -> (mpsc::Receiver<TaskMessage>, TaskDeregister) {
        let entry = self
            .tasks
            .entry(key.clone())
            .or_insert_with(|| {
                debug!("Created task entry for key: {:?}", key);
                Arc::new(TaskHandle::new())
            })
            .clone();

        let (rx, subscriber_id) = entry.subscribe(cmd, self.subscriber_buffer).await;

        let weak_registry = Arc::downgrade(self);
        let dereg_key = key.clone();
        let dereg = TaskDeregister::new(move || {
            trace!("Task deregistered: {:?}", dereg_key);
            if let Some(registry) = weak_registry.upgrade() {
                if let Some(handle) = registry.tasks.get(&dereg_key) {
                    let last = handle.unsubscribe(subscriber_id);
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
///                  or process end  └────────┘
/// ```
struct TaskHandle {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Active {
        cancel_token: CancellationToken,
        /// One sender per active subscriber, identified by a unique id.
        subscribers: Vec<(usize, mpsc::Sender<TaskMessage>)>,
        next_id: usize,
    },
}

impl TaskHandle {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::Idle)),
        }
    }

    /// Subscribe to this task. Spawns the process if not already running.
    ///
    /// Returns the subscriber's channel receiver and its unique id (used to
    /// remove the sender on unsubscribe).
    async fn subscribe(
        &self,
        cmd: Vec<String>,
        buffer: usize,
    ) -> (mpsc::Receiver<TaskMessage>, usize) {
        let (tx, rx) = mpsc::channel(buffer);
        let mut need_spawn: Option<(Vec<String>, CancellationToken)> = None;
        let mut subscriber_id = 0;

        {
            let mut st = self.state.lock().expect("state mutex poisoned");
            match &mut *st {
                State::Idle => {
                    let token = CancellationToken::new();
                    need_spawn = Some((cmd.clone(), token.clone()));
                    *st = State::Active {
                        cancel_token: token,
                        subscribers: vec![(0, tx)],
                        next_id: 1,
                    };
                }
                State::Active {
                    subscribers,
                    next_id,
                    ..
                } => {
                    subscriber_id = *next_id;
                    subscribers.push((*next_id, tx));
                    *next_id += 1;
                }
            }
        }

        if let Some((cmd, token)) = need_spawn {
            if let Err(e) = Self::spawn_process(&self.state, &cmd, token) {
                error!("Spawn task failed! Error: {}", e);
                // Notify every subscriber and reset to Idle so the next
                // subscribe call can retry.
                let mut st = self.state.lock().expect("state mutex poisoned");
                let subscribers = match &mut *st {
                    State::Active { subscribers, .. } => std::mem::take(subscribers),
                    State::Idle => unreachable!("state was set to Active before spawning"),
                };
                *st = State::Idle;
                drop(st);
                for (_, tx) in subscribers {
                    let _ = tx.try_send(TaskMessage::Error("Spawn task failed".to_string()));
                }
            } else {
                debug!("Process spawned successfully");
            }
        }

        (rx, subscriber_id)
    }

    /// Unsubscribe one caller. Returns `true` if this was the last subscriber
    /// (i.e. the handle should be removed from the registry).
    fn unsubscribe(&self, id: usize) -> bool {
        let mut need_cancel: Option<CancellationToken> = None;

        {
            let mut st = self.state.lock().expect("state mutex poisoned");
            match &mut *st {
                State::Active {
                    subscribers,
                    cancel_token,
                    ..
                } => {
                    let before = subscribers.len();
                    subscribers.retain(|(sid, _)| *sid != id);
                    if subscribers.len() != before {
                        debug!(
                            "Subscriber {} unsubscribed ({} left)",
                            id,
                            subscribers.len()
                        );
                    }
                    if subscribers.is_empty() {
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

        let state = Arc::clone(state);
        let mut reader = BufReader::new(stdout).lines();
        let cmd_debug = cmd.to_vec();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        debug!("Cancellation received, killing process: {:?}", cmd_debug);
                        if let Err(e) = child.kill().await {
                            warn!("Failed to kill child process: {:?}: {}", cmd_debug, e);
                        }
                        break;
                    }
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(data)) => forward(&state, TaskMessage::Data(data)),
                            Ok(None) => {
                                warn!("Process finished (EOF): {:?}", cmd_debug);
                                finish(&state, Some(TaskMessage::Eof));
                                break;
                            }
                            Err(e) => {
                                error!("Read error from process {:?}: {}", cmd_debug, e);
                                finish(
                                    &state,
                                    Some(TaskMessage::Error(format!(
                                        "Failed to read line from process: {:?}, err: {}",
                                        cmd_debug, e
                                    ))),
                                );
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

/// Forwards `msg` to all live subscribers.
///
/// Messages are dropped for subscribers whose buffer is full (slow consumer)
/// — the reader never blocks on a subscriber. Senders whose receiver has been
/// dropped (unsubscribed) are removed.
fn forward(state: &Mutex<State>, msg: TaskMessage) {
    let mut st = state.lock().expect("state mutex poisoned");
    if let State::Active { subscribers, .. } = &mut *st {
        subscribers.retain(|(_, tx)| match tx.try_send(msg.clone()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                trace!("Subscriber buffer full, dropping message");
                true
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
    }
}

/// Sends the final message (if any) to all subscribers, closes every
/// subscriber channel, and resets the state to `Idle` so the next subscribe
/// call spawns a fresh process.
fn finish(state: &Mutex<State>, final_msg: Option<TaskMessage>) {
    let mut st = state.lock().expect("state mutex poisoned");
    let subscribers = match &mut *st {
        State::Active { subscribers, .. } => std::mem::take(subscribers),
        State::Idle => Vec::new(),
    };
    *st = State::Idle;
    drop(st);
    for (_, tx) in subscribers {
        if let Some(msg) = &final_msg {
            let _ = tx.try_send(msg.clone());
        }
    }
    // Senders are dropped here, closing every subscriber channel.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sh_cmd(script: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    }

    /// A command that prints the given lines (with real newlines) and exits.
    fn oneshot_cmd(lines: &str) -> Vec<String> {
        sh_cmd(&format!("printf '{}'", lines))
    }

    fn infinite_cmd() -> Vec<String> {
        sh_cmd("while true; do echo x; done")
    }

    async fn recv_timeout(rx: &mut mpsc::Receiver<TaskMessage>, ms: u64) -> Option<TaskMessage> {
        tokio::time::timeout(Duration::from_millis(ms), rx.recv())
            .await
            .ok()
            .flatten()
    }

    #[tokio::test]
    async fn forwards_lines_then_eof_then_closes() {
        let registry = Arc::new(TaskRegistry::new(4));
        let (mut rx, _guard) = registry
            .subscribe("k1".into(), oneshot_cmd("a\\nb\\nc\\n"))
            .await;
        assert_eq!(
            recv_timeout(&mut rx, 3000).await,
            Some(TaskMessage::Data("a".into()))
        );
        assert_eq!(
            recv_timeout(&mut rx, 3000).await,
            Some(TaskMessage::Data("b".into()))
        );
        assert_eq!(
            recv_timeout(&mut rx, 3000).await,
            Some(TaskMessage::Data("c".into()))
        );
        assert_eq!(recv_timeout(&mut rx, 3000).await, Some(TaskMessage::Eof));
        assert_eq!(recv_timeout(&mut rx, 3000).await, None);
    }

    #[tokio::test]
    async fn respawns_fresh_process_after_eof() {
        let registry = Arc::new(TaskRegistry::new(4));
        let key = "respawn".to_string();
        let (mut rx1, guard1) = registry
            .subscribe(key.clone(), oneshot_cmd("hello\\n"))
            .await;
        assert_eq!(
            recv_timeout(&mut rx1, 3000).await,
            Some(TaskMessage::Data("hello".into()))
        );
        assert_eq!(recv_timeout(&mut rx1, 3000).await, Some(TaskMessage::Eof));
        drop(guard1);

        let (mut rx2, _guard2) = registry.subscribe(key, oneshot_cmd("hello\\n")).await;
        assert_eq!(
            recv_timeout(&mut rx2, 3000).await,
            Some(TaskMessage::Data("hello".into()))
        );
        assert_eq!(recv_timeout(&mut rx2, 3000).await, Some(TaskMessage::Eof));
    }

    #[tokio::test]
    async fn shares_one_process_between_subscribers() {
        let marker = crate::test_support::write_temp_file("spawn-count", "");
        let _ = std::fs::remove_file(&marker);
        let script = format!(
            "echo start >> {}; while true; do echo x; done",
            marker.display()
        );
        let cmd = sh_cmd(&script);

        let registry = Arc::new(TaskRegistry::new(4));
        let key = "shared".to_string();
        let (mut rx1, _g1) = registry.subscribe(key.clone(), cmd.clone()).await;
        let (mut rx2, _g2) = registry.subscribe(key, cmd).await;

        assert_eq!(
            recv_timeout(&mut rx1, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );
        assert_eq!(
            recv_timeout(&mut rx2, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );

        std::thread::sleep(Duration::from_millis(300));
        let count = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            count.lines().count(),
            1,
            "only one process should be spawned"
        );
    }

    #[tokio::test]
    async fn drops_messages_for_slow_subscriber_only() {
        let registry = Arc::new(TaskRegistry::new(2));
        let key = "drop".to_string();
        let (mut rx_fast, _g_fast) = registry.subscribe(key.clone(), infinite_cmd()).await;
        // Slow subscriber: never reads from its channel, so its buffer fills
        // up and its messages are dropped.
        let (_rx_slow, _g_slow) = registry.subscribe(key, infinite_cmd()).await;

        // The fast subscriber must keep receiving even though the slow one is
        // saturated — per-subscriber drop, no backpressure.
        for _ in 0..50 {
            assert_eq!(
                recv_timeout(&mut rx_fast, 3000).await,
                Some(TaskMessage::Data("x".into()))
            );
        }
    }

    #[tokio::test]
    async fn removes_subscriber_that_leaves_mid_stream() {
        let registry = Arc::new(TaskRegistry::new(4));
        let key = "leave".to_string();
        let (mut rx_a, guard_a) = registry.subscribe(key.clone(), infinite_cmd()).await;
        let (mut rx_b, guard_b) = registry.subscribe(key, infinite_cmd()).await;

        // Both receive data initially.
        assert_eq!(
            recv_timeout(&mut rx_a, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );
        assert_eq!(
            recv_timeout(&mut rx_b, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );

        // B leaves while the reader keeps forwarding: its sender must be
        // reaped (Closed branch) without disturbing A.
        drop(guard_b);
        for _ in 0..10 {
            assert_eq!(
                recv_timeout(&mut rx_a, 3000).await,
                Some(TaskMessage::Data("x".into()))
            );
        }
        drop(guard_a);
    }

    #[tokio::test]
    async fn spawn_failure_notifies_and_rolls_back() {
        let registry = Arc::new(TaskRegistry::new(4));
        let key = "fail".to_string();
        let (mut rx, _guard) = registry
            .subscribe(key.clone(), vec!["/nonexistent-binary-xyz".into()])
            .await;

        match recv_timeout(&mut rx, 3000).await {
            Some(TaskMessage::Error(e)) => assert!(e.contains("Spawn task failed"), "got: {}", e),
            other => panic!("expected spawn error, got {:?}", other),
        }
        // All senders were dropped: the channel is closed.
        assert_eq!(recv_timeout(&mut rx, 3000).await, None);

        // State rolled back: a subsequent subscribe with a valid command works.
        let (mut rx2, _g2) = registry.subscribe(key, oneshot_cmd("ok\\n")).await;
        assert_eq!(
            recv_timeout(&mut rx2, 3000).await,
            Some(TaskMessage::Data("ok".into()))
        );
    }

    #[tokio::test]
    async fn read_error_notifies_subscribers() {
        let registry = Arc::new(TaskRegistry::new(4));
        // `printf '\377'` emits an invalid UTF-8 byte → line read error.
        let (mut rx, _guard) = registry
            .subscribe("bad-utf8".into(), sh_cmd("printf '\\377'"))
            .await;

        match recv_timeout(&mut rx, 3000).await {
            Some(TaskMessage::Error(e)) => assert!(e.contains("Failed to read line"), "got: {}", e),
            other => panic!("expected read error, got {:?}", other),
        }
        assert_eq!(recv_timeout(&mut rx, 3000).await, None);
    }

    #[tokio::test]
    async fn unsubscribe_after_eof_is_harmless() {
        let registry = Arc::new(TaskRegistry::new(4));
        let (mut rx, guard) = registry.subscribe("eof".into(), oneshot_cmd("x\\n")).await;
        assert_eq!(
            recv_timeout(&mut rx, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );
        assert_eq!(recv_timeout(&mut rx, 3000).await, Some(TaskMessage::Eof));
        // State is already Idle after EOF; dropping the guard must not panic
        // (it logs a warning and leaves the registry alone).
        drop(guard);
        assert_eq!(recv_timeout(&mut rx, 1000).await, None);
    }

    #[tokio::test]
    async fn last_unsubscribe_cancels_process_and_allows_resubscribe() {
        let registry = Arc::new(TaskRegistry::new(4));
        let key = "cancel".to_string();
        let (_rx1, guard1) = registry.subscribe(key.clone(), infinite_cmd()).await;
        let (_rx2, guard2) = registry.subscribe(key.clone(), infinite_cmd()).await;
        drop(guard1);
        drop(guard2);

        // With all subscribers gone the process is cancelled; subscribing
        // again spawns a fresh one that streams data.
        let (mut rx3, _g3) = registry.subscribe(key, infinite_cmd()).await;
        assert_eq!(
            recv_timeout(&mut rx3, 3000).await,
            Some(TaskMessage::Data("x".into()))
        );
    }
}
