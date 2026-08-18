//! Authz admin CRUD (odo-auth): permissions, roles, role-permission grants.
//!
//! Requires the `e2e.odo.admin` test user (sqitch test-data
//! `005_e2e_odo_admin_user`) and the `auth.authz.*` permissions from
//! schema change `090_odo_admin_role`.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/auth/authz{}", auth_base(), path)
}

/// Codes are globally unique and hard-deleted; mint per-run codes anyway so
/// an aborted run can't poison the next one.
fn unique_code(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest.{tag}.{nanos}")
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
async fn list_permissions_requires_token() {
    let c = client();
    let resp = c
        .post(base("/permission/list"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn staff_denied_read() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = post_json(&c, &token, "/permission/list", json!({})).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn staff_denied_write() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = post_json(
        &c,
        &token,
        "/permission/create",
        json!({"code": "staff.denied"}),
    )
    .await;
    assert_eq!(resp.status(), 403);
}

// --- Permission CRUD ---

#[tokio::test]
async fn permission_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let code = unique_code("perm");

    // Create
    let resp = post_json(
        &c,
        &token,
        "/permission/create",
        json!({"code": code, "description": "Test permission"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let perm: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(perm["code"], code.as_str());
    assert_eq!(perm["role_count"], 0);

    // Duplicate -> 409
    let resp = post_json(&c, &token, "/permission/create", json!({"code": code})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "PERMISSION_CODE_TAKEN");

    // Appears in list
    let resp = post_json(&c, &token, "/permission/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["code"] == code.as_str())
    );

    // Update description
    let resp = post_json(
        &c,
        &token,
        "/permission/update",
        json!({"code": code, "description": "Updated description"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["description"], "Updated description");

    // Delete
    let resp = post_json(&c, &token, "/permission/delete", json!({"code": code})).await;
    assert_eq!(resp.status(), 200);

    // Delete again -> 404
    let resp = post_json(&c, &token, "/permission/delete", json!({"code": code})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn permission_list_sorts_by_allowed_columns() {
    let c = client();
    let token = odo_admin_token(&c).await;

    let codes = |data: &serde_json::Value| -> Vec<String> {
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["code"].as_str().unwrap().to_string())
            .collect()
    };

    // Default sort == explicit code ascending (the DB's own collation order;
    // we don't re-sort in Rust, whose byte order differs from Postgres locale
    // collation for punctuation like `.` vs `_`).
    let resp = post_json(&c, &token, "/permission/list", json!({})).await;
    let default_codes = codes(&resp.json().await.unwrap());
    let resp = post_json(
        &c,
        &token,
        "/permission/list",
        json!({"sort_by": "code", "sort_dir": "asc"}),
    )
    .await;
    let asc_codes = codes(&resp.json().await.unwrap());
    assert_eq!(default_codes, asc_codes, "default sort is code ascending");

    // Explicit code descending is the exact reverse of code ascending.
    let resp = post_json(
        &c,
        &token,
        "/permission/list",
        json!({"sort_by": "code", "sort_dir": "desc"}),
    )
    .await;
    let desc_codes = codes(&resp.json().await.unwrap());
    let mut desc_expected = asc_codes.clone();
    desc_expected.reverse();
    assert_eq!(desc_codes, desc_expected, "code desc reverses code asc");

    // An unknown / malicious sort key must fall back to the default, not error
    // and not inject — the allow-list rejects it.
    let resp = post_json(
        &c,
        &token,
        "/permission/list",
        json!({"sort_by": "code; DROP TABLE authz.permission", "sort_dir": "desc"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let bad: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(codes(&bad), asc_codes, "unknown sort key falls back to default");
}

#[tokio::test]
async fn permission_create_rejects_bad_codes() {
    let c = client();
    let token = odo_admin_token(&c).await;

    let resp = post_json(&c, &token, "/permission/create", json!({"code": "  "})).await;
    assert_eq!(resp.status(), 400);

    let resp = post_json(
        &c,
        &token,
        "/permission/create",
        json!({"code": "has whitespace"}),
    )
    .await;
    assert_eq!(resp.status(), 400);
}

// --- Role CRUD + grants ---

#[tokio::test]
async fn role_crud_with_grants() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let role_code = unique_code("role");
    let perm_code = unique_code("perm");

    // Create role + permission
    let resp = post_json(
        &c,
        &token,
        "/role/create",
        json!({"code": role_code, "label": "Test Role", "description": "For testing"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let role: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(role["perm_count"], 0);
    assert_eq!(role["user_count"], 0);

    let resp = post_json(&c, &token, "/permission/create", json!({"code": perm_code})).await;
    assert_eq!(resp.status(), 200);

    // Duplicate role -> 409
    let resp = post_json(
        &c,
        &token,
        "/role/create",
        json!({"code": role_code, "label": "Again"}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ROLE_CODE_TAKEN");

    // Grant the permission at min_depth 1
    let resp = post_json(
        &c,
        &token,
        "/role-permission/create",
        json!({"role": role_code, "perm": perm_code, "min_depth": 1}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let grant: serde_json::Value = resp.json().await.unwrap();
    let grant_id = grant["id"].as_i64().unwrap();
    assert_eq!(grant["min_depth"], 1);

    // Duplicate grant -> 409
    let resp = post_json(
        &c,
        &token,
        "/role-permission/create",
        json!({"role": role_code, "perm": perm_code}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "PERMISSION_ALREADY_GRANTED");

    // role/get shows the grant
    let resp = post_json(&c, &token, "/role/get", json!({"code": role_code})).await;
    assert_eq!(resp.status(), 200);
    let detail: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(detail["role"]["perm_count"], 1);
    assert_eq!(detail["grants"][0]["perm"], perm_code.as_str());
    assert_eq!(detail["grants"][0]["min_depth"], 1);

    // Update role label
    let resp = post_json(
        &c,
        &token,
        "/role/update",
        json!({"code": role_code, "label": "Renamed Role"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["label"], "Renamed Role");

    // Update grant min_depth
    let resp = post_json(
        &c,
        &token,
        "/role-permission/update",
        json!({"id": grant_id, "min_depth": 2}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["min_depth"], 2);

    // Deleting the permission while granted -> 409
    let resp = post_json(&c, &token, "/permission/delete", json!({"code": perm_code})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "PERMISSION_IN_USE");

    // Deleting the role removes its grants with it
    let resp = post_json(&c, &token, "/role/delete", json!({"code": role_code})).await;
    assert_eq!(resp.status(), 200);

    let resp = post_json(&c, &token, "/role/get", json!({"code": role_code})).await;
    assert_eq!(resp.status(), 404);

    // Grant went with the role; the permission is now deletable
    let resp = post_json(&c, &token, "/permission/delete", json!({"code": perm_code})).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn role_list_sorts_by_label() {
    let c = client();
    let token = odo_admin_token(&c).await;

    let labels = |data: &serde_json::Value| -> Vec<String> {
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["label"].as_str().unwrap().to_string())
            .collect()
    };

    // label ascending then descending.
    let resp = post_json(
        &c,
        &token,
        "/role/list",
        json!({"sort_by": "label", "sort_dir": "asc"}),
    )
    .await;
    let asc = labels(&resp.json().await.unwrap());

    let resp = post_json(
        &c,
        &token,
        "/role/list",
        json!({"sort_by": "label", "sort_dir": "desc"}),
    )
    .await;
    let desc = labels(&resp.json().await.unwrap());

    // Same rows both ways; the extremes swap (collation-agnostic, tie-tolerant
    // — we don't re-sort in Rust, whose ordering differs from PG collation).
    assert_eq!(asc.len(), desc.len());
    assert!(!asc.is_empty(), "test data has roles");
    assert_eq!(asc.first(), desc.last(), "asc first == desc last");
    assert_eq!(asc.last(), desc.first(), "asc last == desc first");
}

#[tokio::test]
async fn role_delete_refused_while_assigned() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // e2e.odo.admin holds odo-admin, so deletion must be refused.
    let resp = post_json(&c, &token, "/role/delete", json!({"code": "odo-admin"})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ROLE_ASSIGNED");
}

#[tokio::test]
async fn grant_validation() {
    let c = client();
    let token = odo_admin_token(&c).await;

    // Unknown role / perm -> 404
    let resp = post_json(
        &c,
        &token,
        "/role-permission/create",
        json!({"role": "no-such-role", "perm": "odo.auth.session"}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = post_json(
        &c,
        &token,
        "/role-permission/create",
        json!({"role": "odo-admin", "perm": "no.such.perm"}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Negative min_depth -> 400
    let resp = post_json(
        &c,
        &token,
        "/role-permission/create",
        json!({"role": "odo-admin", "perm": "odo.auth.session", "min_depth": -1}),
    )
    .await;
    assert_eq!(resp.status(), 400);
}
