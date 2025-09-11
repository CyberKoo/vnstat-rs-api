use anyhow::{anyhow, bail, Context, Result};
use log::{debug, error, trace, warn};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast::Sender;
use tokio::sync::{broadcast, OnceCell};
use tokio_util::sync::CancellationToken;

pub type Output = String;

#[derive(Clone)]
#[allow(dead_code)]
pub enum TaskMessage {
    Data(Output),
    Event(Output),
    Comment(Output),
    Error(Output),
    Eof,
}

pub struct TaskHandle {
    tx: Sender<TaskMessage>,
    cancel_token: OnceCell<CancellationToken>,
    ref_count: AtomicUsize,
    spawned: AtomicBool,
}

impl TaskHandle {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            tx,
            cancel_token: OnceCell::new(),
            ref_count: AtomicUsize::new(0),
            spawned: AtomicBool::new(false),
        }
    }

    pub async fn subscribe(self: &Arc<Self>, cmd: Vec<String>) -> broadcast::Receiver<TaskMessage> {
        // 原子性增加引用计数
        let old_count = self.ref_count.fetch_add(1, Ordering::AcqRel);

        // 如果是第一个订阅者且还未spawn，则spawn
        if old_count == 0 && !self.spawned.swap(true, Ordering::AcqRel) {
            let cmd_clone = cmd.clone();
            let tx = self.tx.clone();

            match self.spawn_process(cmd_clone).await {
                Ok(token) => {
                    self.cancel_token.set(token).ok();
                    debug!("Cancel token stored after spawn process");
                }
                Err(error) => {
                    error!("Spawn task failed!!! Error: {}", error);
                    TaskHandle::broadcast(&tx, TaskMessage::Error("Spawn task failed".to_string()));
                }
            }
        }

        self.tx.subscribe()
    }

    pub fn unsubscribe(self: &Arc<Self>) {
        let old_count = self.ref_count.fetch_sub(1, Ordering::AcqRel);
        if old_count == 1 {
            if let Some(token) = self.cancel_token.get() {
                token.cancel();
            }

            self.spawned.store(false, Ordering::Release);
        }
    }

    pub async fn spawn_process(self: &Arc<Self>, cmd: Vec<String>) -> Result<CancellationToken> {
        let tx = self.tx.clone();
        let cancel_token = CancellationToken::new();

        if cmd.is_empty() {
            bail!("spawn_process called with empty cmd, skipping spawn.");
        }

        let program = cmd[0].clone();
        let args = cmd[1..].to_vec();

        trace!("Spawning process: {:?} {:?}", program, args);

        // 用 context 包裹错误信息
        let mut child = Command::new(&program)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn child process: {:?} {:?}", program, args))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdout of process: {:?}", cmd))?;

        let mut reader = BufReader::new(stdout).lines();
        let cancel_token_clone = cancel_token.clone();

        // 进程输出处理还是放到后台任务
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
                                TaskHandle::broadcast(&tx, TaskMessage::Error(format!("Failed to read line from process: {:?}, err: {}", cmd, e)));
                                break;
                            }
                        }
                    }
                }
            }
            trace!("Process handler exited: {:?}", cmd);
        });

        Ok(cancel_token)
    }

    fn broadcast<T>(tx: &Sender<T>, msg: T) {
        if let Err(e) = tx.send(msg) {
            warn!("broadcast send failed: {:?}", e.to_string());
        }
    }
}

pub struct TaskDropGuard {
    cleanup: Option<Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>>,
}

impl TaskDropGuard {
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
            // fire-and-forget
            tokio::spawn(cleanup());
        }
    }
}
