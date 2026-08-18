//! Admin CRUD for an org unit's child data: addresses, closures, and
//! operating hours (org.address / org.closure / org.operating_hours).
//!
//! All writes require `org.unit.write`, checked at the root org unit like
//! the rest of org admin. Addresses are soft-deleted (deleted_at) to match
//! the existing read filter; closures and operating hours carry no inbound
//! references and are hard-deleted.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_service::admin::{clean_optional, clean_required, map_unique_violation};
use odo_client::context::RequestContext;
use odo_entity::org::{address, closure, operating_hours};
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::{QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::admin::find_unit;

const READ_PERM: &str = "odo.org.unit.read";
const WRITE_PERM: &str = "odo.org.unit.write";
const ADDRESS_TYPES: &[&str] = &["physical", "mailing"];

async fn require_read(state: &AppState) -> Result<(), LocalError> {
    state.auth_client.permission_required(READ_PERM, None).await
}

async fn require_write(state: &AppState) -> Result<(), LocalError> {
    state.auth_client.permission_required(WRITE_PERM, None).await
}

// ===========================================================================
// Rows
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct AddressRow {
    pub id: i32,
    pub org_unit: i32,
    pub address_type: String,
    pub label: String,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub state_province: Option<String>,
    pub postal_code: Option<String>,
}

impl From<address::Model> for AddressRow {
    fn from(m: address::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            address_type: m.address_type,
            label: m.label,
            address_line1: m.address_line1,
            address_line2: m.address_line2,
            city: m.city,
            state_province: m.state_province,
            postal_code: m.postal_code,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClosureRow {
    pub id: i32,
    pub org_unit: i32,
    pub closure_date: chrono::NaiveDate,
    pub reason: String,
    pub is_emergency: bool,
}

impl From<closure::Model> for ClosureRow {
    fn from(m: closure::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            closure_date: m.closure_date,
            reason: m.reason,
            is_emergency: m.is_emergency.unwrap_or(false),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OperatingHoursRow {
    pub id: i32,
    pub org_unit: i32,
    pub day_of_week: i32,
    pub open_time: chrono::NaiveTime,
    pub close_time: chrono::NaiveTime,
    pub is_closed: bool,
}

impl From<operating_hours::Model> for OperatingHoursRow {
    fn from(m: operating_hours::Model) -> Self {
        Self {
            id: m.id,
            org_unit: m.org_unit,
            day_of_week: m.day_of_week,
            open_time: m.open_time,
            close_time: m.close_time,
            is_closed: m.is_closed.unwrap_or(false),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgChildSuccessResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgUnitChildrenResponse {
    pub addresses: Vec<AddressRow>,
    pub closures: Vec<ClosureRow>,
    pub operating_hours: Vec<OperatingHoursRow>,
}

#[derive(Deserialize, ToSchema)]
pub struct OrgUnitChildrenRequest {
    org_unit: i32,
}

// ===========================================================================
// Requests
// ===========================================================================

#[derive(Deserialize, ToSchema)]
pub struct CreateAddressRequest {
    org_unit: i32,
    address_type: String,
    label: String,
    #[serde(default)]
    address_line1: Option<String>,
    #[serde(default)]
    address_line2: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    state_province: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateAddressRequest {
    id: i32,
    #[serde(default)]
    address_type: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    address_line1: Option<String>,
    #[serde(default)]
    address_line2: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    state_province: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AddressIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateClosureRequest {
    org_unit: i32,
    closure_date: chrono::NaiveDate,
    reason: String,
    #[serde(default)]
    is_emergency: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateClosureRequest {
    id: i32,
    #[serde(default)]
    closure_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    is_emergency: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct ClosureIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateOperatingHoursRequest {
    org_unit: i32,
    day_of_week: i32,
    open_time: chrono::NaiveTime,
    close_time: chrono::NaiveTime,
    #[serde(default)]
    is_closed: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateOperatingHoursRequest {
    id: i32,
    #[serde(default)]
    day_of_week: Option<i32>,
    #[serde(default)]
    open_time: Option<chrono::NaiveTime>,
    #[serde(default)]
    close_time: Option<chrono::NaiveTime>,
    #[serde(default)]
    is_closed: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct OperatingHoursIdRequest {
    id: i32,
}

// ===========================================================================
// Combined read (admin editor view)
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/unit-children",
    request_body = OrgUnitChildrenRequest,
    responses((status = 200, body = OrgUnitChildrenResponse, description = "All addresses, closures, and operating hours for one org unit")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn list_unit_children(
    State(state): State<Arc<AppState>>,
    Json(params): Json<OrgUnitChildrenRequest>,
) -> ApiResult<Json<OrgUnitChildrenResponse>> {
    // Read-only admin view: returns all closures (including past ones),
    // unlike the public org_unit_detail endpoint.
    require_read(&state).await?;

    find_unit(&state.db, params.org_unit).await?;

    let addresses = address::Entity::find()
        .filter(address::Column::OrgUnit.eq(params.org_unit))
        .filter(address::Column::DeletedAt.is_null())
        .order_by_asc(address::Column::AddressType)
        .order_by_asc(address::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(AddressRow::from)
        .collect();

    let closures = closure::Entity::find()
        .filter(closure::Column::OrgUnit.eq(params.org_unit))
        .order_by_asc(closure::Column::ClosureDate)
        .order_by_asc(closure::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(ClosureRow::from)
        .collect();

    let operating_hours = operating_hours::Entity::find()
        .filter(operating_hours::Column::OrgUnit.eq(params.org_unit))
        .order_by_asc(operating_hours::Column::DayOfWeek)
        .order_by_asc(operating_hours::Column::OpenTime)
        .all(&state.db)
        .await?
        .into_iter()
        .map(OperatingHoursRow::from)
        .collect();

    Ok(Json(OrgUnitChildrenResponse {
        addresses,
        closures,
        operating_hours,
    }))
}

// ===========================================================================
// Addresses
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/address/create",
    request_body = CreateAddressRequest,
    responses((status = 200, body = AddressRow, description = "Newly-created address")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn create_address(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateAddressRequest>,
) -> ApiResult<Json<AddressRow>> {
    require_write(&state).await?;

    find_unit(&state.db, params.org_unit).await?;
    let address_type = clean_address_type(&params.address_type)?;
    let label = clean_required(&params.label, "label")?;

    tracing::info!(org_unit = params.org_unit, "CreateAddress");

    let model = address::ActiveModel {
        org_unit: Set(params.org_unit),
        address_type: Set(address_type),
        label: Set(label),
        address_line1: Set(clean_optional(params.address_line1.as_deref())),
        address_line2: Set(clean_optional(params.address_line2.as_deref())),
        city: Set(clean_optional(params.city.as_deref())),
        state_province: Set(clean_optional(params.state_province.as_deref())),
        postal_code: Set(clean_optional(params.postal_code.as_deref())),
        ..Default::default()
    };

    let inserted = model.insert(&state.db).await.map_err(map_address_label)?;
    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/address/update",
    request_body = UpdateAddressRequest,
    responses((status = 200, body = AddressRow, description = "Updated address")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn update_address(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateAddressRequest>,
) -> ApiResult<Json<AddressRow>> {
    require_write(&state).await?;

    let existing = find_address(&state.db, params.id).await?;

    tracing::info!(id = params.id, "UpdateAddress");

    let mut model = address::ActiveModel::from(existing);
    if let Some(ref t) = params.address_type {
        model.address_type = Set(clean_address_type(t)?);
    }
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if params.address_line1.is_some() {
        model.address_line1 = Set(clean_optional(params.address_line1.as_deref()));
    }
    if params.address_line2.is_some() {
        model.address_line2 = Set(clean_optional(params.address_line2.as_deref()));
    }
    if params.city.is_some() {
        model.city = Set(clean_optional(params.city.as_deref()));
    }
    if params.state_province.is_some() {
        model.state_province = Set(clean_optional(params.state_province.as_deref()));
    }
    if params.postal_code.is_some() {
        model.postal_code = Set(clean_optional(params.postal_code.as_deref()));
    }

    let updated = model.update(&state.db).await.map_err(map_address_label)?;
    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/address/delete",
    request_body = AddressIdRequest,
    responses((status = 200, body = OrgChildSuccessResponse, description = "Address deleted")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn delete_address(
    State(state): State<Arc<AppState>>,
    Json(params): Json<AddressIdRequest>,
) -> ApiResult<Json<OrgChildSuccessResponse>> {
    require_write(&state).await?;

    let existing = find_address(&state.db, params.id).await?;

    tracing::info!(id = params.id, "DeleteAddress");

    let mut model = address::ActiveModel::from(existing);
    model.deleted_at = Set(Some(Utc::now().into()));
    model.update(&state.db).await?;

    Ok(Json(OrgChildSuccessResponse { success: true }))
}

// ===========================================================================
// Closures
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/closure/create",
    request_body = CreateClosureRequest,
    responses((status = 200, body = ClosureRow, description = "Newly-created closure")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn create_closure(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateClosureRequest>,
) -> ApiResult<Json<ClosureRow>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;
    require_write(&state).await?;

    find_unit(&state.db, params.org_unit).await?;
    let reason = clean_required(&params.reason, "reason")?;

    tracing::info!(org_unit = params.org_unit, "CreateClosure");

    let mut model = closure::ActiveModel {
        org_unit: Set(params.org_unit),
        closure_date: Set(params.closure_date),
        reason: Set(reason),
        created_by: Set(Some(user_id)),
        ..Default::default()
    };
    if let Some(v) = params.is_emergency {
        model.is_emergency = Set(Some(v));
    }

    let inserted = model.insert(&state.db).await?;
    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/closure/update",
    request_body = UpdateClosureRequest,
    responses((status = 200, body = ClosureRow, description = "Updated closure")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn update_closure(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateClosureRequest>,
) -> ApiResult<Json<ClosureRow>> {
    require_write(&state).await?;

    let existing = closure::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("closure {}", params.id)))?;

    tracing::info!(id = params.id, "UpdateClosure");

    let mut model = closure::ActiveModel::from(existing);
    if let Some(d) = params.closure_date {
        model.closure_date = Set(d);
    }
    if let Some(ref reason) = params.reason {
        model.reason = Set(clean_required(reason, "reason")?);
    }
    if let Some(v) = params.is_emergency {
        model.is_emergency = Set(Some(v));
    }

    let updated = model.update(&state.db).await?;
    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/closure/delete",
    request_body = ClosureIdRequest,
    responses((status = 200, body = OrgChildSuccessResponse, description = "Closure deleted")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn delete_closure(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ClosureIdRequest>,
) -> ApiResult<Json<OrgChildSuccessResponse>> {
    require_write(&state).await?;

    tracing::info!(id = params.id, "DeleteClosure");

    let result = closure::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("closure {}", params.id)).into());
    }

    Ok(Json(OrgChildSuccessResponse { success: true }))
}

// ===========================================================================
// Operating hours
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/operating-hours/create",
    request_body = CreateOperatingHoursRequest,
    responses((status = 200, body = OperatingHoursRow, description = "Newly-created operating hours row")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn create_operating_hours(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateOperatingHoursRequest>,
) -> ApiResult<Json<OperatingHoursRow>> {
    require_write(&state).await?;

    find_unit(&state.db, params.org_unit).await?;
    let is_closed = params.is_closed.unwrap_or(false);
    validate_hours(params.day_of_week, params.open_time, params.close_time, is_closed)?;

    tracing::info!(org_unit = params.org_unit, "CreateOperatingHours");

    let model = operating_hours::ActiveModel {
        org_unit: Set(params.org_unit),
        day_of_week: Set(params.day_of_week),
        open_time: Set(params.open_time),
        close_time: Set(params.close_time),
        is_closed: Set(Some(is_closed)),
        ..Default::default()
    };

    let inserted = model.insert(&state.db).await?;
    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/operating-hours/update",
    request_body = UpdateOperatingHoursRequest,
    responses((status = 200, body = OperatingHoursRow, description = "Updated operating hours row")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn update_operating_hours(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateOperatingHoursRequest>,
) -> ApiResult<Json<OperatingHoursRow>> {
    require_write(&state).await?;

    let existing = operating_hours::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("operating hours {}", params.id)))?;

    // Validate the resulting row (new values falling back to existing).
    let day = params.day_of_week.unwrap_or(existing.day_of_week);
    let open = params.open_time.unwrap_or(existing.open_time);
    let close = params.close_time.unwrap_or(existing.close_time);
    let is_closed = params
        .is_closed
        .unwrap_or_else(|| existing.is_closed.unwrap_or(false));
    validate_hours(day, open, close, is_closed)?;

    tracing::info!(id = params.id, "UpdateOperatingHours");

    let mut model = operating_hours::ActiveModel::from(existing);
    model.day_of_week = Set(day);
    model.open_time = Set(open);
    model.close_time = Set(close);
    model.is_closed = Set(Some(is_closed));

    let updated = model.update(&state.db).await?;
    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/org/admin/operating-hours/delete",
    request_body = OperatingHoursIdRequest,
    responses((status = 200, body = OrgChildSuccessResponse, description = "Operating hours row deleted")),
    security(("bearer" = [])),
    tag = "org-admin"
)]
pub async fn delete_operating_hours(
    State(state): State<Arc<AppState>>,
    Json(params): Json<OperatingHoursIdRequest>,
) -> ApiResult<Json<OrgChildSuccessResponse>> {
    require_write(&state).await?;

    tracing::info!(id = params.id, "DeleteOperatingHours");

    let result = operating_hours::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("operating hours {}", params.id)).into());
    }

    Ok(Json(OrgChildSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_address(
    db: &DatabaseConnection,
    id: i32,
) -> Result<address::Model, LocalError> {
    address::Entity::find_by_id(id)
        .filter(address::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("address {id}")))
}

fn clean_address_type(value: &str) -> Result<String, LocalError> {
    let value = value.trim().to_lowercase();
    if ADDRESS_TYPES.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(LocalError::invalid_input(format!(
            "address_type must be one of: {}",
            ADDRESS_TYPES.join(", ")
        )))
    }
}

/// Mirror the DB CHECK constraints so bad input is a clean 400.
fn validate_hours(
    day_of_week: i32,
    open: chrono::NaiveTime,
    close: chrono::NaiveTime,
    is_closed: bool,
) -> Result<(), LocalError> {
    if !(0..=6).contains(&day_of_week) {
        return Err(LocalError::invalid_input(
            "day_of_week must be between 0 (Sunday) and 6 (Saturday)",
        ));
    }
    if !is_closed && close <= open {
        return Err(LocalError::invalid_input(
            "close_time must be after open_time unless the day is marked closed",
        ));
    }
    Ok(())
}

fn map_address_label(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "ADDRESS_LABEL_TAKEN",
        Some("label"),
        "An address with this label already exists.",
    )
}
