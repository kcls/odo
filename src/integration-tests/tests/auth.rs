use integration_tests::*;
use serde_json::json;

#[tokio::test]
async fn health() {
    let c = client();
    let resp = c.get(format!("{}/health", auth_base())).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn login_success() {
    let c = client();
    let data = login(&c, &STAFF).await;
    assert!(data["access_token"].is_string());
    assert!(data["user"]["id"].as_i64().unwrap() > 0);
    assert!(data["user"]["email"].is_string());
    assert!(data["user"]["display_name"].is_string());
}

#[tokio::test]
async fn login_bad_password() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": STAFF.username, "password": "wrong"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn login_bad_username() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": "nonexistent", "password": "test123"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn validate_token() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/token/validate", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["valid"], true);
    assert!(data["claims"]["user_id"].as_i64().unwrap() > 0);
    assert!(data["claims"]["email"].is_string());
    assert!(data["claims"].get("display_name").is_some());
}

#[tokio::test]
async fn validate_invalid_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/token/validate", auth_base()))
        .json(&json!({"token": "not-a-real-token"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["valid"], false);
}

#[tokio::test]
async fn refresh_token() {
    // Use a cookie-jar client so the login response's Set-Cookie
    // (HttpOnly refresh token) is sent with the refresh request.
    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let c = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .cookie_provider(jar)
        .build()
        .unwrap();

    let login_resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": STAFF.username, "password": STAFF.password}))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), 200);

    let resp = c
        .post(format!("{}/api/v1/odo/auth/token/refresh", auth_base()))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["access_token"].is_string());
}

#[tokio::test]
async fn revoke_and_verify() {
    let c = client();
    let fresh = login(&c, &STAFF).await;
    let token = fresh["access_token"].as_str().unwrap();

    // Verify valid
    let data: serde_json::Value = c
        .post(format!("{}/api/v1/odo/auth/token/validate", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["valid"], true);

    // Revoke
    let resp = c
        .post(format!("{}/api/v1/odo/auth/token/revoke", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["success"], true);

    // Verify now invalid
    let data: serde_json::Value = c
        .post(format!("{}/api/v1/odo/auth/token/validate", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["valid"], false);

    // Revoke again — should report not found
    let data: serde_json::Value = c
        .post(format!("{}/api/v1/odo/auth/token/revoke", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["success"], false);
}

#[tokio::test]
async fn logout() {
    let c = client();
    let fresh = login(&c, &STAFF).await;
    let token = fresh["access_token"].as_str().unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/auth/logout", auth_base()))
        .json(&json!({"access_token": token}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["success"], true);

    // Verify token is now invalid
    let data: serde_json::Value = c
        .post(format!("{}/api/v1/odo/auth/token/validate", auth_base()))
        .json(&json!({"token": token}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(data["valid"], false);
}
