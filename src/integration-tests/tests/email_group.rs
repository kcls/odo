//! Email group admin CRUD (odo-notify).
//!
//! Requires the `e2e.odo.admin` test user (sqitch test-data
//! `005_e2e_odo_admin_user`) and schema change `090_odo_admin_role`.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/notify/email-group{}", notify_base(), path)
}

/// Group codes are globally unique and groups can only be soft-deleted, so
/// each test run mints its own codes.
fn unique_code(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest-{tag}-{nanos}")
}

async fn create_group(c: &reqwest::Client, token: &str, code: &str) -> serde_json::Value {
    let resp = c
        .post(base("/create"))
        .headers(auth_header(token))
        .json(&json!({"code": code, "label": format!("Test Group {code}")}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "create group failed for {code}");
    resp.json().await.unwrap()
}

// --- Authn/authz ---

#[tokio::test]
async fn list_requires_token() {
    let c = client();
    let resp = c.post(base("/list")).json(&json!({})).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn staff_denied_read() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(base("/list"))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn staff_denied_write() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(base("/create"))
        .headers(auth_header(&token))
        .json(&json!({"code": "staff-denied", "label": "Nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// --- Group CRUD ---

#[tokio::test]
async fn group_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let code = unique_code("crud");

    // Create
    let group = create_group(&c, &token, &code).await;
    let group_id = group["id"].as_i64().unwrap();
    assert_eq!(group["code"], code.as_str());
    assert_eq!(group["is_active"], true);
    assert_eq!(group["member_count"], 0);

    // Appears in list
    let resp = c
        .post(base("/list"))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"].as_i64() == Some(group_id))
    );

    // Update label
    let resp = c
        .post(base("/update"))
        .headers(auth_header(&token))
        .json(&json!({"id": group_id, "label": "Renamed Group"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["label"], "Renamed Group");
    assert_eq!(updated["code"], code.as_str());

    // Deactivate (soft delete)
    let resp = c
        .post(base("/update"))
        .headers(auth_header(&token))
        .json(&json!({"id": group_id, "is_active": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Gone from the default list...
    let resp = c
        .post(base("/list"))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        !data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"].as_i64() == Some(group_id))
    );

    // ...but visible with include_inactive
    let resp = c
        .post(base("/list"))
        .headers(auth_header(&token))
        .json(&json!({"include_inactive": true}))
        .send()
        .await
        .unwrap();
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"].as_i64() == Some(group_id))
    );
}

#[tokio::test]
async fn duplicate_group_code_conflict() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let code = unique_code("dup");

    create_group(&c, &token, &code).await;

    let resp = c
        .post(base("/create"))
        .headers(auth_header(&token))
        .json(&json!({"code": code, "label": "Duplicate"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "EMAIL_GROUP_CODE_TAKEN");
    assert_eq!(err["field"], "code");
}

#[tokio::test]
async fn create_group_rejects_blank_code() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(base("/create"))
        .headers(auth_header(&token))
        .json(&json!({"code": "   ", "label": "Blank"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn get_unknown_group_not_found() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(base("/get"))
        .headers(auth_header(&token))
        .json(&json!({"id": 999999999}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// --- Member CRUD ---

#[tokio::test]
async fn member_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let code = unique_code("member");

    let group = create_group(&c, &token, &code).await;
    let group_id = group["id"].as_i64().unwrap();

    // Add a member
    let resp = c
        .post(base("/member/create"))
        .headers(auth_header(&token))
        .json(&json!({"email_group": group_id, "email": "alice@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let member: serde_json::Value = resp.json().await.unwrap();
    let member_id = member["id"].as_i64().unwrap();
    assert_eq!(member["email"], "alice@example.com");
    assert_eq!(member["is_active"], true);

    // Duplicate member -> conflict
    let resp = c
        .post(base("/member/create"))
        .headers(auth_header(&token))
        .json(&json!({"email_group": group_id, "email": "alice@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "EMAIL_ALREADY_IN_GROUP");

    // Invalid email rejected
    let resp = c
        .post(base("/member/create"))
        .headers(auth_header(&token))
        .json(&json!({"email_group": group_id, "email": "not-an-email"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Unknown group -> 404
    let resp = c
        .post(base("/member/create"))
        .headers(auth_header(&token))
        .json(&json!({"email_group": 999999999, "email": "bob@example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // Update the member
    let resp = c
        .post(base("/member/update"))
        .headers(auth_header(&token))
        .json(&json!({"id": member_id, "email": "alice.smith@example.com", "is_active": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["email"], "alice.smith@example.com");
    assert_eq!(updated["is_active"], false);

    // Counts reflected in get
    let resp = c
        .post(base("/get"))
        .headers(auth_header(&token))
        .json(&json!({"id": group_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(detail["group"]["member_count"], 1);
    assert_eq!(detail["group"]["active_member_count"], 0);
    assert_eq!(detail["members"].as_array().unwrap().len(), 1);

    // Hard-delete the member
    let resp = c
        .post(base("/member/delete"))
        .headers(auth_header(&token))
        .json(&json!({"id": member_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Deleting again -> 404
    let resp = c
        .post(base("/member/delete"))
        .headers(auth_header(&token))
        .json(&json!({"id": member_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
