//! Admin CRUD for authz.permission, authz.role, and authz.role_permission.
//!
//! Reads require `auth.authz.read`; writes require `auth.authz.write`.
//! Codes are immutable once created (they are primary keys referenced by
//! non-cascading FKs); deletes are refused with a 409 while other rows
//! still reference the target, except a role's own permission grants,
//! which are removed with the role.

use axum::Json;
use axum::extract::State;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::authz::{permission, role, role_permission, saml_attr_role_map, usr_role_org_map};
use odo_service::admin::{
    Page, Paginated, Sort, clean_code, clean_optional, clean_required, clean_search,
    map_unique_violation,
};
use sea_orm::prelude::*;
use sea_orm::{Condition, Order, QueryOrder, QuerySelect, Set, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::{AppState, authz};

const READ_PERM: &str = "odo.auth.role.read";
const WRITE_PERM: &str = "odo.auth.role.write";

/// Check the caller holds `perm`. `org_unit = None` checks at the root org
/// unit; `Some(id)` checks at that org unit (org-tree aware via min_depth).
pub(crate) async fn require_perm(
    state: &AppState,
    perm: &'static str,
    org_unit: Option<i32>,
) -> Result<(), LocalError> {
    let caller_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;
    if !authz::user_has_perm(&state.db, caller_id, perm, org_unit).await? {
        return Err(LocalError::permission_denied(perm, org_unit));
    }
    Ok(())
}

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct PermissionRow {
    pub code: String,
    pub description: Option<String>,
    /// Number of roles this permission is granted to.
    pub role_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoleRow {
    pub code: String,
    pub label: String,
    pub description: Option<String>,
    /// Number of permissions granted to this role.
    pub perm_count: i64,
    /// Number of distinct users holding this role at any org unit.
    pub user_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GrantRow {
    pub id: i32,
    pub perm: String,
    pub perm_description: Option<String>,
    pub min_depth: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RolePermissionRow {
    pub id: i32,
    pub role: String,
    pub perm: String,
    pub min_depth: i32,
}

impl From<role_permission::Model> for RolePermissionRow {
    fn from(m: role_permission::Model) -> Self {
        Self {
            id: m.id,
            role: m.role,
            perm: m.perm,
            min_depth: m.min_depth,
        }
    }
}

odo_service::page_type!(PermissionPage, PermissionRow, "One page of permissions.");
odo_service::page_type!(RolePage, RoleRow, "One page of roles.");

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListPermissionsRequest {
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListRolesRequest {
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoleDetailResponse {
    pub role: RoleRow,
    pub grants: Vec<GrantRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthzAdminSuccessResponse {
    pub success: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreatePermissionRequest {
    code: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdatePermissionRequest {
    code: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PermissionCodeRequest {
    code: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateRoleRequest {
    code: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    code: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct RoleCodeRequest {
    code: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateRolePermissionRequest {
    role: String,
    perm: String,
    #[serde(default)]
    min_depth: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRolePermissionRequest {
    id: i32,
    min_depth: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct RolePermissionIdRequest {
    id: i32,
}

// ===========================================================================
// Permissions
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/permission/list",
    request_body = ListPermissionsRequest,
    responses((status = 200, body = PermissionPage, description = "Permissions with grant counts")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn list_permissions(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListPermissionsRequest>,
) -> ApiResult<Json<PermissionPage>> {
    require_perm(&state, READ_PERM, None).await?;

    let mut condition = Condition::all();
    if let Some(search) = clean_search(params.search.as_deref()) {
        condition = condition.add(
            Condition::any()
                .add(permission::Column::Code.contains(&search))
                .add(permission::Column::Description.contains(&search)),
        );
    }

    let total = permission::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // Sort by an allow-listed column (role_count is computed post-query, so
    // it is intentionally not sortable). Code is unique, so no extra
    // tiebreaker is needed for stable paging.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("code", permission::Column::Code),
            ("description", permission::Column::Description),
        ],
        (permission::Column::Code, Order::Asc),
    );

    let permissions = permission::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(permission::Column::Code)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    // Grant counts for the page's permissions.
    let mut role_counts: HashMap<String, i64> = HashMap::new();
    if !permissions.is_empty() {
        let codes: Vec<String> = permissions.iter().map(|p| p.code.clone()).collect();
        for grant in role_permission::Entity::find()
            .filter(role_permission::Column::Perm.is_in(codes))
            .all(&state.db)
            .await?
        {
            *role_counts.entry(grant.perm).or_insert(0) += 1;
        }
    }

    let rows = permissions
        .into_iter()
        .map(|p| PermissionRow {
            role_count: role_counts.get(&p.code).copied().unwrap_or(0),
            code: p.code,
            description: p.description,
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/permission/create",
    request_body = CreatePermissionRequest,
    responses((status = 200, body = PermissionRow, description = "Newly-created permission")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn create_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreatePermissionRequest>,
) -> ApiResult<Json<PermissionRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let code = clean_code(&params.code, "code")?;
    let description = clean_optional(params.description.as_deref());

    tracing::info!(code = %code, "CreatePermission");

    let model = permission::ActiveModel {
        code: Set(code),
        description: Set(description),
    };

    let inserted = model.insert(&state.db).await.map_err(|e| {
        map_unique_violation(
            e,
            "PERMISSION_CODE_TAKEN",
            Some("code"),
            "A permission with this code already exists.",
        )
    })?;

    Ok(Json(PermissionRow {
        code: inserted.code,
        description: inserted.description,
        role_count: 0,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/permission/update",
    request_body = UpdatePermissionRequest,
    responses((status = 200, body = PermissionRow, description = "Updated permission")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn update_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdatePermissionRequest>,
) -> ApiResult<Json<PermissionRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = permission::Entity::find_by_id(&params.code)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("permission {}", params.code)))?;

    tracing::info!(code = %existing.code, "UpdatePermission");

    let mut model = permission::ActiveModel::from(existing);
    model.description = Set(clean_optional(params.description.as_deref()));
    let updated = model.update(&state.db).await?;

    let role_count = role_permission::Entity::find()
        .filter(role_permission::Column::Perm.eq(&updated.code))
        .count(&state.db)
        .await? as i64;

    Ok(Json(PermissionRow {
        code: updated.code,
        description: updated.description,
        role_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/permission/delete",
    request_body = PermissionCodeRequest,
    responses((status = 200, body = AuthzAdminSuccessResponse, description = "Permission deleted")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn delete_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<PermissionCodeRequest>,
) -> ApiResult<Json<AuthzAdminSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let grant_count = role_permission::Entity::find()
        .filter(role_permission::Column::Perm.eq(&params.code))
        .count(&state.db)
        .await?;

    if grant_count > 0 {
        return Err(LocalError::conflict(
            "PERMISSION_IN_USE",
            None,
            format!("Permission is granted to {grant_count} role(s); revoke the grants first."),
        )
        .into());
    }

    tracing::info!(code = %params.code, "DeletePermission");

    let result = permission::Entity::delete_by_id(&params.code)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("permission {}", params.code)).into());
    }

    Ok(Json(AuthzAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Roles
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role/list",
    request_body = ListRolesRequest,
    responses((status = 200, body = RolePage, description = "Roles with grant and user counts")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn list_roles(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListRolesRequest>,
) -> ApiResult<Json<RolePage>> {
    require_perm(&state, READ_PERM, None).await?;

    let mut condition = Condition::all();
    if let Some(search) = clean_search(params.search.as_deref()) {
        condition = condition.add(
            Condition::any()
                .add(role::Column::Code.contains(&search))
                .add(role::Column::Label.contains(&search)),
        );
    }

    let total = role::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // perm_count/user_count are computed post-query, so only real columns sort.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("code", role::Column::Code),
            ("label", role::Column::Label),
            ("description", role::Column::Description),
        ],
        (role::Column::Code, Order::Asc),
    );

    let roles = role::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(role::Column::Code)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    let codes: Vec<String> = roles.iter().map(|r| r.code.clone()).collect();

    let mut perm_counts: HashMap<String, i64> = HashMap::new();
    let mut role_users: HashMap<String, HashSet<i32>> = HashMap::new();
    if !codes.is_empty() {
        for grant in role_permission::Entity::find()
            .filter(role_permission::Column::Role.is_in(codes.clone()))
            .all(&state.db)
            .await?
        {
            *perm_counts.entry(grant.role).or_insert(0) += 1;
        }
        for assignment in usr_role_org_map::Entity::find()
            .filter(usr_role_org_map::Column::Role.is_in(codes))
            .all(&state.db)
            .await?
        {
            role_users
                .entry(assignment.role)
                .or_default()
                .insert(assignment.usr);
        }
    }

    let rows = roles
        .into_iter()
        .map(|r| RoleRow {
            perm_count: perm_counts.get(&r.code).copied().unwrap_or(0),
            user_count: role_users.get(&r.code).map(|u| u.len() as i64).unwrap_or(0),
            code: r.code,
            label: r.label,
            description: r.description,
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role/get",
    request_body = RoleCodeRequest,
    responses((status = 200, body = RoleDetailResponse, description = "Role with its permission grants")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn get_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<RoleCodeRequest>,
) -> ApiResult<Json<RoleDetailResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let role_row = role::Entity::find_by_id(&params.code)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.code)))?;

    let grants = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&role_row.code))
        .order_by_asc(role_permission::Column::Perm)
        .all(&state.db)
        .await?;

    let mut descriptions: HashMap<String, Option<String>> = HashMap::new();
    if !grants.is_empty() {
        let codes: Vec<String> = grants.iter().map(|g| g.perm.clone()).collect();
        for p in permission::Entity::find()
            .filter(permission::Column::Code.is_in(codes))
            .all(&state.db)
            .await?
        {
            descriptions.insert(p.code, p.description);
        }
    }

    let user_count = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Role.eq(&role_row.code))
        .all(&state.db)
        .await?
        .iter()
        .map(|a| a.usr)
        .collect::<HashSet<_>>()
        .len() as i64;

    let grant_rows: Vec<GrantRow> = grants
        .into_iter()
        .map(|g| GrantRow {
            id: g.id,
            perm_description: descriptions.get(&g.perm).cloned().flatten(),
            perm: g.perm,
            min_depth: g.min_depth,
        })
        .collect();

    Ok(Json(RoleDetailResponse {
        role: RoleRow {
            perm_count: grant_rows.len() as i64,
            user_count,
            code: role_row.code,
            label: role_row.label,
            description: role_row.description,
        },
        grants: grant_rows,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role/create",
    request_body = CreateRoleRequest,
    responses((status = 200, body = RoleRow, description = "Newly-created role")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn create_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateRoleRequest>,
) -> ApiResult<Json<RoleRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let code = clean_code(&params.code, "code")?;
    let label = clean_required(&params.label, "label")?;
    let description = clean_optional(params.description.as_deref());

    tracing::info!(code = %code, "CreateRole");

    let model = role::ActiveModel {
        code: Set(code),
        label: Set(label),
        description: Set(description),
    };

    let inserted = model.insert(&state.db).await.map_err(|e| {
        map_unique_violation(
            e,
            "ROLE_CODE_TAKEN",
            Some("code"),
            "A role with this code already exists.",
        )
    })?;

    Ok(Json(RoleRow {
        code: inserted.code,
        label: inserted.label,
        description: inserted.description,
        perm_count: 0,
        user_count: 0,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role/update",
    request_body = UpdateRoleRequest,
    responses((status = 200, body = RoleRow, description = "Updated role")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn update_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateRoleRequest>,
) -> ApiResult<Json<RoleRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = role::Entity::find_by_id(&params.code)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.code)))?;

    tracing::info!(code = %existing.code, "UpdateRole");

    let mut model = role::ActiveModel::from(existing);
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if params.description.is_some() {
        model.description = Set(clean_optional(params.description.as_deref()));
    }
    let updated = model.update(&state.db).await?;

    let perm_count = role_permission::Entity::find()
        .filter(role_permission::Column::Role.eq(&updated.code))
        .count(&state.db)
        .await? as i64;

    let user_count = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Role.eq(&updated.code))
        .all(&state.db)
        .await?
        .iter()
        .map(|a| a.usr)
        .collect::<HashSet<_>>()
        .len() as i64;

    Ok(Json(RoleRow {
        code: updated.code,
        label: updated.label,
        description: updated.description,
        perm_count,
        user_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role/delete",
    request_body = RoleCodeRequest,
    responses((status = 200, body = AuthzAdminSuccessResponse, description = "Role deleted along with its permission grants")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn delete_role(
    State(state): State<Arc<AppState>>,
    Json(params): Json<RoleCodeRequest>,
) -> ApiResult<Json<AuthzAdminSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    role::Entity::find_by_id(&params.code)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.code)))?;

    let assignment_count = usr_role_org_map::Entity::find()
        .filter(usr_role_org_map::Column::Role.eq(&params.code))
        .count(&state.db)
        .await?;

    if assignment_count > 0 {
        return Err(LocalError::conflict(
            "ROLE_ASSIGNED",
            None,
            format!(
                "Role is assigned to users ({assignment_count} assignment(s)); remove the assignments first."
            ),
        )
        .into());
    }

    let saml_count = saml_attr_role_map::Entity::find()
        .filter(saml_attr_role_map::Column::Role.eq(&params.code))
        .count(&state.db)
        .await?;

    if saml_count > 0 {
        return Err(LocalError::conflict(
            "ROLE_SAML_MAPPED",
            None,
            format!(
                "Role is referenced by {saml_count} SAML attribute mapping(s); remove the mappings first."
            ),
        )
        .into());
    }

    tracing::info!(code = %params.code, "DeleteRole");

    // A role's own grants are part of its definition; remove them with it.
    let txn = state.db.begin().await?;
    role_permission::Entity::delete_many()
        .filter(role_permission::Column::Role.eq(&params.code))
        .exec(&txn)
        .await?;
    role::Entity::delete_by_id(&params.code).exec(&txn).await?;
    txn.commit().await?;

    Ok(Json(AuthzAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Role permission grants
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role-permission/create",
    request_body = CreateRolePermissionRequest,
    responses((status = 200, body = RolePermissionRow, description = "Newly-created permission grant")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn create_role_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateRolePermissionRequest>,
) -> ApiResult<Json<RolePermissionRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    validate_min_depth(params.min_depth)?;

    // Resolve both codes first so bad input is a 404 rather than an FK error.
    role::Entity::find_by_id(&params.role)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.role)))?;
    permission::Entity::find_by_id(&params.perm)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("permission {}", params.perm)))?;

    tracing::info!(role = %params.role, perm = %params.perm, "CreateRolePermission");

    let model = role_permission::ActiveModel {
        role: Set(params.role),
        perm: Set(params.perm),
        min_depth: Set(params.min_depth),
        ..Default::default()
    };

    let inserted = model.insert(&state.db).await.map_err(|e| {
        map_unique_violation(
            e,
            "PERMISSION_ALREADY_GRANTED",
            None,
            "This role already has a grant for this permission.",
        )
    })?;

    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role-permission/update",
    request_body = UpdateRolePermissionRequest,
    responses((status = 200, body = RolePermissionRow, description = "Updated permission grant")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn update_role_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateRolePermissionRequest>,
) -> ApiResult<Json<RolePermissionRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    validate_min_depth(params.min_depth)?;

    let existing = role_permission::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("permission grant {}", params.id)))?;

    tracing::info!(
        id = params.id,
        min_depth = params.min_depth,
        "UpdateRolePermission"
    );

    let mut model = role_permission::ActiveModel::from(existing);
    model.min_depth = Set(params.min_depth);
    let updated = model.update(&state.db).await?;

    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/authz/role-permission/delete",
    request_body = RolePermissionIdRequest,
    responses((status = 200, body = AuthzAdminSuccessResponse, description = "Permission grant revoked")),
    security(("bearer" = [])),
    tag = "authz-admin"
)]
pub async fn delete_role_permission(
    State(state): State<Arc<AppState>>,
    Json(params): Json<RolePermissionIdRequest>,
) -> ApiResult<Json<AuthzAdminSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    tracing::info!(id = params.id, "DeleteRolePermission");

    let result = role_permission::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("permission grant {}", params.id)).into());
    }

    Ok(Json(AuthzAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

fn validate_min_depth(min_depth: i32) -> Result<(), LocalError> {
    if min_depth < 0 {
        return Err(LocalError::invalid_input("min_depth may not be negative"));
    }
    Ok(())
}
