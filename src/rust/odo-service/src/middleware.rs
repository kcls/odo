use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use std::sync::Arc;
use uuid::Uuid;

use odo_client::auth::TokenManager;
use odo_client::context::RequestContext;

/// Add the in-process API call request_id and path info to the log span.
pub async fn request_tracing(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        });

    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        client_ip = client_ip.as_deref().unwrap_or("-"),
        method = %method,
        path = %path,
    );

    let ctx = RequestContext::new(&request_id).with_client_ip(client_ip);
    let _guard = span.enter();
    let start = std::time::Instant::now();

    let mut response = ctx.scope(next.run(request)).await;

    // Propagate request ID on the response for caller correlation
    if let Ok(val) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }

    let elapsed = start.elapsed();
    let status = response.status().as_u16();

    if status >= 400 {
        // Extract the full body of the response so we can log it for errors.
        let (parts, body) = response.into_parts();

        let bytes = body
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();
        let body_str = String::from_utf8_lossy(&bytes);

        tracing::error!(
            status,
            elapsed_ms = elapsed.as_millis() as u64,
            body = %body_str,
            "API Request failed"
        );

        Response::from_parts(parts, Body::from(bytes))
    } else {
        tracing::info!(
            status,
            elapsed_ms = elapsed.as_millis() as u64,
            "API Request completed"
        );

        response
    }
}

/// Trait for app states that can provide a TokenManager for auth.
pub trait HasTokenManager {
    fn token_manager(&self) -> &TokenManager;
}

/// Extract the auth header, validates the JWT, and add JWT claims
/// to the current request context.
pub async fn require_auth<S: HasTokenManager + Send + Sync + 'static>(
    State(state): State<Arc<S>>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(auth_error)?;

    let token = header.strip_prefix("Bearer ").ok_or_else(auth_error)?;

    let claims = state
        .token_manager()
        .verify(token)
        .map_err(|_| auth_error())?;

    let ctx = RequestContext::current()
        .unwrap_or_else(RequestContext::generate)
        .with_auth(token.to_string(), claims);

    Ok(ctx.scope(next.run(request)).await)
}

fn auth_error() -> Response {
    let body = serde_json::json!({
        "code": "UNAUTHENTICATED",
        "message": "Missing or invalid authorization",
    });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

/// Returns true if HTTP request body content should be logged.
///
/// NOTE Logs potentially sensitive data.  Use with caution
fn log_request_body() -> bool {
    static VAL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VAL.get_or_init(|| {
        std::env::var("ODO_LOG_HTTP_REQUEST_BODY")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Logs each incoming request with the caller's user ID when available.
/// Layer this inside `require_auth` so the RequestContext is populated.
pub async fn log_access(request: Request, next: Next) -> Response {
    if log_request_body() {
        let (parts, body) = request.into_parts();

        let bytes = body
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();

        let body_str = String::from_utf8_lossy(&bytes);

        match RequestContext::user_id() {
            Some(id) => tracing::info!(user_id = id, body = %body_str, "API Request"),
            None => tracing::info!(body = %body_str, "API Request (unauthenticated)"),
        }

        let request = Request::from_parts(parts, Body::from(bytes));
        next.run(request).await
    } else {
        match RequestContext::user_id() {
            Some(id) => tracing::info!(user_id = id, "API Request"),
            None => tracing::info!("API Request (unauthenticated)"),
        }
        next.run(request).await
    }
}
