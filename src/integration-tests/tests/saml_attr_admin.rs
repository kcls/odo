//! SAML attribute + attribute-role-mapping admin CRUD (odo-auth saml/admin/*).
//!
//! Requires the `e2e.odo.admin` test user and the `auth.saml.*` permissions.

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/auth/saml/admin{}", auth_base(), path)
}

fn unique(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("itest-{tag}-{nanos}")
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

/// Create a throwaway IdP to hang attributes off of; returns its id.
async fn create_test_idp(c: &reqwest::Client, token: &str, tag: &str) -> i64 {
    let resp = post_json(
        c,
        token,
        "/idp/create",
        json!({"name": format!("Attr Test IdP {tag}"), "entity_id": format!("https://{}.example.com", unique(tag))}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let idp: serde_json::Value = resp.json().await.unwrap();
    idp["id"].as_i64().unwrap()
}

#[tokio::test]
async fn staff_denied() {
    let c = client();
    let token = staff_token(&c).await;

    let resp = post_json(&c, &token, "/attribute/list", json!({})).await;
    assert_eq!(resp.status(), 403);

    let resp = post_json(&c, &token, "/attr-role-map/list", json!({})).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn attribute_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let idp_id = create_test_idp(&c, &token, "attr").await;

    // Create with a normalizer
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({
            "idp": idp_id,
            "key": "Department",
            "label": "Department",
            "normalizer": "split_slash_first"
        }),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let attr: serde_json::Value = resp.json().await.unwrap();
    let attr_id = attr["id"].as_i64().unwrap();
    assert_eq!(attr["is_location"], false);
    assert_eq!(attr["normalizer"], "split_slash_first");
    assert_eq!(attr["mapping_count"], 0);
    assert!(!attr["idp_name"].as_str().unwrap().is_empty());

    // Exact duplicate -> 409
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({
            "idp": idp_id,
            "key": "Department",
            "label": "Dup",
            "normalizer": "split_slash_first"
        }),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ATTRIBUTE_EXISTS");

    // Unknown normalizer -> 400
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({"idp": idp_id, "key": "Other", "label": "Other", "normalizer": "bogus"}),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Unknown IdP -> 404
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({"idp": 999999999, "key": "X", "label": "X"}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Update: relabel, mark as location, clear the normalizer
    let resp = post_json(
        &c,
        &token,
        "/attribute/update",
        json!({"id": attr_id, "label": "Dept", "is_location": true, "normalizer": ""}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["label"], "Dept");
    assert_eq!(updated["is_location"], true);
    assert!(updated["normalizer"].is_null());

    // Appears in list
    let resp = post_json(&c, &token, "/attribute/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"].as_i64() == Some(attr_id))
    );

    // Delete; delete again -> 404
    let resp = post_json(&c, &token, "/attribute/delete", json!({"id": attr_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/attribute/delete", json!({"id": attr_id})).await;
    assert_eq!(resp.status(), 404);

    // Cleanup
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn mapping_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let idp_id = create_test_idp(&c, &token, "map").await;
    let role_code = unique("maprole");
    let authz_base = format!("{}/api/v1/odo/auth/authz", auth_base());

    // Attribute + role to map between
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({"idp": idp_id, "key": "Title", "label": "Job Title"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let attr: serde_json::Value = resp.json().await.unwrap();
    let attr_id = attr["id"].as_i64().unwrap();

    let resp = c
        .post(format!("{authz_base}/role/create"))
        .headers(auth_header(&token))
        .json(&json!({"code": role_code, "label": "Mapping Test Role"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Create mapping
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/create",
        json!({"attr": attr_id, "role": role_code, "attr_value": "Test Librarian"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let mapping: serde_json::Value = resp.json().await.unwrap();
    let mapping_id = mapping["id"].as_i64().unwrap();
    assert_eq!(mapping["attr_key"], "Title");
    assert_eq!(mapping["role_label"], "Mapping Test Role");
    assert_eq!(mapping["is_active"], true);

    // Duplicate -> 409
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/create",
        json!({"attr": attr_id, "role": role_code, "attr_value": "Test Librarian"}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "MAPPING_EXISTS");

    // Unknown attr / role -> 404
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/create",
        json!({"attr": 999999999, "role": role_code, "attr_value": "X"}),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/create",
        json!({"attr": attr_id, "role": "no-such-role", "attr_value": "X"}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Update value + deactivate
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/update",
        json!({"id": mapping_id, "attr_value": "Senior Test Librarian", "is_active": false}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["attr_value"], "Senior Test Librarian");
    assert_eq!(updated["is_active"], false);

    // Attribute delete refused while mapped
    let resp = post_json(&c, &token, "/attribute/delete", json!({"id": attr_id})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "ATTRIBUTE_IN_USE");

    // Cleanup: mapping, then attribute, role, IdP
    let resp = post_json(&c, &token, "/attr-role-map/delete", json!({"id": mapping_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/attribute/delete", json!({"id": attr_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role/delete"))
        .headers(auth_header(&token))
        .json(&json!({"code": role_code}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn mapping_list_filters_and_paginates() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let idp_id = create_test_idp(&c, &token, "mapfilter").await;
    let role_code = unique("filterrole");
    let authz_base = format!("{}/api/v1/odo/auth/authz", auth_base());

    // Attribute + role
    let resp = post_json(
        &c,
        &token,
        "/attribute/create",
        json!({"idp": idp_id, "key": "Title", "label": "Job Title"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let attr_id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let resp = c
        .post(format!("{authz_base}/role/create"))
        .headers(auth_header(&token))
        .json(&json!({"code": role_code, "label": "Filter Test Role"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Three mappings with distinguishable values.
    for v in ["Alpha Manager", "Beta Manager", "Gamma Clerk"] {
        let resp = post_json(
            &c,
            &token,
            "/attr-role-map/create",
            json!({"attr": attr_id, "role": role_code, "attr_value": v}),
        )
        .await;
        assert_eq!(resp.status(), 200);
    }

    // Filter by IdP: exactly our three (the throwaway IdP has no others).
    let resp = post_json(&c, &token, "/attr-role-map/list", json!({"idp": idp_id})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["total"], 3);
    assert_eq!(data["rows"].as_array().unwrap().len(), 3);

    // Search narrows to matching values.
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/list",
        json!({"idp": idp_id, "search": "Manager"}),
    )
    .await;
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["total"], 2);
    assert_eq!(data["rows"].as_array().unwrap().len(), 2);

    // Search is case-insensitive.
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/list",
        json!({"idp": idp_id, "search": "manager"}),
    )
    .await;
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["total"], 2);

    // Pagination: total reflects the full match, page is limited.
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/list",
        json!({"idp": idp_id, "limit": 1, "offset": 0}),
    )
    .await;
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["total"], 3);
    assert_eq!(data["rows"].as_array().unwrap().len(), 1);

    // Filter by attr and role directly.
    let resp = post_json(
        &c,
        &token,
        "/attr-role-map/list",
        json!({"attr": attr_id, "role": role_code}),
    )
    .await;
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["total"], 3);

    // Cleanup: mappings, attribute, role, IdP.
    for m in data["rows"].as_array().unwrap() {
        let id = m["id"].as_i64().unwrap();
        let resp = post_json(&c, &token, "/attr-role-map/delete", json!({"id": id})).await;
        assert_eq!(resp.status(), 200);
    }
    let resp = post_json(&c, &token, "/attribute/delete", json!({"id": attr_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(format!("{authz_base}/role/delete"))
        .headers(auth_header(&token))
        .json(&json!({"code": role_code}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/idp/delete", json!({"id": idp_id})).await;
    assert_eq!(resp.status(), 200);
}
