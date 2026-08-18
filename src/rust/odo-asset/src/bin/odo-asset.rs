use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};
use odo_client::auth::TokenManager;
use odo_service::health;
use odo_service::middleware::{log_access, request_tracing, require_auth};
use odo_asset::AppState;
use odo_asset::admin;
use odo_asset::handler;
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "odo-asset", version = "0.1.0", description = "File upload and retrieval service"),
    paths(
        handler::upload,
        handler::retrieve,
        handler::get_files,
        handler::delete_file,
        admin::list_directories,
        admin::create_directory,
        admin::delete_directory,
    ),
    components(schemas(
        handler::UploadResponse,
        handler::GetFilesRequest,
        handler::GetFilesResponse,
        handler::FileMetadata,
        handler::DeleteFileRequest,
        handler::DeleteFileResponse,
        admin::DirectoryRow,
        admin::ListDirectoriesRequest,
        admin::CreateDirectoryRequest,
        admin::DeleteDirectoryRequest,
        admin::DeleteDirectoryResponse,
    )),
    tags(
        (name = "asset", description = "File upload and retrieval"),
    ),
    security(("bearer" = []))
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spec dump: write the OpenAPI JSON and exit before any startup work.
    if let Some(code) = odo_service::openapi::maybe_dump(ApiDoc::openapi()) {
        std::process::exit(code);
    }

    odo_service::logging::init("info,sqlx::query=info");

    let jwt_public_key = env::var("JWT_PUBLIC_KEY")
        .inspect_err(|_| error!("JWT_PUBLIC_KEY environment variable is required"))?;

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let storage_base_path =
        env::var("STORAGE_BASE_PATH").unwrap_or_else(|_| "/app/nfs".to_string());
    let auth_url = env::var("ODO_AUTH_URL").unwrap_or_else(|_| "http://odo-auth:8080".to_string());

    let max_upload_mb: usize = env::var("MAX_UPLOAD_SIZE_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    info!(
        svc = "odo-asset",
        version = env!("CARGO_PKG_VERSION"),
        storage_base_path = storage_base_path,
        max_upload_mb = max_upload_mb,
        "starting"
    );

    let db = odo_service::db::connect().await?;
    info!("database connected");

    let state = Arc::new(AppState {
        db,
        tokens: TokenManager::rsa_verifier(&jwt_public_key),
        storage_base_path,
        auth_client: odo_client::client::ServiceClient::new(auth_url).into(),
    });

    let app = Router::new()
        .route("/api/v1/odo/asset/upload", post(handler::upload))
        .layer(DefaultBodyLimit::max(max_upload_mb * 1024 * 1024))
        // Metadata + delete are JSON endpoints — register them inside
        // the auth-required layer block (above `.layer(require_auth)`).
        // Routes ordering matters: axum applies layers to routes added
        // *before* the layer call.
        .route("/api/v1/odo/asset/files/get", post(handler::get_files))
        .route("/api/v1/odo/asset/files/delete", post(handler::delete_file))
        .route("/api/v1/odo/asset/directory/list", post(admin::list_directories))
        .route("/api/v1/odo/asset/directory/create", post(admin::create_directory))
        .route("/api/v1/odo/asset/directory/delete", post(admin::delete_directory))
        .layer(middleware::from_fn(log_access))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(middleware::from_fn(request_tracing))
        // File retrieval handles its own auth (supports ?token= query param)
        .route("/api/v1/odo/asset/files/{*path}", get(handler::retrieve))
        .route(
            "/api/v1/odo/asset/api-doc/openapi.json",
            get(|| async { axum::Json(ApiDoc::openapi()) }),
        )
        .route("/health", get(health::check::<AppState>))
        .with_state(state);

    let addr = format!("[::]:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, "listening");

    odo_service::server::serve(listener, app).await?;

    info!("shutdown complete");
    Ok(())
}
