use anyhow::{Context, Result, anyhow, bail};
use tracing::{debug, error, trace, warn};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast::{self, Sender};
use tokio_util::sync::CancellationToken;

/// The type of each output line produced by a managed child process.
pub type Output = String;

/// A message emitted by the managed process lifecycle.
///
/// Each variant carries a different kind of payload that subscribers can
/// match on to distinguish normal output from error information or
/// end-of-stream signals.
#[derive(Clone)]
#[allow(dead_code)]
pub enum TaskMessage {
    /// A single line of stdout data from the child process.
    Data(Output),
    /// An error or diagnostic string (e.g., spawn failure, read error).
    Error(Output),
    /// The process has exited and its stdout pipe has been closed.
    Eof,
}

/// Internal state protected by a single mutex to guarantee atomicity
/// across [`ref_count`](State::ref_count) changes and
/// [`cancel_token`](State::cancel_token) lifecycle.
#[derive(Default)]
struct State {
    /// Number of current subscribers (`TaskHandle` references).
    ref_count: usize,
    /// Cancellation token for the running process, if one has been spawned.
    cancel_token: Option<CancellationToken>,
}

/// A handle to a long-running child process that broadcasts its output to
/// multiple subscribers.
///
/// The handle uses a reference-counting scheme so that the underlying process
/// is spawned only when the first subscriber arrives and is cancelled only
/// when the last subscriber leaves. Output lines are forwarded to all
/// current subscribers via a `tokio::sync::broadcast` channel.
///
/// # Cancellation safety
///
/// Spawning and cancellation decisions are made atomically under a mutex,
/// and the potentially blocking I/O work is performed outside the critical
/// section.
pub struct TaskHandle {
    /// Broadcast sender that all subscribers receive output through.
    tx: Sender<TaskMessage>,
    /// Shared, mutex-protected state for reference counting and cancellation.
    state: Arc<Mutex<State>>,
}

impl TaskHandle {
    /// Creates a new `TaskHandle` with no running process.
    ///
    /// # Returns
    ///
    /// A new `TaskHandle` ready to accept subscribers. No process is spawned
    /// until the first [`subscribe`](Self::subscribe) call.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            tx,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    /// Registers a new subscriber and optionally spawns the child process.
    ///
    /// Each call to this method increments an internal reference count and
    /// returns a new broadcast receiver. If this is the **first** subscriber
    /// (`ref_count` transitions from 0 → 1) **and** there is no running
    /// process yet, a [`CancellationToken`] is created and the command is
    /// spawned immediately (outside the lock).
    ///
    /// If spawning fails, an error message is broadcast to all subscribers
    /// and the token is rolled back so the next subscriber can retry.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command and its arguments to run. The first element is
    ///   the program name; subsequent elements are passed as arguments.
    ///
    /// # Returns
    ///
    /// A `tokio::sync::broadcast::Receiver` that yields [`TaskMessage`]
    /// values (data lines, errors, or end-of-stream).
    pub async fn subscribe(
        &self,
        cmd: Vec<String>,
    ) -> tokio::sync::broadcast::Receiver<TaskMessage> {
        // Decide whether to spawn outside the critical section to avoid long hold times.
        let mut need_spawn: Option<(Vec<String>, CancellationToken)> = None;

        {
            let mut st = self.state.lock().expect("TaskHandle.state mutex poisoned");
            st.ref_count += 1;

            // First subscriber and no running process: prepare to spawn.
            if st.ref_count == 1 && st.cancel_token.is_none() {
                let token = CancellationToken::new();
                st.cancel_token = Some(token.clone());
                need_spawn = Some((cmd.clone(), token));
            }
        }

        if let Some((cmd_to_spawn, token)) = need_spawn {
            if let Err(error) = self.spawn_process(cmd_to_spawn, token.clone()) {
                error!("Spawn task failed! Error: {}", error);
                TaskHandle::broadcast(
                    &self.tx,
                    TaskMessage::Error("Spawn task failed".to_string()),
                );

                // Roll back token to allow future retries.
                let mut st = self.state.lock().expect("TaskHandle.state mutex poisoned");
                st.cancel_token = None;
            } else {
                debug!("Cancel token stored and process spawned");
            }
        }

        self.tx.subscribe()
    }

