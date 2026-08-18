use integration_tests::*;
use serde_json::json;

#[tokio::test]
async fn sso_configs() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/sso-configs", auth_base()))
        .json(&json!({"origin": "https://odo.example.org"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["sso_configs"].is_array());
    assert!(data["count"].is_number());
}

#[tokio::test]
async fn sso_configs_unknown_origin() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/sso-configs", auth_base()))
        .json(&json!({"origin": "https://nonexistent.example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["count"], 0);
}

#[tokio::test]
async fn list_idps() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/idps", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"is_active": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["idps"].is_array());
    assert!(data["count"].is_number());
}

#[tokio::test]
async fn list_idps_requires_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/idps", auth_base()))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn get_idp_not_found() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/idp/get", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"idp_id": "https://nonexistent.idp.example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn metadata_unknown_origin() {
    let c = client();
    let resp = c
        .get(format!("{}/api/v1/odo/auth/saml/metadata", auth_base()))
        .query(&[("origin", "https://nonexistent.example.com")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn acs_invalid_response() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/acs", auth_base()))
        .form(&[("SAMLResponse", "bm90LXZhbGlkLXNhbWw=")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn sso_initiate_bad_sp() {
    let c = client();
    let resp = c
        .get(format!("{}/api/v1/odo/auth/saml/sso/initiate", auth_base()))
        .query(&[("sp_id", "999999")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn logout_not_found() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/logout", auth_base()))
        .json(&json!({"session_index": "_nonexistent-session-index"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
