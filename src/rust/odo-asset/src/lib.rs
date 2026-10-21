pub mod admin;
pub mod handler;
pub mod signing;

use odo_client::auth::TokenManager;
use odo_client::client::AuthServiceClient;
use odo_service::health::HasDatabase;
use odo_service::middleware::HasTokenManager;
use sea_orm::DatabaseConnection;
use signing::UrlSigner;

pub struct AppState {
    pub db: DatabaseConnection,
    pub tokens: TokenManager,
    pub storage_base_path: String,
    pub auth_client: AuthServiceClient,
    /// None when ASSET_URL_SIGNING_KEY is unset: signing is refused and
    /// retrieval accepts only JWTs.
    pub signer: Option<UrlSigner>,
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
