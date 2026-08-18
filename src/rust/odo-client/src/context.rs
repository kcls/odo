//! Per-request context propagated via task-local storage.
//!
//! The request_tracing middleware populates `RequestContext` for
//! each inbound request. Any code in the call chain can read it
//! via `RequestContext::current()` without threading it through
//! function signatures.

use crate::auth::Claims;
use uuid::Uuid;

tokio::task_local! {
    static REQUEST_CTX: RequestContext;
}

#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: String,
    pub client_ip: Option<String>,
    pub claims: Option<Claims>,
    pub auth_token: Option<String>,
}

impl RequestContext {
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            client_ip: None,
            claims: None,
            auth_token: None,
        }
    }

    pub fn with_client_ip(mut self, ip: Option<String>) -> Self {
        self.client_ip = ip;
        self
    }

    pub fn with_auth(mut self, token: String, claims: Claims) -> Self {
        self.auth_token = Some(token);
        self.claims = Some(claims);
        self
    }

    pub fn generate() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            client_ip: None,
            claims: None,
            auth_token: None,
        }
    }

    /// Returns the current request context if one is set.
    pub fn current() -> Option<Self> {
        REQUEST_CTX.try_with(|ctx| ctx.clone()).ok()
    }

    /// Returns the current request_id, or generates a new one if
    /// no context is set.
    pub fn request_id() -> String {
        REQUEST_CTX
            .try_with(|ctx| ctx.request_id.clone())
            .unwrap_or_else(|_| Uuid::new_v4().to_string())
    }

    /// Returns the client IP if present.
    pub fn client_ip() -> Option<String> {
        REQUEST_CTX
            .try_with(|ctx| ctx.client_ip.clone())
            .ok()
            .flatten()
    }

    /// Returns the authenticated user's claims if present.
    pub fn claims() -> Option<Claims> {
        REQUEST_CTX
            .try_with(|ctx| ctx.claims.clone())
            .ok()
            .flatten()
    }

    /// Returns the authenticated user's ID if present.
    pub fn user_id() -> Option<i64> {
        Self::claims().and_then(|c| c.user_id().ok())
    }

    /// Returns the raw JWT token string if present.
    pub fn auth_token() -> Option<String> {
        REQUEST_CTX
            .try_with(|ctx| ctx.auth_token.clone())
            .ok()
            .flatten()
    }

    /// Run a future within this request context.
    pub async fn scope<F: std::future::Future>(self, f: F) -> F::Output {
        REQUEST_CTX.scope(self, f).await
    }
}

impl RequestContext {
    /// Stable uuid of the authenticated user (uuid migration dual claim).
    pub fn user_uuid() -> Option<uuid::Uuid> {
        Self::claims().and_then(|c| c.sub_uuid.as_deref().and_then(|s| s.parse().ok()))
    }
}
