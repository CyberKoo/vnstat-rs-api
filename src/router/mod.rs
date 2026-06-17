use crate::service::vnstat_service::VnstatService;
use crate::task_manager::TaskManager;
use axum::Router;
use std::sync::Arc;

mod vnstat;

/// Application-wide shared state injected into every Axum handler via
/// [axum::extract::State].
///
/// Holds the long-lived services that handlers need to query vnstat data
/// and manage background tasks.
#[derive(Clone)]
pub struct AppState {
    /// Service for fetching and streaming network statistics from the
    /// underlying `vnstat` daemon.
    pub vnstat: Arc<VnstatService>,

    /// Manager that controls the lifecycle of background tasks such as
    /// SSE keep-alive watchers.
    pub task_manager: Arc<TaskManager>,
}

/// Assembles the top-level Axum [`Router`] and returns it.
///
/// All routes are prefixed under `/vnstat` and delegate to the sub-module
/// [`vnstat::router`].
pub fn get_router() -> Router<AppState> {
    Router::new().nest("/vnstat", vnstat::router())
}
