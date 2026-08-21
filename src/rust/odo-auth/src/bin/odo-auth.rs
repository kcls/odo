use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use odo_auth::handler;
#[cfg(feature = "saml")]
use odo_auth::saml;
use odo_auth::user;
use odo_auth::{
    AppState, CookieConfig, authz_admin, role_assignments, saml_admin, saml_attr_admin, user_admin,
};
use odo_client::auth::TokenManager;
use odo_service::health;
use odo_service::middleware::{log_access, request_tracing, require_auth};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "odo-auth", version = "0.1.0", description = "Authentication and authorization service"),
    paths(
        handler::login,
        handler::logout,
        handler::validate_token,
        handler::refresh_token,
        handler::revoke_token,
        handler::user_has_perm,
        handler::user_has_role,
        handler::user_roles,
        handler::users_with_role,
        user::get_user,
        user::user_search,
        authz_admin::list_permissions,
        authz_admin::create_permission,
        authz_admin::update_permission,
        authz_admin::delete_permission,
        authz_admin::list_roles,
        authz_admin::get_role,
        authz_admin::create_role,
        authz_admin::update_role,
        authz_admin::delete_role,
        authz_admin::create_role_permission,
        authz_admin::update_role_permission,
        authz_admin::delete_role_permission,
        role_assignments::list_assignments,
        role_assignments::user_perm_scopes,
        role_assignments::create_assignment,
        role_assignments::delete_assignment,
        saml_admin::list_idps,
        saml_admin::create_idp,
        saml_admin::update_idp,
        saml_admin::delete_idp,
        saml_admin::list_sps,
        saml_admin::create_sp,
        saml_admin::update_sp,
        saml_admin::delete_sp,
        saml_attr_admin::list_attributes,
        saml_attr_admin::create_attribute,
        saml_attr_admin::update_attribute,
        saml_attr_admin::delete_attribute,
        saml_attr_admin::list_attr_role_maps,
        saml_attr_admin::create_attr_role_map,
        saml_attr_admin::update_attr_role_map,
        saml_attr_admin::delete_attr_role_map,
        user_admin::user_detail,
        user_admin::update_user,
        user_admin::create_user,
    ),
    components(schemas(
        handler::LoginRequest,
        handler::LoginResponse,
        handler::UserInfo,
        handler::LogoutRequest,
        handler::LogoutResponse,
        handler::ValidateRequest,
        handler::TokenClaims,
        handler::ValidateTokenResponse,
        handler::RefreshRequest,
        handler::RefreshResponse,
        handler::RevokeRequest,
        handler::RevokeResponse,
        handler::UserHasPermRequest,
        handler::AuthzPermResponse,
        handler::UserHasRoleRequest,
        handler::AuthzRoleResponse,
        handler::RoleAssignment,
        handler::UserRolesResponse,
        handler::UsersWithRoleRequest,
        handler::UsersWithRoleResponse,
        user::GetUserRequest,
        user::UserSearchRequest,
        user::UserResponse,
        authz_admin::PermissionRow,
        authz_admin::RoleRow,
        authz_admin::GrantRow,
        authz_admin::RolePermissionRow,
        authz_admin::PermissionPage,
        authz_admin::ListPermissionsRequest,
        authz_admin::RolePage,
        authz_admin::ListRolesRequest,
        authz_admin::RoleDetailResponse,
        authz_admin::AuthzAdminSuccessResponse,
        authz_admin::CreatePermissionRequest,
        authz_admin::UpdatePermissionRequest,
        authz_admin::PermissionCodeRequest,
        authz_admin::CreateRoleRequest,
        authz_admin::UpdateRoleRequest,
        authz_admin::RoleCodeRequest,
        authz_admin::CreateRolePermissionRequest,
        authz_admin::UpdateRolePermissionRequest,
        authz_admin::RolePermissionIdRequest,
        role_assignments::AssignmentRow,
        role_assignments::ListAssignmentsResponse,
        role_assignments::AssignmentSuccessResponse,
        role_assignments::UserPermScopesResponse,
        role_assignments::PermScopeRow,
        role_assignments::ScopeUnit,
        role_assignments::ListAssignmentsRequest,
        role_assignments::CreateAssignmentRequest,
        role_assignments::AssignmentIdRequest,
        saml_admin::IdpRow,
        saml_admin::SpRow,
        saml_admin::ListIdpsResponse,
        saml_admin::ListSpsResponse,
        saml_admin::SamlAdminSuccessResponse,
        saml_admin::CreateIdpRequest,
        saml_admin::UpdateIdpRequest,
        saml_admin::IdpIdRequest,
        saml_admin::CreateSpRequest,
        saml_admin::UpdateSpRequest,
        saml_admin::SpIdRequest,
        saml_attr_admin::AttributeRow,
        saml_attr_admin::AttrRoleMapRow,
        saml_attr_admin::ListAttributesResponse,
        saml_attr_admin::AttrRoleMapPage,
        saml_attr_admin::SamlAttrSuccessResponse,
        saml_attr_admin::CreateAttributeRequest,
        saml_attr_admin::UpdateAttributeRequest,
        saml_attr_admin::AttributeIdRequest,
        saml_attr_admin::CreateAttrRoleMapRequest,
        saml_attr_admin::UpdateAttrRoleMapRequest,
        saml_attr_admin::AttrRoleMapIdRequest,
        saml_attr_admin::ListAttrRoleMapsRequest,
        user_admin::UserDetailRequest,
        user_admin::UserDetailResponse,
        user_admin::UserAccountRow,
        user_admin::LocalAccountRow,
        user_admin::SamlIdentityRow,
        user_admin::SamlUserAttrRow,
        user_admin::SessionRow,
        user_admin::UpdateUserRequest,
        user_admin::CreateUserRequest,
    )),
    tags(
        (name = "auth", description = "Login, logout, and session management"),
        (name = "token", description = "Token validation, refresh, and revocation"),
        (name = "authz", description = "Authorization checks (requires Bearer token)"),
        (name = "authz-admin", description = "Role and permission administration (requires Bearer token)"),
        (name = "saml-admin", description = "SAML IdP/SP configuration administration (requires Bearer token)"),
        (name = "user", description = "User profile and search (requires Bearer token)"),
    ),
    security(("bearer" = []))
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spec dump: write the OpenAPI JSON and exit before any startup work.
    if let Some(code) = odo_service::openapi::maybe_dump(build_doc()) {
        std::process::exit(code);
    }

    odo_service::logging::init("info,sqlx::query=info");

    let jwt_private_key = env::var("JWT_PRIVATE_KEY")
        .inspect_err(|_| error!("JWT_PRIVATE_KEY environment variable is required"))?;

    let jwt_public_key = env::var("JWT_PUBLIC_KEY")
        .inspect_err(|_| error!("JWT_PUBLIC_KEY environment variable is required"))?;

    let jwt_issuer = env::var("JWT_ISSUER").unwrap_or_else(|_| "odo-auth".to_string());

    let access_expire_minutes: i64 = env::var("ACCESS_TOKEN_EXPIRE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let refresh_expire_days: i64 = env::var("REFRESH_TOKEN_EXPIRE_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7);

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());

    info!(
        svc = "odo-auth",
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    let db = odo_service::db::connect().await?;
    info!("database connected");

    let cookie_name =
        env::var("REFRESH_COOKIE_NAME").unwrap_or_else(|_| "refresh_token".to_string());
    let cookie_path =
        env::var("REFRESH_COOKIE_PATH").unwrap_or_else(|_| "/api/v1/odo/auth".to_string());
    let cookie_secure = env::var("REFRESH_COOKIE_SECURE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let state = Arc::new(AppState {
        db,
        tokens: TokenManager::new_rsa(
            &jwt_private_key,
            &jwt_public_key,
            &jwt_issuer,
            access_expire_minutes,
            refresh_expire_days,
        ),
        cookie: CookieConfig {
            name: cookie_name,
            path: cookie_path,
            secure: cookie_secure,
        },
    });

    let authz_routes = Router::new()
        .route(
            "/api/v1/odo/auth/authz/user-has-perm",
            post(handler::user_has_perm),
        )
        .route(
            "/api/v1/odo/auth/authz/user-has-role",
            post(handler::user_has_role),
        )
        .route(
            "/api/v1/odo/auth/authz/user-roles",
            post(handler::user_roles),
        )
        .route(
            "/api/v1/odo/auth/authz/users-with-role",
            post(handler::users_with_role),
        )
        .route(
            "/api/v1/odo/auth/authz/permission/list",
            post(authz_admin::list_permissions),
        )
        .route(
            "/api/v1/odo/auth/authz/permission/create",
            post(authz_admin::create_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/permission/update",
            post(authz_admin::update_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/permission/delete",
            post(authz_admin::delete_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/role/list",
            post(authz_admin::list_roles),
        )
        .route(
            "/api/v1/odo/auth/authz/role/get",
            post(authz_admin::get_role),
        )
        .route(
            "/api/v1/odo/auth/authz/role/create",
            post(authz_admin::create_role),
        )
        .route(
            "/api/v1/odo/auth/authz/role/update",
            post(authz_admin::update_role),
        )
        .route(
            "/api/v1/odo/auth/authz/role/delete",
            post(authz_admin::delete_role),
        )
        .route(
            "/api/v1/odo/auth/authz/role-permission/create",
            post(authz_admin::create_role_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/role-permission/update",
            post(authz_admin::update_role_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/role-permission/delete",
            post(authz_admin::delete_role_permission),
        )
        .route(
            "/api/v1/odo/auth/authz/user-role/list",
            post(role_assignments::list_assignments),
        )
        .route(
            "/api/v1/odo/auth/authz/user-perm-scopes",
            post(role_assignments::user_perm_scopes),
        )
        .route(
            "/api/v1/odo/auth/authz/user-role/create",
            post(role_assignments::create_assignment),
        )
        .route(
            "/api/v1/odo/auth/authz/user-role/delete",
            post(role_assignments::delete_assignment),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/idp/list",
            post(saml_admin::list_idps),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/idp/create",
            post(saml_admin::create_idp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/idp/update",
            post(saml_admin::update_idp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/idp/delete",
            post(saml_admin::delete_idp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/sp/list",
            post(saml_admin::list_sps),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/sp/create",
            post(saml_admin::create_sp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/sp/update",
            post(saml_admin::update_sp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/sp/delete",
            post(saml_admin::delete_sp),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attribute/list",
            post(saml_attr_admin::list_attributes),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attribute/create",
            post(saml_attr_admin::create_attribute),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attribute/update",
            post(saml_attr_admin::update_attribute),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attribute/delete",
            post(saml_attr_admin::delete_attribute),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attr-role-map/list",
            post(saml_attr_admin::list_attr_role_maps),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attr-role-map/create",
            post(saml_attr_admin::create_attr_role_map),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attr-role-map/update",
            post(saml_attr_admin::update_attr_role_map),
        )
        .route(
            "/api/v1/odo/auth/saml/admin/attr-role-map/delete",
            post(saml_attr_admin::delete_attr_role_map),
        )
        .layer(middleware::from_fn(log_access))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let user_routes = Router::new()
        .route("/api/v1/odo/auth/user/get", post(user::get_user))
        .route("/api/v1/odo/auth/user/search", post(user::user_search))
        .route(
            "/api/v1/odo/auth/user/detail",
            post(user_admin::user_detail),
        )
        .route(
            "/api/v1/odo/auth/user/update",
            post(user_admin::update_user),
        )
        .route(
            "/api/v1/odo/auth/user/create",
            post(user_admin::create_user),
        )
        .layer(middleware::from_fn(log_access))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let auth_routes = Router::new()
        .route("/api/v1/odo/auth/login", post(handler::login))
        .route("/api/v1/odo/auth/logout", post(handler::logout))
        .route(
            "/api/v1/odo/auth/token/validate",
            post(handler::validate_token),
        )
        .route(
            "/api/v1/odo/auth/token/refresh",
            post(handler::refresh_token),
        )
        .route("/api/v1/odo/auth/token/revoke", post(handler::revoke_token))
        .layer(middleware::from_fn(log_access));

    #[allow(unused_mut)]
    let mut app = Router::new()
        .merge(auth_routes)
        .merge(authz_routes)
        .merge(user_routes);

    #[cfg(feature = "saml")]
    {
        let saml_authed_routes = Router::new()
            .route("/api/v1/odo/auth/saml/idps", post(saml::list_idps))
            .route("/api/v1/odo/auth/saml/idp/get", post(saml::get_idp))
            .layer(middleware::from_fn(log_access))
            .layer(middleware::from_fn_with_state(state.clone(), require_auth));

        let saml_public_routes = Router::new()
            .route("/api/v1/odo/auth/saml/metadata", get(saml::get_metadata))
            .route(
                "/api/v1/odo/auth/saml/sso/initiate",
                get(saml::initiate_sso),
            )
            .route(
                "/api/v1/odo/auth/saml/acs",
                post(saml::assertion_consumer_service_form),
            )
            .route(
                "/api/v1/odo/auth/saml/sls",
                post(saml::single_logout_service),
            )
            .route("/api/v1/odo/auth/saml/logout", post(saml::initiate_logout))
            .route(
                "/api/v1/odo/auth/saml/sso-configs",
                post(saml::list_sso_configs),
            )
            .layer(middleware::from_fn(log_access));

        app = app.merge(saml_authed_routes).merge(saml_public_routes);
    }

    let app = app
        .route("/api/v1/odo/auth/api-doc/openapi.json", get(openapi_doc))
        .layer(middleware::from_fn(request_tracing))
        .route("/health", get(health::check::<AppState>))
        .route("/.well-known/jwks.json", get(jwks))
        .with_state(state);

    let addr = format!("[::]:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, "listening");

    odo_service::server::serve(listener, app).await?;

    info!("shutdown complete");
    Ok(())
}

/// The full OpenAPI spec this service serves, including the merged SAML
/// paths. Shared by the runtime endpoint and the `--dump-openapi` path so
/// the committed spec matches what the service actually exposes.
fn build_doc() -> utoipa::openapi::OpenApi {
    #[allow(unused_mut)]
    let mut doc = ApiDoc::openapi();
    #[cfg(feature = "saml")]
    doc.merge(saml::SamlApiDoc::openapi());
    doc
}

async fn openapi_doc() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(build_doc())
}

async fn jwks(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    use axum::http::header;
    use axum::response::IntoResponse;

    match state.tokens.jwks() {
        Some(json) => (
            [(header::CONTENT_TYPE, "application/json")],
            json.to_string(),
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "JWKS not available").into_response(),
    }
}
