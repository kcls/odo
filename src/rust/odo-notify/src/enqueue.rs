use axum::Json;
use axum::extract::State;
use chrono::Utc;
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::auth::usr;
use odo_entity::notification::{delivery, email_group, event, template};
use sea_orm::prelude::*;
use sea_orm::{Condition, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

/// A recipient is referenced by id or by stable uuid. Callers outside the
/// odo database (apps that hold only uuids) use the uuid; when both are
/// given, the id wins.
#[derive(Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Recipient {
    User {
        #[serde(default)]
        user_id: Option<i64>,
        #[serde(default)]
        user_uuid: Option<String>,
        channels: Vec<String>,
    },
    EmailGroup {
        #[serde(default)]
        email_group_id: Option<i64>,
        #[serde(default)]
        email_group_uuid: Option<String>,
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

/// A delivery's body for its channel. Email is always sent as HTML: the
/// template's HTML variant, with variables escaped; or, for a text-only
/// template, the text rendered verbatim and then escaped as a whole, with
/// its line breaks kept. In-app bodies are plain text, displayed as such.
fn render_body(
    channel: &str,
    body_template: &str,
    body_template_html: Option<&str>,
    vars: &serde_json::Value,
) -> Result<String, odo_service::template::RenderError> {
    use odo_service::template::{render, render_text, text_to_html};
    match body_template_html.filter(|html| !html.trim().is_empty()) {
        Some(html) if channel == "email" => render(html, vars),
        _ if channel == "email" => render_text(body_template, vars).map(|t| text_to_html(&t)),
        _ => render_text(body_template, vars),
    }
}

/// A recipient with its reference resolved to a database id.
enum ResolvedRecipient<'a> {
    User(i32, &'a [String]),
    EmailGroup(i32, &'a [String]),
}

impl ResolvedRecipient<'_> {
    fn channels(&self) -> &[String] {
        match self {
            ResolvedRecipient::User(_, channels) | ResolvedRecipient::EmailGroup(_, channels) => {
                channels
            }
        }
    }
}

fn parse_uuid(raw: &str) -> Result<Uuid, LocalError> {
    raw.parse()
        .map_err(|_| LocalError::invalid_input(format!("invalid uuid {raw}")))
}

/// Resolve each recipient's id-or-uuid reference. A uuid that matches no
/// row is a 404, so a caller holding a stale uuid finds out rather than
/// silently notifying nobody.
async fn resolve_recipients<'a>(
    db: &DatabaseConnection,
    recipients: &'a [Recipient],
) -> ApiResult<Vec<ResolvedRecipient<'a>>> {
    let mut out = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        let resolved = match recipient {
            Recipient::User {
                user_id: Some(id),
                channels,
                ..
            } => ResolvedRecipient::User(*id as i32, channels),
            Recipient::User {
                user_uuid: Some(uuid),
                channels,
                ..
            } => {
                let row = usr::Entity::find()
                    .filter(usr::Column::Uuid.eq(parse_uuid(uuid)?))
                    .one(db)
                    .await?
                    .ok_or_else(|| LocalError::not_found(format!("user {uuid}")))?;
                ResolvedRecipient::User(row.id, channels)
            }
            Recipient::User { .. } => {
                return Err(
                    LocalError::invalid_input("user recipient needs user_id or user_uuid").into(),
                );
            }
            Recipient::EmailGroup {
                email_group_id: Some(id),
                channels,
                ..
            } => ResolvedRecipient::EmailGroup(*id as i32, channels),
            Recipient::EmailGroup {
                email_group_uuid: Some(uuid),
                channels,
                ..
            } => {
                let row = email_group::Entity::find()
                    .filter(email_group::Column::Uuid.eq(parse_uuid(uuid)?))
                    .one(db)
                    .await?
                    .ok_or_else(|| LocalError::not_found(format!("email group {uuid}")))?;
                ResolvedRecipient::EmailGroup(row.id, channels)
            }
            Recipient::EmailGroup { .. } => {
                return Err(LocalError::invalid_input(
                    "email_group recipient needs email_group_id or email_group_uuid",
                )
                .into());
            }
        };
        out.push(resolved);
    }
    Ok(out)
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

    // The subject is plain text everywhere it goes (an email Subject
    // header, an in-app title), so it is not HTML-escaped.
    let title_rendered =
        odo_service::template::render_text(&tmpl.subject_template, &params.template_variables)
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

    // Resolved before the event is written, so a bad reference leaves no
    // orphan event behind.
    let recipients = resolve_recipients(&state.db, &params.recipients).await?;

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

    for recipient in &recipients {
        for channel in recipient.channels() {
            let body_rendered = render_body(
                channel,
                &tmpl.body_template,
                tmpl.body_template_html.as_deref(),
                &params.template_variables,
            )
            .map_err(|e| LocalError::internal(format!("Template render error: {e}")))?;

            let (recipient_user, recipient_email_group) = match recipient {
                ResolvedRecipient::User(id, _) => (Some(*id), None),
                ResolvedRecipient::EmailGroup(id, _) => (None, Some(*id)),
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

#[cfg(test)]
mod tests {
    use super::render_body;
    use serde_json::json;

    const TEXT: &str = "Hi {{name}}\nSee you";
    const HTML: &str = "<p>Hi {{name}}</p>";

    #[test]
    fn email_uses_the_html_variant_with_escaped_values() {
        let vars = json!({"name": "O'Brien & <Sons>"});
        let body = render_body("email", TEXT, Some(HTML), &vars).unwrap();
        assert_eq!(body, "<p>Hi O&#x27;Brien &amp; &lt;Sons&gt;</p>");
    }

    #[test]
    fn text_only_email_is_escaped_whole_with_line_breaks() {
        // Template text and values alike: nothing in a text template is markup.
        let vars = json!({"name": "<b>Ann</b>"});
        let body = render_body("email", TEXT, None, &vars).unwrap();
        assert_eq!(body, "Hi &lt;b&gt;Ann&lt;/b&gt;<br>\nSee you");
    }

    #[test]
    fn a_blank_html_variant_counts_as_none() {
        let vars = json!({"name": "Ann"});
        let body = render_body("email", TEXT, Some("  "), &vars).unwrap();
        assert_eq!(body, "Hi Ann<br>\nSee you");
    }

    #[test]
    fn in_app_bodies_are_plain_text_verbatim() {
        let vars = json!({"name": "O'Brien & Sons"});
        let body = render_body("in_app", TEXT, Some(HTML), &vars).unwrap();
        assert_eq!(body, "Hi O'Brien & Sons\nSee you");
    }
}
