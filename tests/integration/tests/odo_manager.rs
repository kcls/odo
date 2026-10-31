//! The odo-manager platform role (schema change `011_odo_manager_role`):
//! reads everything the admin UI shows, writes only email groups and
//! email templates.

use integration_tests::*;
use serde_json::json;

fn unique(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest-{tag}-{nanos}")
}

async fn post(c: &reqwest::Client, token: &str, url: String, body: serde_json::Value) -> reqwest::StatusCode {
    c.post(url)
        .headers(auth_header(token))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status()
}

/// A fresh local account holding odo-manager at the root, signed in.
async fn manager_token(c: &reqwest::Client) -> String {
    let admin = odo_admin_token(c).await;
    let username = unique("manager");
    let password = "manager-password-1";
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/create", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({
            "username": username,
            "email": format!("{username}@odo.example.org"),
            "password": password,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let id = resp.json::<serde_json::Value>().await.unwrap()["id"].clone();

    let root = root_org_id(c, &admin).await;
    let status = post(
        c,
        &admin,
        format!("{}/api/v1/odo/auth/authz/user-role/create", auth_base()),
        json!({"usr": id, "role": "odo-manager", "org_unit": root}),
    )
    .await;
    assert_eq!(status, 200, "assign odo-manager");

    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": username, "password": password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "odo-manager can sign in");
    resp.json::<serde_json::Value>().await.unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn manager_reads_everything_and_writes_only_notification_content() {
    let c = client();
    let token = manager_token(&c).await;
    let auth = auth_base();
    let notify = notify_base();
    let org = org_base();

    // Reads behind every admin tool.
    let reads = [
        (format!("{auth}/api/v1/odo/auth/user/detail"), json!({"id": token_user_id(&token)})),
        (format!("{auth}/api/v1/odo/auth/authz/role/list"), json!({})),
        (format!("{auth}/api/v1/odo/auth/authz/permission/list"), json!({})),
        (format!("{auth}/api/v1/odo/auth/authz/user-role/list"), json!({"usr": token_user_id(&token)})),
        (format!("{auth}/api/v1/odo/auth/saml/admin/idp/list"), json!({})),
        (format!("{org}/api/v1/odo/org/admin/unit-type/list"), json!({})),
        (format!("{notify}/api/v1/odo/notify/template/list"), json!({})),
        (format!("{notify}/api/v1/odo/notify/email-group/list"), json!({})),
        (format!("{notify}/api/v1/odo/notify/delivery/list"), json!({})),
    ];
    for (url, body) in reads {
        assert_eq!(post(&c, &token, url.clone(), body).await, 200, "read {url}");
    }

    // The two writes the role is for.
    let code = unique("mgr-tmpl");
    let status = post(
        &c,
        &token,
        format!("{notify}/api/v1/odo/notify/template/create"),
        json!({"code": code, "name": "Manager template", "subject_template": "Hi", "body_template": "Body"}),
    )
    .await;
    assert_eq!(status, 200, "template create");
    let code = unique("mgr-group");
    let status = post(
        &c,
        &token,
        format!("{notify}/api/v1/odo/notify/email-group/create"),
        json!({"code": code, "label": "Manager group"}),
    )
    .await;
    assert_eq!(status, 200, "email group create");

    // Everything else stays read-only. Each request would succeed for an
    // admin, so a 403 here is the permission, not a bad body.
    let root = root_org_id(&c, &odo_admin_token(&c).await).await;
    let writes = [
        (
            format!("{auth}/api/v1/odo/auth/authz/role/create"),
            json!({"code": unique("mgr-role"), "label": "Nope"}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/user/create"),
            json!({"username": unique("mgr-user"), "email": format!("{}@odo.example.org", unique("mgr"))}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/authz/user-role/create"),
            json!({"usr": token_user_id(&token), "role": "odo-admin", "org_unit": root}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/saml/admin/idp/create"),
            json!({"name": "Nope", "entity_id": unique("mgr-idp")}),
        ),
        (
            format!("{org}/api/v1/odo/org/admin/unit-type/create"),
            json!({"label": unique("mgr-type")}),
        ),
        (format!("{notify}/api/v1/odo/notify/delivery/retry"), json!({"id": i32::MAX})),
    ];
    for (url, body) in writes {
        assert_eq!(post(&c, &token, url.clone(), body).await, 403, "write {url}");
    }
}
