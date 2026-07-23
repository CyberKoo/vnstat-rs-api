use crate::service::vnstat_service::VnstatService;
use crate::task_registry::TaskRegistry;
use axum::Router;
use axum::routing::get;
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

    /// Registry of named subprocesses whose output is broadcast to
    /// multiple subscribers (used for SSE live streams).
    pub task_registry: Arc<TaskRegistry>,
}

/// Assembles the top-level Axum [`Router`] and returns it.
///
/// Service-level endpoints (`/health`, `/version`) are registered directly.
/// Interface-related routes live under `/interfaces`.
pub fn get_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(vnstat::get_health))
        .route("/version", get(vnstat::get_version))
        .nest("/interfaces", vnstat::interfaces_router())
}
