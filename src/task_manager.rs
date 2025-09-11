use crate::task_handle::{TaskDropGuard, TaskHandle, TaskMessage};
use dashmap::DashMap;
use log::{debug, trace, warn};
use std::sync::Arc;
use tokio::sync::broadcast;

type TaskKey = String;

/// Manages tasks by key, tracking references and handling spawn/lifecycle.
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<DashMap<TaskKey, Arc<TaskHandle>>>,
}

impl TaskManager {
    /// Create a new TaskManager.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
        }
    }

    /// Subscribe to a task by key, spawning the process if needed.
    ///
    /// Returns a broadcast receiver for task messages and a drop guard
    /// to automatically unsubscribe when dropped.
    pub async fn subscribe(
        self: &Arc<Self>,
        key: TaskKey,
        cmd: Vec<String>,
    ) -> (broadcast::Receiver<TaskMessage>, TaskDropGuard) {
        // Insert new task entry if not present.
        let entry = self
            .tasks
            .entry(key.clone())
            .or_insert_with(|| {
                debug!("Created task entry for key: {:?}", key);
                Arc::new(TaskHandle::new())
            })
            .clone();

        // Return the receiver and a drop guard for cleanup.
        (entry.subscribe(cmd).await, self.get_drop_guard(key))
    }

    /// Unsubscribe from a task, decrementing ref_count and stopping task if needed.
    fn unsubscribe(&self, key: &TaskKey) {
        if let Some(entry) = self.tasks.get(key) {
            entry.unsubscribe();
        } else {
            warn!("Tried to unsubscribe from nonexistent key: {:?}", key);
            return;
        }
    }

    /// Create a drop guard that automatically unsubscribes on drop.
    fn get_drop_guard(self: &Arc<Self>, key: TaskKey) -> TaskDropGuard {
        let weak_manager = Arc::downgrade(self);
        let cleanup_key = key.clone();

        TaskDropGuard::new(move || async move {
            trace!("SSE Stream closed");
            if let Some(manager) = weak_manager.upgrade() {
                manager.unsubscribe(&cleanup_key);
            }
        })
    }
}
