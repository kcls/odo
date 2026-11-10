//! The odo-viewer and odo-notify-admin platform roles (schema change
//! `014_odo_viewer_notify_admin`): odo-viewer reads everything the admin
//! UI shows and writes nothing; odo-notify-admin reads and writes email
//! templates and email groups, and reads delivery history.

use integration_tests::*;
use serde_json::json;

fn unique(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest-{tag}-{nanos}")
}

async fn post(
    c: &reqwest::Client,
    token: &str,
    url: String,
    body: serde_json::Value,
) -> reqwest::StatusCode {
    c.post(url)
        .headers(auth_header(token))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status()
}

/// A fresh local account holding `role` at the root, signed in.
async fn role_token(c: &reqwest::Client, role: &str) -> String {
    let admin = odo_admin_token(c).await;
    let username = unique(role);
    let password = "role-password-1";
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
        json!({"usr": id, "role": role, "org_unit": root}),
    )
    .await;
    assert_eq!(status, 200, "assign {role}");

    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": username, "password": password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{role} can sign in");
    resp.json::<serde_json::Value>().await.unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The three notification writes: template create, email group create,
/// and a test send (gated on the template write; it reaches only the
/// sender). Each would succeed for an admin.
fn notification_writes(notify: &str) -> [(String, serde_json::Value); 3] {
    [
        (
            format!("{notify}/api/v1/odo/notify/template/create"),
            json!({"code": unique("role-tmpl"), "name": "Role template", "subject_template": "Hi", "body_template": "Body"}),
        ),
        (
            format!("{notify}/api/v1/odo/notify/email-group/create"),
            json!({"code": unique("role-group"), "label": "Role group"}),
        ),
        (
            format!("{notify}/api/v1/odo/notify/template/send-test"),
            json!({"subject_template": "Hi", "body_template": "Body"}),
        ),
    ]
}

/// The notification reads: templates, email groups, and the
/// Notifications page's delivery history.
fn notification_reads(notify: &str) -> [(String, serde_json::Value); 3] {
    [
        (
            format!("{notify}/api/v1/odo/notify/template/list"),
            json!({}),
        ),
        (
            format!("{notify}/api/v1/odo/notify/email-group/list"),
            json!({}),
        ),
        (
            format!("{notify}/api/v1/odo/notify/delivery/list"),
            json!({}),
        ),
    ]
}

/// Reads behind every non-notification admin tool, for `token`'s user.
fn platform_reads(token: &str) -> Vec<(String, serde_json::Value)> {
    let auth = auth_base();
    let org = org_base();
    vec![
        (
            format!("{auth}/api/v1/odo/auth/user/detail"),
            json!({"id": token_user_id(token)}),
        ),
        (format!("{auth}/api/v1/odo/auth/authz/role/list"), json!({})),
        (
            format!("{auth}/api/v1/odo/auth/authz/permission/list"),
            json!({}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/authz/user-role/list"),
            json!({"usr": token_user_id(token)}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/saml/admin/idp/list"),
            json!({}),
        ),
        (
            format!("{org}/api/v1/odo/org/admin/unit-type/list"),
            json!({}),
        ),
    ]
}

/// Platform writes neither role holds. Each would succeed for an admin, so
/// a 403 is the permission, not a bad body.
async fn platform_writes(c: &reqwest::Client, token: &str) -> Vec<(String, serde_json::Value)> {
    let auth = auth_base();
    let org = org_base();
    let notify = notify_base();
    let root = root_org_id(c, &odo_admin_token(c).await).await;
    vec![
        (
            format!("{auth}/api/v1/odo/auth/authz/role/create"),
            json!({"code": unique("role-role"), "label": "Nope"}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/user/create"),
            json!({"username": unique("role-user"), "email": format!("{}@odo.example.org", unique("role"))}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/authz/user-role/create"),
            json!({"usr": token_user_id(token), "role": "odo-admin", "org_unit": root}),
        ),
        (
            format!("{auth}/api/v1/odo/auth/saml/admin/idp/create"),
            json!({"name": "Nope", "entity_id": unique("role-idp")}),
        ),
        (
            format!("{org}/api/v1/odo/org/admin/unit-type/create"),
            json!({"label": unique("role-type")}),
        ),
        (
            format!("{notify}/api/v1/odo/notify/delivery/retry"),
            json!({"id": i32::MAX}),
        ),
    ]
}

async fn expect_all(
    c: &reqwest::Client,
    token: &str,
    requests: impl IntoIterator<Item = (String, serde_json::Value)>,
    want: u16,
    what: &str,
) {
    for (url, body) in requests {
        assert_eq!(
            post(c, token, url.clone(), body).await,
            want,
            "{what} {url}"
        );
    }
}

#[tokio::test]
async fn viewer_reads_everything_and_writes_nothing() {
    let c = client();
    let token = role_token(&c, "odo-viewer").await;
    let notify = notify_base();

    expect_all(&c, &token, platform_reads(&token), 200, "read").await;
    expect_all(&c, &token, notification_reads(notify), 200, "read").await;
    expect_all(&c, &token, notification_writes(notify), 403, "write").await;
    let writes = platform_writes(&c, &token).await;
    expect_all(&c, &token, writes, 403, "write").await;
}

#[tokio::test]
async fn notify_admin_manages_notifications_only() {
    let c = client();
    let token = role_token(&c, "odo-notify-admin").await;
    let notify = notify_base();

    expect_all(&c, &token, notification_reads(notify), 200, "read").await;
    expect_all(&c, &token, notification_writes(notify), 200, "write").await;
    expect_all(&c, &token, platform_reads(&token), 403, "read").await;
    let writes = platform_writes(&c, &token).await;
    expect_all(&c, &token, writes, 403, "write").await;
}
