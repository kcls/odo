use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::notification::{delivery, event, template};
use sea_orm::prelude::*;
use sea_orm::{Condition, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

#[derive(Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Recipient {
    User {
        user_id: i64,
        channels: Vec<String>,
    },
    EmailGroup {
        email_group_id: i64,
        channels: Vec<String>,
    },
}

impl Recipient {
    fn channels(&self) -> &[String] {
        match self {
            Recipient::User { channels, .. } => channels,
            Recipient::EmailGroup { channels, .. } => channels,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct EnqueueRequest {
    recipients: Vec<Recipient>,
    template_code: String,
    #[serde(default)]
    template_variables: serde_json::Value,
    #[serde(default)]
    source_service: Option<String>,
    #[serde(default)]
    source_entity_type: Option<String>,
    #[serde(default)]
    source_entity_id: Option<i64>,
    #[serde(default)]
    dedup_key: Option<String>,
    #[serde(default)]
    scheduled_for: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryInfo {
    pub id: i64,
    pub channel: String,
    pub status: String,
    pub recipient_user: Option<i32>,
    pub recipient_email_group: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnqueueResponse {
    pub event_id: i64,
    pub deliveries: Vec<DeliveryInfo>,
    pub is_duplicate: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/enqueue",
    request_body = EnqueueRequest,
    responses((status = 200, body = EnqueueResponse, description = "Notification enqueued")),
    security(("bearer" = [])),
    tag = "enqueue"
)]
pub async fn enqueue(
    State(state): State<Arc<AppState>>,
    Json(params): Json<EnqueueRequest>,
) -> ApiResult<Json<EnqueueResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    state
        .auth_client
        .permission_required("odo.notify.send", None)
        .await?;

    if params.recipients.is_empty() {
        return Err(LocalError::invalid_input("Recipients list is empty").into());
    }

    // Validate recipients
    for recipient in &params.recipients {
        if recipient.channels().is_empty() {
            return Err(LocalError::invalid_input("Recipient has no channels").into());
        }
        for ch in recipient.channels() {
            if ch != "in_app" && ch != "email" {
                return Err(LocalError::invalid_input(format!("Unknown channel: {ch}")).into());
            }
        }
        if let Recipient::EmailGroup { channels, .. } = recipient
            && channels.iter().any(|c| c != "email")
        {
            return Err(
                LocalError::invalid_input("Email groups can only use the email channel").into(),
            );
        }
    }

    // Deduplication check
    if let Some(ref dedup_key) = params.dedup_key
        && let Some(existing_event_id) = find_existing_event(
            &state.db,
            dedup_key,
            params.source_service.as_deref(),
            params.source_entity_type.as_deref(),
            params.source_entity_id,
        )
        .await?
    {
        let deliveries = get_event_deliveries(&state.db, existing_event_id).await?;
        return Ok(Json(EnqueueResponse {
            event_id: existing_event_id,
            deliveries,
            is_duplicate: true,
        }));
    }

    // Fetch and validate template
    let tmpl = template::Entity::find()
        .filter(template::Column::Code.eq(&params.template_code))
        .filter(template::Column::DeletedAt.is_null())
        .filter(template::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found(format!(
            "template code={}",
            params.template_code
        )))?;

    // Render subject
    let title_rendered =
        odo_service::template::render(&tmpl.subject_template, &params.template_variables)
            .map_err(|e| LocalError::internal(format!("Template render error: {e}")))?;

    // Validate action_url
    let action_url = params
        .template_variables
        .get("action_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(ref url) = action_url
        && (!url.starts_with('/') || url.starts_with("//"))
    {
        return Err(LocalError::invalid_input(
            "action_url must be a relative path starting with /",
        )
        .into());
    }

    // Create event
    let scheduled_for = params
        .scheduled_for
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let now = Utc::now();

    let new_event = event::ActiveModel {
        dedup_key: Set(params.dedup_key.clone()),
        template_code: Set(Some(params.template_code.clone())),
        template_variables: Set(params.template_variables.clone()),
        source_service: Set(params.source_service.clone()),
        source_entity_type: Set(params.source_entity_type.clone()),
        source_entity_id: Set(params.source_entity_id),
        created_by: Set(Some(user_id)),
        created_at: Set(now.into()),
        ..Default::default()
    };

    let evt = event::Entity::insert(new_event)
        .exec_with_returning(&state.db)
        .await?;

    // Create deliveries
    let mut delivery_infos = Vec::new();

    for recipient in &params.recipients {
        for channel in recipient.channels() {
            let body_template = if channel == "email" {
                tmpl.body_template_html
                    .as_deref()
                    .unwrap_or(&tmpl.body_template)
            } else {
                &tmpl.body_template
            };

            let body_rendered =
                odo_service::template::render(body_template, &params.template_variables)
                    .map_err(|e| LocalError::internal(format!("Template render error: {e}")))?;

            let (recipient_user, recipient_email_group) = match recipient {
                Recipient::User { user_id, .. } => (Some(*user_id as i32), None),
                Recipient::EmailGroup { email_group_id, .. } => {
                    (None, Some(*email_group_id as i32))
                }
            };

            let delivery_action_url = if channel == "in_app" {
                action_url.clone()
            } else {
                None
            };

            let status = if channel == "in_app" {
                "delivered"
            } else {
                "pending"
            };

            let new_delivery = delivery::ActiveModel {
                event_id: Set(evt.id),
                channel: Set(channel.clone()),
                template_code: Set(Some(params.template_code.clone())),
                title_rendered: Set(title_rendered.clone()),
                body_rendered: Set(Some(body_rendered)),
                action_url: Set(delivery_action_url),
                status: Set(status.to_string()),
                scheduled_for: Set(scheduled_for.map(|dt| dt.into())),
                recipient_user: Set(recipient_user),
                recipient_email_group: Set(recipient_email_group),
                created_at: Set(now.into()),
                updated_at: Set(now.into()),
                ..Default::default()
            };

            let d = delivery::Entity::insert(new_delivery)
                .exec_with_returning(&state.db)
                .await?;

            delivery_infos.push(DeliveryInfo {
                id: d.id,
                channel: d.channel,
                status: d.status,
                recipient_user: d.recipient_user,
                recipient_email_group: d.recipient_email_group,
            });
        }
    }

    Ok(Json(EnqueueResponse {
        event_id: evt.id,
        deliveries: delivery_infos,
        is_duplicate: false,
    }))
}

async fn find_existing_event(
    db: &DatabaseConnection,
    dedup_key: &str,
    source_service: Option<&str>,
    source_entity_type: Option<&str>,
    source_entity_id: Option<i64>,
) -> ApiResult<Option<i64>> {
    let mut condition = Condition::all().add(event::Column::DedupKey.eq(dedup_key));

    match source_service {
        Some(v) => condition = condition.add(event::Column::SourceService.eq(v)),
        None => condition = condition.add(event::Column::SourceService.is_null()),
    }
    match source_entity_type {
        Some(v) => condition = condition.add(event::Column::SourceEntityType.eq(v)),
        None => condition = condition.add(event::Column::SourceEntityType.is_null()),
    }
    match source_entity_id {
        Some(v) => condition = condition.add(event::Column::SourceEntityId.eq(v)),
        None => condition = condition.add(event::Column::SourceEntityId.is_null()),
    }

    let row = event::Entity::find().filter(condition).one(db).await?;

    Ok(row.map(|e| e.id))
}

async fn get_event_deliveries(
    db: &DatabaseConnection,
    event_id: i64,
) -> ApiResult<Vec<DeliveryInfo>> {
    let rows = delivery::Entity::find()
        .filter(delivery::Column::EventId.eq(event_id))
        .all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|d| DeliveryInfo {
            id: d.id,
            channel: d.channel,
            status: d.status,
            recipient_user: d.recipient_user,
            recipient_email_group: d.recipient_email_group,
        })
        .collect())
}
