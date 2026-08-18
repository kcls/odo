//! Admin CRUD for authz.usr_role_org_map (user role assignments).
//!
//! Listing a user's assignments requires `auth.user.read`. Creating or
//! removing an assignment requires `auth.user.role.write` checked **at the
//! assignment's org unit**, so the permission can be delegated regionally
//! via role_permission.min_depth. SAML-managed assignments are refused for
//! deletion — the SAML sync owns them.

use axum::Json;
use axum::extract::State;
use odo_service::admin::map_unique_violation;
use odo_entity::auth::usr;
use odo_entity::authz::{permission, role, usr_role_org_map};
use odo_entity::org::unit;
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::{DbBackend, QueryOrder, Set, Statement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::authz_admin::require_perm;

const WRITE_PERM: &str = "odo.auth.user_role.write";
const READ_PERM: &str = "odo.auth.user_role.read";

#[derive(Debug, Serialize, ToSchema)]
pub struct AssignmentRow {
    pub id: i32,
    pub usr: i32,
    pub role: String,
    pub role_label: String,
    pub org_unit: i32,
    pub org_unit_label: String,
    pub is_managed_by_saml: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListAssignmentsResponse {
    pub assignments: Vec<AssignmentRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssignmentSuccessResponse {
    pub success: bool,
}

/// One org-unit subtree root where a permission applies ("<label> and below").
#[derive(Debug, Serialize, ToSchema)]
pub struct ScopeUnit {
    pub id: i32,
    pub label: String,
}

/// A permission the user effectively holds, plus where it applies.
#[derive(Debug, Serialize, ToSchema)]
pub struct PermScopeRow {
    pub perm: String,
    pub description: Option<String>,
    /// True when the permission applies at every org unit (covers the root);
    /// `scope_units` is then empty.
    pub global: bool,
    /// Minimal set of subtree roots the permission applies within. Empty when
    /// `global` is true.
    pub scope_units: Vec<ScopeUnit>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPermScopesResponse {
    pub perms: Vec<PermScopeRow>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListAssignmentsRequest {
    #[serde(default)]
    usr: Option<i32>,
    /// User by stable uuid (accepted alongside `usr`; id wins).
    #[serde(default)]
    usr_uuid: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAssignmentRequest {
    #[serde(default)]
    usr: Option<i32>,
    /// User by stable uuid (accepted alongside `usr`; id wins).
    #[serde(default)]
    usr_uuid: Option<String>,
    role: String,
    #[serde(default)]
    org_unit: Option<i32>,
    /// Org unit by stable uuid (accepted alongside `org_unit`; id wins).
    #[serde(default)]
    org_unit_uuid: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AssignmentIdRequest {
    id: i32,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-role/list",
    request_body = ListAssignmentsRequest,
    responses((status = 200, body = ListAssignmentsResponse, description = "A user's role assignments")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn list_assignments(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListAssignmentsRequest>,
) -> ApiResult<Json<ListAssignmentsResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let usr_id =
        crate::handler::resolve_usr_ref(&state.db, params.usr, params.usr_uuid.as_deref()).await?;
    find_user(&state.db, usr_id).await?;

    let assignments = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Usr.eq(usr_id))
        .order_by_asc(usr_role_org_map::Column::Role)
        .order_by_asc(usr_role_org_map::Column::OrgUnit)
        .all(&state.db)
        .await?;

    let rows = decorate(&state.db, assignments).await?;

    Ok(Json(ListAssignmentsResponse { assignments: rows }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-perm-scopes",
    request_body = ListAssignmentsRequest,
    responses((status = 200, body = UserPermScopesResponse, description = "A user's effective permissions and where each applies")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn user_perm_scopes(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListAssignmentsRequest>,
) -> ApiResult<Json<UserPermScopesResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let usr_id =
        crate::handler::resolve_usr_ref(&state.db, params.usr, params.usr_uuid.as_deref()).await?;
    find_user(&state.db, usr_id).await?;

    // authz.usr_perm_scopes returns one row per (perm) when global, else one
    // row per minimal covered subtree root. See migration 092. This is the same
    // membership logic authz.usr_has_perm_at enforces, so the display can't
    // diverge from enforcement.
    let scope_rows = state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT perm, is_global, scope_unit_id, scope_unit_label \
             FROM authz.usr_perm_scopes($1)",
            [usr_id.into()],
        ))
        .await?;

    // Group by perm, accumulating scope units.
    let mut order: Vec<String> = Vec::new();
    let mut by_perm: HashMap<String, PermScopeRow> = HashMap::new();
    for row in scope_rows {
        let perm: String = row.try_get("", "perm")?;
        let is_global: bool = row.try_get("", "is_global")?;

        let entry = by_perm.entry(perm.clone()).or_insert_with(|| {
            order.push(perm.clone());
            PermScopeRow {
                perm: perm.clone(),
                description: None,
                global: false,
                scope_units: Vec::new(),
            }
        });

        if is_global {
            entry.global = true;
        } else if let (Ok(id), Ok(label)) = (
            row.try_get::<i32>("", "scope_unit_id"),
            row.try_get::<String>("", "scope_unit_label"),
        ) {
            entry.scope_units.push(ScopeUnit { id, label });
        }
    }

    // Join permission descriptions for the perms in play.
    if !order.is_empty() {
        let descriptions: HashMap<String, Option<String>> = permission::Entity::find()
            .filter(permission::Column::Code.is_in(order.clone()))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|p| (p.code, p.description))
            .collect();
        for code in &order {
            if let Some(entry) = by_perm.get_mut(code) {
                entry.description = descriptions.get(code).cloned().flatten();
            }
        }
    }

    // Stable, readable order: by permission code.
    order.sort();
    let perms: Vec<PermScopeRow> = order
        .into_iter()
        .filter_map(|code| by_perm.remove(&code))
        .map(|mut row| {
            row.scope_units.sort_by(|a, b| a.label.cmp(&b.label));
            row
        })
        .collect();

    Ok(Json(UserPermScopesResponse { perms }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-role/create",
    request_body = CreateAssignmentRequest,
    responses((status = 200, body = AssignmentRow, description = "Newly-created role assignment")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn create_assignment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateAssignmentRequest>,
) -> ApiResult<Json<AssignmentRow>> {
    // Resolve id/uuid references (uuid migration), validate the pieces so
    // bad input is a clean 404, then check the caller's permission at the
    // target org unit.
    let usr_id =
        crate::handler::resolve_usr_ref(&state.db, params.usr, params.usr_uuid.as_deref()).await?;
    let org_unit_id =
        crate::handler::resolve_org_ref(&state.db, params.org_unit, params.org_unit_uuid.as_deref())
            .await?
            .ok_or_else(|| LocalError::invalid_input("org_unit or org_unit_uuid required"))?;

    find_user(&state.db, usr_id).await?;

    let role_row = role::Entity::find_by_id(&params.role)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.role)))?;

    let unit_row = unit::Entity::find_by_id(org_unit_id)
        .filter(unit::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("org unit {org_unit_id}")))?;

    require_perm(&state, WRITE_PERM, Some(org_unit_id)).await?;

    tracing::info!(
        usr = usr_id,
        role = %params.role,
        org_unit = org_unit_id,
        "CreateRoleAssignment"
    );

    let model = usr_role_org_map::ActiveModel {
        usr: Set(usr_id),
        role: Set(params.role),
        org_unit: Set(org_unit_id),
        is_managed_by_saml: Set(false),
        ..Default::default()
    };

    let inserted = model.insert(&state.db).await.map_err(|e| {
        map_unique_violation(
            e,
            "ALREADY_ASSIGNED",
            None,
            "The user already has this role at this org unit.",
        )
    })?;

    Ok(Json(AssignmentRow {
        id: inserted.id,
        usr: inserted.usr,
        role_label: role_row.label,
        role: inserted.role,
        org_unit: inserted.org_unit,
        org_unit_label: unit_row.label,
        is_managed_by_saml: inserted.is_managed_by_saml,
        created_at: inserted.created_at,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/user-role/delete",
    request_body = AssignmentIdRequest,
    responses((status = 200, body = AssignmentSuccessResponse, description = "Role assignment removed")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn delete_assignment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<AssignmentIdRequest>,
) -> ApiResult<Json<AssignmentSuccessResponse>> {
    let existing = usr_role_org_map::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role assignment {}", params.id)))?;

    require_perm(&state, WRITE_PERM, Some(existing.org_unit)).await?;

    if existing.is_managed_by_saml {
        return Err(LocalError::conflict(
            "SAML_MANAGED",
            None,
            "This assignment is managed by SAML and cannot be removed manually.",
        )
        .into());
    }

    tracing::info!(
        id = params.id,
        usr = existing.usr,
        role = %existing.role,
        org_unit = existing.org_unit,
        "DeleteRoleAssignment"
    );

    usr_role_org_map::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    Ok(Json(AssignmentSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_user(db: &DatabaseConnection, id: i32) -> Result<usr::Model, LocalError> {
    usr::Entity::find_by_id(id)
        .filter(usr::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("user {id}")))
}

/// Attach role and org unit labels to raw assignment rows.
pub(crate) async fn decorate(
    db: &DatabaseConnection,
    assignments: Vec<usr_role_org_map::Model>,
) -> Result<Vec<AssignmentRow>, LocalError> {
    if assignments.is_empty() {
        return Ok(vec![]);
    }

    let role_codes: Vec<String> = assignments.iter().map(|a| a.role.clone()).collect();
    let mut role_labels: HashMap<String, String> = HashMap::new();
    for r in role::Entity::find()
        .filter(role::Column::Code.is_in(role_codes))
        .all(db)
        .await?
    {
        role_labels.insert(r.code, r.label);
    }

    let unit_ids: Vec<i32> = assignments.iter().map(|a| a.org_unit).collect();
    let mut unit_labels: HashMap<i32, String> = HashMap::new();
    for u in unit::Entity::find()
        .filter(unit::Column::Id.is_in(unit_ids))
        .all(db)
        .await?
    {
        unit_labels.insert(u.id, u.label);
    }

    Ok(assignments
        .into_iter()
        .map(|a| AssignmentRow {
            id: a.id,
            usr: a.usr,
            role_label: role_labels.get(&a.role).cloned().unwrap_or_default(),
            role: a.role,
            org_unit_label: unit_labels.get(&a.org_unit).cloned().unwrap_or_default(),
            org_unit: a.org_unit,
            is_managed_by_saml: a.is_managed_by_saml,
            created_at: a.created_at,
        })
        .collect())
}
