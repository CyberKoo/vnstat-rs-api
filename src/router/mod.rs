use axum::Router;

mod vnstat;

pub fn get_router() -> Router {
    Router::new().nest_service("/vnstat", vnstat::router())
}
