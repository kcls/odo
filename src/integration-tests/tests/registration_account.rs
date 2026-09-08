//! The odo-registration machine account (repo-split plan Phase 3): apps
//! register their seed data (permissions, roles, grants, users, templates,
//! asset directories, SAML maps) with its credentials instead of a human
//! admin. Verifies the account logs in and holds exactly enough to run a
//! registration flow end to end.

use integration_tests::*;
use serde_json::json;

async fn registration_token(c: &reqwest::Client) -> String {
    let resp = c
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&json!({"username": REGISTRATION_USERNAME, "password": REGISTRATION_PASSWORD}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "odo-registration can log in");
    let data: serde_json::Value = resp.json().await.unwrap();
    data["access_token"].as_str().expect("token").to_string()
}

fn suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[tokio::test]
async fn registration_account_runs_a_registration_flow() {
    let c = client();
    let token = registration_token(&c).await;
    let n = suffix();

    // Ordering matters: permissions -> roles -> grants -> directories
    // (directories FK the permission codes).
    let perm = format!("itest.reg.{n}.read");
    let resp = c
        .post(format!(
            "{}/api/v1/odo/auth/authz/permission/create",
            auth_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"code": perm, "description": "registration flow test perm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "permission/create");

    let role = format!("itest-reg-role-{n}");
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/role/create", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"code": role, "label": "registration flow test role"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "role/create");

    let resp = c
        .post(format!(
            "{}/api/v1/odo/auth/authz/role-permission/create",
            auth_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"role": role, "perm": perm, "min_depth": 0}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "role-permission/create");

    // Provision an app user with a pinned uuid.
    let username = format!("itest-reg-user-{n}");
    let uuid = format!("11e57e90-0000-4000-a000-{:012}", n % 1_000_000_000_000);
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/create", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "username": username,
            "email": format!("{username}@odo.example.org"),
            "uuid": uuid,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "user/create");
    let user_id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let root = root_org_id(&c, &token).await;
    let resp = c
        .post(format!(
            "{}/api/v1/odo/auth/authz/user-role/create",
            auth_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"usr": user_id, "role": role, "org_unit": root}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "user-role/create");

    // Register an asset directory referencing the new permission.
    let dir = format!("itest-reg-{n}");
    let resp = c
        .post(format!(
            "{}/api/v1/odo/asset/directory/create",
            asset_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"path": dir, "read_perm": perm, "write_perm": perm}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "directory/create");

    // Cleanup: directory, then the authz rows via admin deletes.
    let resp = c
        .post(format!(
            "{}/api/v1/odo/asset/directory/delete",
            asset_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"path": dir}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "directory/delete");
}

#[tokio::test]
async fn registration_account_lacks_admin_reach() {
    let c = client();
    let token = registration_token(&c).await;

    // No odo.auth.user.detail.read: session/identity detail stays closed.
    let resp = c
        .post(format!("{}/api/v1/odo/auth/user/detail", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"id": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "user detail denied");

    // No odo.auth.user_role.read: it can grant a role but not enumerate who
    // holds one.
    let resp = c
        .post(format!(
            "{}/api/v1/odo/auth/authz/user-role/list",
            auth_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "user-role list denied");
}

/// Org structure used to be off limits to this account ("org structure is
/// the platform's"). It is not any more: an installation's own org tree is
/// site data, and 005_registration_org_units grants the read/write it needs
/// so that tree arrives as an upsert-only manifest instead of hand-written
/// SQL against the odo database.
#[tokio::test]
async fn registration_account_can_build_org_structure() {
    let c = client();
    let token = registration_token(&c).await;
    let n = suffix();

    // Unit types are needed to type a unit, so the account can list them.
    let resp = c
        .post(format!(
            "{}/api/v1/odo/org/admin/unit-type/list",
            org_base()
        ))
        .headers(auth_header(&token))
        .json(&json!({"limit": 200}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "unit-type/list");
    let types: serde_json::Value = resp.json().await.unwrap();
    let branch = types["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["label"] == "Branch")
        .expect("seeded Branch unit type");

    // The seeded root is the parent: units always attach below an existing
    // unit, which is why a manifest never creates a root.
    let resp = c
        .get(format!("{}/api/v1/odo/org/root", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "org/root");
    let root: serde_json::Value = resp.json().await.unwrap();

    let code = format!("ITREG{n}");
    let resp = c
        .post(format!("{}/api/v1/odo/org/admin/unit/create", org_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "label": format!("Registration Test Branch {n}"),
            "code": code,
            "parent": root["id"],
            "unit_type": branch["id"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "org unit create allowed");

    // And it shows up in the tree by code, which is how odo-register
    // resolves parents and how role assignments find their org unit.
    let resp = c
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "org/tree");
    let body = resp.text().await.unwrap();
    assert!(body.contains(&code), "new unit appears in the tree");
}
