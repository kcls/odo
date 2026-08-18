use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use odo_client::auth::TokenManager;
use odo_service::health;
use odo_service::middleware::{log_access, request_tracing, require_auth};
use odo_org::AppState;
use odo_org::{admin, handler, org_children};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "odo-org", version = "0.1.0", description = "Organization unit service"),
    paths(
        handler::org_unit_tree,
        handler::org_unit_root,
        handler::org_unit_detail,
        handler::org_unit_detail_by_uuid,
        handler::org_unit_ancestors_by_uuid,
        handler::org_unit_descendants_by_uuid,
        handler::org_unit_ancestors,
        handler::org_unit_descendants,
        handler::org_unit_label_batch,
        admin::list_unit_types,
        admin::create_unit_type,
        admin::update_unit_type,
        admin::delete_unit_type,
        admin::create_unit,
        admin::update_unit,
        admin::delete_unit,
        org_children::list_unit_children,
        org_children::create_address,
        org_children::update_address,
        org_children::delete_address,
        org_children::create_closure,
        org_children::update_closure,
        org_children::delete_closure,
        org_children::create_operating_hours,
        org_children::update_operating_hours,
        org_children::delete_operating_hours,
    ),
    components(schemas(
        handler::OrgUnitType,
        handler::OrgUnitResponse,
        handler::OrgUnitDetailResponse,
        handler::AddressResponse,
        handler::OperatingHoursResponse,
        handler::ClosureResponse,
        handler::LabelBatchRequest,
        handler::LabelBatchResponse,
        handler::OrgUnitLabelEntry,
        admin::UnitTypeRow,
        admin::UnitRow,
        admin::UnitTypePage,
        admin::ListUnitTypesRequest,
        admin::OrgAdminSuccessResponse,
        admin::CreateUnitTypeRequest,
        admin::UpdateUnitTypeRequest,
        admin::UnitTypeIdRequest,
        admin::CreateUnitRequest,
        admin::UpdateUnitRequest,
        admin::UnitIdRequest,
        org_children::AddressRow,
        org_children::ClosureRow,
        org_children::OperatingHoursRow,
        org_children::OrgChildSuccessResponse,
        org_children::OrgUnitChildrenResponse,
        org_children::OrgUnitChildrenRequest,
        org_children::CreateAddressRequest,
        org_children::UpdateAddressRequest,
        org_children::AddressIdRequest,
        org_children::CreateClosureRequest,
        org_children::UpdateClosureRequest,
        org_children::ClosureIdRequest,
        org_children::CreateOperatingHoursRequest,
        org_children::UpdateOperatingHoursRequest,
        org_children::OperatingHoursIdRequest,
    )),
    tags(
        (name = "org", description = "Organization unit hierarchy, detail, ancestors, and descendants"),
        (name = "org-admin", description = "Org unit and unit type administration (requires Bearer token)"),
    ),
    security(("bearer" = []))
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Dump the OpenAPI spec and exit before any DB/network setup.
    if let Some(code) = odo_service::openapi::maybe_dump(ApiDoc::openapi()) {
        std::process::exit(code);
    }

    odo_service::logging::init("info,sqlx::query=info");

    let jwt_public_key = env::var("JWT_PUBLIC_KEY")
        .inspect_err(|_| error!("JWT_PUBLIC_KEY environment variable is required"))?;

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let auth_url = env::var("ODO_AUTH_URL").unwrap_or_else(|_| "http://odo-auth:8080".to_string());

    info!(
        svc = "odo-org",
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    let db = odo_service::db::connect().await?;
    info!("database connected");

    let state = Arc::new(AppState {
        db,
        tokens: TokenManager::rsa_verifier(&jwt_public_key),
        auth_client: odo_client::client::ServiceClient::new(auth_url).into(),
    });

    let app = Router::new()
        .route("/api/v1/odo/org/tree", get(handler::org_unit_tree))
        .route("/api/v1/odo/org/root", get(handler::org_unit_root))
        .route("/api/v1/odo/org/unit/{id}", get(handler::org_unit_detail))
        .route("/api/v1/odo/org/unit/uuid/{uuid}", get(handler::org_unit_detail_by_uuid))
        .route("/api/v1/odo/org/unit/uuid/{uuid}/ancestors", get(handler::org_unit_ancestors_by_uuid))
        .route("/api/v1/odo/org/unit/uuid/{uuid}/descendants", get(handler::org_unit_descendants_by_uuid))
        .route(
            "/api/v1/odo/org/unit/{id}/ancestors",
            get(handler::org_unit_ancestors),
        )
        .route(
            "/api/v1/odo/org/unit/{id}/descendants",
            get(handler::org_unit_descendants),
        )
        .route(
            "/api/v1/odo/org/unit/label-batch",
            post(handler::org_unit_label_batch),
        )
        .route(
            "/api/v1/odo/org/admin/unit-type/list",
            post(admin::list_unit_types),
        )
        .route(
            "/api/v1/odo/org/admin/unit-type/create",
            post(admin::create_unit_type),
        )
        .route(
            "/api/v1/odo/org/admin/unit-type/update",
            post(admin::update_unit_type),
        )
        .route(
            "/api/v1/odo/org/admin/unit-type/delete",
            post(admin::delete_unit_type),
        )
        .route("/api/v1/odo/org/admin/unit/create", post(admin::create_unit))
        .route("/api/v1/odo/org/admin/unit/update", post(admin::update_unit))
        .route("/api/v1/odo/org/admin/unit/delete", post(admin::delete_unit))
        .route(
            "/api/v1/odo/org/admin/unit-children",
            post(org_children::list_unit_children),
        )
        .route(
            "/api/v1/odo/org/admin/address/create",
            post(org_children::create_address),
        )
        .route(
            "/api/v1/odo/org/admin/address/update",
            post(org_children::update_address),
        )
        .route(
            "/api/v1/odo/org/admin/address/delete",
            post(org_children::delete_address),
        )
        .route(
            "/api/v1/odo/org/admin/closure/create",
            post(org_children::create_closure),
        )
        .route(
            "/api/v1/odo/org/admin/closure/update",
            post(org_children::update_closure),
        )
        .route(
            "/api/v1/odo/org/admin/closure/delete",
            post(org_children::delete_closure),
        )
        .route(
            "/api/v1/odo/org/admin/operating-hours/create",
            post(org_children::create_operating_hours),
        )
        .route(
            "/api/v1/odo/org/admin/operating-hours/update",
            post(org_children::update_operating_hours),
        )
        .route(
            "/api/v1/odo/org/admin/operating-hours/delete",
            post(org_children::delete_operating_hours),
        )
        .layer(middleware::from_fn(log_access))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(middleware::from_fn(request_tracing))
        .route(
            "/api/v1/odo/org/api-doc/openapi.json",
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
