pub mod authz;
pub mod authz_admin;
pub mod handler;
pub mod role_assignments;
pub mod saml_admin;
pub mod saml_attr_admin;
pub mod user_admin;
#[cfg(feature = "saml")]
pub mod saml;
pub mod user;

use odo_client::auth::TokenManager;
use odo_service::health::HasDatabase;
use odo_service::middleware::HasTokenManager;
use sea_orm::DatabaseConnection;

pub struct CookieConfig {
    pub name: String,
    pub path: String,
    pub secure: bool,
}

pub struct AppState {
    pub db: DatabaseConnection,
    pub tokens: TokenManager,
    pub cookie: CookieConfig,
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
