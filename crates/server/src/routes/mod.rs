mod android;
mod cells;
mod upload;

use axum::{routing::{get, post}, Router};
use std::sync::Arc;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/api/cells", get(cells::cells_handler))
        .route("/api/measurements", get(cells::measurements_handler))
        .route("/api/upload", post(upload::upload_handler))
        .route("/api/ws/android", get(android::android_ws_handler))
}

async fn health() -> &'static str {
    "ok"
}
