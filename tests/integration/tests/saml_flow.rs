use integration_tests::*;
use serde_json::json;

const ORIGIN: &str = "http://localhost:30080";

async fn resolve_sp_id(c: &reqwest::Client) -> Option<i64> {
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/sso-configs", auth_base()))
        .json(&json!({"origin": ORIGIN}))
        .send()
        .await
        .unwrap();
    let data: serde_json::Value = resp.json().await.unwrap();
    let configs = data["sso_configs"].as_array()?;
    configs.first()?["sp_id"].as_i64()
}

#[tokio::test]
async fn sso_configs_for_origin() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/sso-configs", auth_base()))
        .json(&json!({"origin": ORIGIN}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["count"].as_i64().unwrap() > 0, "No SSO configs for {ORIGIN}");
    let config = &data["sso_configs"][0];
    assert!(config["sp_id"].is_number());
    assert!(config["label"].is_string());
}

#[tokio::test]
async fn initiate_sso_redirects() {
    let c = client();
    let sp_id = match resolve_sp_id(&c).await {
        Some(id) => id,
        None => {
            eprintln!("SKIP: No SSO config for {ORIGIN}");
            return;
        }
    };

    let resp = c
        .get(format!("{}/api/v1/odo/auth/saml/sso/initiate", auth_base()))
        .query(&[
            ("sp_id", sp_id.to_string().as_str()),
            ("relay_state", &format!("{ORIGIN}/login")),
        ])
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert!(
        [302, 303, 307].contains(&status),
        "Expected redirect, got {status}"
    );

    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("mocksaml.com"));
    assert!(location.contains("SAMLRequest"));
}

#[tokio::test]
async fn initiate_sso_with_relay_state() {
    let c = client();
    let sp_id = match resolve_sp_id(&c).await {
        Some(id) => id,
        None => {
            eprintln!("SKIP: No SSO config for {ORIGIN}");
            return;
        }
    };

    let relay = format!("{ORIGIN}/incidents?id=42");
    let resp = c
        .get(format!("{}/api/v1/odo/auth/saml/sso/initiate", auth_base()))
        .query(&[
            ("sp_id", sp_id.to_string().as_str()),
            ("relay_state", relay.as_str()),
        ])
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert!([302, 303, 307].contains(&status));

    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("RelayState"));
}

#[tokio::test]
async fn metadata_for_origin() {
    let c = client();
    let resp = c
        .get(format!("{}/api/v1/odo/auth/saml/metadata", auth_base()))
        .query(&[("origin", ORIGIN)])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let content_type = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("application/xml"));
    let body = resp.text().await.unwrap();
    assert!(body.contains("EntityDescriptor"));
    assert!(body.contains(ORIGIN));
}

#[tokio::test]
async fn acs_rejects_garbage() {
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
async fn acs_rejects_empty() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/saml/acs", auth_base()))
        .form(&[("SAMLResponse", "")])
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    assert!([400, 422].contains(&status));
}
