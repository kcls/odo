//! Admin CRUD for notification email groups and their members.
//!
//! Reads require `notification.email_group.read`; writes require
//! `notification.email_group.write`. Groups are soft-deleted only
//! (deactivated) because `notification.delivery` and
//! `incidents.notification_email_routing` reference them; members carry no
//! inbound references and may be hard-deleted.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_service::admin::{
    Page, Paginated, Sort, clean_code, clean_email, clean_required, clean_search,
    map_unique_violation,
};
use odo_client::context::RequestContext;
use odo_entity::notification::{email_group, email_group_member};
use odo_client::error::{ApiResult, LocalError};
use sea_orm::prelude::*;
use sea_orm::{Condition, Order, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;
use crate::inbox::SuccessResponse;

const READ_PERM: &str = "odo.notify.email_group.read";
const WRITE_PERM: &str = "odo.notify.email_group.write";

#[derive(Debug, Serialize, ToSchema)]
pub struct EmailGroupRow {
    pub id: i32,
    pub code: String,
    pub label: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub member_count: i64,
    pub active_member_count: i64,
}

impl EmailGroupRow {
    fn new(group: email_group::Model, member_count: i64, active_member_count: i64) -> Self {
        Self {
            id: group.id,
            code: group.code,
            label: group.label,
            is_active: group.is_active,
            created_at: group.created_at,
            updated_at: group.updated_at,
            member_count,
            active_member_count,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmailGroupMemberRow {
    pub id: i32,
    pub email_group: i32,
    pub email: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<email_group_member::Model> for EmailGroupMemberRow {
    fn from(m: email_group_member::Model) -> Self {
        Self {
            id: m.id,
            email_group: m.email_group,
            email: m.email,
            is_active: m.is_active,
            created_at: m.created_at,
        }
    }
}

odo_service::page_type!(EmailGroupPage, EmailGroupRow, "One page of email groups.");

#[derive(Debug, Serialize, ToSchema)]
pub struct EmailGroupDetailResponse {
    pub group: EmailGroupRow,
    pub members: Vec<EmailGroupMemberRow>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListEmailGroupsRequest {
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    include_inactive: bool,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Deserialize, ToSchema)]
pub struct EmailGroupIdRequest {
    id: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateEmailGroupRequest {
    code: String,
    label: String,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateEmailGroupRequest {
    id: i32,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateEmailGroupMemberRequest {
    email_group: i32,
    email: String,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateEmailGroupMemberRequest {
    id: i32,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct EmailGroupMemberIdRequest {
    id: i32,
}

// ===========================================================================
// Groups
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/list",
    request_body = ListEmailGroupsRequest,
    responses((status = 200, body = EmailGroupPage, description = "Email groups with member counts")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn list_email_groups(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListEmailGroupsRequest>,
) -> ApiResult<Json<EmailGroupPage>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(READ_PERM, None).await?;

    let mut condition = Condition::all();
    if !params.include_inactive {
        condition = condition.add(email_group::Column::IsActive.eq(true));
    }
    if let Some(search) = clean_search(params.search.as_deref()) {
        condition = condition.add(
            Condition::any()
                .add(email_group::Column::Code.contains(&search))
                .add(email_group::Column::Label.contains(&search)),
        );
    }

    let total = email_group::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // member counts are computed post-query, so only real columns sort.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("code", email_group::Column::Code),
            ("label", email_group::Column::Label),
            ("active", email_group::Column::IsActive),
            ("created", email_group::Column::CreatedAt),
            ("updated", email_group::Column::UpdatedAt),
        ],
        (email_group::Column::Code, Order::Asc),
    );

    let groups = email_group::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_asc(email_group::Column::Code)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    let counts = member_counts(&state.db, &groups).await?;

    let rows = groups
        .into_iter()
        .map(|g| {
            let (total, active) = counts.get(&g.id).copied().unwrap_or((0, 0));
            EmailGroupRow::new(g, total, active)
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/get",
    request_body = EmailGroupIdRequest,
    responses((status = 200, body = EmailGroupDetailResponse, description = "Email group with its members")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn get_email_group(
    State(state): State<Arc<AppState>>,
    Json(params): Json<EmailGroupIdRequest>,
) -> ApiResult<Json<EmailGroupDetailResponse>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(READ_PERM, None).await?;

    let group = find_group(&state.db, params.id).await?;

    let members = email_group_member::Entity::find()
        .filter(email_group_member::Column::EmailGroup.eq(group.id))
        .order_by_asc(email_group_member::Column::Email)
        .all(&state.db)
        .await?;

    let total = members.len() as i64;
    let active = members.iter().filter(|m| m.is_active).count() as i64;

    Ok(Json(EmailGroupDetailResponse {
        group: EmailGroupRow::new(group, total, active),
        members: members.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/create",
    request_body = CreateEmailGroupRequest,
    responses((status = 200, body = EmailGroupRow, description = "Newly-created email group")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn create_email_group(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateEmailGroupRequest>,
) -> ApiResult<Json<EmailGroupRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let code = clean_code(&params.code, "code")?;
    let label = clean_required(&params.label, "label")?;

    tracing::info!(code = %code, "CreateEmailGroup");

    let mut model = email_group::ActiveModel {
        code: Set(code),
        label: Set(label),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let inserted = model.insert(&state.db).await.map_err(map_group_write_err)?;

    Ok(Json(EmailGroupRow::new(inserted, 0, 0)))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/update",
    request_body = UpdateEmailGroupRequest,
    responses((status = 200, body = EmailGroupRow, description = "Updated email group")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn update_email_group(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateEmailGroupRequest>,
) -> ApiResult<Json<EmailGroupRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let existing = find_group(&state.db, params.id).await?;
    let group_id = existing.id;

    let mut model = email_group::ActiveModel::from(existing);
    if let Some(ref code) = params.code {
        model.code = Set(clean_code(code, "code")?);
    }
    if let Some(ref label) = params.label {
        model.label = Set(clean_required(label, "label")?);
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }
    model.updated_at = Set(Utc::now().into());

    tracing::info!(id = group_id, "UpdateEmailGroup");

    let updated = model.update(&state.db).await.map_err(map_group_write_err)?;

    let counts = member_counts_for(&state.db, group_id).await?;

    Ok(Json(EmailGroupRow::new(updated, counts.0, counts.1)))
}

// ===========================================================================
// Members
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/member/create",
    request_body = CreateEmailGroupMemberRequest,
    responses((status = 200, body = EmailGroupMemberRow, description = "Newly-added email group member")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn create_email_group_member(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateEmailGroupMemberRequest>,
) -> ApiResult<Json<EmailGroupMemberRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let email = clean_email(&params.email)?;

    // Resolve the group first so a bad id is a 404 rather than an FK error.
    find_group(&state.db, params.email_group).await?;

    tracing::info!(email_group = params.email_group, "CreateEmailGroupMember");

    let mut model = email_group_member::ActiveModel {
        email_group: Set(params.email_group),
        email: Set(email),
        ..Default::default()
    };
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    let inserted = model
        .insert(&state.db)
        .await
        .map_err(map_member_write_err)?;

    Ok(Json(inserted.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/member/update",
    request_body = UpdateEmailGroupMemberRequest,
    responses((status = 200, body = EmailGroupMemberRow, description = "Updated email group member")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn update_email_group_member(
    State(state): State<Arc<AppState>>,
    Json(params): Json<UpdateEmailGroupMemberRequest>,
) -> ApiResult<Json<EmailGroupMemberRow>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let existing = email_group_member::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("email group member {}", params.id)))?;

    let mut model = email_group_member::ActiveModel::from(existing);
    if let Some(ref email) = params.email {
        model.email = Set(clean_email(email)?);
    }
    if let Some(is_active) = params.is_active {
        model.is_active = Set(is_active);
    }

    tracing::info!(id = params.id, "UpdateEmailGroupMember");

    let updated = model
        .update(&state.db)
        .await
        .map_err(map_member_write_err)?;

    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/email-group/member/delete",
    request_body = EmailGroupMemberIdRequest,
    responses((status = 200, body = SuccessResponse, description = "Email group member deleted")),
    security(("bearer" = [])),
    tag = "email-group"
)]
pub async fn delete_email_group_member(
    State(state): State<Arc<AppState>>,
    Json(params): Json<EmailGroupMemberIdRequest>,
) -> ApiResult<Json<SuccessResponse>> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;

    state.auth_client.permission_required(WRITE_PERM, None).await?;

    tracing::info!(id = params.id, "DeleteEmailGroupMember");

    let result = email_group_member::Entity::delete_by_id(params.id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found(format!("email group member {}", params.id)).into());
    }

    Ok(Json(SuccessResponse { success: true }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn find_group(db: &DatabaseConnection, id: i32) -> Result<email_group::Model, LocalError> {
    email_group::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("email group {id}")))
}

/// (total, active) member counts keyed by group id.
async fn member_counts(
    db: &DatabaseConnection,
    groups: &[email_group::Model],
) -> Result<HashMap<i32, (i64, i64)>, LocalError> {
    let mut counts: HashMap<i32, (i64, i64)> = HashMap::new();
    if groups.is_empty() {
        return Ok(counts);
    }

    let members = email_group_member::Entity::find()
        .filter(
            email_group_member::Column::EmailGroup
                .is_in(groups.iter().map(|g| g.id).collect::<Vec<_>>()),
        )
        .all(db)
        .await?;

    for m in members {
        let entry = counts.entry(m.email_group).or_insert((0, 0));
        entry.0 += 1;
        if m.is_active {
            entry.1 += 1;
        }
    }

    Ok(counts)
}

async fn member_counts_for(db: &DatabaseConnection, group_id: i32) -> Result<(i64, i64), LocalError> {
    let members = email_group_member::Entity::find()
        .filter(email_group_member::Column::EmailGroup.eq(group_id))
        .all(db)
        .await?;

    let total = members.len() as i64;
    let active = members.iter().filter(|m| m.is_active).count() as i64;

    Ok((total, active))
}

fn map_group_write_err(e: DbErr) -> LocalError {
    map_unique_violation(
        e,
        "EMAIL_GROUP_CODE_TAKEN",
        Some("code"),
        "An email group with this code already exists.",
    )
}

fn map_member_write_err(e: DbErr) -> LocalError {
    match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(detail))
            if detail.contains("uq_email_group_member") =>
        {
            LocalError::conflict(
                "EMAIL_ALREADY_IN_GROUP",
                Some("email"),
                "This email address is already a member of the group.",
            )
        }
        Some(SqlErr::ForeignKeyConstraintViolation(_)) => LocalError::not_found("email group"),
        _ => LocalError::internal(e.to_string()),
    }
}
