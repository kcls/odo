use axum::Json;
use axum::extract::State;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use odo_client::auth::generate_session_id;
use odo_entity::auth::{local_account, session, usr};
use odo_entity::org::unit as org_unit_entity;
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::{DbBackend, FromQueryResult, QuerySelect, Set, Statement};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::AppState;
use crate::authz;

fn build_refresh_cookie(state: &AppState, token: &str, expires_at_ms: i64) -> Cookie<'static> {
    let now_ms = Utc::now().timestamp_millis();
    let max_age_secs = ((expires_at_ms - now_ms) / 1000).max(0);

    Cookie::build((state.cookie.name.clone(), token.to_string()))
        .http_only(true)
        .secure(state.cookie.secure)
        .same_site(SameSite::Lax)
        .path(state.cookie.path.clone())
        .max_age(time::Duration::seconds(max_age_secs))
        .build()
}

fn build_clear_cookie(state: &AppState) -> Cookie<'static> {
    Cookie::build((state.cookie.name.clone(), String::new()))
        .http_only(true)
        .secure(state.cookie.secure)
        .same_site(SameSite::Lax)
        .path(state.cookie.path.clone())
        .max_age(time::Duration::ZERO)
        .build()
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---- Login ----

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    username: String,
    password: String,
    /// Working org unit by stable uuid (uuid migration 1d.3: the
    /// login/session surface no longer accepts integer org ids; the
    /// `org_unit_uuid` name is kept as an alias for older clients).
    #[serde(default, alias = "org_unit_uuid")]
    org_unit: Option<String>,
    #[serde(default)]
    ip_address: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
}


/// Resolve an org unit's stable uuid for the JWT dual claim (uuid
/// migration phase 1: the token carries both the integer id and the
/// uuid until all readers move to the uuid). None when no org unit is
/// selected or the unit is unknown.
pub(crate) async fn org_unit_uuid_claim(
    db: &DatabaseConnection,
    org_unit: Option<i64>,
) -> Result<Option<String>, sea_orm::DbErr> {
    let Some(id) = org_unit else {
        return Ok(None);
    };
    Ok(org_unit_entity::Entity::find_by_id(id as i32)
        .one(db)
        .await?
        .map(|u| u.uuid.to_string()))
}

/// Resolve an org-unit reference given as an id and/or a uuid (uuid
/// migration: endpoints accept both). Explicit id wins; a uuid that
/// matches no unit is a 404.
pub(crate) async fn resolve_org_ref(
    db: &DatabaseConnection,
    org_unit: Option<i32>,
    org_unit_uuid: Option<&str>,
) -> Result<Option<i32>, odo_client::error::ApiError> {
    if org_unit.is_some() {
        return Ok(org_unit);
    }
    let Some(raw) = org_unit_uuid else {
        return Ok(None);
    };
    let uuid: sea_orm::prelude::Uuid = raw
        .parse()
        .map_err(|_| LocalError::invalid_input("invalid org_unit_uuid"))?;
    let row = org_unit_entity::Entity::find()
        .filter(org_unit_entity::Column::Uuid.eq(uuid))
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("org unit {raw}")))?;
    Ok(Some(row.id))
}

/// Resolve a user reference given as an id and/or a uuid. Explicit id
/// wins; a uuid that matches no user is a 404.
pub(crate) async fn resolve_usr_ref(
    db: &DatabaseConnection,
    usr: Option<i32>,
    usr_uuid: Option<&str>,
) -> Result<i32, odo_client::error::ApiError> {
    if let Some(id) = usr {
        return Ok(id);
    }
    let Some(raw) = usr_uuid else {
        return Err(LocalError::invalid_input("usr or usr_uuid required").into());
    };
    let uuid: sea_orm::prelude::Uuid = raw
        .parse()
        .map_err(|_| LocalError::invalid_input("invalid usr_uuid"))?;
    let row = usr::Entity::find()
        .filter(usr::Column::Uuid.eq(uuid))
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("user {raw}")))?;
    Ok(row.id)
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    access_token: String,
    token_type: String,
    expires_in: i64,
    refresh_expires_at: i64,
    user: UserInfo,
}

