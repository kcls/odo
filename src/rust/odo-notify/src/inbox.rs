use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::notification::{delivery, event, user_state};
use sea_orm::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::{Condition, QueryOrder, QuerySelect, Set};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

const MAX_LIMIT: u64 = 100;
const DEFAULT_LIMIT: u64 = 50;

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationItem {
    pub delivery_id: i64,
    pub event_id: i64,
    pub template_code: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub action_url: Option<String>,
    pub source_service: Option<String>,
    pub source_entity_type: Option<String>,
    pub source_entity_id: Option<i64>,
    pub is_read: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListResponse {
    pub notifications: Vec<NotificationItem>,
    pub total: i64,
    pub unread_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SuccessResponse {
    pub success: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct ListRequest {
    #[serde(default = "default_limit")]
    limit: u64,
    #[serde(default)]
    offset: u64,
}

fn default_limit() -> u64 {
    DEFAULT_LIMIT
}

#[derive(Deserialize, ToSchema)]
pub struct DeliveryIdRequest {
    delivery_id: i64,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/inbox/list",
    request_body = ListRequest,
    responses((status = 200, body = ListResponse, description = "User's in-app notifications")),
    security(("bearer" = [])),
    tag = "inbox"
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListRequest>,
) -> ApiResult<Json<ListResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let limit = cmp::min(params.limit, MAX_LIMIT);
    let offset = params.offset;

    let (read_watermark, dismiss_watermark) = get_user_watermarks(&state.db, user_id).await?;

    let mut condition = Condition::all()
        .add(delivery::Column::Channel.eq("in_app"))
        .add(delivery::Column::RecipientUser.eq(user_id))
        .add(delivery::Column::DismissedAt.is_null());

    if let Some(ref dw) = dismiss_watermark {
        condition = condition.add(delivery::Column::CreatedAt.gt(*dw));
    }

    let deliveries = delivery::Entity::find()
        .filter(condition.clone())
        .order_by_desc(delivery::Column::CreatedAt)
        .order_by_desc(delivery::Column::Id)
        .offset(offset)
        .limit(limit)
        .all(&state.db)
        .await?;

    let event_ids: Vec<i64> = deliveries.iter().map(|d| d.event_id).collect();

    let events: Vec<event::Model> = if event_ids.is_empty() {
        vec![]
    } else {
        event::Entity::find()
            .filter(event::Column::Id.is_in(event_ids.clone()))
            .all(&state.db)
            .await?
    };

    let event_map: std::collections::HashMap<i64, &event::Model> =
        events.iter().map(|e| (e.id, e)).collect();

    let notifications: Vec<NotificationItem> = deliveries
        .iter()
        .map(|d| {
            let evt = event_map.get(&d.event_id);
            let is_read = d.read_at.is_some()
                || read_watermark
                    .as_ref()
                    .is_some_and(|rw| d.created_at <= *rw);

            NotificationItem {
                delivery_id: d.id,
                event_id: d.event_id,
                template_code: d.template_code.clone(),
                title: d.title_rendered.clone(),
                body: d.body_rendered.clone(),
                action_url: d.action_url.clone(),
                source_service: evt.and_then(|e| e.source_service.clone()),
                source_entity_type: evt.and_then(|e| e.source_entity_type.clone()),
                source_entity_id: evt.and_then(|e| e.source_entity_id),
                is_read,
                created_at: d.created_at,
            }
        })
        .collect();

    let total = count_visible(&state.db, user_id, dismiss_watermark.as_ref()).await?;
    let unread_count = count_unread(
        &state.db,
        user_id,
        dismiss_watermark.as_ref(),
        read_watermark.as_ref(),
    )
    .await?;

    Ok(Json(ListResponse {
        notifications,
        total,
        unread_count,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/inbox/mark-read",
    request_body = DeliveryIdRequest,
    responses((status = 200, body = SuccessResponse, description = "Notification marked as read")),
    security(("bearer" = [])),
    tag = "inbox"
)]
pub async fn mark_read(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeliveryIdRequest>,
) -> ApiResult<Json<SuccessResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let result = delivery::Entity::update_many()
        .col_expr(delivery::Column::ReadAt, Expr::value(Utc::now()))
        .filter(delivery::Column::Id.eq(params.delivery_id))
        .filter(delivery::Column::RecipientUser.eq(user_id))
        .filter(delivery::Column::Channel.eq("in_app"))
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found("notification").into());
    }

    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/inbox/mark-all-read",
    responses((status = 200, body = SuccessResponse, description = "All notifications marked as read")),
    security(("bearer" = [])),
    tag = "inbox"
)]
pub async fn mark_all_read(State(state): State<Arc<AppState>>) -> ApiResult<Json<SuccessResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    upsert_watermark(&state.db, user_id, Watermark::Read).await?;

    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/inbox/dismiss",
    request_body = DeliveryIdRequest,
    responses((status = 200, body = SuccessResponse, description = "Notification dismissed")),
    security(("bearer" = [])),
    tag = "inbox"
)]
pub async fn dismiss(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeliveryIdRequest>,
) -> ApiResult<Json<SuccessResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let result = delivery::Entity::update_many()
        .col_expr(delivery::Column::DismissedAt, Expr::value(Utc::now()))
        .filter(delivery::Column::Id.eq(params.delivery_id))
        .filter(delivery::Column::RecipientUser.eq(user_id))
        .filter(delivery::Column::Channel.eq("in_app"))
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(LocalError::not_found("notification").into());
    }

    Ok(Json(SuccessResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/inbox/dismiss-all",
    responses((status = 200, body = SuccessResponse, description = "All notifications dismissed")),
    security(("bearer" = [])),
    tag = "inbox"
)]
pub async fn dismiss_all(State(state): State<Arc<AppState>>) -> ApiResult<Json<SuccessResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    upsert_watermark(&state.db, user_id, Watermark::Dismiss).await?;

    Ok(Json(SuccessResponse { success: true }))
}

