//! UUID reference support across the odo APIs (repo-split plan Phase 1):
//! lookups and mutations accept stable uuids alongside database ids, and
//! the JWT's `org_unit` claim carries the working org unit's uuid
//! (single claim since the 1d.3 flip; the login/refresh surface accepts
//! `org_unit` with `org_unit_uuid` kept as an alias).
//!
//! Tests resolve fixture uuids at runtime (get_user / unit detail by id)
//! so they don't depend on pinned fixture values.

use integration_tests::*;
use serde_json::json;

/// Decode a JWT payload without verifying (test-side inspection).
fn jwt_payload(token: &str) -> serde_json::Value {
    use base64::Engine;
    let payload = token.split('.').nth(1).expect("JWT payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("decodes");
    serde_json::from_slice(&bytes).expect("json")
}

async fn fetch_user_uuid(c: &reqwest::Client, token: &str, id: i64) -> String {
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(token))
        .json(&json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    data["uuid"].as_str().expect("user uuid").to_string()
}

async fn fetch_unit_uuid(c: &reqwest::Client, token: &str, id: i64) -> String {
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{id}", org_base()))
        .headers(auth_header(token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    data["org_unit"]["uuid"].as_str().expect("unit uuid").to_string()
}

// ---- JWT dual claim ----

#[tokio::test]
async fn login_token_carries_org_unit_uuid() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let root_id = org_id_by_code(&c, &admin, "OLS").await;
    let unit = unit_uuid_by_code(&c, &admin, "OLS").await;

    // Login with an explicit org unit: the claim is the unit's uuid.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": STAFF.username, "password": STAFF.password, "org_unit": unit}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let token = data["access_token"].as_str().unwrap();

    let claims = jwt_payload(token);
    assert_eq!(claims["org_unit"].as_str(), Some(unit.as_str()));
    let claim_uuid = claims["org_unit"].as_str().expect("org_unit claim");
    let unit_uuid = fetch_unit_uuid(&c, token, root_id).await;
    assert_eq!(claim_uuid, unit_uuid);
    assert!(
        claims.get("org_unit_uuid").is_none(),
        "legacy dual claim is gone"
    );

    // Login without an org unit: no claim present.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": STAFF.username, "password": STAFF.password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let claims = jwt_payload(data["access_token"].as_str().unwrap());
    assert!(claims.get("org_unit").is_none() || claims["org_unit"].is_null());
}

// ---- auth: get_user by uuid ----

#[tokio::test]
async fn get_user_by_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let sid = staff_id(&c).await;
    let uuid = fetch_user_uuid(&c, &token, sid).await;

    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuid": uuid}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["id"].as_i64(), Some(sid));

    // Unknown uuid -> 404; invalid uuid -> 400.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuid": "00000000-0000-4000-a000-00000000dead"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuid": "not-a-uuid"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ---- org: label-batch and unit routes by uuid ----

#[tokio::test]
async fn label_batch_accepts_uuids() {
    let c = client();
    let token = staff_token(&c).await;
    let root_id = org_id_by_code(&c, &token, "OLS").await;
    let root_uuid = fetch_unit_uuid(&c, &token, root_id).await;

    // Mixed id + uuid request for the same unit dedupes to one entry.
    let resp = c
        .post(format!("{}/api/v1/odo/org/unit/label-batch", org_base()))
        .headers(auth_header(&token))
        .json(&json!({"ids": [root_id], "uuids": [root_uuid]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let labels = data["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0]["id"].as_i64(), Some(root_id));
    assert_eq!(labels[0]["uuid"].as_str(), Some(root_uuid.as_str()));

    // Unknown uuids are dropped like unknown ids.
    let resp = c
        .post(format!("{}/api/v1/odo/org/unit/label-batch", org_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuids": ["00000000-0000-4000-a000-00000000dead"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["labels"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn unit_routes_by_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let root_id = org_id_by_code(&c, &token, "OLS").await;
    let root_uuid = fetch_unit_uuid(&c, &token, root_id).await;

    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/uuid/{root_uuid}", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["org_unit"]["id"].as_i64(), Some(root_id));

    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/uuid/{root_uuid}/descendants", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let rows: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!rows.is_empty());
    assert_eq!(rows[0]["id"].as_i64(), Some(root_id));

    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/uuid/{root_uuid}/ancestors", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Unknown uuid 404s.
    let resp = c
        .get(format!(
            "{}/api/v1/odo/org/unit/uuid/00000000-0000-4000-a000-00000000dead",
            org_base()
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---- authz: uuid params ----

#[tokio::test]
async fn user_has_perm_accepts_org_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let root_id = org_id_by_code(&c, &token, "OLS").await;
    let root_uuid = fetch_unit_uuid(&c, &token, root_id).await;

    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-has-perm", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"perm": "odo.auth.session", "org_unit_uuid": root_uuid}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["has_perm"], true);
    assert_eq!(data["org_unit"].as_i64(), Some(root_id), "resolved to the id");
}

#[tokio::test]
async fn users_with_role_accepts_uuids() {
    let c = client();
    let token = staff_token(&c).await;
    let sid = staff_id(&c).await;
    let staff_uuid = fetch_user_uuid(&c, &token, sid).await;
    let root_id = org_id_by_code(&c, &token, "OLS").await;
    let root_uuid = fetch_unit_uuid(&c, &token, root_id).await;

    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/users-with-role", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "role": "e2e-test-role",
            "user_uuids": [staff_uuid],
            "org_unit_uuid": root_uuid,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["user_ids"].as_array().unwrap().len(), 1);
    assert_eq!(data["user_ids"][0].as_i64(), Some(sid));
    assert_eq!(data["user_uuids"][0].as_str(), Some(staff_uuid.as_str()));
}

#[tokio::test]
async fn user_role_assignment_by_uuid() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let sid = staff_id(&c).await;
    let staff_uuid = fetch_user_uuid(&c, &admin, sid).await;
    let branch_id = org_id_by_code(&c, &admin, "MAIN").await;
    let branch_uuid = fetch_unit_uuid(&c, &admin, branch_id).await;

    // Create by uuid refs.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-role/create", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({
            "usr_uuid": staff_uuid,
            "role": "e2e-test-role",
            "org_unit_uuid": branch_uuid,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "create: {:?}", resp.text().await);
    let row: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(row["usr"].as_i64(), Some(sid));
    assert_eq!(row["org_unit"].as_i64(), Some(branch_id));
    let id = row["id"].as_i64().unwrap();

    // List by uuid.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-role/list", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({"usr_uuid": staff_uuid}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"].as_i64() == Some(id))
    );

    // Cleanup.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-role/delete", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// ---- asset: files by uuid ----

#[tokio::test]
async fn get_files_accepts_uuids() {
    let c = client();
    let token = staff_token(&c).await;

    // Upload a small file, then fetch it back by uuid (via the suite's
    // self-provisioned e2e-file catch-all mapping).
    ensure_upload_fixtures().await;
    let form = reqwest::multipart::Form::new()
        .text("category", "document")
        .text("entity_type", "e2e-file")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"uuid-test".to_vec())
                .file_name("uuid-test.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
    let resp = c
        .post(format!("{}/api/v1/odo/asset/upload", asset_base()))
        .headers(auth_header(&token))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "upload: {:?}", resp.text().await);
    let up: serde_json::Value = resp.json().await.unwrap();
    let id = up["id"].as_i64().unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&json!({"ids": [id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let uuid = data["files"][0]["uuid"].as_str().unwrap().to_string();

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuids": [uuid]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["files"][0]["id"].as_i64(), Some(id));

    // Delete by uuid.
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .headers(auth_header(&token))
        .json(&json!({"uuid": uuid}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["id"].as_i64(), Some(id));
}

#[tokio::test]
async fn login_accepts_org_unit_uuid() {
    // Cookie store: the refresh token comes back as an httpOnly cookie.
    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .cookie_store(true)
        .build()
        .unwrap();
    let rc = client();
    let admin = odo_admin_token(&rc).await;
    let unit = unit_uuid_by_code(&rc, &admin, "OLS").await;

    // Login selecting the working location by uuid: both claims resolve.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({
            "username": STAFF.username,
            "password": STAFF.password,
            "org_unit_uuid": unit,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let claims = jwt_payload(data["access_token"].as_str().unwrap());
    assert_eq!(claims["org_unit"].as_str(), Some(unit.as_str()));

    // Refresh switching the working location by uuid (refresh token
    // rides the session cookie; `org_unit_uuid` is the legacy alias).
    let other = unit_uuid_by_code(&rc, &admin, "MAIN").await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/token/refresh", auth_base()))
        .json(&json!({"org_unit_uuid": other}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let claims = jwt_payload(data["access_token"].as_str().unwrap());
    assert_eq!(claims["org_unit"].as_str(), Some(other.as_str()));

    // Unknown uuid is a client error, not a silent None.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({
            "username": STAFF.username,
            "password": STAFF.password,
            "org_unit_uuid": "00000000-0000-4000-a000-00000000dead",
        }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 404 || resp.status() == 400,
        "unknown working-location uuid should 4xx, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn user_get_returns_working_org_unit_uuids() {
    let c = client();
    let token = login_token(&c, &STAFF).await;

    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": token_user_id(&token), "options": {"with_working_locations": true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let ids = data["working_org_units"].as_array().expect("ids array");
    let uuids = data["working_org_unit_uuids"].as_array().expect("uuids array");
    assert_eq!(ids.len(), uuids.len(), "parallel lists");
    for (id, uuid) in ids.iter().zip(uuids) {
        let expected = fetch_unit_uuid(&c, &token, id.as_i64().unwrap()).await;
        assert_eq!(uuid.as_str(), Some(expected.as_str()));
    }
}