#[derive(Debug, FromQueryResult)]
struct LoginLookup {
    id: i32,
    uuid: Uuid,
    email: String,
    username: Option<String>,
    display_name: String,
    verified: bool,
}

#[derive(Serialize, ToSchema)]
pub struct UserInfo {
    id: i32,
    email: String,
    username: String,
    display_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct LogoutResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct TokenClaims {
    pub user_id: i64,
    pub email: String,
    pub auth_method: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub session_id: String,
    pub display_name: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ValidateTokenResponse {
    pub valid: bool,
    pub error: String,
    pub claims: Option<TokenClaims>,
}

#[derive(Serialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_expires_at: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AuthzPermResponse {
    pub has_perm: bool,
    pub user_id: i32,
    pub perm: String,
    pub org_unit: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct AuthzRoleResponse {
    pub has_role: bool,
    pub user_id: i32,
    pub role: String,
    pub org_unit: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct RoleAssignment {
    pub role: String,
    pub org_unit: i32,
}

#[derive(Serialize, ToSchema)]
pub struct UserRolesResponse {
    pub user_id: i32,
    pub roles: Vec<RoleAssignment>,
}

#[derive(Deserialize, ToSchema)]
pub struct UsersWithRoleRequest {
    pub role: String,
    #[serde(default)]
    pub user_ids: Vec<i32>,
    /// Users by stable uuid (accepted alongside `user_ids`).
    #[serde(default)]
    pub user_uuids: Vec<String>,
    #[serde(default)]
    pub org_unit: Option<i32>,
    /// Org unit by stable uuid (accepted alongside `org_unit`; id wins).
    #[serde(default)]
    pub org_unit_uuid: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct UsersWithRoleResponse {
    pub role: String,
    pub org_unit: Option<i32>,
    /// Subset of the requested user_ids who hold the role.
    pub user_ids: Vec<i32>,
    /// Uuids of the holders (covers users requested by uuid too).
    pub user_uuids: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct RevokeResponse {
    pub success: bool,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
    ),
    tag = "auth"
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(params): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<LoginResponse>)> {
    info!(username = params.username, "Processing login");

    // `verified` is computed in-DB via auth.verify_password() so the
    // plaintext password is never compared in application code.
    let row: Option<LoginLookup> = usr::Entity::find()
        .select_only()
        .column(usr::Column::Id)
        .column(usr::Column::Uuid)
        .column(usr::Column::Email)
        .column(usr::Column::Username)
        .column(usr::Column::DisplayName)
        .column_as(
            Expr::cust_with_values(
                "auth.verify_password($1, \"local_account\".\"password_hash\")",
                [params.password.clone()],
            ),
            "verified",
        )
        .inner_join(local_account::Entity)
        .filter(usr::Column::Username.eq(params.username.clone()))
        .filter(usr::Column::DeletedAt.is_null())
        .limit(1)
        .into_model::<LoginLookup>()
        .one(&state.db)
        .await?;

    let Some(row) = row.filter(|r| r.verified) else {
        warn!(username = params.username, "Login failed");
        return Err(LocalError::unauthenticated().into());
    };

    let user_id = row.id;
    let user_uuid = row.uuid.to_string();
    let email = row.email;
    let username = row.username.unwrap_or_else(|| params.username.clone());
    let display_name = row.display_name;

    let org_unit_i32 = resolve_org_ref(&state.db, None, params.org_unit.as_deref()).await?;
    if !authz::user_has_perm(&state.db, user_id, "odo.auth.session", org_unit_i32).await? {
        warn!(user_id = user_id, "User lacks auth.session permission");
        return Err(LocalError::unauthenticated().into());
    }

    let session_id = generate_session_id();
    // Canonicalize the claim through the resolved id so the token always
    // carries the unit's stored uuid (and a stale uuid can't leak in).
    let org_unit_claim = org_unit_uuid_claim(&state.db, org_unit_i32.map(|n| n as i64)).await?;
    let (access_token, refresh_token, refresh_expires_at) = state.tokens.generate_token_pair(
        user_id as i64,
        Some(user_uuid),
        &email,
        "local",
        &session_id,
        org_unit_claim,
    )?;

    let expires_at = Utc::now() + chrono::Duration::seconds(state.tokens.refresh_expire_seconds());

    // Raw SQL: the `ip_address` column is INET, and sea-orm-codegen
    // marks it `#[sea_orm(ignore)]` in `session::Model` — so it is not
    // settable via `session::ActiveModel`. Until the entity is reworked
    // (or we add a custom SeaORM type binding for INET), we INSERT this
    // row by hand. The session SELECT/UPDATE paths elsewhere in this
    // crate already use SeaORM since they don't touch `ip_address`.
    state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
            INSERT INTO auth.session
                (usr, uuid, token_hash, refresh_token_hash, auth_method,
                 ip_address, user_agent, is_active, expires_at, org_unit)
            VALUES ($1, $2, $3, $4, 'local',
                    $5::inet, $6, true, $7, $8)
            "#,
            [
                (user_id as i64).into(),
                session_id.into(),
                hash_token(&access_token).into(),
                hash_token(&refresh_token).into(),
                params.ip_address.into(),
                params.user_agent.into(),
                expires_at.into(),
                org_unit_i32.into(),
            ],
        ))
        .await?;

    // Stamp last_login_at, matching the SAML login path (saml.rs). A targeted
    // ActiveModel update touches only this column.
    usr::ActiveModel {
        id: Set(user_id),
        last_login_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .update(&state.db)
    .await?;

    info!(user_id = user_id, "Login successful");

    let updated_jar = jar.add(build_refresh_cookie(
        &state,
        &refresh_token,
        refresh_expires_at,
    ));

    Ok((
        updated_jar,
        Json(LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: state.tokens.access_expire_seconds(),
            refresh_expires_at,
            user: UserInfo {
                id: user_id,
                email,
                username,
                display_name,
            },
        }),
    ))
}

// ---- Logout ----

#[derive(Deserialize, ToSchema)]
pub struct LogoutRequest {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    revoke_all_sessions: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/logout",
    request_body = LogoutRequest,
    responses(
        (status = 200, body = LogoutResponse, description = "Logout processed"),
    ),
    tag = "auth"
)]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(params): Json<LogoutRequest>,
) -> ApiResult<(CookieJar, Json<LogoutResponse>)> {
    let updated_jar = jar.add(build_clear_cookie(&state));

    let token = match params.access_token {
        Some(t) => t,
        None => {
            warn!("No token in logout request");
            return Ok((
                updated_jar,
                Json(LogoutResponse {
                    success: false,
                    message: "No token provided".to_string(),
                }),
            ));
        }
    };

    let claims = match state.tokens.validate_token(&token) {
        Ok(c) => c,
        Err(_) => {
            warn!("Invalid token in logout request");
            return Ok((
                updated_jar,
                Json(LogoutResponse {
                    success: false,
                    message: "Invalid token".to_string(),
                }),
            ));
        }
    };

    let now = Utc::now();

    let result = if params.revoke_all_sessions {
        let user_id: i64 = claims.sub.parse().unwrap_or(0);
        session::Entity::update_many()
            .col_expr(session::Column::IsActive, Expr::value(false))
            .col_expr(session::Column::ExpiresAt, Expr::value(now))
            .filter(session::Column::Usr.eq(user_id as i32))
            .filter(session::Column::IsActive.eq(true))
            .exec(&state.db)
            .await?
    } else {
        session::Entity::update_many()
            .col_expr(session::Column::IsActive, Expr::value(false))
            .col_expr(session::Column::ExpiresAt, Expr::value(now))
            .filter(session::Column::Uuid.eq(&claims.session_id))
            .filter(session::Column::IsActive.eq(true))
            .exec(&state.db)
            .await?
    };

    info!(
        user_id = claims.sub,
        sessions_revoked = result.rows_affected,
        "Logout processed"
    );

    Ok((
        updated_jar,
        Json(LogoutResponse {
            success: true,
            message: format!("Revoked {} session(s)", result.rows_affected),
        }),
    ))
}

