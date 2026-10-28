//! Notification delivery history (odo-notify `delivery/*`, read-only).
//!
//! Requires the `e2e.odo.admin` test user, whose odo-admin role holds
//! `odo.notify.delivery.read` (sqitch change `008_notify_delivery_read`)
//! and `odo.notify.send`. Each test enqueues to an email group it creates,
//! so it can find its own deliveries among everything else in the table.
//! The background processor may claim, fail, or retry a delivery at any
//! moment, so tests never assert a particular status.

use integration_tests::*;
use serde_json::{Value, json};

const TEMPLATE: &str = "itest-delivery-template";
const STATUSES: &[&str] = &["pending", "processing", "delivered", "failed", "skipped"];
const ENDPOINTS: &[&str] = &["/list", "/get", "/summary", "/recipients", "/daily", "/retry"];

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/notify/delivery{}", notify_base(), path)
}

async fn post(c: &reqwest::Client, token: &str, path: &str, body: Value) -> reqwest::Response {
    c.post(base(path))
        .headers(auth_header(token))
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn post_ok(c: &reqwest::Client, token: &str, path: &str, body: Value) -> Value {
    let resp = post(c, token, path, body).await;
    assert_eq!(resp.status(), 200, "{path} failed");
    resp.json().await.unwrap()
}

/// A moment safely before anything this test enqueues, allowing for
/// clock skew between the test host and the database.
fn just_before() -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339()
}

