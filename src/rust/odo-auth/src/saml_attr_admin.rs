//! Admin CRUD for auth.saml_idp_attribute and authz.saml_attr_role_map.
//!
//! Attributes define which SAML assertion attributes the system tracks per
//! IdP (optionally normalized DB-side by auth.normalize_saml_attr_value);
//! attr-role mappings drive automatic role assignment from attribute
//! values. Reads require `auth.saml.read`; writes require `auth.saml.write`.
//!
//! Although saml_attr_role_map.attr cascades on delete, attribute deletion
//! is refused while mappings reference it — silently dropping role mappings
//! would be surprising.

use axum::Json;
use axum::extract::State;
use odo_service::admin::{Page, Paginated, Sort, clean_required, map_unique_violation};
use odo_entity::auth::{saml_idp_attribute, saml_idp_config};
use odo_entity::authz::{role, saml_attr_role_map};
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::{Condition, Order, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::authz_admin::require_perm;

const READ_PERM: &str = "odo.auth.saml.read";
const WRITE_PERM: &str = "odo.auth.saml.write";

/// Normalizers understood by auth.normalize_saml_attr_value().
const NORMALIZERS: &[&str] = &["split_slash_first", "split_slash_last"];

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct AttributeRow {
    pub id: i32,
    pub idp: i32,
    pub idp_name: String,
    pub key: String,
    pub label: String,
    pub is_location: bool,
    pub normalizer: Option<String>,
    /// Role mappings referencing this attribute.
    pub mapping_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttrRoleMapRow {
    pub id: i32,
    pub attr: i32,
    pub attr_key: String,
    pub idp_name: String,
    pub attr_value: String,
    pub role: String,
    pub role_label: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListAttributesResponse {
    pub attributes: Vec<AttributeRow>,
}

odo_service::page_type!(
    AttrRoleMapPage,
    AttrRoleMapRow,
    "One page of SAML attribute-to-role mappings."
);

#[derive(Deserialize, ToSchema)]
pub struct ListAttrRoleMapsRequest {
    /// Restrict to mappings whose attribute belongs to this IdP.
    #[serde(default)]
    idp: Option<i32>,
    /// Restrict to a single attribute.
    #[serde(default)]
    attr: Option<i32>,
    /// Restrict to a single role code.
    #[serde(default)]
    role: Option<String>,
    /// Case-insensitive substring match on the attribute value.
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SamlAttrSuccessResponse {
    pub success: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttributeRequest {
    idp: i32,
    key: String,
    label: String,
    #[serde(default)]
    is_location: Option<bool>,
    #[serde(default)]
    normalizer: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateAttributeRequest {
    id: i32,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    is_location: Option<bool>,
    /// Empty string clears the normalizer.
    #[serde(default)]
    normalizer: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AttributeIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttrRoleMapRequest {
    attr: i32,
    role: String,
    attr_value: String,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateAttrRoleMapRequest {
    id: i32,
    #[serde(default)]
    attr: Option<i32>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    attr_value: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct AttrRoleMapIdRequest {
    id: i32,
}

// ===========================================================================
// IdP attributes
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attribute/list",
    responses((status = 200, body = ListAttributesResponse, description = "All tracked SAML attributes")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn list_attributes(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<ListAttributesResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let attributes = saml_idp_attribute::Entity::find()
        .order_by_asc(saml_idp_attribute::Column::Idp)
        .order_by_asc(saml_idp_attribute::Column::Key)
        .all(&state.db)
        .await?;

    let idp_names = idp_name_map(&state.db).await?;

    let mut mapping_counts: HashMap<i32, i64> = HashMap::new();
    for mapping in saml_attr_role_map::Entity::find().all(&state.db).await? {
        *mapping_counts.entry(mapping.attr).or_insert(0) += 1;
    }

    let attributes = attributes
        .into_iter()
        .map(|a| AttributeRow {
            idp_name: idp_names.get(&a.idp).cloned().unwrap_or_default(),
            mapping_count: mapping_counts.get(&a.id).copied().unwrap_or(0),
            id: a.id,
            idp: a.idp,
            key: a.key,
            label: a.label,
            is_location: a.is_location,
            normalizer: a.normalizer,
        })
        .collect();

    Ok(Json(ListAttributesResponse { attributes }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attribute/create",
    request_body = CreateAttributeRequest,
    responses((status = 200, body = AttributeRow, description = "Newly-created SAML attribute")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn create_attribute(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateAttributeRequest>,
) -> ApiResult<Json<AttributeRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let key = clean_required(&params.key, "key")?;
    let label = clean_required(&params.label, "label")?;
    let normalizer = clean_normalizer(params.normalizer.as_deref())?;

    let idp = saml_idp_config::Entity::find_by_id(params.idp)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("SAML IdP config {}", params.idp)))?;

    tracing::info!(idp = params.idp, key = %key, "CreateSamlAttribute");

    let mut model = saml_idp_attribute::ActiveModel {
        idp: Set(params.idp),
        key: Set(key),
        label: Set(label),
        normalizer: Set(normalizer),
        ..Default::default()
    };
    if let Some(is_location) = params.is_location {
        model.is_location = Set(is_location);
    }

    let inserted = model.insert(&state.db).await.map_err(map_attr_exists)?;

    Ok(Json(AttributeRow {
        idp_name: idp.name,
        mapping_count: 0,
        id: inserted.id,
        idp: inserted.idp,
        key: inserted.key,
        label: inserted.label,
        is_location: inserted.is_location,
        normalizer: inserted.normalizer,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attribute/update",
    request_body = UpdateAttributeRequest,
    responses((status = 200, body = AttributeRow, description = "Updated SAML attribute")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn update_attribute(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateAttributeRequest>,
) -> ApiResult<Json<AttributeRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = find_attribute(&state.db, params.id).await?;
    let attr_id = existing.id;

    tracing::info!(id = attr_id, "UpdateSamlAttribute");

    let mut model = saml_idp_attribute::ActiveModel::from(existing);
    if let Some(ref key) = params.key {
        model.key = Set(clean_required(key, "key")?);
    }
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if let Some(is_location) = params.is_location {
        model.is_location = Set(is_location);
    }
    if params.normalizer.is_some() {
        model.normalizer = Set(clean_normalizer(params.normalizer.as_deref())?);
    }

    let updated = model.update(&state.db).await.map_err(map_attr_exists)?;

    let idp_names = idp_name_map(&state.db).await?;
    let mapping_count = saml_attr_role_map::Entity::find()
        .filter(saml_attr_role_map::Column::Attr.eq(attr_id))
        .count(&state.db)
        .await? as i64;

    Ok(Json(AttributeRow {
        idp_name: idp_names.get(&updated.idp).cloned().unwrap_or_default(),
        mapping_count,
        id: updated.id,
        idp: updated.idp,
        key: updated.key,
        label: updated.label,
        is_location: updated.is_location,
        normalizer: updated.normalizer,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attribute/delete",
    request_body = AttributeIdRequest,
    responses((status = 200, body = SamlAttrSuccessResponse, description = "SAML attribute deleted")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn delete_attribute(
    State(state): State<Arc<AppState>>,
    Json(params): Json<AttributeIdRequest>,
) -> ApiResult<Json<SamlAttrSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    find_attribute(&state.db, params.id).await?;

    let mapping_count = saml_attr_role_map::Entity::find()
        .filter(saml_attr_role_map::Column::Attr.eq(params.id))
        .count(&state.db)
        .await?;
    if mapping_count > 0 {
        return Err(LocalError::conflict(
            "ATTRIBUTE_IN_USE",
            None,
            format!("{mapping_count} role mapping(s) reference this attribute; remove them first."),
        )
        .into());
    }

    tracing::info!(id = params.id, "DeleteSamlAttribute");

    saml_idp_attribute::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await
        .map_err(|e| {
            if let Some(SqlErr::ForeignKeyConstraintViolation(_)) = e.sql_err() {
                return LocalError::conflict(
                    "ATTRIBUTE_HAS_VALUES",
                    None,
                    "Captured user attribute values reference this attribute.",
                );
            }
            LocalError::internal(e.to_string())
        })?;

    Ok(Json(SamlAttrSuccessResponse { success: true }))
}

// ===========================================================================
// Attribute role mappings
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attr-role-map/list",
    request_body = ListAttrRoleMapsRequest,
    responses((status = 200, body = AttrRoleMapPage, description = "Filtered SAML attribute-to-role mappings")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn list_attr_role_maps(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListAttrRoleMapsRequest>,
) -> ApiResult<Json<AttrRoleMapPage>> {
    require_perm(&state, READ_PERM, None).await?;

    let mut condition = Condition::all();

    // `idp` filters through the attribute; resolve its attribute ids first.
    // No attributes for the IdP means no mappings — short-circuit.
    if let Some(idp) = params.idp {
        let attr_ids: Vec<i32> = saml_idp_attribute::Entity::find()
            .filter(saml_idp_attribute::Column::Idp.eq(idp))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|a| a.id)
            .collect();
        if attr_ids.is_empty() {
            return Ok(Json(Paginated::new(vec![], 0).into()));
        }
        condition = condition.add(saml_attr_role_map::Column::Attr.is_in(attr_ids));
    }

    if let Some(attr) = params.attr {
        condition = condition.add(saml_attr_role_map::Column::Attr.eq(attr));
    }
    if let Some(ref role) = params.role {
        condition = condition.add(saml_attr_role_map::Column::Role.eq(role));
    }
    if let Some(ref search) = params.search {
        let needle = search.trim();
        if !needle.is_empty() {
            // Case-insensitive substring; escape LIKE wildcards so a literal
            // % or _ in the search isn't treated as a pattern.
            let escaped = needle
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            condition = condition
                .add(saml_attr_role_map::Column::AttrValue.ilike(format!("%{escaped}%")));
        }
    }

    let total = saml_attr_role_map::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // attr_key/idp_name/role_label are decorated post-query, so only the base
    // mapping's own columns are sortable.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("role", saml_attr_role_map::Column::Role),
            ("value", saml_attr_role_map::Column::AttrValue),
            ("active", saml_attr_role_map::Column::IsActive),
        ],
        (saml_attr_role_map::Column::Role, Order::Asc),
    );

    let mappings = saml_attr_role_map::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(saml_attr_role_map::Column::AttrValue)
        .order_by_asc(saml_attr_role_map::Column::Id)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    let rows = decorate_mappings(&state.db, mappings).await?;

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attr-role-map/create",
    request_body = CreateAttrRoleMapRequest,
    responses((status = 200, body = AttrRoleMapRow, description = "Newly-created attribute role mapping")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn create_attr_role_map(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateAttrRoleMapRequest>,
) -> ApiResult<Json<AttrRoleMapRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let attr_value = clean_required(&params.attr_value, "attr_value")?;

    find_attribute(&state.db, params.attr).await?;
    role::Entity::find_by_id(&params.role)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("role {}", params.role)))?;

    tracing::info!(attr = params.attr, role = %params.role, "CreateSamlAttrRoleMap");

    let mut model = saml_attr_role_map::ActiveModel {
        attr: Set(params.attr),
        role: Set(params.role),
        attr_value: Set(attr_value),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let inserted = model.insert(&state.db).await.map_err(map_mapping_exists)?;

    let rows = decorate_mappings(&state.db, vec![inserted]).await?;
    Ok(Json(rows.into_iter().next().expect("row decorated")))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attr-role-map/update",
    request_body = UpdateAttrRoleMapRequest,
    responses((status = 200, body = AttrRoleMapRow, description = "Updated attribute role mapping")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn update_attr_role_map(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateAttrRoleMapRequest>,
) -> ApiResult<Json<AttrRoleMapRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = saml_attr_role_map::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("attribute role mapping {}", params.id)))?;

    tracing::info!(id = params.id, "UpdateSamlAttrRoleMap");

    let mut model = saml_attr_role_map::ActiveModel::from(existing);
    if let Some(attr) = params.attr {
        find_attribute(&state.db, attr).await?;
        model.attr = Set(attr);
    }
    if let Some(ref role_code) = params.role {
        role::Entity::find_by_id(role_code)
            .one(&state.db)
            .await?
            .ok_or_else(|| LocalError::not_found(format!("role {role_code}")))?;
        model.role = Set(role_code.clone());
    }
    if let Some(ref attr_value) = params.attr_value {
        model.attr_value = Set(clean_required(attr_value, "attr_value")?);
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let updated = model.update(&state.db).await.map_err(map_mapping_exists)?;

    let rows = decorate_mappings(&state.db, vec![updated]).await?;
    Ok(Json(rows.into_iter().next().expect("row decorated")))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/attr-role-map/delete",
    request_body = AttrRoleMapIdRequest,
    responses((status = 200, body = SamlAttrSuccessResponse, description = "Attribute role mapping deleted")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn delete_attr_role_map(
    State(state): State<Arc<AppState>>,
    Json(params): Json<AttrRoleMapIdRequest>,
) -> ApiResult<Json<SamlAttrSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    tracing::info!(id = params.id, "DeleteSamlAttrRoleMap");

    let result = saml_attr_role_map::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(
            LocalError::not_found(format!("attribute role mapping {}", params.id)).into(),
        );
    }

    Ok(Json(SamlAttrSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_attribute(
    db: &DatabaseConnection,
    id: i32,
) -> Result<saml_idp_attribute::Model, LocalError> {
    saml_idp_attribute::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("SAML attribute {id}")))
}

async fn idp_name_map(db: &DatabaseConnection) -> Result<HashMap<i32, String>, LocalError> {
    let mut names = HashMap::new();
    for idp in saml_idp_config::Entity::find().all(db).await? {
        names.insert(idp.id, idp.name);
    }
    Ok(names)
}

/// Attach attribute key, IdP name, and role label to raw mapping rows.
async fn decorate_mappings(
    db: &DatabaseConnection,
    mappings: Vec<saml_attr_role_map::Model>,
) -> Result<Vec<AttrRoleMapRow>, LocalError> {
    if mappings.is_empty() {
        return Ok(vec![]);
    }

    let idp_names = idp_name_map(db).await?;

    let mut attrs: HashMap<i32, saml_idp_attribute::Model> = HashMap::new();
    for a in saml_idp_attribute::Entity::find().all(db).await? {
        attrs.insert(a.id, a);
    }

    let mut role_labels: HashMap<String, String> = HashMap::new();
    for r in role::Entity::find().all(db).await? {
        role_labels.insert(r.code, r.label);
    }

    Ok(mappings
        .into_iter()
        .map(|m| {
            let attr = attrs.get(&m.attr);
            AttrRoleMapRow {
                attr_key: attr.map(|a| a.key.clone()).unwrap_or_default(),
                idp_name: attr
                    .and_then(|a| idp_names.get(&a.idp).cloned())
                    .unwrap_or_default(),
                role_label: role_labels.get(&m.role).cloned().unwrap_or_default(),
                id: m.id,
                attr: m.attr,
                attr_value: m.attr_value,
                role: m.role,
                is_active: m.is_active,
            }
        })
        .collect())
}

/// Empty/missing clears the normalizer; otherwise it must be one the
/// normalize_saml_attr_value() DB function understands.
fn clean_normalizer(value: Option<&str>) -> Result<Option<String>, LocalError> {
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    match value {
        None => Ok(None),
        Some(v) if NORMALIZERS.contains(&v) => Ok(Some(v.to_string())),
        Some(v) => Err(LocalError::invalid_input(format!(
            "Unknown normalizer '{v}'; expected one of: {}",
            NORMALIZERS.join(", ")
        ))),
    }
}

fn map_attr_exists(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "ATTRIBUTE_EXISTS",
        None,
        "This IdP already tracks this attribute key with the same normalizer.",
    )
}

fn map_mapping_exists(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "MAPPING_EXISTS",
        None,
        "This attribute value is already mapped to this role.",
    )
}
