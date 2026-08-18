pub mod email_group;
pub mod enqueue;
pub mod inbox;
pub mod processor;
pub mod template_admin;

use odo_client::auth::TokenManager;
use odo_client::client::AuthServiceClient;
use odo_service::health::HasDatabase;
use odo_service::middleware::HasTokenManager;
use sea_orm::DatabaseConnection;

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_starttls: bool,
    /// Bypass TLS certificate/hostname validation. For servers with
    /// self-signed or otherwise invalid certs. Dangerous — off by default.
    pub dangerous_accept_invalid_certs: bool,
}

impl SmtpConfig {
    pub fn from_env() -> Self {
        use std::env;
        Self {
            host: env::var("SMTP_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(587),
            username: env::var("SMTP_USERNAME").unwrap_or_default(),
            password: env::var("SMTP_PASSWORD").unwrap_or_default(),
            from_email: env::var("SMTP_FROM_EMAIL")
                .unwrap_or_else(|_| "noreply@localhost".to_string()),
            from_name: env::var("SMTP_FROM_NAME")
                .unwrap_or_else(|_| "Odo Notification Service".to_string()),
            use_tls: env::var("SMTP_USE_TLS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            use_starttls: env::var("SMTP_USE_STARTTLS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            dangerous_accept_invalid_certs: env::var("SMTP_DANGEROUS_ACCEPT_INVALID_CERTS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}

pub struct AppState {
    pub db: DatabaseConnection,
    pub tokens: TokenManager,
    pub auth_client: AuthServiceClient,
}

impl HasTokenManager for AppState {
    fn token_manager(&self) -> &TokenManager {
        &self.tokens
    }
}

impl HasDatabase for AppState {
    fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}