/// The test template plus a fresh email group with one email delivery
/// queued to it. Returns (admin token, group json, delivery id).
async fn enqueue_to_new_group(c: &reqwest::Client) -> (String, Value, i64) {
    let token = odo_admin_token(c).await;

    let resp = c
        .post(format!(
            "{}/api/v1/odo/notify/template/create",
            notify_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({
            "code": TEMPLATE,
            "name": "Delivery admin test",
            "subject_template": "Delivery admin test for {{name}}",
            "body_template": "Body for {{name}}",
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 200 || resp.status() == 409,
        "template ensure failed"
    );

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let resp = c
        .post(format!("{}/api/v1/odo/notify/email-group/create", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"code": format!("itest-dlv-{nanos}"), "label": format!("Delivery test {nanos}")}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let group: Value = resp.json().await.unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "email_group", "email_group_id": group["id"], "channels": ["email"]}],
            "template_code": TEMPLATE,
            "template_variables": {"name": "Ada"},
            "source_service": "integration-tests",
            "source_entity_type": "delivery-admin-test",
            "source_entity_id": 42,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let queued: Value = resp.json().await.unwrap();
    let delivery_id = queued["deliveries"][0]["id"].as_i64().expect("delivery id");

    (token, group, delivery_id)
}

// --- Authn/authz ---

#[tokio::test]
async fn every_endpoint_requires_a_token() {
    let c = client();
    for path in ENDPOINTS {
        let resp = c
            .post(base(path))
            .json(&json!({"id": 1}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "{path} without a token");
    }
}

#[tokio::test]
async fn staff_without_the_permission_is_refused() {
    let c = client();
    let token = staff_token(&c).await;
    for path in ENDPOINTS {
        let resp = post(&c, &token, path, json!({"id": 1})).await;
        assert_eq!(resp.status(), 403, "{path} as staff");
    }
}

// --- list ---

#[tokio::test]
async fn list_finds_a_delivery_by_recipient_group() {
    let c = client();
    let (token, group, delivery_id) = enqueue_to_new_group(&c).await;

    let page = post_ok(
        &c,
        &token,
        "/list",
        json!({"recipient_email_group": group["id"]}),
    )
    .await;
    assert_eq!(page["total"], 1);
    let row = &page["rows"][0];
    assert_eq!(row["id"], delivery_id);
    assert_eq!(row["channel"], "email");
    assert_eq!(row["template_code"], TEMPLATE);
    assert_eq!(row["subject"], "Delivery admin test for Ada");
    assert!(STATUSES.contains(&row["status"].as_str().unwrap()));
    assert_eq!(row["recipient"]["kind"], "email_group");
    assert_eq!(row["recipient"]["id"], group["id"]);
    assert_eq!(row["recipient"]["label"], group["label"]);
    assert_eq!(row["recipient"]["detail"], group["code"]);
    assert_eq!(row["recipient"]["active"], true);
    assert!(row["failed_address_count"].is_number());
}

#[tokio::test]
async fn list_filters_compose() {
    let c = client();
    let (token, group, _) = enqueue_to_new_group(&c).await;
    let since = just_before();

    let matching = post_ok(
        &c,
        &token,
        "/list",
        json!({
            "recipient_email_group": group["id"],
            "channel": "email",
            "template_code": TEMPLATE,
            "since": since,
            "search": "for ada",
            "status": STATUSES,
        }),
    )
    .await;
    assert_eq!(matching["total"], 1, "every filter matches");

    for (field, value) in [
        ("channel", json!("in_app")),
        ("template_code", json!("no-such-template")),
        ("search", json!("no such subject")),
        ("until", json!(since)),
    ] {
        let mut body = json!({"recipient_email_group": group["id"]});
        body[field] = value;
        let page = post_ok(&c, &token, "/list", body).await;
        assert_eq!(page["total"], 0, "{field} excludes it");
    }
}

#[tokio::test]
async fn list_status_filter_only_returns_those_statuses() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let page = post_ok(
        &c,
        &token,
        "/list",
        json!({"status": ["failed", "skipped"], "limit": 50}),
    )
    .await;
    for row in page["rows"].as_array().unwrap() {
        let status = row["status"].as_str().unwrap();
        assert!(status == "failed" || status == "skipped", "got {status}");
    }
}

#[tokio::test]
async fn list_is_newest_first_by_default_and_honors_limit() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;
    let page = post_ok(&c, &token, "/list", json!({"limit": 5})).await;
    let rows = page["rows"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() <= 5);
    let created: Vec<chrono::DateTime<chrono::FixedOffset>> = rows
        .iter()
        .map(|r| chrono::DateTime::parse_from_rfc3339(r["created_at"].as_str().unwrap()).unwrap())
        .collect();
    assert!(
        created.windows(2).all(|w| w[0] >= w[1]),
        "not newest first: {created:?}"
    );
}

#[tokio::test]
async fn list_rejects_unknown_status_and_channel() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let resp = post(&c, &token, "/list", json!({"status": ["sent"]})).await;
    assert_eq!(resp.status(), 400, "unknown status");
    let resp = post(&c, &token, "/list", json!({"channel": "sms"})).await;
    assert_eq!(resp.status(), 400, "unknown channel");
}

#[tokio::test]
async fn list_labels_user_recipients() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;
    let staff = staff_id(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff, "channels": ["in_app"]}],
            "template_code": TEMPLATE,
            "template_variables": {"name": "Staff"},
            "source_service": "integration-tests",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let page = post_ok(
        &c,
        &token,
        "/list",
        json!({"recipient_user": staff, "channel": "in_app", "limit": 1}),
    )
    .await;
    let recipient = &page["rows"][0]["recipient"];
    assert_eq!(recipient["kind"], "user");
    assert_eq!(recipient["id"], staff);
    assert!(
        recipient["detail"].as_str().unwrap().contains('@'),
        "detail is the email"
    );
}

// --- get ---

#[tokio::test]
async fn get_returns_the_message_and_its_event() {
    let c = client();
    let (token, group, delivery_id) = enqueue_to_new_group(&c).await;

    let detail = post_ok(&c, &token, "/get", json!({"id": delivery_id})).await;
    assert_eq!(detail["delivery"]["id"], delivery_id);
    assert_eq!(detail["delivery"]["recipient"]["id"], group["id"]);
    assert!(detail["body"].as_str().unwrap().contains("Ada"));
    assert!(detail["delivered_addresses"].is_array());
    assert!(detail["failed_addresses"].is_array());
    let event = &detail["event"];
    assert_eq!(event["id"], detail["delivery"]["event_id"]);
    assert_eq!(event["template_code"], TEMPLATE);
    assert_eq!(event["source_service"], "integration-tests");
    assert_eq!(event["source_entity_type"], "delivery-admin-test");
    assert_eq!(event["source_entity_id"], 42);
    assert!(event["created_by_name"].is_string(), "enqueuer's name");
}

