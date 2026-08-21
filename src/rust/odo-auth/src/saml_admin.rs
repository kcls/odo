//! Admin CRUD for auth.saml_idp_config and auth.saml_sp_config.
//!
//! Reads require `auth.saml.read`; writes require `auth.saml.write`.
//! The SP `private_key` is write-only: accepted on create/update, never
//! returned (responses expose `has_private_key` instead). Deletes are
//! refused with a 409 while other rows reference the target.
//!
//! Routes live under /api/v1/odo/auth/saml/admin/* — the /saml PathPrefix
//! is already routed by envoy; authentication is enforced service-side by
//! the require_auth middleware plus these permission checks.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::auth::{saml_idp_config, saml_sp_config, usr_saml_identities};
use odo_service::admin::{clean_optional, clean_required, map_unique_violation};
use sea_orm::prelude::*;
use sea_orm::{QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::authz_admin::require_perm;

const READ_PERM: &str = "odo.auth.saml.read";
const WRITE_PERM: &str = "odo.auth.saml.write";

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct IdpRow {
    pub id: i32,
    pub name: String,
    pub entity_id: String,
    pub sso_url: Option<String>,
    pub slo_url: Option<String>,
    pub metadata_url: Option<String>,
    pub is_active: bool,
    pub session_lifetime_hours: Option<i32>,
    pub allow_idp_initiated: bool,
    pub attribute_mapping: Option<serde_json::Value>,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// Service providers configured against this IdP.
    pub sp_count: i64,
    /// Users with a SAML identity from this IdP.
    pub user_count: i64,
}

impl IdpRow {
    fn new(m: saml_idp_config::Model, sp_count: i64, user_count: i64) -> Self {
        Self {
            id: m.id,
            name: m.name,
            entity_id: m.entity_id,
            sso_url: m.sso_url,
            slo_url: m.slo_url,
            metadata_url: m.metadata_url,
            is_active: m.is_active.unwrap_or(true),
            session_lifetime_hours: m.session_lifetime_hours,
            allow_idp_initiated: m.allow_idp_initiated.unwrap_or(false),
            attribute_mapping: m.attribute_mapping,
            created_at: m.created_at,
            updated_at: m.updated_at,
            sp_count,
            user_count,
        }
    }
}

/// SP row for API responses. Never carries the private key.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpRow {
    pub id: i32,
    pub entity_id: String,
    pub label: Option<String>,
    pub acs_url: String,
    pub slo_url: Option<String>,
    pub metadata_url: Option<String>,
    pub callback_url: Option<String>,
    pub idp: Option<i32>,
    pub idp_name: Option<String>,
    pub is_active: bool,
    pub x509_cert: String,
    pub idp_x509_cert: Option<String>,
    pub has_private_key: bool,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl SpRow {
    fn new(m: saml_sp_config::Model, idp_name: Option<String>) -> Self {
        Self {
            id: m.id,
            entity_id: m.entity_id,
            label: m.label,
            acs_url: m.acs_url,
            slo_url: m.slo_url,
            metadata_url: m.metadata_url,
            callback_url: m.callback_url,
            idp: m.idp,
            idp_name,
            is_active: m.is_active,
            x509_cert: m.x509_cert,
            idp_x509_cert: m.idp_x509_cert,
            has_private_key: !m.private_key.is_empty(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListIdpsResponse {
    pub idps: Vec<IdpRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListSpsResponse {
    pub sps: Vec<SpRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SamlAdminSuccessResponse {
    pub success: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateIdpRequest {
    name: String,
    entity_id: String,
    #[serde(default)]
    sso_url: Option<String>,
    #[serde(default)]
    slo_url: Option<String>,
    #[serde(default)]
    metadata_url: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    session_lifetime_hours: Option<i32>,
    #[serde(default)]
    allow_idp_initiated: Option<bool>,
    #[serde(default)]
    attribute_mapping: Option<serde_json::Value>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateIdpRequest {
    id: i32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    sso_url: Option<String>,
    #[serde(default)]
    slo_url: Option<String>,
    #[serde(default)]
    metadata_url: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    session_lifetime_hours: Option<i32>,
    #[serde(default)]
    allow_idp_initiated: Option<bool>,
    #[serde(default)]
    attribute_mapping: Option<serde_json::Value>,
}

#[derive(Deserialize, ToSchema)]
pub struct IdpIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateSpRequest {
    entity_id: String,
    acs_url: String,
    x509_cert: String,
    private_key: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    slo_url: Option<String>,
    #[serde(default)]
    metadata_url: Option<String>,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    idp: Option<i32>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    idp_x509_cert: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateSpRequest {
    id: i32,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    acs_url: Option<String>,
    /// Empty or absent leaves the existing key unchanged.
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    x509_cert: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    slo_url: Option<String>,
    #[serde(default)]
    metadata_url: Option<String>,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    idp: Option<i32>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    idp_x509_cert: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SpIdRequest {
    id: i32,
}

// ===========================================================================
// Identity providers
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/idp/list",
    responses((status = 200, body = ListIdpsResponse, description = "All SAML identity provider configs")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn list_idps(State(state): State<Arc<AppState>>) -> ApiResult<Json<ListIdpsResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let idps = saml_idp_config::Entity::find()
        .order_by_asc(saml_idp_config::Column::Name)
        .all(&state.db)
        .await?;

    let mut sp_counts: HashMap<i32, i64> = HashMap::new();
    for sp in saml_sp_config::Entity::find().all(&state.db).await? {
        if let Some(idp) = sp.idp {
            *sp_counts.entry(idp).or_insert(0) += 1;
        }
    }

    let mut user_counts: HashMap<i32, i64> = HashMap::new();
    for identity in usr_saml_identities::Entity::find().all(&state.db).await? {
        *user_counts.entry(identity.idp_id).or_insert(0) += 1;
    }

    let idps = idps
        .into_iter()
        .map(|m| {
            let sp_count = sp_counts.get(&m.id).copied().unwrap_or(0);
            let user_count = user_counts.get(&m.id).copied().unwrap_or(0);
            IdpRow::new(m, sp_count, user_count)
        })
        .collect();

    Ok(Json(ListIdpsResponse { idps }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/idp/create",
    request_body = CreateIdpRequest,
    responses((status = 200, body = IdpRow, description = "Newly-created IdP config")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn create_idp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateIdpRequest>,
) -> ApiResult<Json<IdpRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let name = clean_required(&params.name, "name")?;
    let entity_id = clean_required(&params.entity_id, "entity_id")?;

    tracing::info!(name = %name, "CreateSamlIdp");

    let mut model = saml_idp_config::ActiveModel {
        name: Set(name),
        entity_id: Set(entity_id),
        sso_url: Set(clean_optional(params.sso_url.as_deref())),
        slo_url: Set(clean_optional(params.slo_url.as_deref())),
        metadata_url: Set(clean_optional(params.metadata_url.as_deref())),
        attribute_mapping: Set(params.attribute_mapping),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(Some(is_active));
    }
    if let Some(hours) = params.session_lifetime_hours {
        model.session_lifetime_hours = Set(Some(hours));
    }
    if let Some(allow) = params.allow_idp_initiated {
        model.allow_idp_initiated = Set(Some(allow));
    }

    let inserted = model.insert(&state.db).await.map_err(map_entity_id_taken)?;

    Ok(Json(IdpRow::new(inserted, 0, 0)))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/idp/update",
    request_body = UpdateIdpRequest,
    responses((status = 200, body = IdpRow, description = "Updated IdP config")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn update_idp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateIdpRequest>,
) -> ApiResult<Json<IdpRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = find_idp(&state.db, params.id).await?;
    let idp_id = existing.id;

    tracing::info!(id = idp_id, "UpdateSamlIdp");

    let mut model = saml_idp_config::ActiveModel::from(existing);
    if let Some(ref name) = params.name {
        model.name = Set(clean_required(name, "name")?);
    }
    if let Some(ref entity_id) = params.entity_id {
        model.entity_id = Set(clean_required(entity_id, "entity_id")?);
    }
    if params.sso_url.is_some() {
        model.sso_url = Set(clean_optional(params.sso_url.as_deref()));
    }
    if params.slo_url.is_some() {
        model.slo_url = Set(clean_optional(params.slo_url.as_deref()));
    }
    if params.metadata_url.is_some() {
        model.metadata_url = Set(clean_optional(params.metadata_url.as_deref()));
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(Some(is_active));
    }
    if let Some(hours) = params.session_lifetime_hours {
        model.session_lifetime_hours = Set(Some(hours));
    }
    if let Some(allow) = params.allow_idp_initiated {
        model.allow_idp_initiated = Set(Some(allow));
    }
    if params.attribute_mapping.is_some() {
        model.attribute_mapping = Set(params.attribute_mapping);
    }
    model.updated_at = Set(Some(Utc::now().into()));

    let updated = model.update(&state.db).await.map_err(map_entity_id_taken)?;

    let sp_count = saml_sp_config::Entity::find()
        .filter(saml_sp_config::Column::Idp.eq(idp_id))
        .count(&state.db)
        .await? as i64;
    let user_count = usr_saml_identities::Entity::find()
        .filter(usr_saml_identities::Column::IdpId.eq(idp_id))
        .count(&state.db)
        .await? as i64;

    Ok(Json(IdpRow::new(updated, sp_count, user_count)))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/idp/delete",
    request_body = IdpIdRequest,
    responses((status = 200, body = SamlAdminSuccessResponse, description = "IdP config deleted")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn delete_idp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<IdpIdRequest>,
) -> ApiResult<Json<SamlAdminSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    find_idp(&state.db, params.id).await?;

    let sp_count = saml_sp_config::Entity::find()
        .filter(saml_sp_config::Column::Idp.eq(params.id))
        .count(&state.db)
        .await?;
    if sp_count > 0 {
        return Err(LocalError::conflict(
            "IDP_IN_USE",
            None,
            format!("IdP is referenced by {sp_count} service provider config(s)."),
        )
        .into());
    }

    let user_count = usr_saml_identities::Entity::find()
        .filter(usr_saml_identities::Column::IdpId.eq(params.id))
        .count(&state.db)
        .await?;
    if user_count > 0 {
        return Err(LocalError::conflict(
            "IDP_HAS_IDENTITIES",
            None,
            format!("{user_count} user SAML identit(ies) reference this IdP."),
        )
        .into());
    }

    tracing::info!(id = params.id, "DeleteSamlIdp");

    saml_idp_config::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await
        .map_err(|e| {
            if let Some(SqlErr::ForeignKeyConstraintViolation(_)) = e.sql_err() {
                return LocalError::conflict(
                    "IDP_IN_USE",
                    None,
                    "IdP is referenced by other records (sessions or attribute mappings).",
                );
            }
            LocalError::internal(e.to_string())
        })?;

    Ok(Json(SamlAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Service providers
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/sp/list",
    responses((status = 200, body = ListSpsResponse, description = "All SAML service provider configs (private keys omitted)")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn list_sps(State(state): State<Arc<AppState>>) -> ApiResult<Json<ListSpsResponse>> {
    require_perm(&state, READ_PERM, None).await?;

    let sps = saml_sp_config::Entity::find()
        .order_by_asc(saml_sp_config::Column::EntityId)
        .all(&state.db)
        .await?;

    let mut idp_names: HashMap<i32, String> = HashMap::new();
    for idp in saml_idp_config::Entity::find().all(&state.db).await? {
        idp_names.insert(idp.id, idp.name);
    }

    let sps = sps
        .into_iter()
        .map(|m| {
            let idp_name = m.idp.and_then(|id| idp_names.get(&id).cloned());
            SpRow::new(m, idp_name)
        })
        .collect();

    Ok(Json(ListSpsResponse { sps }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/sp/create",
    request_body = CreateSpRequest,
    responses((status = 200, body = SpRow, description = "Newly-created SP config")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn create_sp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateSpRequest>,
) -> ApiResult<Json<SpRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let entity_id = clean_required(&params.entity_id, "entity_id")?;
    let acs_url = clean_required(&params.acs_url, "acs_url")?;
    let x509_cert = clean_required(&params.x509_cert, "x509_cert")?;
    let private_key = clean_required(&params.private_key, "private_key")?;

    // The sp_config table has no DB unique constraint on entity_id (unlike
    // idp_config), so enforce it here.
    ensure_sp_entity_id_free(&state.db, &entity_id, None).await?;

    let idp_name = resolve_idp_name(&state.db, params.idp).await?;

    tracing::info!(entity_id = %entity_id, "CreateSamlSp");

    let mut model = saml_sp_config::ActiveModel {
        entity_id: Set(entity_id),
        acs_url: Set(acs_url),
        x509_cert: Set(x509_cert),
        private_key: Set(private_key),
        label: Set(clean_optional(params.label.as_deref())),
        slo_url: Set(clean_optional(params.slo_url.as_deref())),
        metadata_url: Set(clean_optional(params.metadata_url.as_deref())),
        callback_url: Set(clean_optional(params.callback_url.as_deref())),
        idp: Set(params.idp),
        idp_x509_cert: Set(clean_optional(params.idp_x509_cert.as_deref())),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let inserted = model.insert(&state.db).await.map_err(map_entity_id_taken)?;

    Ok(Json(SpRow::new(inserted, idp_name)))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/sp/update",
    request_body = UpdateSpRequest,
    responses((status = 200, body = SpRow, description = "Updated SP config")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn update_sp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateSpRequest>,
) -> ApiResult<Json<SpRow>> {
    require_perm(&state, WRITE_PERM, None).await?;

    let existing = saml_sp_config::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("SAML SP config {}", params.id)))?;

    tracing::info!(id = params.id, "UpdateSamlSp");

    let mut model = saml_sp_config::ActiveModel::from(existing);
    if let Some(ref entity_id) = params.entity_id {
        let entity_id = clean_required(entity_id, "entity_id")?;
        ensure_sp_entity_id_free(&state.db, &entity_id, Some(params.id)).await?;
        model.entity_id = Set(entity_id);
    }
    if let Some(ref acs_url) = params.acs_url {
        model.acs_url = Set(clean_required(acs_url, "acs_url")?);
    }
    if let Some(ref x509_cert) = params.x509_cert
        && !x509_cert.trim().is_empty()
    {
        model.x509_cert = Set(x509_cert.trim().to_string());
    }
    // Only replace the private key when a non-empty value is supplied.
    if let Some(ref private_key) = params.private_key
        && !private_key.trim().is_empty()
    {
        model.private_key = Set(private_key.trim().to_string());
    }
    if params.label.is_some() {
        model.label = Set(clean_optional(params.label.as_deref()));
    }
    if params.slo_url.is_some() {
        model.slo_url = Set(clean_optional(params.slo_url.as_deref()));
    }
    if params.metadata_url.is_some() {
        model.metadata_url = Set(clean_optional(params.metadata_url.as_deref()));
    }
    if params.callback_url.is_some() {
        model.callback_url = Set(clean_optional(params.callback_url.as_deref()));
    }
    if params.idp.is_some() {
        resolve_idp_name(&state.db, params.idp).await?;
        model.idp = Set(params.idp);
    }
    if params.idp_x509_cert.is_some() {
        model.idp_x509_cert = Set(clean_optional(params.idp_x509_cert.as_deref()));
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }
    model.updated_at = Set(Some(Utc::now().into()));

    let updated = model.update(&state.db).await.map_err(map_entity_id_taken)?;
    let idp_name = resolve_idp_name(&state.db, updated.idp).await?;

    Ok(Json(SpRow::new(updated, idp_name)))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/admin/sp/delete",
    request_body = SpIdRequest,
    responses((status = 200, body = SamlAdminSuccessResponse, description = "SP config deleted")),
    security(("bearer" = [])),
    tag = "saml-admin"
)]
pub async fn delete_sp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<SpIdRequest>,
) -> ApiResult<Json<SamlAdminSuccessResponse>> {
    require_perm(&state, WRITE_PERM, None).await?;

    tracing::info!(id = params.id, "DeleteSamlSp");

    let result = saml_sp_config::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await
        .map_err(|e| {
            if let Some(SqlErr::ForeignKeyConstraintViolation(_)) = e.sql_err() {
                return LocalError::conflict(
                    "SP_IN_USE",
                    None,
                    "SP is referenced by other records (pending auth requests).",
                );
            }
            LocalError::internal(e.to_string())
        })?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("SAML SP config {}", params.id)).into());
    }

    Ok(Json(SamlAdminSuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_idp(db: &DatabaseConnection, id: i32) -> Result<saml_idp_config::Model, LocalError> {
    saml_idp_config::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("SAML IdP config {id}")))
}

/// App-side uniqueness check for SP entity_ids (no DB constraint exists).
async fn ensure_sp_entity_id_free(
    db: &DatabaseConnection,
    entity_id: &str,
    exclude_id: Option<i32>,
) -> Result<(), LocalError> {
    let mut query =
        saml_sp_config::Entity::find().filter(saml_sp_config::Column::EntityId.eq(entity_id));
    if let Some(id) = exclude_id {
        query = query.filter(saml_sp_config::Column::Id.ne(id));
    }
    if query.count(db).await? > 0 {
        return Err(LocalError::conflict(
            "ENTITY_ID_TAKEN",
            Some("entity_id"),
            "A config with this entity ID already exists.",
        ));
    }
    Ok(())
}

/// Validate an optional IdP reference and return its name.
async fn resolve_idp_name(
    db: &DatabaseConnection,
    idp: Option<i32>,
) -> Result<Option<String>, LocalError> {
    match idp {
        Some(id) => Ok(Some(find_idp(db, id).await?.name)),
        None => Ok(None),
    }
}

fn map_entity_id_taken(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "ENTITY_ID_TAKEN",
        Some("entity_id"),
        "A config with this entity ID already exists.",
    )
}
