//! Org unit and unit type admin CRUD (odo-org admin/*).
//!
//! Requires the `e2e.odo.admin` test user and the `org.unit.write`
//! permission from schema change `090_odo_admin_role`. The root org unit
//! id is resolved at runtime (code OLS in the platform seed).

use integration_tests::*;
use serde_json::json;

fn base(path: &str) -> String {
    format!("{}/api/v1/odo/org/admin{}", org_base(), path)
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


#[tokio::test]
async fn staff_denied() {
    let c = client();
    let token = staff_token(&c).await;
    let root = root_org_id(&c, &odo_admin_token(&c).await).await;

    let resp = post_json(&c, &token, "/unit-type/list", json!({})).await;
    assert_eq!(resp.status(), 403);

    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": "Denied", "code": "denied", "parent": root, "unit_type": 1}),
    )
    .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn unit_type_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let label = unique("type");

    // Create
    let resp = post_json(
        &c,
        &token,
        "/unit-type/create",
        json!({"label": label, "can_have_staff": true}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let unit_type: serde_json::Value = resp.json().await.unwrap();
    let type_id = unit_type["id"].as_i64().unwrap();
    assert_eq!(unit_type["can_have_staff"], true);
    assert_eq!(unit_type["unit_count"], 0);

    // Duplicate label -> 409
    let resp = post_json(&c, &token, "/unit-type/create", json!({"label": label})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "TYPE_LABEL_TAKEN");

    // Update
    let renamed = unique("type-renamed");
    let resp = post_json(
        &c,
        &token,
        "/unit-type/update",
        json!({"id": type_id, "label": renamed, "can_have_patrons": true}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["label"], renamed.as_str());
    assert_eq!(updated["can_have_patrons"], true);

    // Re-read from the list to confirm the edit persisted (not just echoed
    // back in the update response).
    let resp = post_json(&c, &token, "/unit-type/list", json!({})).await;
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let persisted = data["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_i64() == Some(type_id))
        .expect("updated unit type missing from list");
    assert_eq!(persisted["label"], renamed.as_str());
    assert_eq!(persisted["can_have_patrons"], true);

    // Delete (soft); delete again -> 404
    let resp = post_json(&c, &token, "/unit-type/delete", json!({"id": type_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/unit-type/delete", json!({"id": type_id})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn unit_crud_lifecycle() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let root = root_org_id(&c, &token).await;

    // Backing type
    let resp = post_json(
        &c,
        &token,
        "/unit-type/create",
        json!({"label": unique("unit-type")}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let unit_type: serde_json::Value = resp.json().await.unwrap();
    let type_id = unit_type["id"].as_i64().unwrap();

    // Create unit A under root
    let a_label = unique("unit-a");
    let a_code = unique("code-a");
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": a_label, "code": a_code, "parent": root, "unit_type": type_id, "timezone": "America/Los_Angeles"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let unit_a: serde_json::Value = resp.json().await.unwrap();
    let a_id = unit_a["id"].as_i64().unwrap();
    assert_eq!(unit_a["timezone"], "America/Los_Angeles");
    assert!(!unit_a["unit_type_label"].as_str().unwrap().is_empty());

    // Duplicate code / label -> field-specific 409s
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": unique("unit-dup"), "code": a_code, "parent": root, "unit_type": type_id}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "UNIT_CODE_TAKEN");

    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": a_label, "code": unique("code-dup"), "parent": root, "unit_type": type_id}),
    )
    .await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "UNIT_LABEL_TAKEN");

    // Type deletion refused while a unit uses it
    let resp = post_json(&c, &token, "/unit-type/delete", json!({"id": type_id})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "TYPE_IN_USE");

    // Unknown parent / type -> 404
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": unique("x"), "code": unique("x"), "parent": 999999999, "unit_type": type_id}),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": unique("x"), "code": unique("x"), "parent": root, "unit_type": 999999999}),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Create unit B under root, then move A under B
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": unique("unit-b"), "code": unique("code-b"), "parent": root, "unit_type": type_id}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let unit_b: serde_json::Value = resp.json().await.unwrap();
    let b_id = unit_b["id"].as_i64().unwrap();

    let resp = post_json(&c, &token, "/unit/update", json!({"id": a_id, "parent": b_id})).await;
    assert_eq!(resp.status(), 200);
    let moved: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(moved["parent"], b_id);

    // Confirm the move persisted by reading the tree fresh (odo-org has no
    // cache; this is a real round-trip through the DB).
    let resp = c
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let tree: serde_json::Value = resp.json().await.unwrap();
    let node_a = find_unit(&tree, a_id).expect("moved unit missing from tree");
    assert_eq!(node_a["parent"], b_id);

    // Cycle (B under A while A is under B) -> 400
    let resp = post_json(&c, &token, "/unit/update", json!({"id": b_id, "parent": a_id})).await;
    assert_eq!(resp.status(), 400);

    // Self-parent -> 400
    let resp = post_json(&c, &token, "/unit/update", json!({"id": a_id, "parent": a_id})).await;
    assert_eq!(resp.status(), 400);

    // Deleting B while A is below it -> 409
    let resp = post_json(&c, &token, "/unit/delete", json!({"id": b_id})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "UNIT_HAS_CHILDREN");

    // Root cannot be deleted
    let resp = post_json(&c, &token, "/unit/delete", json!({"id": root})).await;
    assert_eq!(resp.status(), 409);
    let err: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(err["code"], "UNIT_IS_ROOT");

    // Cleanup: A, then B, then the type; deleted unit -> 404 on re-delete
    let resp = post_json(&c, &token, "/unit/delete", json!({"id": a_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/unit/delete", json!({"id": a_id})).await;
    assert_eq!(resp.status(), 404);
    let resp = post_json(&c, &token, "/unit/delete", json!({"id": b_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/unit-type/delete", json!({"id": type_id})).await;
    assert_eq!(resp.status(), 200);
}

/// Depth-first search for a unit by id in the nested /tree response.
fn find_unit(node: &serde_json::Value, id: i64) -> Option<serde_json::Value> {
    if node["id"].as_i64() == Some(id) {
        return Some(node.clone());
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            if let Some(found) = find_unit(child, id) {
                return Some(found);
            }
        }
    }
    None
}

#[tokio::test]
async fn org_unit_children_crud() {
    let c = client();
    let token = odo_admin_token(&c).await;
    let root = root_org_id(&c, &token).await;

    // Throwaway unit to hang children off of.
    let resp = post_json(
        &c,
        &token,
        "/unit-type/create",
        json!({"label": unique("child-type")}),
    )
    .await;
    let type_id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let resp = post_json(
        &c,
        &token,
        "/unit/create",
        json!({"label": unique("child-unit"), "code": unique("child-code"), "parent": root, "unit_type": type_id}),
    )
    .await;
    let unit_id = resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    // --- Address ---
    let resp = post_json(
        &c,
        &token,
        "/address/create",
        json!({"org_unit": unit_id, "address_type": "physical", "label": unique("addr"), "city": "Seattle"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let addr_id = resp.json::<serde_json::Value>().await.unwrap()["id"].as_i64().unwrap();

    let resp = post_json(
        &c,
        &token,
        "/address/create",
        json!({"org_unit": unit_id, "address_type": "bogus", "label": unique("addr")}),
    )
    .await;
    assert_eq!(resp.status(), 400);

    let resp = post_json(
        &c,
        &token,
        "/address/update",
        json!({"id": addr_id, "city": "Bellevue"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.json::<serde_json::Value>().await.unwrap()["city"], "Bellevue");

    // --- Closure ---
    let resp = post_json(
        &c,
        &token,
        "/closure/create",
        json!({"org_unit": unit_id, "closure_date": "2020-12-25", "reason": "Holiday", "is_emergency": false}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let closure_id = resp.json::<serde_json::Value>().await.unwrap()["id"].as_i64().unwrap();

    // --- Operating hours ---
    let resp = post_json(
        &c,
        &token,
        "/operating-hours/create",
        json!({"org_unit": unit_id, "day_of_week": 1, "open_time": "09:00:00", "close_time": "17:00:00"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let hours_id = resp.json::<serde_json::Value>().await.unwrap()["id"].as_i64().unwrap();

    let resp = post_json(
        &c,
        &token,
        "/operating-hours/create",
        json!({"org_unit": unit_id, "day_of_week": 2, "open_time": "17:00:00", "close_time": "09:00:00"}),
    )
    .await;
    assert_eq!(resp.status(), 400);

    let resp = post_json(
        &c,
        &token,
        "/operating-hours/create",
        json!({"org_unit": unit_id, "day_of_week": 9, "open_time": "09:00:00", "close_time": "17:00:00"}),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Combined read returns all three (past closure included).
    let resp = post_json(&c, &token, "/unit-children", json!({"org_unit": unit_id})).await;
    assert_eq!(resp.status(), 200);
    let children: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(children["addresses"].as_array().unwrap().len(), 1);
    assert_eq!(children["closures"].as_array().unwrap().len(), 1);
    assert_eq!(children["operating_hours"].as_array().unwrap().len(), 1);
    assert_eq!(children["addresses"][0]["city"], "Bellevue");

    for (path, id) in [
        ("/address/delete", addr_id),
        ("/closure/delete", closure_id),
        ("/operating-hours/delete", hours_id),
    ] {
        let resp = post_json(&c, &token, path, json!({"id": id})).await;
        assert_eq!(resp.status(), 200);
    }
    let resp = post_json(&c, &token, "/unit-children", json!({"org_unit": unit_id})).await;
    let children: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(children["addresses"].as_array().unwrap().len(), 0);
    assert_eq!(children["closures"].as_array().unwrap().len(), 0);
    assert_eq!(children["operating_hours"].as_array().unwrap().len(), 0);

    let resp = post_json(&c, &token, "/unit/delete", json!({"id": unit_id})).await;
    assert_eq!(resp.status(), 200);
    let resp = post_json(&c, &token, "/unit-type/delete", json!({"id": type_id})).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn org_children_staff_denied() {
    let c = client();
    let token = staff_token(&c).await;
    let root = root_org_id(&c, &odo_admin_token(&c).await).await;
    let resp = post_json(&c, &token, "/unit-children", json!({"org_unit": root})).await;
    assert_eq!(resp.status(), 403);
    let resp = post_json(
        &c,
        &token,
        "/address/create",
        json!({"org_unit": root, "address_type": "physical", "label": "x"}),
    )
    .await;
    assert_eq!(resp.status(), 403);
}
