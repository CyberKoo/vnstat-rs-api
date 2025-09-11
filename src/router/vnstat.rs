use crate::error_code::ErrorCode;
use crate::model::jsend::JsendResponse;
use crate::model::vnstat::{Interface, VnstatData};
use crate::service::vnstat_service;
use crate::task_manager::TaskManager;
use crate::utils::sse::sse_with_default_headers;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::KeepAlive;
use axum::response::{IntoResponse, Redirect, Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use log::{info, trace};
use std::sync::Arc;

pub fn router() -> Router {
    let task_manager = Arc::new(TaskManager::new());

    Router::new()
        .route("/", get(get_data))
        .route("/version", get(get_version))
        .route("/interfaces", get(get_interfaces))
        .route("/interfaces/{if_name}", get(redir_to_traffic))
        .route("/interfaces/{if_name}/traffic", get(get_interface_data))
        .route("/interfaces/{if_name}/live", get(get_interface_live_sse))
        .with_state(task_manager) // attach to axum
}

async fn get_version()
-> Result<Json<JsendResponse<String>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = vnstat_service::fetch_vnstat_data().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(data.vnstatversion)))
}

async fn redir_to_traffic(Path(if_name): Path<String>) -> impl IntoResponse {
    Redirect::permanent(&format!("{}/traffic", if_name))
}

async fn get_data()
-> Result<Json<JsendResponse<VnstatData>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = vnstat_service::fetch_vnstat_data().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(data)))
}

async fn get_interfaces()
-> Result<Json<JsendResponse<Vec<String>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let interfaces = vnstat_service::list_vnstat_interfaces().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    // let response = InterfacesResponse { name: interfaces };

    Ok(Json(JsendResponse::success_with_data(interfaces)))
}

async fn get_interface_data(
    Path(if_name): Path<String>,
) -> Result<Json<JsendResponse<Interface>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = vnstat_service::fetch_interface_stats(if_name)
        .await
        .map_err(|e| {
            info!("err: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
            )
        })?;

    Ok(Json(JsendResponse::success_with_data(data)))
}

// sse live
pub async fn get_interface_live_sse(
    Path(if_name): Path<String>,
    State(manager): State<Arc<TaskManager>>,
) -> Response {
    trace!("SSE stream for interface `{}` connected.", if_name);

    let stream = vnstat_service::stream_interface_live_stats(manager, if_name).await;
    let sse = Sse::new(stream).keep_alive(KeepAlive::default());

    sse_with_default_headers(sse)
}
