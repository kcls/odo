pub mod admin;
pub mod handler;
pub mod org_children;

use odo_client::auth::TokenManager;
use odo_client::client::AuthServiceClient;
use odo_service::health::HasDatabase;
use odo_service::middleware::HasTokenManager;
use sea_orm::DatabaseConnection;

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