// ---- Validate Token ----

#[derive(Deserialize, ToSchema)]
pub struct ValidateRequest {
    token: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/token/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, body = ValidateTokenResponse, description = "Validation result"),
    ),
    tag = "token"
)]
pub async fn validate_token(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ValidateRequest>,
) -> ApiResult<Json<ValidateTokenResponse>> {
    let claims = match state.tokens.validate_token(&params.token) {
        Ok(c) => c,
        Err(e) => {
            return Ok(Json(ValidateTokenResponse {
                valid: false,
                error: e.to_string(),
                claims: None,
            }));
        }
    };

    let session_active = session::Entity::find()
        .filter(session::Column::Uuid.eq(&claims.session_id))
        .filter(session::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .is_some();

    if !session_active {
        return Ok(Json(ValidateTokenResponse {
            valid: false,
            error: "Session not found or inactive".to_string(),
            claims: None,
        }));
    }

    let user_id: i64 = claims.sub.parse().unwrap_or(0);
    let display_name = usr::Entity::find_by_id(user_id as i32)
        .one(&state.db)
        .await?
        .map(|u| u.display_name);

    Ok(Json(ValidateTokenResponse {
        valid: true,
        error: String::new(),
        claims: Some(TokenClaims {
            user_id,
            email: claims.email,
            auth_method: claims.auth_method,
            issued_at: claims.iat,
            expires_at: claims.exp,
            session_id: claims.session_id,
            display_name,
        }),
    }))
}

// ---- Refresh Token ----

#[derive(Deserialize, Default, ToSchema)]
pub struct RefreshRequest {
    #[serde(default)]
    refresh_token: Option<String>,
    /// Working org unit by stable uuid (uuid migration 1d.3: the
    /// login/session surface no longer accepts integer org ids; the
    /// `org_unit_uuid` name is kept as an alias for older clients).
    #[serde(default, alias = "org_unit_uuid")]
    org_unit: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/token/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, body = RefreshResponse, description = "Token refreshed"),
        (status = 401, description = "Invalid or expired refresh token"),
    ),
    tag = "token"
)]
pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    body: Option<Json<RefreshRequest>>,
) -> ApiResult<(CookieJar, Json<RefreshResponse>)> {
    let params = body.map(|Json(r)| r).unwrap_or_default();

    let token_value = jar
        .get(&state.cookie.name)
        .map(|c| c.value().to_string())
        .or(params.refresh_token)
        .ok_or(LocalError::unauthenticated())?;

    let claims = state
        .tokens
        .validate_token(&token_value)
        .map_err(|_| LocalError::unauthenticated())?;

    if claims.token_type != "refresh" {
        return Err(LocalError::invalid_input("Not a refresh token").into());
    }

    let refresh_hash = hash_token(&token_value);
    let session = session::Entity::find()
        .filter(session::Column::RefreshTokenHash.eq(&refresh_hash))
        .filter(session::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .ok_or(LocalError::unauthenticated())?;

    // A refresh may switch the working location; otherwise the claim's
    // uuid carries forward. Resolve to the integer id and back so the new
    // token always holds the unit's stored uuid (a claim referencing a
    // since-deleted unit degrades to no working location).
    let requested = params.org_unit.as_deref().or(claims.org_unit.as_deref());
    let org_unit_i32 = resolve_org_ref(&state.db, None, requested).await?;
    let user_id: i64 = claims.sub.parse().unwrap_or(0);

    let org_unit_claim = org_unit_uuid_claim(&state.db, org_unit_i32.map(|n| n as i64)).await?;
    // Carry the user's uuid forward; older refresh tokens without it fall
    // back to a lookup so rotated tokens gain the claim.
    let sub_uuid = match claims.sub_uuid.clone() {
        Some(u) => Some(u),
        None => usr::Entity::find_by_id(user_id as i32)
            .one(&state.db)
            .await?
            .map(|u| u.uuid.to_string()),
    };
    let (new_access_token, new_refresh_token, refresh_expires_at) =
        state.tokens.generate_token_pair(
            user_id,
            sub_uuid,
            &claims.email,
            &claims.auth_method,
            &claims.session_id,
            org_unit_claim,
        )?;

    let new_expires_at =
        chrono::DateTime::from_timestamp_millis(refresh_expires_at).unwrap_or_else(Utc::now);

    let mut active: session::ActiveModel = session.into();
    active.token_hash = Set(hash_token(&new_access_token));
    active.refresh_token_hash = Set(Some(hash_token(&new_refresh_token)));
    active.last_activity_at = Set(Some(Utc::now().into()));
    active.expires_at = Set(new_expires_at.into());
    active.update(&state.db).await?;

    info!(user_id = user_id, "Token refreshed");

    let updated_jar = jar.add(build_refresh_cookie(
        &state,
        &new_refresh_token,
        refresh_expires_at,
    ));

    Ok((
        updated_jar,
        Json(RefreshResponse {
            access_token: new_access_token,
            token_type: "Bearer".to_string(),
            expires_in: state.tokens.access_expire_seconds(),
            refresh_expires_at,
        }),
    ))
}

// ---- User Has Perm ----

#[derive(Deserialize, ToSchema)]
pub struct UserHasPermRequest {
    perm: String,
    #[serde(default)]
    org_unit: Option<i32>,
    /// Org unit by stable uuid (accepted alongside `org_unit`; id wins).
    #[serde(default)]
    org_unit_uuid: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-has-perm",
    request_body = UserHasPermRequest,
    responses((status = 200, body = AuthzPermResponse, description = "Permission check result")),
    security(("bearer" = [])),
    tag = "authz"
)]
pub async fn user_has_perm(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UserHasPermRequest>,
) -> ApiResult<Json<AuthzPermResponse>> {
    let user_id =
        odo_client::context::RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let org_unit =
        resolve_org_ref(&state.db, params.org_unit, params.org_unit_uuid.as_deref()).await?;
    let has_perm = authz::user_has_perm(&state.db, user_id, &params.perm, org_unit).await?;

    Ok(Json(AuthzPermResponse {
        has_perm,
        user_id,
        perm: params.perm,
        org_unit,
    }))
}

