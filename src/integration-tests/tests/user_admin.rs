//! User detail admin view (odo-auth user/detail).
//!
//! Requires the `e2e.odo.admin` test user and the `auth.user.detail.read`
//! permission from schema change `090_odo_admin_role`.

use integration_tests::*;
use serde_json::json;

fn detail_url() -> String {
    format!("{}/api/v1/odo/auth/user/detail", auth_base())
}

#[tokio::test]
async fn requires_token() {
    let c = client();
    let resp = c
        .post(detail_url())
        .json(&json!({"id": staff_id(&c).await}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

/// auth.user.read (which staff hold for search) must NOT be enough for the
/// detail view — it exposes sessions and IP addresses.
#[tokio::test]
async fn staff_denied() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(detail_url())
        .headers(auth_header(&token))
        .json(&json!({"id": staff_id(&c).await}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn unknown_user_not_found() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let resp = c
        .post(detail_url())
        .headers(auth_header(&token))
        .json(&json!({"id": 999999999}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn detail_aggregates_account_info() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // Ensure the target user has at least one session.
    let _ = staff_token(&c).await;

    let resp = c
        .post(detail_url())
        .headers(auth_header(&token))
        .json(&json!({"id": staff_id(&c).await}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();

    // Account basics
    assert_eq!(detail["user"]["username"], STAFF.username);
    assert_eq!(detail["user"]["email"], STAFF.email);
    assert!(detail["user"]["deleted_at"].is_null());

    // Local account exists (test users authenticate with passwords) and no
    // secrets leak.
    assert!(detail["local_account"].is_object());
    assert!(detail["local_account"].get("password_hash").is_none());

    // Roles include the seeded e2e-test-role assignment, manually managed.
    let roles = detail["roles"].as_array().unwrap();
    let staff_role = roles
        .iter()
        .find(|r| r["role"] == "e2e-test-role")
        .expect("e2e-test-role assignment missing");
    assert_eq!(staff_role["is_managed_by_saml"], false);
    assert!(!staff_role["org_unit_label"].as_str().unwrap().is_empty());

    // Local login stamps last_login_at (matching the SAML path), so a user who
    // has logged in has a non-null value.
    assert!(
        detail["user"]["last_login_at"].is_string(),
        "last_login_at should be set after a local login, got {:?}",
        detail["user"]["last_login_at"]
    );

    // Sessions: at least one (we just logged in), newest first, no hashes.
    let sessions = detail["sessions"].as_array().unwrap();
    assert!(!sessions.is_empty());
    assert!(sessions[0].get("token_hash").is_none());
    assert!(sessions[0].get("refresh_token_hash").is_none());
    assert!(sessions[0]["auth_method"].is_string());

    // SAML sections present as arrays (may be empty for a local user).
    assert!(detail["saml_identities"].is_array());
    assert!(detail["saml_attributes"].is_array());
}

fn update_url() -> String {
    format!("{}/api/v1/odo/auth/user/update", auth_base())
}

#[tokio::test]
async fn staff_denied_update() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({"id": staff_id(&c).await, "family_name": "Nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn update_local_user_names_and_deletion() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // Rename the mutable guinea-pig user (restored below); the DB trigger
    // recomputes display_name. Nothing logs in as e2e.odo.mutable, so mutating
    // it cannot race other tests.
    let sso_id = user_id_by_uuid(&c, &token, MUTABLE.uuid).await;
    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({
            "id": sso_id,
            "first_given_name": "Edited",
            "second_given_name": "Q",
            "family_name": "Person"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["first_given_name"], "Edited");
    assert_eq!(updated["family_name"], "Person");
    assert!(updated["display_name"].as_str().unwrap().contains("Edited"));

    // Soft delete, then restore.
    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({"id": sso_id, "deleted": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let deleted: serde_json::Value = resp.json().await.unwrap();
    assert!(deleted["deleted_at"].is_string(), "soft delete stamps deleted_at");

    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({
            "id": sso_id,
            "deleted": false,
            "first_given_name": "E2E",
            "second_given_name": "",
            "family_name": "Mutable"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let restored: serde_json::Value = resp.json().await.unwrap();
    assert!(restored["deleted_at"].is_null(), "restore clears deleted_at");
    assert_eq!(restored["first_given_name"], "E2E");
}

#[tokio::test]
async fn update_refused_for_saml_accounts() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // e2e.odo.sso authenticates via SAML.
    let sso_id = user_id_by_uuid(&c, &token, SSO.uuid).await;
    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({"id": sso_id, "family_name": "Nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "NOT_LOCAL_ACCOUNT");
}

#[tokio::test]
async fn self_deletion_refused() {
    let c = client();
    let token = odo_admin_token(&c).await;

    let resp = c
        .post(update_url())
        .headers(auth_header(&token))
        .json(&json!({"id": token_user_id(&token), "deleted": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// user/create: API-based account provisioning (app registration / fixtures)
// ---------------------------------------------------------------------------

fn create_url() -> String {
    format!("{}/api/v1/odo/auth/user/create", auth_base())
}

#[tokio::test]
async fn staff_denied_create() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(create_url())
        .headers(auth_header(&token))
        .json(&json!({"username": "itest-nope", "email": "itest-nope@odo.example.org"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn create_user_lifecycle() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let username = format!("itest-user-{nanos}");
    let email = format!("{username}@odo.example.org");
    let uuid = format!("11e57000-0000-4000-a000-{:012}", nanos % 1_000_000_000_000);

    // Create a local user with a password and a pinned uuid.
    let resp = c
        .post(create_url())
        .headers(auth_header(&admin))
        .json(&json!({
            "username": username,
            "email": email,
            "first_given_name": "Itest",
            "family_name": "User",
            "password": "itest-password-1",
            "uuid": uuid,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "create: {:?}", resp.text().await);
    let created: serde_json::Value = resp.json().await.unwrap();
    let user_id = created["id"].as_i64().unwrap();
    assert_eq!(created["auth_method"], "local");
    assert!(created["display_name"].as_str().unwrap().contains("Itest"));

    // Pinned uuid resolves via get_user-by-uuid.
    assert_eq!(user_id_by_uuid(&c, &admin, &uuid).await, user_id);

    // Duplicate active username -> 409.
    let resp = c
        .post(create_url())
        .headers(auth_header(&admin))
        .json(&json!({"username": username, "email": format!("other-{email}")}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "USERNAME_TAKEN");

    // Grant a login-only role, then the new account can log in with the
    // created password (proving the in-DB hash round-trip). The role is
    // provisioned via the authz admin APIs so the test carries its own
    // fixture (create is idempotent-enough: an existing role just 409s).
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/role/create", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({"code": "e2e-test-role", "label": "E2E login-only role"}))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 200 || resp.status() == 409,
        "role create: got {}",
        resp.status()
    );
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/role-permission/create", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({"role": "e2e-test-role", "perm": "odo.auth.session", "min_depth": 0}))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 200 || resp.status() == 409,
        "role-permission create: got {}",
        resp.status()
    );

    let root = root_org_id(&c, &admin).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-role/create", auth_base()))
        .headers(auth_header(&admin))
        .json(&json!({"usr": user_id, "role": "e2e-test-role", "org_unit": root}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": username, "password": "itest-password-1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "created user can log in");

    // Cleanup: soft-delete the account (assignments are historical).
    let resp = c
        .post(update_url())
        .headers(auth_header(&admin))
        .json(&json!({"id": user_id, "deleted": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn create_saml_user_rejects_password() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let resp = c
        .post(create_url())
        .headers(auth_header(&admin))
        .json(&json!({
            "username": "itest-saml-nope",
            "email": "itest-saml-nope@example.com",
            "auth_method": "saml",
            "password": "should-not-work",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
