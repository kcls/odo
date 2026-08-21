use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use odo_client::auth::TokenManager;
use odo_notify::AppState;
use odo_notify::{email_group, enqueue, inbox, template_admin};
use odo_service::health;
use odo_service::middleware::{log_access, request_tracing, require_auth};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "odo-notify", version = "0.1.0", description = "Notification service"),
    paths(
        inbox::list,
        inbox::mark_read,
        inbox::mark_all_read,
        inbox::dismiss,
        inbox::dismiss_all,
        enqueue::enqueue,
        email_group::list_email_groups,
        email_group::get_email_group,
        email_group::create_email_group,
        email_group::update_email_group,
        email_group::create_email_group_member,
        email_group::update_email_group_member,
        email_group::delete_email_group_member,
        template_admin::list_templates,
        template_admin::create_template,
        template_admin::update_template,
        template_admin::delete_template,
        template_admin::preview_template,
    ),
    components(schemas(
        inbox::ListRequest,
        inbox::DeliveryIdRequest,
        inbox::NotificationItem,
        inbox::ListResponse,
        inbox::SuccessResponse,
        enqueue::EnqueueRequest,
        enqueue::Recipient,
        enqueue::DeliveryInfo,
        enqueue::EnqueueResponse,
        email_group::ListEmailGroupsRequest,
        email_group::EmailGroupIdRequest,
        email_group::CreateEmailGroupRequest,
        email_group::UpdateEmailGroupRequest,
        email_group::CreateEmailGroupMemberRequest,
        email_group::UpdateEmailGroupMemberRequest,
        email_group::EmailGroupMemberIdRequest,
        email_group::EmailGroupRow,
        email_group::EmailGroupMemberRow,
        email_group::EmailGroupPage,
        email_group::EmailGroupDetailResponse,
        template_admin::TemplateRow,
        template_admin::TemplatePage,
        template_admin::PreviewResponse,
        template_admin::ListTemplatesRequest,
        template_admin::CreateTemplateRequest,
        template_admin::UpdateTemplateRequest,
        template_admin::TemplateIdRequest,
        template_admin::PreviewRequest,
    )),
    tags(
        (name = "inbox", description = "Notification inbox (list, read, dismiss)"),
        (name = "enqueue", description = "Send notifications"),
        (name = "email-group", description = "Email group administration"),
        (name = "template-admin", description = "Notification template administration"),
    ),
    security(("bearer" = []))
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(code) = odo_service::openapi::maybe_dump(ApiDoc::openapi()) {
        std::process::exit(code);
    }

    odo_service::logging::init("info,sqlx::query=info");

    let jwt_public_key = env::var("JWT_PUBLIC_KEY")
        .inspect_err(|_| error!("JWT_PUBLIC_KEY environment variable is required"))?;

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let auth_url = env::var("ODO_AUTH_URL").unwrap_or_else(|_| "http://odo-auth:8080".to_string());

    info!(
        svc = "odo-notify",
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    let poll_interval_secs: u64 = env::var("QUEUE_POLL_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let batch_size: u64 = env::var("QUEUE_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let lease_duration_secs: i64 = env::var("PROCESSOR_LEASE_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);

    let db = odo_service::db::connect().await?;
    info!("database connected");

    // Start background email processor
    let smtp_config = Arc::new(odo_notify::SmtpConfig::from_env());
    let processor_db = db.clone();
    let processor_smtp = smtp_config.clone();
    let worker_id =
        env::var("K8S_POD_NAME").unwrap_or_else(|_| format!("worker-{}", std::process::id()));

    tokio::spawn(async move {
        odo_notify::processor::run(
            processor_db,
            processor_smtp,
            worker_id,
            std::time::Duration::from_secs(poll_interval_secs),
            batch_size,
            lease_duration_secs,
        )
        .await;
    });

    let state = Arc::new(AppState {
        db,
        tokens: TokenManager::rsa_verifier(&jwt_public_key),
        auth_client: odo_client::client::ServiceClient::new(auth_url).into(),
    });

    let app = Router::new()
        .route("/api/v1/odo/notify/inbox/list", post(inbox::list))
        .route("/api/v1/odo/notify/inbox/mark-read", post(inbox::mark_read))
        .route(
            "/api/v1/odo/notify/inbox/mark-all-read",
            post(inbox::mark_all_read),
        )
        .route("/api/v1/odo/notify/inbox/dismiss", post(inbox::dismiss))
        .route(
            "/api/v1/odo/notify/inbox/dismiss-all",
            post(inbox::dismiss_all),
        )
        .route("/api/v1/odo/notify/enqueue", post(enqueue::enqueue))
        .route(
            "/api/v1/odo/notify/email-group/list",
            post(email_group::list_email_groups),
        )
        .route(
            "/api/v1/odo/notify/email-group/get",
            post(email_group::get_email_group),
        )
        .route(
            "/api/v1/odo/notify/email-group/create",
            post(email_group::create_email_group),
        )
        .route(
            "/api/v1/odo/notify/email-group/update",
            post(email_group::update_email_group),
        )
        .route(
            "/api/v1/odo/notify/email-group/member/create",
            post(email_group::create_email_group_member),
        )
        .route(
            "/api/v1/odo/notify/email-group/member/update",
            post(email_group::update_email_group_member),
        )
        .route(
            "/api/v1/odo/notify/email-group/member/delete",
            post(email_group::delete_email_group_member),
        )
        .route(
            "/api/v1/odo/notify/template/list",
            post(template_admin::list_templates),
        )
        .route(
            "/api/v1/odo/notify/template/create",
            post(template_admin::create_template),
        )
        .route(
            "/api/v1/odo/notify/template/update",
            post(template_admin::update_template),
        )
        .route(
            "/api/v1/odo/notify/template/delete",
            post(template_admin::delete_template),
        )
        .route(
            "/api/v1/odo/notify/template/preview",
            post(template_admin::preview_template),
        )
        .layer(middleware::from_fn(log_access))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(middleware::from_fn(request_tracing))
        .route(
            "/api/v1/odo/notify/api-doc/openapi.json",
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
