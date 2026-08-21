//! Admin detail view over auth.usr and auth.local_account, aggregating role
//! assignments (with SAML-managed flags), SAML identities, captured SAML
//! attribute values, and recent sessions — plus a narrow update endpoint
//! for local accounts (names and soft deletion).
//!
//! Reads require `auth.user.detail.read` — deliberately stronger than
//! `auth.user.read` (which incident staff hold for user search) because
//! sessions expose IP addresses and user agents. Updates require
//! `auth.user.write` and are refused for SAML accounts (the IdP owns
//! them). Secrets (password hashes, token hashes) never leave the database.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::auth::{
    local_account, saml_idp_attribute, saml_idp_config, saml_usr_attr, session, usr,
    usr_saml_identities,
};
use odo_entity::authz::usr_role_org_map;
use odo_entity::org::unit;
use odo_service::admin::clean_optional;
use sea_orm::prelude::*;
use sea_orm::{QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::authz_admin::require_perm;
use crate::role_assignments::{self, AssignmentRow};

const DETAIL_PERM: &str = "odo.auth.user.detail.read";
const WRITE_PERM: &str = "odo.auth.user.write";
const RECENT_SESSIONS: u64 = 10;

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct UserAccountRow {
    pub id: i32,
    pub username: Option<String>,
    pub email: String,
    pub display_name: String,
    pub first_given_name: Option<String>,
    pub family_name: Option<String>,
    pub status: Option<String>,
    pub auth_method: String,
    /// Soft-delete timestamp; null when the account is active. odo never
    /// hard-deletes shared rows, so a deleted account stays resolvable.
    pub deleted_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// Id of the user who soft-deleted this account (if deleted).
    pub deleted_by: Option<i32>,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_login_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LocalAccountRow {
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub failed_login_attempts: i32,
    pub locked_until: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SamlIdentityRow {
    pub idp: i32,
    pub idp_name: String,
    pub name_id: String,
    pub name_id_format: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SamlUserAttrRow {
    pub key: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionRow {
    pub auth_method: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_active: bool,
    pub org_unit: Option<i32>,
    pub org_unit_label: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub last_activity_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub expires_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDetailResponse {
    pub user: UserAccountRow,
    /// Present when the user has a local (username/password) account.
    pub local_account: Option<LocalAccountRow>,
    pub roles: Vec<AssignmentRow>,
    pub saml_identities: Vec<SamlIdentityRow>,
    pub saml_attributes: Vec<SamlUserAttrRow>,
    /// Most recent sessions, newest first.
    pub sessions: Vec<SessionRow>,
}

#[derive(Deserialize, ToSchema)]
pub struct UserDetailRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    id: i32,
    /// Empty string clears the field.
    #[serde(default)]
    first_given_name: Option<String>,
    #[serde(default)]
    second_given_name: Option<String>,
    #[serde(default)]
    family_name: Option<String>,
    /// Soft-delete action: `true` marks the account deleted (server stamps
    /// deleted_at/deleted_by), `false` restores it. Omitted leaves it as-is.
    #[serde(default)]
    deleted: Option<bool>,
}

// ===========================================================================
// Handler
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/user/detail",
    request_body = UserDetailRequest,
    responses((status = 200, body = UserDetailResponse, description = "Detailed account view for one user")),
    security(("bearer" = [])),
    tag = "user"
)]
pub async fn user_detail(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UserDetailRequest>,
) -> ApiResult<Json<UserDetailResponse>> {
    require_perm(&state, DETAIL_PERM, None).await?;

    // Deleted accounts remain visible here; the row flags it.
    let user = usr::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("user {}", params.id)))?;

    let local = local_account::Entity::find()
        .filter(local_account::Column::Usr.eq(user.id))
        .one(&state.db)
        .await?
        .map(|a| LocalAccountRow {
            created_at: a.created_at,
            updated_at: a.updated_at,
            failed_login_attempts: a.failed_login_attempts.unwrap_or(0),
            locked_until: a.locked_until,
        });

    let assignments = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Usr.eq(user.id))
        .order_by_asc(usr_role_org_map::Column::Role)
        .order_by_asc(usr_role_org_map::Column::OrgUnit)
        .all(&state.db)
        .await?;
    let roles = role_assignments::decorate(&state.db, assignments).await?;

    let idp_names: HashMap<i32, String> = saml_idp_config::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|i| (i.id, i.name))
        .collect();

    let saml_identities = usr_saml_identities::Entity::find()
        .filter(usr_saml_identities::Column::UserId.eq(user.id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|i| SamlIdentityRow {
            idp_name: idp_names.get(&i.idp_id).cloned().unwrap_or_default(),
            idp: i.idp_id,
            name_id: i.name_id,
            name_id_format: i.name_id_format,
            created_at: i.created_at,
            updated_at: i.updated_at,
        })
        .collect();

    let attr_meta: HashMap<i32, (String, String)> = saml_idp_attribute::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|a| (a.id, (a.key, a.label)))
        .collect();

    let saml_attributes = saml_usr_attr::Entity::find()
        .filter(saml_usr_attr::Column::Ident.eq(user.id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|a| {
            let (key, label) = attr_meta.get(&a.attr).cloned().unwrap_or_default();
            SamlUserAttrRow {
                key,
                label,
                value: a.value,
            }
        })
        .collect();

    let recent_sessions = session::Entity::find()
        .filter(session::Column::Usr.eq(user.id))
        .order_by_desc(session::Column::CreatedAt)
        .limit(RECENT_SESSIONS)
        .all(&state.db)
        .await?;

    let unit_ids: Vec<i32> = recent_sessions.iter().filter_map(|s| s.org_unit).collect();
    let mut unit_labels: HashMap<i32, String> = HashMap::new();
    if !unit_ids.is_empty() {
        for u in unit::Entity::find()
            .filter(unit::Column::Id.is_in(unit_ids))
            .all(&state.db)
            .await?
        {
            unit_labels.insert(u.id, u.label);
        }
    }

    let sessions = recent_sessions
        .into_iter()
        .map(|s| SessionRow {
            org_unit_label: s.org_unit.and_then(|id| unit_labels.get(&id).cloned()),
            auth_method: s.auth_method,
            ip_address: s.ip_address,
            user_agent: s.user_agent,
            is_active: s.is_active.unwrap_or(false),
            org_unit: s.org_unit,
            created_at: s.created_at,
            last_activity_at: s.last_activity_at,
            expires_at: s.expires_at,
        })
        .collect();

    Ok(Json(UserDetailResponse {
        user: UserAccountRow {
            id: user.id,
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            first_given_name: user.first_given_name,
            family_name: user.family_name,
            status: user.status,
            auth_method: user.auth_method,
            deleted_at: user.deleted_at,
            deleted_by: user.deleted_by,
            created_at: user.created_at,
            updated_at: user.updated_at,
            last_login_at: user.last_login_at,
        },
        local_account: local,
        roles,
        saml_identities,
        saml_attributes,
        sessions,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/user/update",
    request_body = UpdateUserRequest,
    responses((status = 200, body = UserAccountRow, description = "Updated user account")),
    security(("bearer" = [])),
    tag = "user"
)]
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateUserRequest>,
) -> ApiResult<Json<UserAccountRow>> {
    let caller_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = usr::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("user {}", params.id)))?;

    if existing.auth_method != "local" {
        return Err(LocalError::conflict(
            "NOT_LOCAL_ACCOUNT",
            None,
            "Only local accounts can be edited here; SAML accounts are managed by the identity provider.",
        )
        .into());
    }

    if params.deleted == Some(true) && existing.id == caller_id {
        return Err(LocalError::invalid_input("You may not delete your own account.").into());
    }

    tracing::info!(id = params.id, "UpdateUser");

    let mut model = usr::ActiveModel::from(existing);
    if params.first_given_name.is_some() {
        model.first_given_name = Set(clean_optional(params.first_given_name.as_deref()));
    }
    if params.second_given_name.is_some() {
        model.second_given_name = Set(clean_optional(params.second_given_name.as_deref()));
    }
    if params.family_name.is_some() {
        model.family_name = Set(clean_optional(params.family_name.as_deref()));
    }
    if let Some(deleted) = params.deleted {
        if deleted {
            // Server-stamped soft delete: capture when and by whom, so the
            // account stays resolvable (odo never hard-deletes shared rows).
            model.deleted_at = Set(Some(Utc::now().into()));
            model.deleted_by = Set(Some(caller_id));
        } else {
            model.deleted_at = Set(None);
            model.deleted_by = Set(None);
        }
    }
    model.update(&state.db).await?;

    // Re-fetch: a DB trigger recomputes display_name from the name fields.
    let updated = usr::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::internal("user vanished during update"))?;

    Ok(Json(UserAccountRow {
        id: updated.id,
        username: updated.username,
        email: updated.email,
        display_name: updated.display_name,
        first_given_name: updated.first_given_name,
        family_name: updated.family_name,
        status: updated.status,
        auth_method: updated.auth_method,
        deleted_at: updated.deleted_at,
        deleted_by: updated.deleted_by,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        last_login_at: updated.last_login_at,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    username: String,
    email: String,
    /// "local" (default) or "saml". SAML accounts are normally created at
    /// first SSO login; creating one here pre-provisions it.
    #[serde(default)]
    auth_method: Option<String>,
    #[serde(default)]
    first_given_name: Option<String>,
    #[serde(default)]
    second_given_name: Option<String>,
    #[serde(default)]
    family_name: Option<String>,
    /// Sets the local-account password. Local accounts without a password
    /// cannot log in until one is set. Rejected for SAML accounts.
    #[serde(default)]
    password: Option<String>,
    /// Pin the stable uuid (durable references; fixtures and app
    /// registration pin well-known uuids). Random when omitted.
    #[serde(default)]
    uuid: Option<String>,
}

/// Create a user account (and optionally its local-account password).
///
/// This is the API face of what create-local-account.sh used to do with
/// direct SQL: app registration and test fixtures provision their users
/// here, then assign roles via authz/user-role/create.
#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/user/create",
    request_body = CreateUserRequest,
    responses((status = 200, body = UserAccountRow)),
    security(("bearer" = []))
)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateUserRequest>,
) -> ApiResult<Json<UserAccountRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let username = params.username.trim().to_string();
    let email = params.email.trim().to_string();
    if username.is_empty() || username.chars().any(char::is_whitespace) {
        return Err(
            LocalError::invalid_input("username must be non-empty without whitespace").into(),
        );
    }
    if email.is_empty() || !email.contains('@') {
        return Err(LocalError::invalid_input("email must be a valid address").into());
    }

    let auth_method = params.auth_method.as_deref().unwrap_or("local");
    if !matches!(auth_method, "local" | "saml") {
        return Err(LocalError::invalid_input("auth_method must be 'local' or 'saml'").into());
    }
    if auth_method == "saml" && params.password.is_some() {
        return Err(LocalError::invalid_input("SAML accounts cannot have a local password").into());
    }

    let pinned_uuid = match params.uuid.as_deref() {
        Some(u) => Some(
            u.parse::<Uuid>()
                .map_err(|_| LocalError::invalid_input("invalid uuid"))?,
        ),
        None => None,
    };

    // Uniqueness among active accounts (soft-deleted rows don't block reuse,
    // matching the partial unique indexes).
    let taken = usr::Entity::find()
        .filter(usr::Column::DeletedAt.is_null())
        .filter(
            sea_orm::Condition::any()
                .add(usr::Column::Username.eq(username.clone()))
                .add(usr::Column::Email.eq(email.clone())),
        )
        .one(&state.db)
        .await?;
    if let Some(existing) = taken {
        let (code, field) = if existing.username.as_deref() == Some(username.as_str()) {
            ("USERNAME_TAKEN", "username")
        } else {
            ("EMAIL_TAKEN", "email")
        };
        return Err(LocalError::conflict(
            code,
            Some(field),
            "An active account with this identity already exists.",
        )
        .into());
    }

    tracing::info!(username = %username, auth_method = %auth_method, "CreateUser");

    let mut model = usr::ActiveModel {
        username: Set(Some(username)),
        email: Set(email),
        auth_method: Set(auth_method.to_string()),
        status: Set(Some("active".to_string())),
        first_given_name: Set(clean_optional(params.first_given_name.as_deref())),
        second_given_name: Set(clean_optional(params.second_given_name.as_deref())),
        family_name: Set(clean_optional(params.family_name.as_deref())),
        ..Default::default()
    };
    if let Some(u) = pinned_uuid {
        model.uuid = Set(u);
    }
    let created = model.insert(&state.db).await?;

    if let Some(password) = params.password.as_deref() {
        if password.len() < 8 {
            return Err(LocalError::invalid_input("password must be at least 8 characters").into());
        }
        // Hash in-DB with the same function login verifies against.
        use sea_orm::ConnectionTrait;
        let hashed = state
            .db
            .query_one_raw(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Postgres,
                "SELECT auth.hash_password($1) AS hash",
                [password.into()],
            ))
            .await?
            .ok_or_else(|| LocalError::internal("hash_password returned nothing"))?;
        let hash: String = hashed
            .try_get("", "hash")
            .map_err(|e| LocalError::internal(e.to_string()))?;

        let account = local_account::ActiveModel {
            usr: Set(created.id),
            password_hash: Set(hash),
            ..Default::default()
        };
        account.insert(&state.db).await?;
    }

    // Re-fetch: a DB trigger recomputes display_name from the name fields.
    let created = usr::Entity::find_by_id(created.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::internal("user vanished during create"))?;

    Ok(Json(UserAccountRow {
        id: created.id,
        username: created.username,
        email: created.email,
        display_name: created.display_name,
        first_given_name: created.first_given_name,
        family_name: created.family_name,
        status: created.status,
        auth_method: created.auth_method,
        deleted_at: created.deleted_at,
        deleted_by: created.deleted_by,
        created_at: created.created_at,
        updated_at: created.updated_at,
        last_login_at: created.last_login_at,
    }))
}
