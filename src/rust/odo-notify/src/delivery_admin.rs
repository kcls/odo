//! Read-only admin views of notification delivery history: who was sent
//! what, when, and what failed.
//!
//! Everything requires `odo.notify.delivery.read`. Deliveries carry
//! rendered message bodies, so this is its own permission rather than
//! riding on template or email-group access.
//!
//! A delivery's status is one of `pending` (queued, possibly awaiting a
//! retry), `processing` (claimed by a worker), `delivered`, `failed`
//! (retries exhausted), or `skipped` (the recipient was deleted or
//! switched off before sending; terminal). For a group, `delivered` means
//! at least one member's address accepted it: the per-address outcome is
//! in `channel_metadata`, surfaced here as the failed/delivered addresses.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use odo_client::context::RequestContext;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::auth::usr;
use odo_entity::notification::{delivery, email_group, event};
use odo_service::admin::{Page, Paginated, Sort, SortDir, clean_search};
use sea_orm::prelude::*;
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::{Condition, DbBackend, Order, QueryOrder, QuerySelect, Statement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

const READ_PERM: &str = "odo.notify.delivery.read";

const STATUSES: &[&str] = &["pending", "processing", "delivered", "failed", "skipped"];
const CHANNELS: &[&str] = &["email", "in_app"];

/// The window summary and recipient views use when the caller gives none.
const DEFAULT_WINDOW_DAYS: i64 = 7;

// ===========================================================================
// Shapes
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecipientKind {
    User,
    EmailGroup,
}

/// Who a delivery was addressed to, decorated for display.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeliveryRecipient {
    pub kind: RecipientKind,
    pub id: i32,
    /// A user's display name, or a group's label.
    pub label: String,
    /// A user's email, or a group's code.
    pub detail: String,
    /// False for a deleted user, or a deleted or deactivated group: email
    /// to it is skipped at send time.
    pub active: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryRow {
    pub id: i64,
    pub event_id: i64,
    pub channel: String,
    pub template_code: Option<String>,
    /// The rendered subject (email) or title (in-app).
    pub subject: String,
    pub status: String,
    /// None only if the recipient row no longer exists at all.
    pub recipient: Option<DeliveryRecipient>,
    pub created_at: DateTime<FixedOffset>,
    pub scheduled_for: Option<DateTime<FixedOffset>>,
    pub processed_at: Option<DateTime<FixedOffset>>,
    pub next_retry_at: Option<DateTime<FixedOffset>>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// Addresses that refused a delivery reported as delivered (a group's
    /// partial failure); 0 when none or not recorded.
    pub failed_address_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryEvent {
    pub id: i64,
    pub template_code: Option<String>,
    pub source_service: Option<String>,
    pub source_entity_type: Option<String>,
    pub source_entity_id: Option<i64>,
    pub created_by: Option<i32>,
    pub created_by_name: Option<String>,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryDetailResponse {
    pub delivery: DeliveryRow,
    /// The rendered body: HTML for email, text for in-app.
    pub body: Option<String>,
    pub action_url: Option<String>,
    pub delivered_addresses: Vec<String>,
    pub failed_addresses: Vec<String>,
    pub read_at: Option<DateTime<FixedOffset>>,
    pub dismissed_at: Option<DateTime<FixedOffset>>,
    pub event: Option<DeliveryEvent>,
}

odo_service::page_type!(
    DeliveryPage,
    DeliveryRow,
    "One page of notification deliveries."
);

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusCount {
    pub channel: String,
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliverySummaryResponse {
    pub since: DateTime<FixedOffset>,
    pub until: Option<DateTime<FixedOffset>>,
    /// One row per (channel, status) with any deliveries in the window.
    pub counts: Vec<StatusCount>,
}

/// One recipient's deliveries in a window.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecipientActivityRow {
    pub recipient: Option<DeliveryRecipient>,
    pub total: i64,
    pub delivered: i64,
    pub failed: i64,
    pub skipped: i64,
    /// Pending or processing.
    pub queued: i64,
    pub last_created_at: DateTime<FixedOffset>,
    pub last_delivered_at: Option<DateTime<FixedOffset>>,
    pub last_failed_at: Option<DateTime<FixedOffset>>,
}

odo_service::page_type!(
    RecipientActivityPage,
    RecipientActivityRow,
    "One page of per-recipient delivery activity."
);

#[derive(Deserialize, ToSchema)]
pub struct ListDeliveriesRequest {
    /// Any of: pending, processing, delivered, failed, skipped.
    #[serde(default)]
    status: Option<Vec<String>>,
    /// email or in_app.
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    template_code: Option<String>,
    #[serde(default)]
    recipient_user: Option<i32>,
    #[serde(default)]
    recipient_email_group: Option<i32>,
    /// Created at or after.
    #[serde(default)]
    since: Option<DateTime<FixedOffset>>,
    /// Created before.
    #[serde(default)]
    until: Option<DateTime<FixedOffset>>,
    /// Case-insensitive substring of the rendered subject.
    #[serde(default)]
    search: Option<String>,
    #[serde(flatten)]
    page: Page,
    #[serde(flatten)]
    sort: Sort,
}

#[derive(Deserialize, ToSchema)]
pub struct DeliveryIdRequest {
    id: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct DeliveryWindowRequest {
    /// Created at or after; defaults to 7 days ago.
    #[serde(default)]
    since: Option<DateTime<FixedOffset>>,
    /// Created before; open-ended when absent.
    #[serde(default)]
    until: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    channel: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListRecipientActivityRequest {
    /// Created at or after; defaults to 7 days ago.
    #[serde(default)]
    since: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    until: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    kind: Option<RecipientKind>,
    #[serde(flatten)]
    page: Page,
    /// Sortable by total, failed, or last (most recent delivery).
    #[serde(flatten)]
    sort: Sort,
}

// ===========================================================================
// Handlers
// ===========================================================================

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/delivery/list",
    request_body = ListDeliveriesRequest,
    responses((status = 200, body = DeliveryPage, description = "Deliveries, newest first by default")),
    security(("bearer" = [])),
    tag = "delivery-admin"
)]
pub async fn list_deliveries(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListDeliveriesRequest>,
) -> ApiResult<Json<DeliveryPage>> {
    require_read(&state).await?;

    let mut condition = Condition::all();
    if let Some(statuses) = params.status.filter(|s| !s.is_empty()) {
        for s in &statuses {
            check_allowed("status", s, STATUSES)?;
        }
        condition = condition.add(delivery::Column::Status.is_in(statuses));
    }
    if let Some(channel) = params.channel.as_deref() {
        check_allowed("channel", channel, CHANNELS)?;
        condition = condition.add(delivery::Column::Channel.eq(channel));
    }
    if let Some(code) = params
        .template_code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        condition = condition.add(delivery::Column::TemplateCode.eq(code));
    }
    if let Some(user) = params.recipient_user {
        condition = condition.add(delivery::Column::RecipientUser.eq(user));
    }
    if let Some(group) = params.recipient_email_group {
        condition = condition.add(delivery::Column::RecipientEmailGroup.eq(group));
    }
    if let Some(since) = params.since {
        condition = condition.add(delivery::Column::CreatedAt.gte(since));
    }
    if let Some(until) = params.until {
        condition = condition.add(delivery::Column::CreatedAt.lt(until));
    }
    if let Some(search) = clean_search(params.search.as_deref()) {
        // Column::contains is a case-sensitive LIKE; subjects are searched
        // the way people type them.
        condition = condition
            .add(Expr::col(delivery::Column::TitleRendered).ilike(contains_pattern(&search)));
    }

    let total = delivery::Entity::find()
        .filter(condition.clone())
        .count(&state.db)
        .await? as i64;

    // Newest first unless asked otherwise: this is a history.
    let (sort_col, sort_ord) = params.sort.resolve(
        &[
            ("created", delivery::Column::CreatedAt),
            ("processed", delivery::Column::ProcessedAt),
            ("status", delivery::Column::Status),
            ("channel", delivery::Column::Channel),
            ("template", delivery::Column::TemplateCode),
        ],
        (delivery::Column::CreatedAt, Order::Desc),
    );

    let deliveries = delivery::Entity::find()
        .filter(condition)
        .order_by(sort_col, sort_ord)
        .order_by_desc(delivery::Column::Id)
        .limit(params.page.limit())
        .offset(params.page.offset())
        .all(&state.db)
        .await?;

    let labels = RecipientLabels::load(
        &state.db,
        deliveries.iter().filter_map(|d| d.recipient_user),
        deliveries.iter().filter_map(|d| d.recipient_email_group),
    )
    .await?;

    let rows = deliveries
        .iter()
        .map(|d| delivery_row(d, &labels))
        .collect();
    Ok(Json(Paginated::new(rows, total).into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/delivery/get",
    request_body = DeliveryIdRequest,
    responses((status = 200, body = DeliveryDetailResponse, description = "One delivery with its message and event")),
    security(("bearer" = [])),
    tag = "delivery-admin"
)]
pub async fn get_delivery(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeliveryIdRequest>,
) -> ApiResult<Json<DeliveryDetailResponse>> {
    require_read(&state).await?;

    let dlv = delivery::Entity::find_by_id(params.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("delivery {}", params.id)))?;

    let evt = event::Entity::find_by_id(dlv.event_id)
        .one(&state.db)
        .await?;
    let creator = match evt.as_ref().and_then(|e| e.created_by) {
        Some(id) => usr::Entity::find_by_id(id).one(&state.db).await?,
        None => None,
    };

    let labels =
        RecipientLabels::load(&state.db, dlv.recipient_user, dlv.recipient_email_group).await?;

    Ok(Json(DeliveryDetailResponse {
        delivery: delivery_row(&dlv, &labels),
        body: dlv.body_rendered.clone(),
        action_url: dlv.action_url.clone(),
        delivered_addresses: metadata_addresses(dlv.channel_metadata.as_ref(), "successful_emails"),
        failed_addresses: metadata_addresses(dlv.channel_metadata.as_ref(), "failed_emails"),
        read_at: dlv.read_at,
        dismissed_at: dlv.dismissed_at,
        event: evt.map(|e| DeliveryEvent {
            id: e.id,
            template_code: e.template_code,
            source_service: e.source_service,
            source_entity_type: e.source_entity_type,
            source_entity_id: e.source_entity_id,
            created_by: e.created_by,
            created_by_name: creator.map(|u| u.display_name),
            created_at: e.created_at,
        }),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/delivery/summary",
    request_body = DeliveryWindowRequest,
    responses((status = 200, body = DeliverySummaryResponse, description = "Delivery counts by channel and status")),
    security(("bearer" = [])),
    tag = "delivery-admin"
)]
pub async fn delivery_summary(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeliveryWindowRequest>,
) -> ApiResult<Json<DeliverySummaryResponse>> {
    require_read(&state).await?;

    let since = params.since.unwrap_or_else(default_since);
    let mut condition = Condition::all().add(delivery::Column::CreatedAt.gte(since));
    if let Some(until) = params.until {
        condition = condition.add(delivery::Column::CreatedAt.lt(until));
    }
    if let Some(channel) = params.channel.as_deref() {
        check_allowed("channel", channel, CHANNELS)?;
        condition = condition.add(delivery::Column::Channel.eq(channel));
    }

    let counts: Vec<(String, String, i64)> = delivery::Entity::find()
        .select_only()
        .column(delivery::Column::Channel)
        .column(delivery::Column::Status)
        .column_as(delivery::Column::Id.count(), "count")
        .filter(condition)
        .group_by(delivery::Column::Channel)
        .group_by(delivery::Column::Status)
        .order_by_asc(delivery::Column::Channel)
        .order_by_asc(delivery::Column::Status)
        .into_tuple()
        .all(&state.db)
        .await?;

    Ok(Json(DeliverySummaryResponse {
        since,
        until: params.until,
        counts: counts
            .into_iter()
            .map(|(channel, status, count)| StatusCount {
                channel,
                status,
                count,
            })
            .collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/notify/delivery/recipients",
    request_body = ListRecipientActivityRequest,
    responses((status = 200, body = RecipientActivityPage, description = "Delivery activity per recipient")),
    security(("bearer" = [])),
    tag = "delivery-admin"
)]
pub async fn list_recipient_activity(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListRecipientActivityRequest>,
) -> ApiResult<Json<RecipientActivityPage>> {
    require_read(&state).await?;

    if let Some(channel) = params.channel.as_deref() {
        check_allowed("channel", channel, CHANNELS)?;
    }
    let since = params.since.unwrap_or_else(default_since);
    let kind = params.kind.map(|k| match k {
        RecipientKind::User => "user",
        RecipientKind::EmailGroup => "email_group",
    });

    // The filters are fixed SQL with nullable parameters ($n IS NULL OR ...)
    // so the statement text never varies with client input.
    const FROM_WHERE: &str = "FROM notification.delivery \
         WHERE created_at >= $1 \
           AND ($2::timestamptz IS NULL OR created_at < $2) \
           AND ($3::text IS NULL OR channel = $3) \
           AND ($4::text IS NULL \
                OR ($4 = 'user' AND recipient_user IS NOT NULL) \
                OR ($4 = 'email_group' AND recipient_email_group IS NOT NULL))";
    let filter_values: [sea_orm::Value; 4] = [
        since.into(),
        params.until.into(),
        params.channel.clone().into(),
        kind.map(str::to_string).into(),
    ];

    let total_row = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "SELECT COUNT(*) AS total FROM (SELECT 1 {FROM_WHERE} \
                 GROUP BY recipient_user, recipient_email_group) g"
            ),
            filter_values.clone(),
        ))
        .await?;
    let total: i64 = match total_row {
        Some(row) => row.try_get("", "total")?,
        None => 0,
    };

    // Aggregates aren't entity columns, so Sort::resolve can't map them;
    // the same rule applies: only allow-listed keys, each a fixed SQL
    // expression, and anything else falls back to the default.
    let (order_expr, dir) = match params.sort.sort_by.as_deref() {
        Some("total") => ("total", params.sort.sort_dir),
        Some("failed") => ("failed", params.sort.sort_dir),
        Some("last") => ("last_created_at", params.sort.sort_dir),
        _ => ("last_created_at", Some(SortDir::Desc)),
    };
    let dir = match dir {
        Some(SortDir::Desc) => "DESC",
        _ => "ASC",
    };

    let mut values = filter_values.to_vec();
    values.push((params.page.limit() as i64).into());
    values.push((params.page.offset() as i64).into());

    let rows = state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "SELECT recipient_user, recipient_email_group, \
                        COUNT(*) AS total, \
                        COUNT(*) FILTER (WHERE status = 'delivered') AS delivered, \
                        COUNT(*) FILTER (WHERE status = 'failed') AS failed, \
                        COUNT(*) FILTER (WHERE status = 'skipped') AS skipped, \
                        COUNT(*) FILTER (WHERE status IN ('pending', 'processing')) AS queued, \
                        MAX(created_at) AS last_created_at, \
                        MAX(processed_at) FILTER (WHERE status = 'delivered') AS last_delivered_at, \
                        MAX(updated_at) FILTER (WHERE status = 'failed') AS last_failed_at \
                 {FROM_WHERE} \
                 GROUP BY recipient_user, recipient_email_group \
                 ORDER BY {order_expr} {dir} NULLS LAST, \
                          recipient_email_group NULLS LAST, recipient_user NULLS LAST \
                 LIMIT $5 OFFSET $6"
            ),
            values,
        ))
        .await?;

    struct Aggregate {
        user: Option<i32>,
        group: Option<i32>,
        row: RecipientActivityRow,
    }
    let mut aggregates = Vec::with_capacity(rows.len());
    for r in rows {
        aggregates.push(Aggregate {
            user: r.try_get("", "recipient_user")?,
            group: r.try_get("", "recipient_email_group")?,
            row: RecipientActivityRow {
                recipient: None,
                total: r.try_get("", "total")?,
                delivered: r.try_get("", "delivered")?,
                failed: r.try_get("", "failed")?,
                skipped: r.try_get("", "skipped")?,
                queued: r.try_get("", "queued")?,
                last_created_at: r.try_get("", "last_created_at")?,
                last_delivered_at: r.try_get("", "last_delivered_at")?,
                last_failed_at: r.try_get("", "last_failed_at")?,
            },
        });
    }

    let labels = RecipientLabels::load(
        &state.db,
        aggregates.iter().filter_map(|a| a.user),
        aggregates.iter().filter_map(|a| a.group),
    )
    .await?;

    let rows = aggregates
        .into_iter()
        .map(|a| RecipientActivityRow {
            recipient: labels.recipient(a.user, a.group),
            ..a.row
        })
        .collect();

    Ok(Json(Paginated::new(rows, total).into()))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn require_read(state: &AppState) -> ApiResult<()> {
    RequestContext::user_id().ok_or(LocalError::unauthenticated())?;
    state
        .auth_client
        .permission_required(READ_PERM, None)
        .await?;
    Ok(())
}

fn default_since() -> DateTime<FixedOffset> {
    (Utc::now() - Duration::days(DEFAULT_WINDOW_DAYS)).into()
}

/// A filter value outside its known set is a client bug; say so rather
/// than return an empty list that looks like "nothing happened".
fn check_allowed(field: &str, value: &str, allowed: &[&str]) -> Result<(), LocalError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(LocalError::invalid_input(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

/// An ILIKE pattern matching `term` anywhere, with LIKE's wildcards in
/// the term itself matched literally (backslash is Postgres's default
/// escape character).
fn contains_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// The addresses under `key` in a delivery's `channel_metadata`, as the
/// processor records them (`successful_emails` / `failed_emails`).
fn metadata_addresses(metadata: Option<&serde_json::Value>, key: &str) -> Vec<String> {
    metadata
        .and_then(|m| m.get(key))
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn delivery_row(d: &delivery::Model, labels: &RecipientLabels) -> DeliveryRow {
    DeliveryRow {
        id: d.id,
        event_id: d.event_id,
        channel: d.channel.clone(),
        template_code: d.template_code.clone(),
        subject: d.title_rendered.clone(),
        status: d.status.clone(),
        recipient: labels.recipient(d.recipient_user, d.recipient_email_group),
        created_at: d.created_at,
        scheduled_for: d.scheduled_for,
        processed_at: d.processed_at,
        next_retry_at: d.next_retry_at,
        retry_count: d.retry_count,
        max_retries: d.max_retries,
        error_code: d.error_code.clone(),
        error_message: d.error_message.clone(),
        failed_address_count: metadata_addresses(d.channel_metadata.as_ref(), "failed_emails").len()
            as i64,
    }
}

/// Display data for the users and groups a set of deliveries names,
/// fetched in two queries rather than one per row.
struct RecipientLabels {
    users: HashMap<i32, usr::Model>,
    groups: HashMap<i32, email_group::Model>,
}

impl RecipientLabels {
    async fn load(
        db: &DatabaseConnection,
        user_ids: impl IntoIterator<Item = i32>,
        group_ids: impl IntoIterator<Item = i32>,
    ) -> Result<Self, DbErr> {
        let mut user_ids: Vec<i32> = user_ids.into_iter().collect();
        user_ids.sort_unstable();
        user_ids.dedup();
        let mut group_ids: Vec<i32> = group_ids.into_iter().collect();
        group_ids.sort_unstable();
        group_ids.dedup();

        let users = if user_ids.is_empty() {
            HashMap::new()
        } else {
            usr::Entity::find()
                .filter(usr::Column::Id.is_in(user_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|u| (u.id, u))
                .collect()
        };
        let groups = if group_ids.is_empty() {
            HashMap::new()
        } else {
            email_group::Entity::find()
                .filter(email_group::Column::Id.is_in(group_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|g| (g.id, g))
                .collect()
        };
        Ok(Self { users, groups })
    }

    fn recipient(&self, user: Option<i32>, group: Option<i32>) -> Option<DeliveryRecipient> {
        if let Some(id) = user {
            let u = self.users.get(&id)?;
            return Some(DeliveryRecipient {
                kind: RecipientKind::User,
                id,
                label: u.display_name.clone(),
                detail: u.email.clone(),
                active: u.deleted_at.is_none() && u.status.as_deref() != Some("deleted"),
            });
        }
        let id = group?;
        let g = self.groups.get(&id)?;
        Some(DeliveryRecipient {
            kind: RecipientKind::EmailGroup,
            id,
            label: g.label.clone(),
            detail: g.code.clone(),
            active: g.deleted_at.is_none() && g.is_active,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn metadata_addresses_reads_processor_output() {
        let meta = json!({
            "successful_emails": ["a@example.org", "b@example.org"],
            "failed_emails": ["c@example.org"],
            "total_recipients": 3,
        });
        assert_eq!(
            metadata_addresses(Some(&meta), "failed_emails"),
            vec!["c@example.org".to_string()]
        );
        assert_eq!(
            metadata_addresses(Some(&meta), "successful_emails").len(),
            2
        );
    }

    #[test]
    fn metadata_addresses_tolerates_missing_or_odd_metadata() {
        assert!(metadata_addresses(None, "failed_emails").is_empty());
        assert!(metadata_addresses(Some(&json!({})), "failed_emails").is_empty());
        assert!(
            metadata_addresses(Some(&json!({"failed_emails": "x"})), "failed_emails").is_empty()
        );
        assert_eq!(
            metadata_addresses(Some(&json!({"failed_emails": ["a@x", 7]})), "failed_emails"),
            vec!["a@x".to_string()]
        );
    }

    #[test]
    fn contains_pattern_escapes_like_wildcards() {
        assert_eq!(contains_pattern("ada"), "%ada%");
        assert_eq!(contains_pattern("100%"), "%100\\%%");
        assert_eq!(contains_pattern("a_b"), "%a\\_b%");
        assert_eq!(contains_pattern("c:\\x"), "%c:\\\\x%");
    }

    #[test]
    fn check_allowed_rejects_unknown_values() {
        assert!(check_allowed("status", "failed", STATUSES).is_ok());
        assert!(check_allowed("status", "FAILED", STATUSES).is_err());
        assert!(check_allowed("channel", "sms", CHANNELS).is_err());
    }
}
