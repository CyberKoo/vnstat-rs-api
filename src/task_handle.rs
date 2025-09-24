use anyhow::{anyhow, bail, Context, Result};
use log::{debug, error, trace, warn};
use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast::{self, Sender};
use tokio_util::sync::CancellationToken;

pub type Output = String;

#[derive(Clone)]
#[allow(dead_code)]
pub enum TaskMessage {
    Data(Output),
    Error(Output),
    Eof,
}

/// Internal state protected by a single mutex to guarantee atomicity
/// across ref_count changes and token lifecycle.
#[derive(Default)]
struct State {
    /// Number of current subscribers.
    ref_count: usize,
    /// Cancellation token for the running process (if any).
    cancel_token: Option<CancellationToken>,
}

pub struct TaskHandle {
    tx: Sender<TaskMessage>,
    state: Arc<Mutex<State>>,
}

impl TaskHandle {
    /// Create a new task handle.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            tx,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    /// Subscribe to the task output.
    ///
    /// If this is the first subscriber (ref_count moves 0 -> 1) and there is
    /// no running process (no cancel_token), a new CancellationToken is created
    /// and stored atomically and a process will be spawned outside the lock.
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

    /// Unsubscribe from the task output.
    ///
    /// When the last subscriber unsubscribes (ref_count moves 1 -> 0),
    /// we take the token (atomically) and cancel outside the lock.
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

    /// Spawn the child process and a background task to forward its output.
    ///
    /// This function assumes the provided token has already been stored in state
    /// under the mutex. It does not touch ref_count or the token storage; it only
    /// uses the token to observe cancellation.
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

    fn broadcast<T>(tx: &Sender<T>, msg: T) {
        if let Err(e) = tx.send(msg) {
            warn!("broadcast failed: {:?}", e.to_string());
        }
    }
}

pub struct TaskDropGuard {
    cleanup: Option<Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>>,
}

impl TaskDropGuard {
    /// Create a guard that runs the provided async cleanup function on drop.
    pub fn new<F, Fut>(cleanup_fn: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(move || Box::pin(cleanup_fn()))),
        }
    }
}

impl Drop for TaskDropGuard {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            // Fire-and-forget cleanup.
            tokio::spawn(cleanup());
        }
    }
}
