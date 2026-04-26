use axum::{extract::State, http::StatusCode, response::Json};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

pub async fn health_check(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let db_healthy = match state.db.query("SELECT 1", ()).await {
        Ok(mut rows) => rows.next().await.is_ok(),
        Err(e) => {
            tracing::error!("Health check DB query failed: {}", e);
            false
        }
    };

    let status = if db_healthy { "healthy" } else { "unhealthy" };
    let code = if db_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(json!({
            "status": status,
            "database": db_healthy,
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}
