use integration_tests::*;
use serde_json::json;

#[tokio::test]
async fn user_has_perm() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-has-perm", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"perm": "odo.auth.session"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["has_perm"], true);
}

#[tokio::test]
async fn user_has_perm_negative() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-has-perm", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"perm": "fake.nonexistent.perm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["has_perm"], false);
}

#[tokio::test]
async fn user_has_role() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-has-role", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({"role": "e2e-test-role"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["has_role"], true);
}

#[tokio::test]
async fn user_roles() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-roles", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let roles = data["roles"].as_array().unwrap();
    assert!(!roles.is_empty());
    assert!(roles[0]["role"].is_string());
    assert!(roles[0]["org_unit"].is_number());
}

#[tokio::test]
async fn authz_requires_token() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/user-has-perm", auth_base()))
        .json(&json!({"perm": "odo.auth.session"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// --- users_with_role ---

#[tokio::test]
async fn users_with_role_filters_to_holders() {
    let c = client();
    let token = staff_token(&c).await;
    let admin = odo_admin_token(&c).await;

    // Mix users: only e2e.odo.admin holds odo-admin.
    let sid = staff_id(&c).await;
    let sso_id = user_id_by_uuid(&c, &admin, SSO.uuid).await;
    let admin_id = token_user_id(&admin);
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/users-with-role", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "role": "odo-admin",
            "user_ids": [sid, sso_id, admin_id],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let returned: Vec<i64> = data["user_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();

    assert_eq!(returned, vec![admin_id]);
    assert_eq!(data["role"], "odo-admin");
    assert!(data["org_unit"].is_null());
}

#[tokio::test]
async fn users_with_role_with_org_unit() {
    let c = client();
    let token = staff_token(&c).await;
    let admin = odo_admin_token(&c).await;

    // e2e-test-role is granted at the root, so it should propagate down to
    // descendants like the branch. Both fixture holders should match when
    // checking at the branch.
    let sid = staff_id(&c).await;
    let sso_id = user_id_by_uuid(&c, &admin, SSO.uuid).await;
    let admin_id = token_user_id(&admin);
    let branch = branch_org_id(&c, &token).await;
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/users-with-role", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "role": "e2e-test-role",
            "user_ids": [sid, sso_id, admin_id],
            "org_unit": branch,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let mut returned: Vec<i64> = data["user_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    returned.sort();

    let mut expected = vec![sid, sso_id];
    expected.sort();
    assert_eq!(returned, expected);
    assert_eq!(data["org_unit"].as_i64(), Some(branch));
}

#[tokio::test]
async fn users_with_role_empty_input() {
    let c = client();
    let token = staff_token(&c).await;

    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/users-with-role", auth_base()))
        .headers(auth_header(&token))
        .json(&json!({
            "role": "e2e-test-role",
            "user_ids": [],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["user_ids"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn users_with_role_requires_auth() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/auth/authz/users-with-role", auth_base()))
        .json(&json!({
            "role": "e2e-test-role",
            "user_ids": [1],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
