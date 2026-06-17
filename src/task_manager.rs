use crate::task_handle::{TaskDropGuard, TaskHandle, TaskMessage};
use dashmap::DashMap;
use tracing::{debug, trace, warn};
use std::sync::Arc;
use tokio::sync::broadcast;

/// A unique identifier for a managed task.
///
/// Used as a lookup key within the [`TaskManager`] to track, subscribe to, and
/// clean up running processes. Tasks with the same key share a single underlying
/// [`TaskHandle`].
type TaskKey = String;

/// Manages the lifecycle of keyed subprocesses with reference counting.
///
/// `TaskManager` provides a central registry of running tasks, each identified
/// by a [`TaskKey`]. Multiple callers can subscribe to the same task; the task
/// is spawned on first subscription and is kept alive until all subscribers have
/// dropped their [`TaskDropGuard`].
///
/// Internally, the manager stores a shared [`DashMap`] mapping keys to
/// [`Arc<TaskHandle>`] instances. Subscriptions increment an internal reference
/// count on the handle, and automatic cleanup via the drop guard decrements it
/// and stops the process when the count reaches zero.
///
/// # Clone semantics
///
/// Cloning a `TaskManager` is cheap (an `Arc` clone) and produces another
/// handle to the **same** underlying registry.
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<DashMap<TaskKey, Arc<TaskHandle>>>,
}

impl TaskManager {
    /// Creates a new, empty `TaskManager`.
    ///
    /// No tasks are registered at construction time. Tasks are added lazily
    /// when [`subscribe`](Self::subscribe) is called with a new key.
    ///
    /// # Returns
    ///
    /// A new `TaskManager` with no registered tasks.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
        }
    }

    /// Subscribes to a task identified by `key`, spawning it if necessary.
    ///
    /// If a task with the given `key` already exists, this method attaches a
    /// new subscriber to the existing handle. Otherwise, a new
    /// [`TaskHandle`] is created and stored in the registry, and the
    /// associated process is spawned using the provided `cmd`.
    ///
    /// The returned [`TaskDropGuard`] **must** be kept alive for the duration
    /// of the subscription. Dropping it will automatically unsubscribe, and
    /// when the last subscriber drops, the underlying task is stopped.
    ///
    /// # Arguments
    ///
    /// * `key` - The unique key identifying the task. Callers that use the
    ///   same key share the same underlying process.
    /// * `cmd` - The command and its arguments used to spawn the process
    ///   when the task is first created.
    ///
    /// # Returns
    ///
    /// A tuple of:
    ///
    /// * [`broadcast::Receiver<TaskMessage>`] — a receiver for messages
    ///   emitted by the task (e.g. output lines, exit events).
    /// * [`TaskDropGuard`] — an RAII guard that, when dropped, unsubscribes
    ///   this caller from the task.
    pub async fn subscribe(
        self: &Arc<Self>,
        key: TaskKey,
        cmd: Vec<String>,
    ) -> (broadcast::Receiver<TaskMessage>, TaskDropGuard) {
        // Insert a new task entry if one does not already exist for this key.
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

    /// Unsubscribes from the task identified by `key`.
    ///
    /// Decrements the reference count on the associated [`TaskHandle`]. When
    /// the count reaches zero, the handle stops its underlying process and
    /// removes itself from the registry.
    ///
    /// This method is a no-op (with a warning) if no task is registered for
    /// the given `key`.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the task to unsubscribe from.
    fn unsubscribe(&self, key: &TaskKey) {
        if let Some(entry) = self.tasks.get(key) {
            entry.unsubscribe();
        } else {
            warn!("Tried to unsubscribe from nonexistent key: {:?}", key);
        }
    }

    /// Creates a [`TaskDropGuard`] for the given `key`.
    ///
    /// The returned guard holds a weak reference to the `TaskManager`. When
    /// dropped, it calls [`unsubscribe`](Self::unsubscribe) on the manager,
    /// ensuring the caller's subscription is cleaned up even if the guard is
    /// discarded without an explicit unsubscribe call.
    ///
    /// # Arguments
    ///
    /// * `key` - The task key that the drop guard should clean up.
    ///
    /// # Returns
    ///
    /// A [`TaskDropGuard`] that will unsubscribe from the given `key` when
    /// it is dropped.
    fn get_drop_guard(self: &Arc<Self>, key: TaskKey) -> TaskDropGuard {
        let weak_manager = Arc::downgrade(self);
        let cleanup_key = key.clone();

        TaskDropGuard::new(move || {
            trace!("SSE Stream closed");
            if let Some(manager) = weak_manager.upgrade() {
                manager.unsubscribe(&cleanup_key);
            }
        })
    }
}