// ---- User Has Role ----

#[derive(Deserialize, ToSchema)]
pub struct UserHasRoleRequest {
    /// Org unit by stable uuid (accepted alongside `org_unit`; id wins).
    #[serde(default)]
    org_unit_uuid: Option<String>,
    role: String,
    #[serde(default)]
    org_unit: Option<i32>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-has-role",
    request_body = UserHasRoleRequest,
    responses((status = 200, body = AuthzRoleResponse, description = "Role check result")),
    security(("bearer" = [])),
    tag = "authz"
)]
pub async fn user_has_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UserHasRoleRequest>,
) -> ApiResult<Json<AuthzRoleResponse>> {
    let user_id =
        odo_client::context::RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let org_unit =
        resolve_org_ref(&state.db, params.org_unit, params.org_unit_uuid.as_deref()).await?;
    let has_role = authz::user_has_role(&state.db, user_id, &params.role, org_unit).await?;

    Ok(Json(AuthzRoleResponse {
        has_role,
        user_id,
        role: params.role,
        org_unit,
    }))
}

// ---- User Roles ----

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-roles",
    responses((status = 200, body = UserRolesResponse, description = "User's role assignments")),
    security(("bearer" = [])),
    tag = "authz"
)]
pub async fn user_roles(State(state): State<Arc<AppState>>) -> ApiResult<Json<UserRolesResponse>> {
    let user_id =
        odo_client::context::RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let roles = authz::get_user_roles(&state.db, user_id).await?;

    Ok(Json(UserRolesResponse {
        user_id,
        roles: roles
            .into_iter()
            .map(|(role, org_unit)| RoleAssignment { role, org_unit })
            .collect(),
    }))
}

