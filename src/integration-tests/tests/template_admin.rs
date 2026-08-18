//! Notification template admin CRUD + preview (odo-notify template/*).
//!
//! Requires the `e2e.odo.admin` test user and the `notification.template.*`
//! permissions from schema change `090_odo_admin_role`.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/notify/template{}", notify_base(), path)
}

fn unique_code(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest-{tag}-{nanos}")
}

async fn post_json(
    c: &reqwest::Client,
    token: &str,
    path: &str,
    body: serde_json::Value,
) -> reqwest::Response {
    c.post(base(path))
        .headers(auth_header(token))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn staff_denied() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = post_json(&c, &token, "/list", json!({})).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn template_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let code = unique_code("tmpl");

    // Create with sample data
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({
            "code": code,
            "name": "Test Template",
            "subject_template": "Hello {{name}}",
            "body_template": "Body for {{name}}",
            "sample_data": {"name": "World"}
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let tmpl: serde_json::Value = resp.json().await.unwrap();
    let tmpl_id = tmpl["id"].as_i64().unwrap();
    assert_eq!(tmpl["code"], code.as_str());
    assert_eq!(tmpl["is_active"], true);
    assert!(tmpl["org_unit"].as_i64().unwrap() > 0);

    // Duplicate code -> 409
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({
            "code": code,
            "name": "Dup",
            "subject_template": "S",
            "body_template": "B"
        }),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "TEMPLATE_CODE_TAKEN");

    // Broken handlebars -> 400 naming the field
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({
            "code": unique_code("bad"),
            "name": "Bad",
            "subject_template": "{{#if broken}}",
            "body_template": "B"
        }),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert!(err["message"].as_str().unwrap().contains("subject_template"));

    // Update: add HTML body and rename
    let resp = post_json(
        &c,
        &token,
        "/update",
        json!({
            "id": tmpl_id,
            "name": "Renamed Template",
            "body_template_html": "<p>Hi {{name}}</p>"
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["name"], "Renamed Template");
    assert_eq!(updated["body_template_html"], "<p>Hi {{name}}</p>");

    // Update with broken template -> 400, unchanged
    let resp = post_json(
        &c,
        &token,
        "/update",
        json!({"id": tmpl_id, "body_template": "{{#each}}"}),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Appears in list
    let resp = post_json(&c, &token, "/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"].as_i64() == Some(tmpl_id))
    );

    // Soft delete; gone from list; re-delete -> 404
    let resp = post_json(&c, &token, "/delete", json!({"id": tmpl_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/list", json!({})).await;
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        !data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"].as_i64() == Some(tmpl_id))
    );
    let resp = post_json(&c, &token, "/delete", json!({"id": tmpl_id})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn preview_renders_and_validates() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // Successful render
    let resp = post_json(
        &c,
        &token,
        "/preview",
        json!({
            "subject_template": "Hello {{name}}",
            "body_template": "You have {{count}} item(s)",
            "body_template_html": "<b>{{name}}</b>",
            "variables": {"name": "World", "count": 3}
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let preview: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(preview["subject"], "Hello World");
    assert_eq!(preview["body"], "You have 3 item(s)");
    assert_eq!(preview["body_html"], "<b>World</b>");

    // Broken template -> 400 naming the field
    let resp = post_json(
        &c,
        &token,
        "/preview",
        json!({"body_template": "{{#if oops}}"}),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert!(err["message"].as_str().unwrap().contains("body_template"));
}