type Watermarks = (Option<DateTimeWithTimeZone>, Option<DateTimeWithTimeZone>);

async fn get_user_watermarks(db: &DatabaseConnection, user_id: i32) -> ApiResult<Watermarks> {
    let row = user_state::Entity::find_by_id(user_id).one(db).await?;

    Ok(match row {
        Some(s) => (s.read_watermark_at, s.dismiss_watermark_at),
        None => (None, None),
    })
}

enum Watermark {
    Read,
    Dismiss,
}

async fn upsert_watermark(
    db: &DatabaseConnection,
    user_id: i32,
    watermark: Watermark,
) -> ApiResult<()> {
    let now: DateTimeWithTimeZone = Utc::now().into();
    let existing = user_state::Entity::find_by_id(user_id).one(db).await?;

    if let Some(row) = existing {
        let mut model: user_state::ActiveModel = row.into();
        model.updated_at = Set(now);
        match watermark {
            Watermark::Read => model.read_watermark_at = Set(Some(now)),
            Watermark::Dismiss => model.dismiss_watermark_at = Set(Some(now)),
        }
        model.update(db).await?;
    } else {
        let mut model = user_state::ActiveModel {
            user_id: Set(user_id),
            updated_at: Set(now),
            ..Default::default()
        };
        match watermark {
            Watermark::Read => model.read_watermark_at = Set(Some(now)),
            Watermark::Dismiss => model.dismiss_watermark_at = Set(Some(now)),
        }
        user_state::Entity::insert(model).exec(db).await?;
    }

    Ok(())
}

async fn count_visible(
    db: &DatabaseConnection,
    user_id: i32,
    dismiss_watermark: Option<&DateTimeWithTimeZone>,
) -> ApiResult<i64> {
    let mut condition = Condition::all()
        .add(delivery::Column::Channel.eq("in_app"))
        .add(delivery::Column::RecipientUser.eq(user_id))
        .add(delivery::Column::DismissedAt.is_null());

    if let Some(dw) = dismiss_watermark {
        condition = condition.add(delivery::Column::CreatedAt.gt(*dw));
    }

    let count = delivery::Entity::find().filter(condition).count(db).await?;

    Ok(count as i64)
}

async fn count_unread(
    db: &DatabaseConnection,
    user_id: i32,
    dismiss_watermark: Option<&DateTimeWithTimeZone>,
    read_watermark: Option<&DateTimeWithTimeZone>,
) -> ApiResult<i64> {
    let mut condition = Condition::all()
        .add(delivery::Column::Channel.eq("in_app"))
        .add(delivery::Column::RecipientUser.eq(user_id))
        .add(delivery::Column::DismissedAt.is_null())
        .add(delivery::Column::ReadAt.is_null());

    if let Some(dw) = dismiss_watermark {
        condition = condition.add(delivery::Column::CreatedAt.gt(*dw));
    }

    if let Some(rw) = read_watermark {
        condition = condition.add(delivery::Column::CreatedAt.gt(*rw));
    }

    let count = delivery::Entity::find().filter(condition).count(db).await?;

    Ok(count as i64)
}