#[tokio::test]
async fn get_unknown_delivery_is_404() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let resp = post(&c, &token, "/get", json!({"id": i64::MAX})).await;
    assert_eq!(resp.status(), 404);
}

// --- summary ---

#[tokio::test]
async fn summary_counts_recent_deliveries() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;

    let summary = post_ok(
        &c,
        &token,
        "/summary",
        json!({"since": just_before(), "channel": "email"}),
    )
    .await;
    let counts = summary["counts"].as_array().unwrap();
    assert!(counts.iter().all(|r| r["channel"] == "email"));
    let total: i64 = counts.iter().map(|r| r["count"].as_i64().unwrap()).sum();
    assert!(total >= 1, "the delivery just queued is counted");
}

#[tokio::test]
async fn summary_defaults_to_the_last_week() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let summary = post_ok(&c, &token, "/summary", json!({})).await;
    let since = chrono::DateTime::parse_from_rfc3339(summary["since"].as_str().unwrap()).unwrap();
    let days = (chrono::Utc::now() - since.with_timezone(&chrono::Utc)).num_hours() as f64 / 24.0;
    assert!((6.9..7.1).contains(&days), "window was {days} days");
}

#[tokio::test]
async fn summary_of_an_empty_window_is_empty() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let future = (chrono::Utc::now() + chrono::Duration::days(365)).to_rfc3339();
    let summary = post_ok(&c, &token, "/summary", json!({"since": future})).await;
    assert_eq!(summary["counts"], json!([]));
}

// --- recipients ---

#[tokio::test]
async fn recipients_aggregate_per_group() {
    let c = client();
    let (token, group, _) = enqueue_to_new_group(&c).await;

    let page = post_ok(
        &c,
        &token,
        "/recipients",
        json!({"since": just_before(), "kind": "email_group", "limit": 200}),
    )
    .await;
    assert!(page["total"].as_i64().unwrap() >= 1);
    let rows = page["rows"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["recipient"]["kind"] == "email_group"));

    let ours = rows
        .iter()
        .find(|r| r["recipient"]["id"] == group["id"])
        .expect("the new group appears");
    assert_eq!(ours["total"], 1);
    let parts: i64 = ["delivered", "failed", "skipped", "queued"]
        .iter()
        .map(|k| ours[*k].as_i64().unwrap())
        .sum();
    assert_eq!(parts, 1, "statuses partition the total");
    assert!(ours["last_created_at"].is_string());
}

#[tokio::test]
async fn recipients_sort_by_total() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;
    let page = post_ok(
        &c,
        &token,
        "/recipients",
        json!({"since": just_before(), "sort_by": "total", "sort_dir": "desc", "limit": 50}),
    )
    .await;
    let totals: Vec<i64> = page["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["total"].as_i64().unwrap())
        .collect();
    assert!(
        totals.windows(2).all(|w| w[0] >= w[1]),
        "not sorted: {totals:?}"
    );
}