// ---- Users With Role (bulk) ----

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/users-with-role",
    request_body = UsersWithRoleRequest,
    responses((status = 200, body = UsersWithRoleResponse, description = "Subset of user_ids that hold the role")),
    security(("bearer" = [])),
    tag = "authz"
)]
pub async fn users_with_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UsersWithRoleRequest>,
) -> ApiResult<Json<UsersWithRoleResponse>> {
    let org_unit =
        resolve_org_ref(&state.db, params.org_unit, params.org_unit_uuid.as_deref()).await?;

    // Union ids with uuid-resolved users (unknown uuids are dropped, matching
    // unknown-id behavior).
    let mut user_ids = params.user_ids.clone();
    if !params.user_uuids.is_empty() {
        let uuids: Vec<sea_orm::prelude::Uuid> = params
            .user_uuids
            .iter()
            .filter_map(|u| u.parse().ok())
            .collect();
        let rows = usr::Entity::find()
            .filter(usr::Column::Uuid.is_in(uuids))
            .all(&state.db)
            .await?;
        user_ids.extend(rows.iter().map(|u| u.id));
        user_ids.sort_unstable();
        user_ids.dedup();
    }

    let matched = authz::users_with_role(&state.db, &user_ids, &params.role, org_unit).await?;

    // Echo uuids for the holders so uuid-based callers need no id mapping.
    let matched_uuids: Vec<String> = if matched.is_empty() {
        Vec::new()
    } else {
        usr::Entity::find()
            .filter(usr::Column::Id.is_in(matched.clone()))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|u| u.uuid.to_string())
            .collect()
    };

    Ok(Json(UsersWithRoleResponse {
        role: params.role,
        org_unit,
        user_ids: matched,
        user_uuids: matched_uuids,
    }))
}

