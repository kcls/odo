//! Admin CRUD for notification.template.
//!
//! Reads require `notification.template.read`; writes require
//! `notification.template.write`. Templates are soft-deleted (deleted_at) —
//! events and deliveries reference templates by code string, so history
//! stays intact. Handlebars syntax is validated server-side on every write
//! using the same renderer the enqueue path uses, and a preview endpoint
//! renders drafts against sample data without saving anything.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_service::admin::{
    Page, Paginated, Sort, clean_code, clean_optional, clean_required, clean_search,
    map_unique_violation,
};
use odo_client::context::RequestContext;
use odo_entity::notification::template;
use odo_entity::org::unit;
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::{Condition, Order, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::inbox::SuccessResponse;

const READ_PERM: &str = "odo.notify.template.read";
const WRITE_PERM: &str = "odo.notify.template.write";

// ===========================================================================
// Types
// ===========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateRow {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub org_unit: i32,
    pub subject_template: String,
    pub body_template: String,
    pub body_template_html: Option<String>,
    pub sample_data: Option<serde_json::Value>,
    pub is_active: bool,
    pub created_by: i32,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<template::Model> for TemplateRow {
    fn from(m: template::Model) -> Self {
        Self {
            id: m.id,
            code: m.code,
            name: m.name,
            description: m.description,
            org_unit: m.org_unit,
            subject_template: m.subject_template,
            body_template: m.body_template,
            body_template_html: m.body_template_html,
            sample_data: m.sample_data,
            is_active: m.is_active,
            created_by: m.created_by,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

odo_service::page_type!(TemplatePage, TemplateRow, "One page of email templates.");

#[derive(Deserialize, ToSchema)]
pub struct ListTemplatesRequest {
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreviewResponse {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub body_html: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTemplateRequest {
    code: String,
    name: String,
    subject_template: String,
    body_template: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    body_template_html: Option<String>,
    #[serde(default)]
    sample_data: Option<serde_json::Value>,
    #[serde(default)]
    is_active: Option<bool>,
    /// Defaults to the root org unit.
    #[serde(default)]
    org_unit: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateTemplateRequest {
    id: i32,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    subject_template: Option<String>,
    #[serde(default)]
    body_template: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// Empty string clears the HTML variant.
    #[serde(default)]
    body_template_html: Option<String>,
    #[serde(default)]
    sample_data: Option<serde_json::Value>,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct TemplateIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct PreviewRequest {
    #[serde(default)]
    subject_template: Option<String>,
    #[serde(default)]
    body_template: Option<String>,
    #[serde(default)]
    body_template_html: Option<String>,
    #[serde(default)]
    variables: Option<serde_json::Value>,
}

// ===========================================================================
// Handlers
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/list",
    request_body = ListTemplatesRequest,
    responses((status = 200, body = TemplatePage, description = "Notification templates")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn list_templates(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListTemplatesRequest>,
) -> ApiResult<Json<TemplatePage>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state.auth_client.permission_required(READ_PERM, None).await?;

    let mut condition = Condition::all().add(template::Column::DeletedAt.is_null());
    if let Some(search) = clean_search(params.search.as_deref()) {
        condition = condition.add(
            Condition::any()
                .add(template::Column::Code.contains(&search))
                .add(template::Column::Name.contains(&search)),
        );
    }

    let total = template::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("code", template::Column::Code),
            ("name", template::Column::Name),
            ("active", template::Column::IsActive),
            ("created", template::Column::CreatedAt),
            ("updated", template::Column::UpdatedAt),
        ],
        (template::Column::Code, Order::Asc),
    );

    let templates = template::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(template::Column::Code)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    Ok(Json(
        Paginated::new(templates.into_iter().map(Into::into).collect(), total).into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/create",
    request_body = CreateTemplateRequest,
    responses((status = 200, body = TemplateRow, description = "Newly-created template")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn create_template(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateTemplateRequest>,
) -> ApiResult<Json<TemplateRow>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;
    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let code = clean_code(&params.code, "code")?;
    let name = clean_required(&params.name, "name")?;
    let subject = clean_required(&params.subject_template, "subject_template")?;
    let body = clean_required(&params.body_template, "body_template")?;
    let body_html = clean_optional(params.body_template_html.as_deref());

    let vars = params.sample_data.clone().unwrap_or(serde_json::json!({}));
    validate_template("subject_template", &subject, &vars)?;
    validate_template("body_template", &body, &vars)?;
    if let Some(ref html) = body_html {
        validate_template("body_template_html", html, &vars)?;
    }

    let org_unit = match params.org_unit {
        Some(id) => {
            find_org_unit(&state.db, id).await?;
            id
        }
        None => root_org_unit(&state.db).await?,
    };

    tracing::info!(code = %code, "CreateTemplate");

    let mut model = template::ActiveModel {
        code: Set(code),
        name: Set(name),
        subject_template: Set(subject),
        body_template: Set(body),
        body_template_html: Set(body_html),
        description: Set(clean_optional(params.description.as_deref())),
        sample_data: Set(params.sample_data),
        org_unit: Set(org_unit),
        created_by: Set(user_id),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let inserted = model.insert(&state.db).await.map_err(map_code_taken)?;

    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/update",
    request_body = UpdateTemplateRequest,
    responses((status = 200, body = TemplateRow, description = "Updated template")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn update_template(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateTemplateRequest>,
) -> ApiResult<Json<TemplateRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let existing = find_template(&state.db, params.id).await?;

    // Validate the resulting template set against the resulting sample data.
    let vars = params
        .sample_data
        .clone()
        .or_else(|| existing.sample_data.clone())
        .unwrap_or(serde_json::json!({}));
    let subject = params
        .subject_template
        .as_deref()
        .unwrap_or(&existing.subject_template);
    let body = params
        .body_template
        .as_deref()
        .unwrap_or(&existing.body_template);
    validate_template("subject_template", subject, &vars)?;
    validate_template("body_template", body, &vars)?;
    let new_html = clean_optional(params.body_template_html.as_deref());
    let effective_html = if params.body_template_html.is_some() {
        new_html.clone()
    } else {
        existing.body_template_html.clone()
    };
    if let Some(ref html) = effective_html {
        validate_template("body_template_html", html, &vars)?;
    }

    tracing::info!(id = params.id, "UpdateTemplate");

    let mut model = template::ActiveModel::from(existing);
    if let Some(ref code) = params.code {
        model.code = Set(clean_code(code, "code")?);
    }
    if let Some(ref name) = params.name {
        model.name = Set(clean_required(name, "name")?);
    }
    if let Some(ref subject) = params.subject_template {
        model.subject_template = Set(clean_required(subject, "subject_template")?);
    }
    if let Some(ref body) = params.body_template {
        model.body_template = Set(clean_required(body, "body_template")?);
    }
    if params.body_template_html.is_some() {
        model.body_template_html = Set(new_html);
    }
    if params.description.is_some() {
        model.description = Set(clean_optional(params.description.as_deref()));
    }
    if params.sample_data.is_some() {
        model.sample_data = Set(params.sample_data);
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }
    model.updated_at = Set(Utc::now().into());

    let updated = model.update(&state.db).await.map_err(map_code_taken)?;

    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/delete",
    request_body = TemplateIdRequest,
    responses((status = 200, body = SuccessResponse, description = "Template soft-deleted")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn delete_template(
    State(state): State<Arc<AppState>>,
    Json(params): Json<TemplateIdRequest>,
) -> ApiResult<Json<SuccessResponse>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let existing = find_template(&state.db, params.id).await?;

    tracing::info!(id = params.id, "DeleteTemplate");

    let mut model = template::ActiveModel::from(existing);
    model.deleted_at = Set(Some(Utc::now().into()));
    model.update(&state.db).await?;

    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/preview",
    request_body = PreviewRequest,
    responses((status = 200, body = PreviewResponse, description = "Rendered template preview")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn preview_template(
    State(state): State<Arc<AppState>>,
    Json(params): Json<PreviewRequest>,
) -> ApiResult<Json<PreviewResponse>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state.auth_client.permission_required(READ_PERM, None).await?;

    let vars = params.variables.unwrap_or(serde_json::json!({}));

    let render = |field: &str, text: &Option<String>| -> Result<Option<String>, LocalError> {
        match text {
            Some(t) => Ok(Some(odo_service::template::render(t, &vars).map_err(|e| {
                LocalError::invalid_input(format!("{field}: {e}"))
            })?)),
            None => Ok(None),
        }
    };

    Ok(Json(PreviewResponse {
        subject: render("subject_template", &params.subject_template)?,
        body: render("body_template", &params.body_template)?,
        body_html: render("body_template_html", &params.body_template_html)?,
    }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_template(
    db: &DatabaseConnection,
    id: i32,
) -> Result<template::Model, LocalError> {
    template::Entity::find_by_id(id)
        .filter(template::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("template {id}")))
}

async fn find_org_unit(db: &DatabaseConnection, id: i32) -> Result<unit::Model, LocalError> {
    unit::Entity::find_by_id(id)
        .filter(unit::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("org unit {id}")))
}

async fn root_org_unit(db: &DatabaseConnection) -> Result<i32, LocalError> {
    unit::Entity::find()
        .filter(unit::Column::Parent.is_null())
        .filter(unit::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .map(|u| u.id)
        .ok_or_else(|| LocalError::internal("no root org unit found"))
}

/// Compile-check a handlebars template against sample variables; render
/// errors surface as 400s naming the offending field.
fn validate_template(
    field: &str,
    text: &str,
    vars: &serde_json::Value,
) -> Result<(), LocalError> {
    odo_service::template::render(text, vars)
        .map(|_| ())
        .map_err(|e| LocalError::invalid_input(format!("{field}: {e}")))
}

fn map_code_taken(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "TEMPLATE_CODE_TAKEN",
        Some("code"),
        "A template with this code already exists (deleted templates keep their codes).",
    )
}
