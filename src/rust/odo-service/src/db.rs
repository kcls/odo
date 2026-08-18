use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::env;
use std::time::Duration;

/// Connect to the database using DATABASE_URL and optional pool tuning
/// env vars (DB_MAX_CONNECTIONS, DB_MIN_CONNECTIONS, DB_CONNECT_TIMEOUT_SECS,
/// DB_IDLE_TIMEOUT_SECS).
pub async fn connect() -> Result<DatabaseConnection, sea_orm::DbErr> {
    let mut url = env::var("DATABASE_URL").expect("DATABASE_URL is required");

    if !url.contains("application_name") {
        let app_name = env::var("K8S_SERVICE").unwrap_or_else(|_| "odo".to_string());
        let sep = if url.contains('?') { "&" } else { "?" };
        url = format!("{url}{sep}application_name={app_name}");
    }

    let max_conn: u32 = env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let min_conn: u32 = env::var("DB_MIN_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    let connect_timeout: u64 = env::var("DB_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let idle_timeout: u64 = env::var("DB_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);

    let mut opts = ConnectOptions::new(&url);

    opts.max_connections(max_conn)
        .min_connections(min_conn)
        .connect_timeout(Duration::from_secs(connect_timeout))
        .idle_timeout(Duration::from_secs(idle_timeout))
        .sqlx_logging_level(log::LevelFilter::Info)
        // Logs SQL with placeholders which is only slightly useful.
        // Leaving disabled for now.
        .sqlx_logging(false);

    Database::connect(opts).await
}
