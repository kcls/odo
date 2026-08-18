use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::json;
use std::sync::Arc;

pub trait HasDatabase {
    fn db(&self) -> &DatabaseConnection;
}

/// Health check which fails if the database is not responding.
pub async fn check<S: HasDatabase + Send + Sync + 'static>(
    State(state): State<Arc<S>>,
) -> Response {
    let ok = state.db().execute_unprepared("SELECT 1").await.is_ok();

    if ok {
        (StatusCode::OK, Json(json!({"status": "ok"}))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "degraded", "reason": "database unreachable"})),
        )
            .into_response()
    }
}
