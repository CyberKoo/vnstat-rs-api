mod guard;
mod handle;
mod message;
mod process_runner;
mod state;

pub use guard::TaskDeregister;
use handle::TaskHandle;
#[allow(unused_imports)]
pub use message::{Output, TaskMessage};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, trace};

type TaskKey = String;

/// A registry of named long-running subprocesses with per-subscriber output.
#[derive(Clone)]
pub struct TaskRegistry {
    tasks: Arc<Mutex<HashMap<TaskKey, Arc<TaskHandle>>>>,
    subscriber_buffer: usize,
}

impl TaskRegistry {
    pub fn new(subscriber_buffer: usize) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            subscriber_buffer,
        }
    }

    pub async fn subscribe(
        self: &Arc<Self>,
        key: TaskKey,
        cmd: Vec<String>,
    ) -> (mpsc::Receiver<TaskMessage>, TaskDeregister) {
        let mut tasks = self.tasks.lock().expect("tasks mutex poisoned");
        let entry = tasks
            .entry(key.clone())
            .or_insert_with(|| {
                debug!("created task entry for key: {:?}", key);
                Arc::new(TaskHandle::new())
            })
            .clone();

        let (receiver, subscriber_id) = entry.subscribe(cmd, self.subscriber_buffer);
        drop(tasks);
        let tasks_ref = Arc::downgrade(&self.tasks);
        let dereg_key = key.clone();
        let dereg_handle = entry;
        let deregister = TaskDeregister::new(move || {
            trace!("task deregistered: {:?}", dereg_key);
            if let Some(tasks_ref) = tasks_ref.upgrade() {
                let mut tasks = tasks_ref.lock().expect("tasks mutex poisoned");
                dereg_handle.unsubscribe(subscriber_id);
                if dereg_handle.is_idle()
                    && tasks
                        .get(&dereg_key)
                        .is_some_and(|current| Arc::ptr_eq(current, &dereg_handle))
                {
                    tasks.remove(&dereg_key);
                }
            } else {
                dereg_handle.unsubscribe(subscriber_id);
            }
        });

        (receiver, deregister)
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
    async fn stale_guard_does_not_remove_new_generation_subscriber() {
        let registry = Arc::new(TaskRegistry::new(4));
        let (mut old_rx, old_guard) = registry
            .subscribe("generation".into(), oneshot_cmd("old\\n"))
            .await;
        assert_eq!(
            recv_timeout(&mut old_rx, 3000).await,
            Some(TaskMessage::Data("old".into()))
        );
        assert_eq!(
            recv_timeout(&mut old_rx, 3000).await,
            Some(TaskMessage::Eof)
        );

        let (mut new_rx, new_guard) = registry
            .subscribe("generation".into(), infinite_cmd())
            .await;
        drop(old_guard);

        assert!(matches!(
            recv_timeout(&mut new_rx, 3000).await,
            Some(TaskMessage::Data(_))
        ));
        drop(new_guard);
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
    async fn cloned_registry_still_cleans_up_when_original_is_dropped() {
        let registry = Arc::new(TaskRegistry::new(4));
        let clone = Arc::new((*registry).clone());
        let (_rx, guard) = registry.subscribe("cloned".into(), infinite_cmd()).await;
        drop(registry);
        drop(guard);
        assert!(clone.tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn last_subscriber_removes_registry_key() {
        let registry = Arc::new(TaskRegistry::new(4));
        let (_rx1, guard1) = registry.subscribe("cleanup".into(), infinite_cmd()).await;
        let (_rx2, guard2) = registry.subscribe("cleanup".into(), infinite_cmd()).await;
        assert_eq!(registry.tasks.lock().unwrap().len(), 1);

        drop(guard1);
        assert!(registry.tasks.lock().unwrap().contains_key("cleanup"));
        drop(guard2);
        assert!(!registry.tasks.lock().unwrap().contains_key("cleanup"));
    }

    #[tokio::test]
    async fn old_guard_cannot_remove_a_new_generation() {
        let registry = Arc::new(TaskRegistry::new(4));
        let (mut old_rx, old_guard) = registry
            .subscribe("generation-cleanup".into(), oneshot_cmd("old\\n"))
            .await;
        assert_eq!(
            recv_timeout(&mut old_rx, 3000).await,
            Some(TaskMessage::Data("old".into()))
        );
        assert_eq!(
            recv_timeout(&mut old_rx, 3000).await,
            Some(TaskMessage::Eof)
        );

        let (_new_rx, new_guard) = registry
            .subscribe("generation-cleanup".into(), infinite_cmd())
            .await;
        drop(old_guard);
        assert!(
            registry
                .tasks
                .lock()
                .unwrap()
                .contains_key("generation-cleanup")
        );
        drop(new_guard);
        assert!(
            !registry
                .tasks
                .lock()
                .unwrap()
                .contains_key("generation-cleanup")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_resubscribe_and_cleanup_keep_one_registered_handle() {
        let registry = Arc::new(TaskRegistry::new(4));
        for _ in 0..50 {
            let (_old_rx, old_guard) = registry.subscribe("racing".into(), infinite_cmd()).await;
            let start = Arc::new(tokio::sync::Barrier::new(2));
            let joining = {
                let registry = Arc::clone(&registry);
                let start = Arc::clone(&start);
                tokio::spawn(async move {
                    start.wait().await;
                    registry.subscribe("racing".into(), infinite_cmd()).await
                })
            };
            start.wait().await;
            drop(old_guard);
            let (_new_rx, new_guard) = joining.await.unwrap();
            assert!(registry.tasks.lock().unwrap().contains_key("racing"));
            assert_eq!(registry.tasks.lock().unwrap().len(), 1);
            drop(new_guard);
            assert!(registry.tasks.lock().unwrap().is_empty());
        }
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
