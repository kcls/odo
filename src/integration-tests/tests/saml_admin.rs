//! SAML IdP/SP config admin CRUD (odo-auth saml/admin/*).
//!
//! Requires the `e2e.odo.admin` test user and the `auth.saml.*` permissions
//! from schema change `090_odo_admin_role`.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/auth/saml/admin{}", auth_base(), path)
}

fn unique(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("https://itest-{tag}-{nanos}.example.com")
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

// --- Authn/authz ---

#[tokio::test]
async fn idp_list_requires_token() {
    let c = client();
    let resp = c.post(base("/idp/list")).json(&json!({})).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn staff_denied_read() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = post_json(&c, &token, "/idp/list", json!({})).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn staff_denied_write() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = post_json(
        &c,
        &token,
        "/idp/create",
        json!({"name": "Denied", "entity_id": unique("denied")}),
    )
    .await;
    assert_eq!(resp.status(), 403);
}

// --- IdP CRUD ---

#[tokio::test]
async fn idp_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let entity_id = unique("idp");

    // Create
    let resp = post_json(
        &c,
        &token,
        "/idp/create",
        json!({
            "name": "Test IdP",
            "entity_id": entity_id,
            "sso_url": "https://idp.example.com/sso",
            "session_lifetime_hours": 8
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let idp: serde_json::Value = resp.json().await.unwrap();
    let idp_id = idp["id"].as_i64().unwrap();
    assert_eq!(idp["name"], "Test IdP");
    assert_eq!(idp["is_active"], true);
    assert_eq!(idp["sp_count"], 0);

    // Duplicate entity_id -> 409
    let resp = post_json(
        &c,
        &token,
        "/idp/create",
        json!({"name": "Dup", "entity_id": entity_id}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ENTITY_ID_TAKEN");

    // Update
    let resp = post_json(
        &c,
        &token,
        "/idp/update",
        json!({"id": idp_id, "name": "Renamed IdP", "allow_idp_initiated": true, "is_active": false}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["name"], "Renamed IdP");
    assert_eq!(updated["allow_idp_initiated"], true);
    assert_eq!(updated["is_active"], false);

    // Appears in list
    let resp = post_json(&c, &token, "/idp/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["idps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"].as_i64() == Some(idp_id))
    );

    // Delete
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 200);

    // Delete again -> 404
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 404);
}

// --- SP CRUD + private key handling ---

#[tokio::test]
async fn sp_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let idp_entity = unique("sp-idp");
    let sp_entity = unique("sp");

    // Backing IdP
    let resp = post_json(
        &c,
        &token,
        "/idp/create",
        json!({"name": "SP Test IdP", "entity_id": idp_entity}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let idp: serde_json::Value = resp.json().await.unwrap();
    let idp_id = idp["id"].as_i64().unwrap();

    // Create SP
    let resp = post_json(
        &c,
        &token,
        "/sp/create",
        json!({
            "entity_id": sp_entity,
            "label": "Test SP",
            "acs_url": "https://sp.example.com/acs",
            "x509_cert": "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----",
            "private_key": "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----",
            "idp": idp_id
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let sp: serde_json::Value = resp.json().await.unwrap();
    let sp_id = sp["id"].as_i64().unwrap();
    assert_eq!(sp["has_private_key"], true);
    assert_eq!(sp["idp_name"], "SP Test IdP");
    // The private key must never be returned
    assert!(sp.get("private_key").is_none());

    // List omits private keys too
    let resp = post_json(&c, &token, "/sp/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let listed = data["sps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"].as_i64() == Some(sp_id))
        .expect("created SP missing from list")
        .clone();
    assert!(listed.get("private_key").is_none());

    // Update without private_key keeps the existing key
    let resp = post_json(
        &c,
        &token,
        "/sp/update",
        json!({"id": sp_id, "label": "Renamed SP"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["label"], "Renamed SP");
    assert_eq!(updated["has_private_key"], true);

    // Duplicate entity_id -> 409
    let resp = post_json(
        &c,
        &token,
        "/sp/create",
        json!({
            "entity_id": sp_entity,
            "acs_url": "https://sp2.example.com/acs",
            "x509_cert": "c",
            "private_key": "k"
        }),
    )
    .await;
    assert_eq!(resp.status(), 409);

    // IdP delete refused while the SP references it
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "IDP_IN_USE");

    // Cleanup: SP first, then the IdP is deletable
    let resp = post_json(&c, &token, "/sp/delete", json!({"id": sp_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn sp_create_validation() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // Unknown IdP -> 404
    let resp = post_json(
        &c,
        &token,
        "/sp/create",
        json!({
            "entity_id": unique("bad-idp"),
            "acs_url": "https://x.example.com/acs",
            "x509_cert": "c",
            "private_key": "k",
            "idp": 999999999
        }),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Blank required field -> 400
    let resp = post_json(
        &c,
        &token,
        "/sp/create",
        json!({
            "entity_id": unique("blank-acs"),
            "acs_url": "  ",
            "x509_cert": "c",
            "private_key": "k"
        }),
    )
    .await;
    assert_eq!(resp.status(), 400);
}
