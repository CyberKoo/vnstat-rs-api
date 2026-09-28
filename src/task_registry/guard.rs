/// An RAII guard that runs a cleanup closure when dropped.
pub struct TaskDeregister {
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

impl TaskDeregister {
    pub(super) fn new<F>(cleanup: F) -> Self
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
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}
