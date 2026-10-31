//! Version history for notification templates: list a template's
//! versions, read one, and restore one.
//!
//! The versions themselves are written by a trigger on
//! notification.template (sqitch `012_notify_template_history`), so every
//! write leaves one, whatever made it. Writers here identify the acting
//! user, and mark a restore, with transaction-local settings the trigger
//! reads (`set_actor`); a write outside a transaction would lose them.
//!
//! Reads require `notification.template.read`; restoring requires
//! `notification.template.write`. Only live templates have a reachable
//! history: a deleted template's versions stay in the table, but these
//! endpoints treat the template as not found.

use axum::Json;
use axum::extract::State;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::auth::usr;
use odo_entity::notification::{template, template_version};
use odo_service::admin::{Page, Paginated};
use sea_orm::prelude::*;
use sea_orm::{DatabaseBackend, QueryOrder, QuerySelect, Set, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::template_admin::{
    READ_PERM, TemplateRow, WRITE_PERM, find_template, map_code_taken, validate_template,
};

// ===========================================================================
// Types
// ===========================================================================

/// One version in a template's history, without its content.
#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateVersionSummary {
    pub version: i32,
    /// `baseline` (content when history began), `create`, `update`,
    /// `restore`, or `delete`.
    pub action: String,
    /// The version a restore brought back.
    pub restored_from: Option<i32>,
    /// Fields that differ from the previous version (none for the first).
    pub changed_fields: Vec<String>,
    /// Null when unknown: the baseline, or a change made outside odo-notify.
    pub changed_by: Option<i32>,
    pub changed_by_name: Option<String>,
    pub changed_at: chrono::DateTime<chrono::FixedOffset>,
}

odo_service::page_type!(
    TemplateVersionPage,
    TemplateVersionSummary,
    "One page of a template's versions, newest first."
);

/// A template as it stood at one version.
#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateVersionRow {
    pub template: i32,
    pub version: i32,
    pub action: String,
    pub restored_from: Option<i32>,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub subject_template: String,
    pub body_template: String,
    pub body_template_html: Option<String>,
    pub sample_data: Option<serde_json::Value>,
    pub is_active: bool,
    pub changed_by: Option<i32>,
    pub changed_by_name: Option<String>,
    pub changed_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListTemplateVersionsRequest {
    template: i32,
    #[serde(flatten)]
    page: Page,
}

#[derive(Deserialize, ToSchema)]
pub struct TemplateVersionRequest {
    template: i32,
    version: i32,
}

// ===========================================================================
// Handlers
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/versions",
    request_body = ListTemplateVersionsRequest,
    responses((status = 200, body = TemplateVersionPage, description = "The template's versions, newest first")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListTemplateVersionsRequest>,
) -> ApiResult<Json<TemplateVersionPage>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state
        .auth_client
        .permission_required(READ_PERM, None)
        .await?;
    find_template(&state.db, params.template).await?;

    let of_template = template_version::Column::Template.eq(params.template);
    let total = template_version::Entity::find()
        .filter(of_template.clone())
        .count(&state.db)
        .await? as i64;

    // One extra, older row: what the page's oldest version changed from.
    let mut versions = template_version::Entity::find()
        .filter(of_template)
        .order_by_desc(template_version::Column::Version)
        .limit(params.page.limit() + 1)
        .offset(params.page.offset())
        .all(&state.db)
        .await?;
    let previous = if versions.len() as u64 > params.page.limit() {
        versions.pop()
    } else {
        None
    };

    let names = user_names(&state.db, versions.iter().filter_map(|v| v.changed_by)).await?;
    let older = versions.iter().skip(1).map(Some).chain([previous.as_ref()]);
    let rows = versions
        .iter()
        .zip(older)
        .map(|(v, older)| TemplateVersionSummary {
            version: v.version,
            action: v.action.clone(),
            restored_from: v.restored_from,
            changed_fields: older.map(|o| changed_fields(o, v)).unwrap_or_default(),
            changed_by: v.changed_by,
            changed_by_name: v.changed_by.and_then(|id| names.get(&id).cloned()),
            changed_at: v.changed_at,
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/version",
    request_body = TemplateVersionRequest,
    responses((status = 200, body = TemplateVersionRow, description = "The template as it stood at that version")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn get_version(
    State(state): State<Arc<AppState>>,
    Json(params): Json<TemplateVersionRequest>,
) -> ApiResult<Json<TemplateVersionRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state
        .auth_client
        .permission_required(READ_PERM, None)
        .await?;
    find_template(&state.db, params.template).await?;

    let v = find_version(&state.db, params.template, params.version).await?;
    let names = user_names(&state.db, v.changed_by).await?;
    Ok(Json(TemplateVersionRow {
        template: v.template,
        version: v.version,
        action: v.action,
        restored_from: v.restored_from,
        code: v.code,
        name: v.name,
        description: v.description,
        subject_template: v.subject_template,
        body_template: v.body_template,
        body_template_html: v.body_template_html,
        sample_data: v.sample_data,
        is_active: v.is_active,
        changed_by_name: v.changed_by.and_then(|id| names.get(&id).cloned()),
        changed_by: v.changed_by,
        changed_at: v.changed_at,
    }))
}

/// Put a template back as it stood at an earlier version: its code, name,
/// description, subject, bodies, sample data, and active flag. Recorded
/// as a new `restore` version, so a restore can itself be undone.
/// Restoring the content the template already has changes nothing.
#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/template/restore",
    request_body = TemplateVersionRequest,
    responses((status = 200, body = TemplateRow, description = "The template after the restore")),
    security(("bearer" = [])),
    tag = "template-admin"
)]
pub async fn restore_version(
    State(state): State<Arc<AppState>>,
    Json(params): Json<TemplateVersionRequest>,
) -> ApiResult<Json<TemplateRow>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;
    state
        .auth_client
        .permission_required(WRITE_PERM, None)
        .await?;

    let existing = find_template(&state.db, params.template).await?;
    let v = find_version(&state.db, params.template, params.version).await?;

    // The renderer may have tightened since the version was saved.
    let vars = v.sample_data.clone().unwrap_or(serde_json::json!({}));
    validate_template("subject_template", &v.subject_template, &vars)?;
    validate_template("body_template", &v.body_template, &vars)?;
    if let Some(ref html) = v.body_template_html {
        validate_template("body_template_html", html, &vars)?;
    }

    tracing::info!(
        id = params.template,
        version = params.version,
        "RestoreTemplate"
    );

    let mut model = template::ActiveModel::from(existing);
    model.code = Set(v.code);
    model.name = Set(v.name);
    model.description = Set(v.description);
    model.subject_template = Set(v.subject_template);
    model.body_template = Set(v.body_template);
    model.body_template_html = Set(v.body_template_html);
    model.sample_data = Set(v.sample_data);
    model.is_active = Set(v.is_active);
    model.updated_at = Set(chrono::Utc::now().into());

    let txn = state.db.begin().await?;
    set_actor(&txn, user_id, Some(params.version)).await?;
    let updated = model.update(&txn).await.map_err(map_code_taken)?;
    txn.commit().await?;

    Ok(Json(updated.into()))
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Tell the version trigger who is acting in this transaction, and which
/// version a restore brings back.
pub(crate) async fn set_actor(
    txn: &impl ConnectionTrait,
    user_id: i32,
    restored_from: Option<i32>,
) -> Result<(), DbErr> {
    txn.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT set_config('app.actor_id', $1, true), set_config('app.restored_from', $2, true)",
        [
            user_id.to_string().into(),
            restored_from
                .map(|v| v.to_string())
                .unwrap_or_default()
                .into(),
        ],
    ))
    .await?;
    Ok(())
}

