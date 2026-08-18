//! User role assignment CRUD (odo-auth authz/user-role/*).
//!
//! Requires the e2e test users (src/test-data). The org-scoped test also
//! exercises min_depth enforcement end-to-end using the platform seed's
//! org tree: OLS is the root and MAIN (Main Street Branch) is a branch two
//! levels below it. Org unit and user ids are resolved at runtime.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/auth/authz/user-role{}", auth_base(), path)
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

#[tokio::test]
async fn list_requires_token() {
    let c = client();
    let resp = c
        .post(base("/list"))
        .json(&json!({"usr": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn staff_denied_write() {
    let c = client();
    let token = staff_token(&c).await;
    let sid = staff_id(&c).await;
    let root = root_org_id(&c, &token).await;
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": sid, "role": "e2e-test-role", "org_unit": root}),
    )
    .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn assignment_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let sid = staff_id(&c).await;
    let branch = branch_org_id(&c, &token).await;

    // Assign e2e-test-role to e2e.odo.staff at the branch
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": sid, "role": "e2e-test-role", "org_unit": branch}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let assignment: serde_json::Value = resp.json().await.unwrap();
    let assignment_id = assignment["id"].as_i64().unwrap();
    assert_eq!(assignment["role"], "e2e-test-role");
    assert_eq!(assignment["org_unit"].as_i64(), Some(branch));
    assert_eq!(assignment["is_managed_by_saml"], false);
    assert!(!assignment["role_label"].as_str().unwrap().is_empty());
    assert!(!assignment["org_unit_label"].as_str().unwrap().is_empty());

    // Duplicate -> 409
    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": sid, "role": "e2e-test-role", "org_unit": branch}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ALREADY_ASSIGNED");

    // Shows up in the user's assignment list
    let resp = post_json(&c, &token, "/list", json!({"usr": sid})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"].as_i64() == Some(assignment_id))
    );

    // Remove
    let resp = post_json(&c, &token, "/delete", json!({"id": assignment_id})).await;
    assert_eq!(resp.status(), 200);

    // Remove again -> 404
    let resp = post_json(&c, &token, "/delete", json!({"id": assignment_id})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn create_validation() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let sid = staff_id(&c).await;
    let root = root_org_id(&c, &token).await;

    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": 999999999, "role": "e2e-test-role", "org_unit": root}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": sid, "role": "no-such-role", "org_unit": root}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = post_json(
        &c,
        &token,
        "/create",
        json!({"usr": sid, "role": "e2e-test-role", "org_unit": 999999999}),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

/// Effective-permission scopes: global via a root grant, and a bounded scope
/// via a branch-assigned role whose permission has min_depth 1 (which expands
/// to the region containing the branch).
#[tokio::test]
async fn user_perm_scopes_endpoint() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let scopes_url = format!("{}/api/v1/odo/auth/authz/user-perm-scopes", auth_base());
    let authz_base = format!("{}/api/v1/odo/auth/authz", auth_base());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scoped_role = format!("itest-scope-{nanos}");
    let scoped_perm = format!("itest.scope.{nanos}");
    let root = root_org_id(&c, &admin).await;
    let branch = branch_org_id(&c, &admin).await;
    let sso_id = user_id_by_uuid(&c, &admin, SSO.uuid).await;

    let scopes = |token: &str, usr: i32| {
        let url = scopes_url.clone();
        let headers = auth_header(token);
        let c = c.clone();
        async move {
            let resp = c
                .post(&url)
                .headers(headers)
                .json(&json!({ "usr": usr }))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            let body: serde_json::Value = resp.json().await.unwrap();
            body["perms"].as_array().unwrap().clone()
        }
    };

    // Requires the read perm.
    let staff = staff_token(&c).await;
    let resp = c
        .post(&scopes_url)
        .headers(auth_header(&staff))
        .json(&json!({ "usr": staff_id(&c).await }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    // odoadmin holds admin perms globally (assigned at root, min_depth 0).
    let admin_perms = scopes(&admin, token_user_id(&admin) as i32).await;
    let role_read = admin_perms
        .iter()
        .find(|p| p["perm"] == "odo.auth.role.read")
        .expect("odoadmin has odo.auth.role.read");
    assert_eq!(role_read["global"], true, "root grant is global");
    assert!(
        role_read["scope_units"].as_array().unwrap().is_empty(),
        "global perm has no scope units"
    );

    // Create a role granting a fresh perm at min_depth 1, assigned at the
    // branch (depth 2) -> scope expands to its region.
    let resp = c
        .post(format!("{authz_base}/permission/create"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_perm, "description": "itest scope perm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role/create"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_role, "label": "Scope Test Role"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role-permission/create"))
        .headers(auth_header(&admin))
        .json(&json!({"role": scoped_role, "perm": scoped_perm, "min_depth": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = post_json(
        &c,
        &admin,
        "/create",
        json!({"usr": sso_id, "role": scoped_role, "org_unit": branch}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let assignment_id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    // The target user now has the scoped perm, non-global, at one region root.
    let coord_perms = scopes(&admin, sso_id as i32).await;
    let scoped = coord_perms
        .iter()
        .find(|p| p["perm"] == scoped_perm)
        .expect("target user has the scoped perm");
    assert_eq!(scoped["global"], false, "min_depth 1 branch grant is not global");
    let units = scoped["scope_units"].as_array().unwrap();
    assert_eq!(units.len(), 1, "one minimal scope root (the region)");
    assert!(
        !units[0]["label"].as_str().unwrap().is_empty(),
        "scope unit has a label"
    );
    // The scope root is an ancestor of the branch (its region), not the root.
    assert_ne!(
        units[0]["id"].as_i64(),
        Some(root),
        "min_depth 1 does not reach the root"
    );

    // Cleanup.
    let resp = post_json(&c, &admin, "/delete", json!({"id": assignment_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role/delete"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_role}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/permission/delete"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_perm}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

/// End-to-end min_depth enforcement: a user holding `auth.user.role.write`
/// via a role granted at a branch with min_depth 1 can manage assignments
/// within that subtree but not at the root.
#[tokio::test]
async fn org_scoped_enforcement() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let root = root_org_id(&c, &admin).await;
    let branch = branch_org_id(&c, &admin).await;
    let sid = staff_id(&c).await;
    let sso_id = user_id_by_uuid(&c, &admin, SSO.uuid).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scoped_role = format!("itest-scoped-{nanos}");

    let authz_base = format!("{}/api/v1/odo/auth/authz", auth_base());

    // Create a role carrying auth.user.role.write at min_depth 1
    let resp = c
        .post(format!("{authz_base}/role/create"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_role, "label": "Scoped Role Admin"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = c
        .post(format!("{authz_base}/role-permission/create"))
        .headers(auth_header(&admin))
        .json(&json!({"role": scoped_role, "perm": "odo.auth.user_role.write", "min_depth": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Clean up leftovers from prior runs (a panicked run skips its cleanup):
    // drop any of the sso user's assignments except the fixture's
    // e2e-test-role @ root login grant.
    let resp = post_json(&c, &admin, "/list", json!({"usr": sso_id})).await;
    assert_eq!(resp.status(), 200);
    let existing: serde_json::Value = resp.json().await.unwrap();
    for a in existing["assignments"].as_array().unwrap() {
        let is_fixture_grant =
            a["role"] == "e2e-test-role" && a["org_unit"].as_i64() == Some(root);
        if !is_fixture_grant {
            let resp =
                post_json(&c, &admin, "/delete", json!({"id": a["id"].as_i64().unwrap()})).await;
            assert_eq!(resp.status(), 200);
        }
    }

    // Give e2e.odo.staff the scoped role at the branch (staff otherwise holds
    // only the login-only e2e-test-role, so all write ability comes from
    // this grant)
    let resp = post_json(
        &c,
        &admin,
        "/create",
        json!({"usr": sid, "role": scoped_role, "org_unit": branch}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let manager_assignment: serde_json::Value = resp.json().await.unwrap();
    let manager_assignment_id = manager_assignment["id"].as_i64().unwrap();

    // Fresh token; permission checks read the DB per request
    let manager = login_token(&c, &STAFF).await;

    // Within the branch subtree: allowed
    let resp = post_json(
        &c,
        &manager,
        "/create",
        json!({"usr": sso_id, "role": "e2e-test-role", "org_unit": branch}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let branch_assignment: serde_json::Value = resp.json().await.unwrap();
    let branch_assignment_id = branch_assignment["id"].as_i64().unwrap();

    // At the root (above min_depth): denied
    let resp = post_json(
        &c,
        &manager,
        "/create",
        json!({"usr": sso_id, "role": "e2e-test-role", "org_unit": root}),
    )
    .await;
    assert_eq!(resp.status(), 403);

    // Deleting a root-level assignment is denied too. (Use the test's own
    // scoped role: the fixture already assigns e2e-test-role @ root to the
    // sso user, so that combination would 409.)
    let resp = post_json(
        &c,
        &admin,
        "/create",
        json!({"usr": sso_id, "role": scoped_role, "org_unit": root}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let root_assignment: serde_json::Value = resp.json().await.unwrap();
    let root_assignment_id = root_assignment["id"].as_i64().unwrap();

    let resp = post_json(&c, &manager, "/delete", json!({"id": root_assignment_id})).await;
    assert_eq!(resp.status(), 403);

    // ...but a branch-level one is allowed
    let resp = post_json(&c, &manager, "/delete", json!({"id": branch_assignment_id})).await;
    assert_eq!(resp.status(), 200);

    // Cleanup
    let resp = post_json(&c, &admin, "/delete", json!({"id": root_assignment_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &admin, "/delete", json!({"id": manager_assignment_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role/delete"))
        .headers(auth_header(&admin))
        .json(&json!({"code": scoped_role}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