    /// Deregisters a subscriber and cancels the process if it was the last.
    ///
    /// This method decrements the internal reference count. When the count
    /// reaches zero (i.e., the last subscriber has unsubscribed), the stored
    /// [`CancellationToken`] is taken from the state (atomically under the
    /// mutex) and cancelled **outside** the lock, allowing the background
    /// task to clean up the child process.
    pub fn unsubscribe(&self) {
        let token_to_cancel = {
            let mut st = self.state.lock().expect("TaskHandle.state mutex poisoned");

            if st.ref_count == 0 {
                warn!("unsubscribe called with ref_count == 0");
                None
            } else {
                st.ref_count -= 1;
                if st.ref_count == 0 {
                    st.cancel_token.take()
                } else {
                    None
                }
            }
        };

        if let Some(token) = token_to_cancel {
            token.cancel();
        }
    }

    /// Spawns the child process and launches a background task to forward
    /// its stdout.
    ///
    /// The caller **must** have already stored the provided
    /// `cancel_token` in [`State::cancel_token`] under the mutex before
    /// calling this method. This function does **not** touch the reference
    /// count or the token storage; it only uses the token to observe
    /// cancellation.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command vector where `cmd[0]` is the program and
    ///   `cmd[1..]` are its arguments.
    /// * `cancel_token` - A cancellation token that will signal the
    ///   background reader task to kill the child process.
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful spawn and background-task registration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `cmd` is empty.
    /// * The OS fails to spawn the child process (e.g., the program is not
    ///   found).
    /// * The spawned process's stdout cannot be captured.
    fn spawn_process(&self, cmd: Vec<String>, cancel_token: CancellationToken) -> Result<()> {
        if cmd.is_empty() {
            bail!("spawn_process called with empty cmd, skipping spawn.");
        }

        let program = cmd[0].clone();
        let args = cmd[1..].to_vec();

        trace!("Spawning process: {} {:?}", &program, &args);

        let mut child = Command::new(&program)
            .args(&args)
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn child process: {} {:?}", program, args))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdout of process: {:?}", cmd))?;

        let tx = self.tx.clone();
        let mut reader = BufReader::new(stdout).lines();
        let cancel_token_clone = cancel_token.clone();
        let state = Arc::clone(&self.state);

        // Background task: forward output lines, handle cancellation, clean up token on exit.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token_clone.cancelled() => {
                        debug!("Cancellation received, killing process: {:?}", cmd);
                        if let Err(e) = child.kill().await {
                            TaskHandle::broadcast(&tx, TaskMessage::Error(format!("Failed to kill child: {e}")));
                        }
                        break;
                    }
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                TaskHandle::broadcast(&tx, TaskMessage::Data(line));
                            }
                            Ok(None) => {
                                warn!("Process finished (EOF): {:?}", cmd);
                                TaskHandle::broadcast(&tx, TaskMessage::Eof);
                                break;
                            }
                            Err(e) => {
                                error!("Read error from process {:?}: {}", cmd, e);
                                TaskHandle::broadcast(&tx, TaskMessage::Error(format!(
                                    "Failed to read line from process: {:?}, err: {}", cmd, e
                                )));
                                break;
                            }
                        }
                    }
                }
            }

            trace!("Process handler exited: {:?}", cmd);

            // Clean up token after the process ends, enabling next 0->1 transition to spawn again.
            let mut st = state.lock().expect("TaskHandle.state mutex poisoned");
            st.cancel_token = None;
        });

        Ok(())
    }

    /// Sends a message to all active broadcast receivers, suppressing errors
    /// when there are no receivers.
    ///
    /// # Arguments
    ///
    /// * `tx` - The broadcast sender.
    /// * `msg` - The message to broadcast.
    fn broadcast<T>(tx: &Sender<T>, msg: T) {
        if let Err(e) = tx.send(msg) {
            warn!("broadcast failed: {:?}", e.to_string());
        }
    }
}

/// A guard that runs a cleanup closure when dropped.
///
/// This type is useful for tying resource cleanup (e.g., calling
/// [`TaskHandle::unsubscribe`]) to the lifetime of a scope, ensuring the
/// cleanup runs automatically when the guard goes out of scope, even
/// through early returns or panics (subject to normal panic unwind
/// behaviour).
pub struct TaskDropGuard {
    /// The optional closure to execute on `drop`.
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl TaskDropGuard {
    /// Creates a new `TaskDropGuard` with the given cleanup function.
    ///
    /// # Arguments
    ///
    /// * `cleanup_fn` - A closure to run when this guard is dropped. The
    ///   closure must be `Send + 'static`.
    ///
    /// # Returns
    ///
    /// A new `TaskDropGuard` that will run `cleanup_fn` on `drop`.
    pub fn new<F>(cleanup_fn: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(cleanup_fn)),
        }
    }
}

impl Drop for TaskDropGuard {
    /// Executes the stored cleanup closure, if one is present.
    ///
    /// The closure is consumed (`FnOnce`) so it will only run once. If the
    /// closure has already been taken (e.g., via a manual invocation), this
    /// is a no-op.
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}