#[tokio::test]
async fn recipients_ignore_an_unknown_sort_key() {
    let c = client();
    let token = odo_admin_token(&c).await;
    // Not an allow-listed key: falls back to the default, never reaches SQL.
    let resp = post(
        &c,
        &token,
        "/recipients",
        json!({"sort_by": "total; DROP TABLE notification.delivery", "sort_dir": "asc"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
}

// --- daily ---

#[tokio::test]
async fn daily_has_every_day_and_counts_today() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;
    let since = (chrono::Utc::now() - chrono::Duration::days(6)).to_rfc3339();

    let daily = post_ok(&c, &token, "/daily", json!({"since": since, "channel": "email", "timezone": "UTC"})).await;
    assert_eq!(daily["timezone"], "UTC");
    let days = daily["days"].as_array().unwrap();
    // Six days back through today, quiet days included.
    assert!((7..=8).contains(&days.len()), "got {} days", days.len());
    let dates: Vec<&str> = days.iter().map(|d| d["day"].as_str().unwrap()).collect();
    assert!(dates.windows(2).all(|w| w[0] < w[1]), "oldest first: {dates:?}");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let last = days.last().unwrap();
    assert_eq!(last["day"], today);
    let total: i64 = ["delivered", "failed", "skipped", "queued"].iter().map(|k| last[*k].as_i64().unwrap()).sum();
    assert!(total >= 1, "today counts the delivery just queued");
}

#[tokio::test]
async fn daily_rejects_bad_input() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let now = chrono::Utc::now();
    for body in [
        json!({"timezone": "Mars/Olympus_Mons"}),
        json!({"channel": "sms"}),
        json!({"since": (now - chrono::Duration::days(400)).to_rfc3339()}),
        json!({"since": now.to_rfc3339(), "until": (now - chrono::Duration::days(1)).to_rfc3339()}),
    ] {
        let resp = post(&c, &token, "/daily", body.clone()).await;
        assert_eq!(resp.status(), 400, "{body}");
    }
}

// --- retry ---

#[tokio::test]
async fn retry_refuses_unknown_and_unfinished_deliveries() {
    let c = client();
    let (token, _, delivery_id) = enqueue_to_new_group(&c).await;

    let resp = post(&c, &token, "/retry", json!({"id": i64::MAX})).await;
    assert_eq!(resp.status(), 404);

    // Just queued: pending (or being sent), so there's nothing to retry yet.
    let status = post_ok(&c, &token, "/get", json!({"id": delivery_id})).await["delivery"]["status"].clone();
    if status == "pending" || status == "processing" {
        let resp = post(&c, &token, "/retry", json!({"id": delivery_id})).await;
        assert_eq!(resp.status(), 409);
        let err: Value = resp.json().await.unwrap();
        assert_eq!(err["code"], "DELIVERY_NOT_RETRYABLE");
    }
}

#[tokio::test]
async fn retry_refuses_in_app_deliveries() {
    let c = client();
    let (token, _, _) = enqueue_to_new_group(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
            "template_code": TEMPLATE,
            "template_variables": {"name": "Staff"},
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let id = resp.json::<Value>().await.unwrap()["deliveries"][0]["id"].clone();
    let resp = post(&c, &token, "/retry", json!({"id": id})).await;
    assert_eq!(resp.status(), 409, "in-app deliveries are delivered on enqueue");
}

#[tokio::test]
async fn retry_requeues_a_skipped_delivery() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let (_, group, delivery_id) = enqueue_to_new_group(&c).await;
    // Switch the group off: the processor skips its deliveries (terminal).
    let resp = c
        .post(format!("{}/api/v1/odo/notify/email-group/update", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": group["id"], "is_active": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // The processor polls every ~10s; give it a few cycles.
    let mut status = Value::Null;
    for _ in 0..30 {
        status = post_ok(&c, &token, "/get", json!({"id": delivery_id})).await["delivery"]["status"].clone();
        if status == "skipped" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    assert_eq!(status, "skipped", "the processor skipped the inactive group's delivery");

    let row = post_ok(&c, &token, "/retry", json!({"id": delivery_id})).await;
    assert_eq!(row["id"], delivery_id);
    // Queued again with a clean slate (unless a worker already re-claimed it).
    assert!(["pending", "processing", "skipped"].contains(&row["status"].as_str().unwrap()));
    assert_eq!(row["retry_count"], 0);
    if row["status"] == "pending" {
        assert!(row["error_code"].is_null(), "old error cleared: {row}");
    }
}
