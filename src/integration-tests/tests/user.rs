use integration_tests::*;
use serde_json::json;

#[tokio::test]
async fn get_self() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["id"].as_i64(), Some(token_user_id(&token)));
    assert!(data["username"].is_string());
}

#[tokio::test]
async fn get_by_id() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": token_user_id(&token)}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["id"].as_i64(), Some(token_user_id(&token)));
}

#[tokio::test]
async fn get_with_working_locations() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"options": {"with_working_locations": true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data.get("working_org_units").is_some());
}

#[tokio::test]
async fn search() {
    let c = client();
    // user search requires odo.auth.user.read
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/search", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"keywords": "e2e"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!data.is_empty());
}

#[tokio::test]
async fn search_no_keywords() {
    let c = client();
    // user search requires odo.auth.user.read
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/search", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn search_case_insensitive() {
    let c = client();
    // user search requires odo.auth.user.read
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/search", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"keywords": "E2E"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!data.is_empty());
}

#[tokio::test]
async fn search_multi_word() {
    let c = client();
    // user search requires odo.auth.user.read
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/search", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"keywords": "e2e staff"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!data.is_empty());
    assert_eq!(data[0]["display_name"].as_str(), Some("E2E Staff"));
}

#[tokio::test]
async fn user_requires_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// Durable-references changes: stable uuid + get_user resolve-deleted (opt-in).
// Fixture (test-data 004): the e2e.odo.deleted user is soft-deleted; its id is
// resolved from its pinned uuid (looking up other users requires
// odo.auth.user.read, so these run as odo-admin).
// ---------------------------------------------------------------------------

async fn deleted_user_id(c: &reqwest::Client, admin: &str) -> i64 {
    user_id_by_uuid(c, admin, DELETED_USER_UUID).await
}

#[tokio::test]
async fn get_user_includes_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["uuid"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn get_deleted_user_omitted_by_default() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let deleted = deleted_user_id(&c, &token).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": deleted}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "soft-deleted user is not resolvable by default");
}

#[tokio::test]
async fn get_deleted_user_with_deleted_flagged() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let deleted = deleted_user_id(&c, &token).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": deleted, "options": {"with_deleted": true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "with_deleted resolves the soft-deleted user");
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["id"].as_i64(), Some(deleted));
    assert!(data["deleted_at"].as_str().is_some(), "deleted user must carry deleted_at");
    assert!(data["uuid"].as_str().is_some_and(|s| !s.is_empty()));
}
