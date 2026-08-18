use integration_tests::*;

#[tokio::test]
async fn health() {
    let c = client();
    let resp = c.get(format!("{}/health", org_base())).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn tree() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["id"].is_number());
    assert!(data["label"].is_string());
    assert!(data["parent"].is_null());
    assert!(data["children"].is_array());
    let children = data["children"].as_array().unwrap();
    if !children.is_empty() {
        assert_eq!(children[0]["parent"], data["id"]);
    }
}

#[tokio::test]
async fn detail() {
    let c = client();
    let token = staff_token(&c).await;
    let root = root_org_id(&c, &token).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{root}", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["org_unit"]["id"].as_i64(), Some(root));
    assert!(data["addresses"].is_array());
    assert!(data["operating_hours"].is_array());
    assert!(data["future_closures"].is_array());
}

#[tokio::test]
async fn ancestors() {
    let c = client();
    let token = staff_token(&c).await;
    // A branch two levels below the root gives the ancestors path depth.
    let branch = branch_org_id(&c, &token).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{branch}/ancestors", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!data.is_empty());
    assert!(data[0]["parent"].is_null(), "Ancestor list should start with root");
    assert_eq!(data.last().unwrap()["id"].as_i64(), Some(branch));
}

#[tokio::test]
async fn descendants() {
    let c = client();
    let token = staff_token(&c).await;
    let root = root_org_id(&c, &token).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{root}/descendants", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(!data.is_empty());
    assert_eq!(data[0]["id"].as_i64(), Some(root));

    let mut seen = std::collections::HashSet::new();
    for (i, node) in data.iter().enumerate() {
        let id = node["id"].as_i64().unwrap();
        if i > 0 {
            if let Some(parent) = node["parent"].as_i64() {
                assert!(seen.contains(&parent), "Parent {parent} should appear before child {id}");
            }
        }
        seen.insert(id);
    }
}

#[tokio::test]
async fn tree_requires_token() {
    let c = client();
    let resp = c
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// Durable-references changes: stable uuid + resolve-by-id include_deleted.
// Each test self-provisions its soft-deleted unit through the admin API
// (create under the root, then soft-delete), so no fixture ids are needed.
// ---------------------------------------------------------------------------

async fn provision_deleted_unit(c: &reqwest::Client, admin: &str) -> i64 {
    let root = root_org_id(c, admin).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Branch unit type id via the admin list.
    let resp = c
        .post(format!("{}/api/v1/odo/org/admin/unit-type/list", org_base()))
        .headers(auth_header(admin))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let types: serde_json::Value = resp.json().await.unwrap();
    let branch_type = types["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"] == "Branch")
        .expect("Branch unit type exists")["id"]
        .as_i64()
        .unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/org/admin/unit/create", org_base()))
        .headers(auth_header(admin))
        .json(&serde_json::json!({
            "label": format!("itest-deleted-{nanos}"),
            "code": format!("IDEL{}", nanos % 1_000_000),
            "parent": root,
            "unit_type": branch_type,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "create unit for deletion");
    let unit: serde_json::Value = resp.json().await.unwrap();
    let id = unit["id"].as_i64().unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/org/admin/unit/delete", org_base()))
        .headers(auth_header(admin))
        .json(&serde_json::json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "soft-delete the unit");
    id
}

#[tokio::test]
async fn tree_includes_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["uuid"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn unit_detail_includes_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let root = root_org_id(&c, &token).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{root}", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["org_unit"]["uuid"].as_str().is_some_and(|s| !s.is_empty()));
    // Active unit: no deleted_at.
    assert!(data["org_unit"].get("deleted_at").is_none() || data["org_unit"]["deleted_at"].is_null());
}

#[tokio::test]
async fn label_batch_returns_uuid_and_omits_deleted_by_default() {
    let c = client();
    let token = staff_token(&c).await;
    let admin = odo_admin_token(&c).await;
    let root = root_org_id(&c, &token).await;
    let deleted = provision_deleted_unit(&c, &admin).await;
    let resp = c
        .post(format!("{}/api/v1/odo/org/unit/label-batch", org_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [root, deleted]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let labels = data["labels"].as_array().unwrap();
    let active = labels.iter().find(|l| l["id"].as_i64() == Some(root)).expect("active root present");
    assert!(active["uuid"].as_str().is_some_and(|s| !s.is_empty()));
    // Soft-deleted unit is not returned without include_deleted.
    assert!(
        !labels.iter().any(|l| l["id"].as_i64() == Some(deleted)),
        "soft-deleted unit must be omitted by default"
    );
}

#[tokio::test]
async fn label_batch_include_deleted_returns_soft_deleted_flagged() {
    let c = client();
    let token = staff_token(&c).await;
    let admin = odo_admin_token(&c).await;
    let deleted = provision_deleted_unit(&c, &admin).await;
    let resp = c
        .post(format!("{}/api/v1/odo/org/unit/label-batch", org_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [deleted], "include_deleted": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let labels = data["labels"].as_array().unwrap();
    let del = labels
        .iter()
        .find(|l| l["id"].as_i64() == Some(deleted))
        .expect("soft-deleted unit returned with include_deleted");
    assert!(del["uuid"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(del["deleted_at"].as_str().is_some(), "deleted unit must carry deleted_at");
}

#[tokio::test]
async fn unit_detail_resolves_soft_deleted_flagged() {
    let c = client();
    let token = staff_token(&c).await;
    let admin = odo_admin_token(&c).await;
    let deleted = provision_deleted_unit(&c, &admin).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/{deleted}", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "soft-deleted unit resolves by id");
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["org_unit"]["id"].as_i64(), Some(deleted));
    assert!(data["org_unit"]["deleted_at"].as_str().is_some());
    assert!(data["org_unit"]["uuid"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn unit_detail_unknown_id_404() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .get(format!("{}/api/v1/odo/org/unit/2000000000", org_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