// ---- Revoke Token ----

#[derive(Deserialize, ToSchema)]
pub struct RevokeRequest {
    token: String,
    #[serde(default = "default_bearer")]
    token_type: String,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/token/revoke",
    request_body = RevokeRequest,
    responses(
        (status = 200, body = RevokeResponse, description = "Revocation result"),
    ),
    tag = "token"
)]
pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    Json(params): Json<RevokeRequest>,
) -> ApiResult<Json<RevokeResponse>> {
    let token_hash = hash_token(&params.token);
    let now = Utc::now();

    let result = if params.token_type.eq_ignore_ascii_case("refresh") {
        session::Entity::update_many()
            .col_expr(session::Column::IsActive, Expr::value(false))
            .col_expr(session::Column::ExpiresAt, Expr::value(now))
            .filter(session::Column::RefreshTokenHash.eq(&token_hash))
            .filter(session::Column::IsActive.eq(true))
            .exec(&state.db)
            .await?
    } else {
        session::Entity::update_many()
            .col_expr(session::Column::IsActive, Expr::value(false))
            .col_expr(session::Column::ExpiresAt, Expr::value(now))
            .filter(session::Column::TokenHash.eq(&token_hash))
            .filter(session::Column::IsActive.eq(true))
            .exec(&state.db)
            .await?
    };

    let revoked = result.rows_affected > 0;

    if revoked {
        info!(rows = result.rows_affected, "Token revoked");
    }

    Ok(Json(RevokeResponse {
        success: revoked,
        message: if revoked {
            "Token revoked successfully".to_string()
        } else {
            "Token not found or already revoked".to_string()
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_deterministic() {
        let hash1 = hash_token("test-token-value");
        let hash2 = hash_token("test-token-value");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_token_different_inputs() {
        let hash1 = hash_token("token-a");
        let hash2 = hash_token("token-b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_token_is_hex() {
        let hash = hash_token("some-token");
        assert_eq!(hash.len(), 64); // SHA256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_token_known_value() {
        // SHA256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let hash = hash_token("hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