async fn find_version(
    db: &DatabaseConnection,
    template: i32,
    version: i32,
) -> Result<template_version::Model, LocalError> {
    template_version::Entity::find()
        .filter(template_version::Column::Template.eq(template))
        .filter(template_version::Column::Version.eq(version))
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("template {template} version {version}")))
}

async fn user_names(
    db: &DatabaseConnection,
    ids: impl IntoIterator<Item = i32>,
) -> Result<HashMap<i32, String>, DbErr> {
    let ids: Vec<i32> = ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(usr::Entity::find()
        .filter(usr::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.display_name))
        .collect())
}

/// The names of the versioned fields that differ between two versions.
fn changed_fields(older: &template_version::Model, newer: &template_version::Model) -> Vec<String> {
    let fields: [(&str, bool); 9] = [
        ("code", older.code != newer.code),
        ("name", older.name != newer.name),
        ("description", older.description != newer.description),
        (
            "subject_template",
            older.subject_template != newer.subject_template,
        ),
        ("body_template", older.body_template != newer.body_template),
        (
            "body_template_html",
            older.body_template_html != newer.body_template_html,
        ),
        ("sample_data", older.sample_data != newer.sample_data),
        ("is_active", older.is_active != newer.is_active),
        (
            "deleted",
            older.deleted_at.is_some() != newer.deleted_at.is_some(),
        ),
    ];
    fields
        .into_iter()
        .filter(|(_, changed)| *changed)
        .map(|(name, _)| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(n: i32) -> template_version::Model {
        template_version::Model {
            id: n,
            template: 1,
            version: n,
            action: "update".into(),
            restored_from: None,
            code: "code".into(),
            name: "Name".into(),
            description: None,
            org_unit: 1,
            subject_template: "Hi".into(),
            body_template: "Body".into(),
            body_template_html: None,
            sample_data: None,
            is_active: true,
            deleted_at: None,
            changed_by: None,
            changed_at: chrono::Utc::now().into(),
        }
    }

    #[test]
    fn changed_fields_lists_only_what_differs() {
        let older = version(1);
        let mut newer = version(2);
        assert!(changed_fields(&older, &newer).is_empty());

        newer.subject_template = "Hello".into();
        newer.sample_data = Some(serde_json::json!({"name": "x"}));
        newer.is_active = false;
        assert_eq!(
            changed_fields(&older, &newer),
            ["subject_template", "sample_data", "is_active"]
        );
    }

    #[test]
    fn changed_fields_ignores_bookkeeping() {
        let older = version(1);
        let mut newer = version(2);
        newer.changed_by = Some(7);
        newer.action = "restore".into();
        newer.restored_from = Some(1);
        assert!(changed_fields(&older, &newer).is_empty());
    }
}
