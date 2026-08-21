//! Admin CRUD for org.unit and org.unit_type.
//!
//! All writes require `org.unit.write`, checked at the root org unit —
//! org tree edits change permission-depth semantics globally, so this is
//! deliberately not regionally delegated. Units and unit types are
//! soft-deleted (deleted_at); deletes are refused while active children or
//! references exist, and the root unit cannot be deleted. Parent changes
//! are checked for cycles.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::org::{unit, unit_type};
use odo_service::admin::{
    Page, Paginated, Sort, clean_code, clean_optional, clean_required, clean_search,
    map_unique_violation,
};
use sea_orm::prelude::*;
use sea_orm::{Condition, Order, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

const READ_PERM: &str = "odo.org.unit.read";
const WRITE_PERM: &str = "odo.org.unit.write";

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct UnitTypeRow {
    pub id: i32,
    pub label: String,
    pub parent: Option<i32>,
    pub parent_label: Option<String>,
    pub can_have_staff: bool,
    pub can_have_patrons: bool,
    /// Active org units of this type.
    pub unit_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnitRow {
    pub id: i32,
    pub label: String,
    pub code: String,
    pub parent: Option<i32>,
    pub unit_type: i32,
    pub unit_type_label: String,
    pub timezone: Option<String>,
}

odo_service::page_type!(UnitTypePage, UnitTypeRow, "One page of org unit types.");

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListUnitTypesRequest {
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgAdminSuccessResponse {
    pub success: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUnitTypeRequest {
    label: String,
    #[serde(default)]
    parent: Option<i32>,
    #[serde(default)]
    can_have_staff: Option<bool>,
    #[serde(default)]
    can_have_patrons: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUnitTypeRequest {
    id: i32,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    parent: Option<i32>,
    #[serde(default)]
    can_have_staff: Option<bool>,
    #[serde(default)]
    can_have_patrons: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UnitTypeIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUnitRequest {
    label: String,
    code: String,
    /// Required: new units always attach below an existing unit.
    parent: i32,
    unit_type: i32,
    #[serde(default)]
    timezone: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUnitRequest {
    id: i32,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    parent: Option<i32>,
    #[serde(default)]
    unit_type: Option<i32>,
    /// Empty string clears the timezone.
    #[serde(default)]
    timezone: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UnitIdRequest {
    id: i32,
}

// ===========================================================================
// Unit types
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit-type/list",
    request_body = ListUnitTypesRequest,
    responses((status = 200, body = UnitTypePage, description = "Active org unit types with usage counts")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn list_unit_types(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListUnitTypesRequest>,
) -> ApiResult<Json<UnitTypePage>> {
    state
        .auth_client
        .permission_required(READ_PERM, None)
        .await?;

    let mut condition = Condition::all().add(unit_type::Column::DeletedAt.is_null());
    if let Some(search) = clean_search(params.search.as_deref()) {
        condition = condition.add(unit_type::Column::Label.contains(&search));
    }

    let total = unit_type::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // parent_label is resolved and unit_count computed post-query, so only
    // real columns sort. Label is unique, so it needs no extra tiebreaker.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("label", unit_type::Column::Label),
            ("staff", unit_type::Column::CanHaveStaff),
            ("patrons", unit_type::Column::CanHavePatrons),
        ],
        (unit_type::Column::Label, Order::Asc),
    );

    let types = unit_type::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(unit_type::Column::Label)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    // Parent labels may point at types outside the current page, so resolve
    // them directly rather than from the page's own rows.
    let mut labels: HashMap<i32, String> = types.iter().map(|t| (t.id, t.label.clone())).collect();
    let parent_ids: Vec<i32> = types
        .iter()
        .filter_map(|t| t.parent)
        .filter(|p| !labels.contains_key(p))
        .collect();
    if !parent_ids.is_empty() {
        for p in unit_type::Entity::find()
            .filter(unit_type::Column::Id.is_in(parent_ids))
            .all(&state.db)
            .await?
        {
            labels.insert(p.id, p.label);
        }
    }

    // Active unit counts for the page's types.
    let mut unit_counts: HashMap<i32, i64> = HashMap::new();
    if !types.is_empty() {
        let type_ids: Vec<i32> = types.iter().map(|t| t.id).collect();
        for u in unit::Entity::find()
            .filter(unit::Column::UnitType.is_in(type_ids))
            .filter(unit::Column::DeletedAt.is_null())
            .all(&state.db)
            .await?
        {
            *unit_counts.entry(u.unit_type).or_insert(0) += 1;
        }
    }

    let rows = types
        .into_iter()
        .map(|t| UnitTypeRow {
            parent_label: t.parent.and_then(|p| labels.get(&p).cloned()),
            unit_count: unit_counts.get(&t.id).copied().unwrap_or(0),
            id: t.id,
            label: t.label,
            parent: t.parent,
            can_have_staff: t.can_have_staff,
            can_have_patrons: t.can_have_patrons,
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit-type/create",
    request_body = CreateUnitTypeRequest,
    responses((status = 200, body = UnitTypeRow, description = "Newly-created org unit type")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn create_unit_type(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateUnitTypeRequest>,
) -> ApiResult<Json<UnitTypeRow>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let label = clean_required(&params.label, "label")?;

    let parent_label = match params.parent {
        Some(parent_id) => Some(find_unit_type(&state.db, parent_id).await?.label),
        None => None,
    };

    tracing::info!(label = %label, "CreateUnitType");

    let mut model = unit_type::ActiveModel {
        label: Set(label),
        parent: Set(params.parent),
        ..Default::default()
    };
    if let Some(v) = params.can_have_staff {
        model.can_have_staff = Set(v);
    }
    if let Some(v) = params.can_have_patrons {
        model.can_have_patrons = Set(v);
    }

    let inserted = model
        .insert(&state.db)
        .await
        .map_err(map_type_label_taken)?;

    Ok(Json(UnitTypeRow {
        parent_label,
        unit_count: 0,
        id: inserted.id,
        label: inserted.label,
        parent: inserted.parent,
        can_have_staff: inserted.can_have_staff,
        can_have_patrons: inserted.can_have_patrons,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit-type/update",
    request_body = UpdateUnitTypeRequest,
    responses((status = 200, body = UnitTypeRow, description = "Updated org unit type")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn update_unit_type(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateUnitTypeRequest>,
) -> ApiResult<Json<UnitTypeRow>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let existing = find_unit_type(&state.db, params.id).await?;
    let type_id = existing.id;

    tracing::info!(id = type_id, "UpdateUnitType");

    let mut model = unit_type::ActiveModel::from(existing);
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if let Some(parent_id) = params.parent {
        if parent_id == type_id {
            return Err(LocalError::invalid_input("a unit type may not be its own parent").into());
        }
        find_unit_type(&state.db, parent_id).await?;
        assert_no_type_cycle(&state.db, type_id, parent_id).await?;
        model.parent = Set(Some(parent_id));
    }
    if let Some(v) = params.can_have_staff {
        model.can_have_staff = Set(v);
    }
    if let Some(v) = params.can_have_patrons {
        model.can_have_patrons = Set(v);
    }

    let updated = model
        .update(&state.db)
        .await
        .map_err(map_type_label_taken)?;

    let parent_label = match updated.parent {
        Some(p) => unit_type::Entity::find_by_id(p)
            .one(&state.db)
            .await?
            .map(|t| t.label),
        None => None,
    };
    let unit_count = unit::Entity::find()
        .filter(unit::Column::UnitType.eq(type_id))
        .filter(unit::Column::DeletedAt.is_null())
        .count(&state.db)
        .await? as i64;

    Ok(Json(UnitTypeRow {
        parent_label,
        unit_count,
        id: updated.id,
        label: updated.label,
        parent: updated.parent,
        can_have_staff: updated.can_have_staff,
        can_have_patrons: updated.can_have_patrons,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit-type/delete",
    request_body = UnitTypeIdRequest,
    responses((status = 200, body = OrgAdminSuccessResponse, description = "Org unit type deactivated")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn delete_unit_type(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UnitTypeIdRequest>,
) -> ApiResult<Json<OrgAdminSuccessResponse>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let existing = find_unit_type(&state.db, params.id).await?;

    let unit_count = unit::Entity::find()
        .filter(unit::Column::UnitType.eq(params.id))
        .filter(unit::Column::DeletedAt.is_null())
        .count(&state.db)
        .await?;
    if unit_count > 0 {
        return Err(LocalError::conflict(
            "TYPE_IN_USE",
            None,
            format!("{unit_count} active org unit(s) use this type."),
        )
        .into());
    }

    let child_count = unit_type::Entity::find()
        .filter(unit_type::Column::Parent.eq(params.id))
        .filter(unit_type::Column::DeletedAt.is_null())
        .count(&state.db)
        .await?;
    if child_count > 0 {
        return Err(LocalError::conflict(
            "TYPE_HAS_CHILDREN",
            None,
            format!("{child_count} unit type(s) list this type as their parent."),
        )
        .into());
    }

    tracing::info!(id = params.id, "DeleteUnitType");

    let mut model = unit_type::ActiveModel::from(existing);
    model.deleted_at = Set(Some(Utc::now().into()));
    model.update(&state.db).await?;

    Ok(Json(OrgAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Units
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit/create",
    request_body = CreateUnitRequest,
    responses((status = 200, body = UnitRow, description = "Newly-created org unit")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn create_unit(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateUnitRequest>,
) -> ApiResult<Json<UnitRow>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let label = clean_required(&params.label, "label")?;
    let code = clean_code(&params.code, "code")?;

    find_unit(&state.db, params.parent).await?;
    let type_row = find_unit_type(&state.db, params.unit_type).await?;

    tracing::info!(label = %label, parent = params.parent, "CreateUnit");

    let model = unit::ActiveModel {
        label: Set(label),
        code: Set(code),
        parent: Set(Some(params.parent)),
        unit_type: Set(params.unit_type),
        timezone: Set(clean_optional(params.timezone.as_deref())),
        ..Default::default()
    };

    let inserted = model.insert(&state.db).await.map_err(map_unit_unique)?;

    Ok(Json(UnitRow {
        unit_type_label: type_row.label,
        id: inserted.id,
        label: inserted.label,
        code: inserted.code,
        parent: inserted.parent,
        unit_type: inserted.unit_type,
        timezone: inserted.timezone,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit/update",
    request_body = UpdateUnitRequest,
    responses((status = 200, body = UnitRow, description = "Updated org unit")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn update_unit(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateUnitRequest>,
) -> ApiResult<Json<UnitRow>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let existing = find_unit(&state.db, params.id).await?;
    let unit_id = existing.id;
    let is_root = existing.parent.is_none();

    tracing::info!(id = unit_id, "UpdateUnit");

    let mut model = unit::ActiveModel::from(existing);
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if let Some(ref code) = params.code {
        model.code = Set(clean_code(code, "code")?);
    }
    if let Some(parent_id) = params.parent {
        if is_root {
            return Err(LocalError::invalid_input("the root org unit may not be moved").into());
        }
        if parent_id == unit_id {
            return Err(LocalError::invalid_input("an org unit may not be its own parent").into());
        }
        find_unit(&state.db, parent_id).await?;
        assert_no_unit_cycle(&state.db, unit_id, parent_id).await?;
        model.parent = Set(Some(parent_id));
    }
    if let Some(type_id) = params.unit_type {
        find_unit_type(&state.db, type_id).await?;
        model.unit_type = Set(type_id);
    }
    if params.timezone.is_some() {
        model.timezone = Set(clean_optional(params.timezone.as_deref()));
    }

    let updated = model.update(&state.db).await.map_err(map_unit_unique)?;

    let unit_type_label = unit_type::Entity::find_by_id(updated.unit_type)
        .one(&state.db)
        .await?
        .map(|t| t.label)
        .unwrap_or_default();

    Ok(Json(UnitRow {
        unit_type_label,
        id: updated.id,
        label: updated.label,
        code: updated.code,
        parent: updated.parent,
        unit_type: updated.unit_type,
        timezone: updated.timezone,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit/delete",
    request_body = UnitIdRequest,
    responses((status = 200, body = OrgAdminSuccessResponse, description = "Org unit deactivated")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn delete_unit(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UnitIdRequest>,
) -> ApiResult<Json<OrgAdminSuccessResponse>> {
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let existing = find_unit(&state.db, params.id).await?;

    if existing.parent.is_none() {
        return Err(LocalError::conflict(
            "UNIT_IS_ROOT",
            None,
            "The root org unit cannot be deleted.",
        )
        .into());
    }

    let child_count = unit::Entity::find()
        .filter(unit::Column::Parent.eq(params.id))
        .filter(unit::Column::DeletedAt.is_null())
        .count(&state.db)
        .await?;
    if child_count > 0 {
        return Err(LocalError::conflict(
            "UNIT_HAS_CHILDREN",
            None,
            format!("{child_count} active child unit(s) are below this unit."),
        )
        .into());
    }

    tracing::info!(id = params.id, "DeleteUnit");

    let mut model = unit::ActiveModel::from(existing);
    model.deleted_at = Set(Some(Utc::now().into()));
    model.update(&state.db).await?;

    Ok(Json(OrgAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

pub(crate) async fn find_unit(db: &DatabaseConnection, id: i32) -> Result<unit::Model, LocalError> {
    unit::Entity::find_by_id(id)
        .filter(unit::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("org unit {id}")))
}

async fn find_unit_type(db: &DatabaseConnection, id: i32) -> Result<unit_type::Model, LocalError> {
    unit_type::Entity::find_by_id(id)
        .filter(unit_type::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("org unit type {id}")))
}

/// Reject a parent change that would make `unit_id` its own ancestor.
async fn assert_no_unit_cycle(
    db: &DatabaseConnection,
    unit_id: i32,
    new_parent: i32,
) -> Result<(), LocalError> {
    let mut current = Some(new_parent);
    let mut hops = 0;
    while let Some(id) = current {
        if id == unit_id {
            return Err(LocalError::invalid_input(
                "parent change would create a cycle in the org tree",
            ));
        }
        hops += 1;
        if hops > 100 {
            return Err(LocalError::internal("org tree too deep or already cyclic"));
        }
        current = unit::Entity::find_by_id(id)
            .one(db)
            .await?
            .and_then(|u| u.parent);
    }
    Ok(())
}

/// Reject a parent change that would make `type_id` its own ancestor.
async fn assert_no_type_cycle(
    db: &DatabaseConnection,
    type_id: i32,
    new_parent: i32,
) -> Result<(), LocalError> {
    let mut current = Some(new_parent);
    let mut hops = 0;
    while let Some(id) = current {
        if id == type_id {
            return Err(LocalError::invalid_input(
                "parent change would create a cycle in the unit type hierarchy",
            ));
        }
        hops += 1;
        if hops > 100 {
            return Err(LocalError::internal(
                "unit type hierarchy too deep or already cyclic",
            ));
        }
        current = unit_type::Entity::find_by_id(id)
            .one(db)
            .await?
            .and_then(|t| t.parent);
    }
    Ok(())
}

fn map_type_label_taken(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "TYPE_LABEL_TAKEN",
        Some("label"),
        "A unit type with this label already exists.",
    )
}

/// org.unit has separate unique constraints on code and label.
fn map_unit_unique(e: DbErr) -> LocalError {
    if let Some(SqlErr::UniqueConstraintViolation(detail)) = e.sql_err() {
        if detail.contains("unit_code_key") {
            return LocalError::conflict(
                "UNIT_CODE_TAKEN",
                Some("code"),
                "An org unit with this code already exists.",
            );
        }
        return LocalError::conflict(
            "UNIT_LABEL_TAKEN",
            Some("label"),
            "An org unit with this label already exists.",
        );
    }
    LocalError::internal(e.to_string())
}
