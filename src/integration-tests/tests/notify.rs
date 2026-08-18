use integration_tests::*;
use serde_json::json;

/// The notify tests' own template. Templates are app-registered in
/// production, so the platform tests carry their own via the template
/// admin API. Returns the odo-admin token (which also holds
/// odo.notify.send, so callers reuse it for enqueue).
async fn ensure_test_template(c: &reqwest::Client) -> String {
    let admin = odo_admin_token(c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/template/create", notify_base()))
        .headers(auth_header(&admin))
        .json(&json!({
            "code": TEST_TEMPLATE,
            "name": "E2E Notify Template",
            "subject_template": "E2E notification for {{recipient_name}}",
            "body_template": "E2E body {{action_url}}",
            "sample_data": {"recipient_name": "Test", "action_url": "/test"}
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 200 || resp.status() == 409,
        "template ensure failed: {}",
        resp.status()
    );
    admin
}

const TEST_TEMPLATE: &str = "e2e-notify-template";


#[tokio::test]
async fn health() {
    let c = client();
    let resp = c
        .get(format!("{}/health", notify_base()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// --- Inbox ---

#[tokio::test]
async fn inbox_list() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/list", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"limit": 10}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["notifications"].is_array());
    assert!(data["total"].is_number());
    assert!(data["unread_count"].is_number());
}

#[tokio::test]
async fn inbox_list_default_limit() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/list", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn inbox_requires_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/list", notify_base()))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn inbox_mark_read_not_found() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/mark-read", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"delivery_id": -1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn inbox_dismiss_not_found() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/dismiss", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"delivery_id": -1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn inbox_mark_all_read() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/mark-all-read", notify_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["success"], true);
}

#[tokio::test]
async fn inbox_dismiss_all() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/dismiss-all", notify_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["success"], true);
}

// --- Enqueue ---

#[tokio::test]
async fn enqueue_requires_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
            "template_code": TEST_TEMPLATE,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn enqueue_empty_recipients() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [],
            "template_code": TEST_TEMPLATE,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn enqueue_invalid_channel() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["carrier_pigeon"]}],
            "template_code": TEST_TEMPLATE,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn enqueue_email_group_in_app_rejected() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "email_group", "email_group_id": 1, "channels": ["in_app"]}],
            "template_code": TEST_TEMPLATE,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn enqueue_template_not_found() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
            "template_code": "nonexistent-template-code",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn enqueue_email_channel() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["email"]}],
            "template_code": TEST_TEMPLATE,
            "template_variables": {
			  "incident_occurred_at": "2026-03-26T00:00:00-0800",
			  "incident_id": 999,
			  "incident_timezone": "America/Los_Angeles",
			  "recipient_name": "Test User"
            },
            "source_service": "integration-tests",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let deliveries = data["deliveries"].as_array().unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0]["channel"], "email");
    assert_eq!(deliveries[0]["status"], "pending");
}

#[tokio::test]
async fn enqueue_invalid_action_url() {
    let c = client();
    let token = ensure_test_template(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
            "template_code": TEST_TEMPLATE,
            "template_variables": {"action_url": "//evil.com/phish"},
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// --- Service account (background jobs) ---

#[tokio::test]
async fn service_account_can_login_and_enqueue() {
    let c = client();
    ensure_test_template(&c).await;

    let login_resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({
            "username": NOTIFY_SERVICE_USERNAME,
            "password": NOTIFY_SERVICE_PASSWORD,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        login_resp.status(),
        200,
        "odo-notify-service login failed (is migration 089 deployed?)"
    );
    let login_data: serde_json::Value = login_resp.json().await.unwrap();
    let token = login_data["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_string();

    let resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["email"]}],
            "template_code": TEST_TEMPLATE,
            "template_variables": {
                "incident_id": 999,
                "incident_type": "Test",
                "incident_location": "Test Library",
                "incident_created_at": "2026-03-26T00:00:00-0800",
                "incident_timezone": "America/Los_Angeles",
                "days_pending": 3,
                "incident_url": "https://example.com/incidents/999"
            },
            "source_service": "integration-tests",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "odo-notify-service enqueue failed — token lacks notification.send?"
    );
    let data: serde_json::Value = resp.json().await.unwrap();
    let deliveries = data["deliveries"].as_array().unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0]["channel"], "email");
}

// --- Enqueue + Inbox round-trip ---

#[tokio::test]
async fn enqueue_and_read_inbox() {
    let c = client();
    // odo-admin enqueues (odo.notify.send); staff reads its own inbox.
    let admin = ensure_test_template(&c).await;
    let token = staff_token(&c).await;

    // Enqueue an in_app notification to the staff user
    let enqueue_resp = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&admin))
        .json(&json!({
            "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
            "template_code": TEST_TEMPLATE,
            "template_variables": {
                "action_url": "/test/round-trip",
            },
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(enqueue_resp.status(), 200);
    let enqueue_data: serde_json::Value = enqueue_resp.json().await.unwrap();
    assert!(enqueue_data["event_id"].as_i64().unwrap() > 0);
    assert_eq!(enqueue_data["is_duplicate"], false);

    let deliveries = enqueue_data["deliveries"].as_array().unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0]["channel"], "in_app");
    assert_eq!(deliveries[0]["status"], "delivered");

    let delivery_id = deliveries[0]["id"].as_i64().unwrap();

    // Verify it appears in the inbox
    let inbox_resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/list", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"limit": 100}))
        .send()
        .await
        .unwrap();
    assert_eq!(inbox_resp.status(), 200);
    let inbox_data: serde_json::Value = inbox_resp.json().await.unwrap();
    let notifications = inbox_data["notifications"].as_array().unwrap();
    let found = notifications
        .iter()
        .any(|n| n["delivery_id"].as_i64() == Some(delivery_id));
    assert!(found, "Enqueued notification not found in inbox");

    // Mark it read
    let mark_resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/mark-read", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"delivery_id": delivery_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(mark_resp.status(), 200);

    // Dismiss it
    let dismiss_resp = c
        .post(format!("{}/api/v1/odo/notify/inbox/dismiss", notify_base()))
        .headers(auth_header(&token))
        .json(&json!({"delivery_id": delivery_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(dismiss_resp.status(), 200);
}

#[tokio::test]
async fn enqueue_dedup() {
    let c = client();
    let token = ensure_test_template(&c).await;

    let dedup_key = format!(
        "test-dedup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let body = json!({
        "recipients": [{"type": "user", "user_id": staff_id(&c).await, "channels": ["in_app"]}],
        "template_code": TEST_TEMPLATE,
        "template_variables": {
            "incident_occurred_at": "2026-03-26T00:00:00-0800",
            "incident_id": 999,
            "incident_timezone": "America/Los_Angeles",
            "recipient_name": "Test User",
        },
        "dedup_key": dedup_key,
        "source_service": "integration-tests",
        "source_entity_type": "test",
        "source_entity_id": 999999,
    });

    // First enqueue
    let resp1 = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);
    let data1: serde_json::Value = resp1.json().await.unwrap();
    assert_eq!(data1["is_duplicate"], false);

    // Second enqueue with same dedup_key — should be duplicate
    let resp2 = c
        .post(format!("{}/api/v1/odo/notify/enqueue", notify_base()))
        .headers(auth_header(&token))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200);
    let data2: serde_json::Value = resp2.json().await.unwrap();
    assert_eq!(data2["is_duplicate"], true);
    assert_eq!(data2["event_id"], data1["event_id"]);
}
