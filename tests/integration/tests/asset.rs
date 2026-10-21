use integration_tests::*;
use reqwest::multipart;
use std::path::PathBuf;

fn create_test_file(filename: &str, content: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(filename);
    std::fs::write(&path, content).expect("Failed to write test file");
    path
}

async fn upload_file(
    token: &str,
    file_path: &PathBuf,
    category: &str,
    entity_type: &str,
    entity_id: Option<&str>,
) -> reqwest::Response {
    ensure_upload_fixtures().await;
    let file_content = std::fs::read(file_path).expect("Failed to read file");
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("test.txt")
        .to_string();

    let file_part = multipart::Part::bytes(file_content).file_name(file_name);

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("category", category.to_string())
        .text("entity_type", entity_type.to_string());

    if let Some(ei) = entity_id {
        form = form.text("entity_id", ei.to_string());
    }

    client()
        .post(format!("{}/api/v1/odo/asset/upload", asset_base()))
        .headers(auth_header(token))
        .multipart(form)
        .send()
        .await
        .expect("Upload request failed")
}

#[tokio::test]
async fn health() {
    let c = client();
    let resp = c
        .get(format!("{}/health", asset_base()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn upload_document() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"Integration test document content.";
    let path = create_test_file("odo_test_doc.txt", content);

    let resp = upload_file(&token, &path, "document", "e2e-file", Some("999")).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 200);

    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["id"].as_i64().unwrap() > 0);
    assert!(data["filename"].is_string());
    assert!(data["original_name"].as_str() == Some("odo_test_doc.txt"));
    assert!(data["relative_path"].as_str().unwrap().contains("e2e-assets/files"));
    assert_eq!(data["size"].as_i64(), Some(content.len() as i64));
    assert_eq!(data["mime_type"].as_str(), Some("text/plain"));
    assert_eq!(data["uploaded_by"].as_i64(), Some(staff_id(&c).await));
    assert_eq!(data["entity_type"].as_str(), Some("e2e-file"));
    assert_eq!(data["entity_id"].as_str(), Some("999"));
}

#[tokio::test]
async fn upload_file_field_after_metadata() {
    ensure_upload_fixtures().await;
    let c = client();
    let token = staff_token(&c).await;

    // The file is staged before the metadata that routes it is known;
    // the reverse field order must work too.
    let content = b"metadata first";
    let form = multipart::Form::new()
        .text("category", "document")
        .text("entity_type", "e2e-file")
        .part(
            "file",
            multipart::Part::bytes(content.to_vec()).file_name("odo_test_meta_first.txt"),
        );
    let resp = c
        .post(format!("{}/api/v1/odo/asset/upload", asset_base()))
        .headers(auth_header(&token))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["size"].as_i64(), Some(content.len() as i64));

    let body = c
        .get(format!(
            "{}/api/v1/odo/asset/files/{}",
            asset_base(),
            data["relative_path"].as_str().unwrap()
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(body.as_ref(), content);
}

#[tokio::test]
async fn retrieve_staging_dir_not_found() {
    let c = client();
    let token = staff_token(&c).await;

    // Any upload creates the staging directory; it must still not be served.
    let path = create_test_file("odo_test_staging.txt", b"x");
    let resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(resp.status(), 200);

    let resp = c
        .get(format!("{}/api/v1/odo/asset/files/.staging", asset_base()))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn upload_photo() {
    let c = client();
    let token = staff_token(&c).await;

    // Minimal valid JPEG (2 bytes: SOI marker)
    let content = &[0xFF, 0xD8, 0xFF, 0xE0];
    let path = create_test_file("odo_test_photo.jpg", content);

    let resp = upload_file(&token, &path, "photo", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 200);

    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["relative_path"].as_str().unwrap().contains("e2e-assets/photos"));
    assert_eq!(data["mime_type"].as_str(), Some("image/jpeg"));
}

#[tokio::test]
async fn upload_portrait_photo() {
    let c = client();
    let token = staff_token(&c).await;

    let content = &[0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
    let path = create_test_file("odo_test_patron.png", content);

    let resp = upload_file(&token, &path, "photo", "e2e-portrait", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 200);

    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(data["relative_path"].as_str().unwrap().contains("e2e-assets/portraits"));
}

#[tokio::test]
async fn upload_invalid_extension() {
    let c = client();
    let token = staff_token(&c).await;

    let path = create_test_file("odo_test_bad.exe", b"not a photo");

    let resp = upload_file(&token, &path, "photo", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn upload_invalid_entity_type() {
    let c = client();
    let token = staff_token(&c).await;

    let path = create_test_file("odo_test_entity.txt", b"test");

    let resp = upload_file(&token, &path, "document", "spaceship", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn upload_requires_token() {
    let path = create_test_file("odo_test_noauth.txt", b"test");

    let resp = upload_file("invalid-token", &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(resp.status(), 401);
}

// --- Retrieval ---

#[tokio::test]
async fn upload_and_retrieve() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"Content for retrieval round-trip test.";
    let path = create_test_file("odo_test_retrieve.txt", content);

    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();

    assert_eq!(upload_resp.status(), 200);

    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let relative_path = upload_data["relative_path"].as_str().unwrap();

    // Retrieve via Authorization header
    let retrieve_resp = c
        .get(format!("{}/api/v1/odo/asset/files/{}", asset_base(), relative_path))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();

    assert_eq!(retrieve_resp.status(), 200);
    assert_eq!(
        retrieve_resp.headers().get("content-type").unwrap(),
        "text/plain"
    );

    let body = retrieve_resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), content);
}

#[tokio::test]
async fn retrieve_large_file_streams_intact() {
    let c = client();
    let token = staff_token(&c).await;

    // Several MB of non-repeating-ish bytes, spanning many stream chunks.
    let content: Vec<u8> = (0..3 * 1024 * 1024 + 123)
        .map(|i: u32| (i.wrapping_mul(2654435761) >> 24) as u8)
        .collect();
    let path = create_test_file("odo_test_retrieve_large.txt", &content);

    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);

    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let relative_path = upload_data["relative_path"].as_str().unwrap();
    assert_eq!(upload_data["size"].as_i64(), Some(content.len() as i64));

    let retrieve_resp = c
        .get(format!("{}/api/v1/odo/asset/files/{}", asset_base(), relative_path))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();

    assert_eq!(retrieve_resp.status(), 200);
    assert_eq!(
        retrieve_resp.headers().get("content-length").unwrap(),
        &content.len().to_string()
    );

    let body = retrieve_resp.bytes().await.unwrap();
    assert_eq!(body.len(), content.len());
    assert!(body.as_ref() == content.as_slice(), "streamed body differs");
}

#[tokio::test]
async fn retrieve_range_requests() {
    let c = client();
    let token = staff_token(&c).await;

    let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let path = create_test_file("odo_test_retrieve_range.txt", &content);

    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);

    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let url = format!(
        "{}/api/v1/odo/asset/files/{}",
        asset_base(),
        upload_data["relative_path"].as_str().unwrap()
    );
    let len = content.len();

    // Full GET advertises range support.
    let full = c.get(&url).headers(auth_header(&token)).send().await.unwrap();
    assert_eq!(full.status(), 200);
    assert_eq!(full.headers().get("accept-ranges").unwrap(), "bytes");

    // Bounded range.
    let resp = c
        .get(&url)
        .headers(auth_header(&token))
        .header("Range", "bytes=100000-100999")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        &format!("bytes 100000-100999/{len}")
    );
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/plain");
    assert_eq!(resp.headers().get("cache-control").unwrap(), "no-cache");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &content[100_000..101_000]);

    // Suffix range.
    let resp = c
        .get(&url)
        .headers(auth_header(&token))
        .header("Range", "bytes=-50")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &content[len - 50..]);

    // Open-ended range.
    let resp = c
        .get(&url)
        .headers(auth_header(&token))
        .header("Range", format!("bytes={}-", len - 10))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &content[len - 10..]);

    // Past the end of the file.
    let resp = c
        .get(&url)
        .headers(auth_header(&token))
        .header("Range", format!("bytes={}-", len + 10))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn retrieve_range_requires_token() {
    // Range handling must not bypass auth.
    let resp = client()
        .get(format!("{}/api/v1/odo/asset/files/some/file.txt", asset_base()))
        .header("Range", "bytes=0-10")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn retrieve_head() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"HEAD request test content";
    let path = create_test_file("odo_test_retrieve_head.txt", content);

    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);

    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let resp = c
        .head(format!(
            "{}/api/v1/odo/asset/files/{}",
            asset_base(),
            upload_data["relative_path"].as_str().unwrap()
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-length").unwrap(),
        &content.len().to_string()
    );
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/plain");
}

#[tokio::test]
async fn retrieve_via_query_token() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"Query token retrieval test.";
    let path = create_test_file("odo_test_query_token.txt", content);

    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);

    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let relative_path = upload_data["relative_path"].as_str().unwrap();

    // Retrieve via ?token= query param (no Authorization header)
    let retrieve_resp = c
        .get(format!(
            "{}/api/v1/odo/asset/files/{}?token={}",
            asset_base(),
            relative_path,
            token
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(retrieve_resp.status(), 200);

    let body = retrieve_resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), content);
}

#[tokio::test]
async fn retrieve_not_found() {
    let c = client();
    let token = staff_token(&c).await;

    let resp = c
        .get(format!(
            "{}/api/v1/odo/asset/files/nonexistent/path/file.txt",
            asset_base()
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn retrieve_requires_token() {
    let c = client();

    let resp = c
        .get(format!("{}/api/v1/odo/asset/files/some/file.txt", asset_base()))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn retrieve_path_traversal_blocked() {
    let c = client();
    let token = staff_token(&c).await;

    let resp = c
        .get(format!(
            "{}/api/v1/odo/asset/files/../../etc/passwd",
            asset_base()
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();

    // Should return 404 (path traversal components stripped) or 400
    assert!(
        resp.status() == 404 || resp.status() == 400,
        "Path traversal should be blocked, got {}",
        resp.status()
    );
}

// --- files/get (batch metadata lookup) ---

#[tokio::test]
async fn files_get_requires_auth() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .json(&serde_json::json!({"ids": [1]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn files_get_empty_ids_returns_empty() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["files"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn files_get_returns_metadata_for_uploaded_file() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"files/get round-trip";
    let path = create_test_file("odo_test_files_get.txt", content);
    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);
    let id = upload_resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let files = data["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["id"].as_i64(), Some(id));
    assert_eq!(files[0]["file_name"].as_str(), Some("odo_test_files_get.txt"));
    assert_eq!(files[0]["file_size"].as_i64(), Some(content.len() as i64));
    assert!(files[0]["relative_path"].as_str().unwrap().contains("e2e-assets/files"));
    assert_eq!(files[0]["uploaded_by"].as_i64(), Some(staff_id(&c).await));
}

#[tokio::test]
async fn files_get_silently_omits_missing_ids() {
    // Bad ids don't 404 — the endpoint is batch-style. Callers should
    // detect missing rows by id-set diff, not by HTTP status.
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [2_000_000_000_i64]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["files"].as_array().unwrap().len(), 0);
}

// --- files/delete ---

#[tokio::test]
async fn files_delete_requires_auth() {
    let c = client();
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .json(&serde_json::json!({"id": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn files_delete_unknown_id_returns_404() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"id": 2_000_000_000_i64}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn files_delete_round_trip_hides_from_files_get_and_retrieve() {
    let c = client();
    let token = staff_token(&c).await;

    let content = b"files/delete round-trip";
    let path = create_test_file("odo_test_files_delete.txt", content);
    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);
    let upload_data: serde_json::Value = upload_resp.json().await.unwrap();
    let id = upload_data["id"].as_i64().unwrap();
    let relative_path = upload_data["relative_path"].as_str().unwrap().to_string();

    // Delete.
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(data["success"].as_bool(), Some(true));
    assert_eq!(data["id"].as_i64(), Some(id));
    // Fresh upload's file is on disk, so removal should succeed.
    assert_eq!(data["file_removed"].as_bool(), Some(true));

    // files/get now omits the deleted id.
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        data["files"].as_array().unwrap().len(),
        0,
        "deleted file should be omitted from files/get"
    );

    // /files/{path} now 404s — the file is gone from disk.
    let resp = c
        .get(format!(
            "{}/api/v1/odo/asset/files/{}",
            asset_base(),
            relative_path
        ))
        .headers(auth_header(&token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn files_delete_twice_is_invalid_input() {
    let c = client();
    let token = staff_token(&c).await;

    let path = create_test_file("odo_test_delete_twice.txt", b"twice");
    let upload_resp = upload_file(&token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    let id = upload_resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    assert_eq!(
        c.post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
            .headers(auth_header(&token))
            .json(&serde_json::json!({"id": id}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Durable-references changes: stable uuid + files/get include_deleted.
// Soft-deleted fixture is created live (upload -> delete).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn files_get_includes_uuid() {
    let c = client();
    let token = staff_token(&c).await;
    let path = create_test_file("odo_test_files_uuid.txt", b"uuid");
    let id = upload_file(&token, &path, "document", "e2e-file", None)
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    std::fs::remove_file(&path).ok();

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let f = &data["files"].as_array().unwrap()[0];
    assert!(f["uuid"].as_str().is_some_and(|s| !s.is_empty()));
    // Active file: no deleted_at.
    assert!(f.get("deleted_at").is_none() || f["deleted_at"].is_null());
}

#[tokio::test]
async fn files_get_include_deleted_returns_deleted_flagged() {
    let c = client();
    let token = staff_token(&c).await;
    let path = create_test_file("odo_test_files_incl_deleted.txt", b"incl-deleted");
    let id = upload_file(&token, &path, "document", "e2e-file", None)
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    std::fs::remove_file(&path).ok();

    // Soft-delete it.
    assert_eq!(
        c.post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
            .headers(auth_header(&token))
            .json(&serde_json::json!({"id": id}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Default: omitted.
    let default_resp: serde_json::Value = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [id]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(default_resp["files"].as_array().unwrap().len(), 0);

    // include_deleted: returned, flagged.
    let incl: serde_json::Value = c
        .post(format!("{}/api/v1/odo/asset/files/get", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"ids": [id], "include_deleted": true}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let files = incl["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "include_deleted returns the soft-deleted file");
    assert_eq!(files[0]["id"].as_i64(), Some(id));
    assert!(files[0]["deleted_at"].as_str().is_some(), "must carry deleted_at");
    assert!(files[0]["uuid"].as_str().is_some_and(|s| !s.is_empty()));
}

// ---------------------------------------------------------------------------
// Directory registry admin: apps register their directories (and the
// permissions those directories reference) through this API.
// ---------------------------------------------------------------------------

fn dir_url(path: &str) -> String {
    format!("{}/api/v1/odo/asset/directory{}", asset_base(), path)
}

#[tokio::test]
async fn staff_denied_directory_admin() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(dir_url("/list"))
        .headers(auth_header(&token))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn directory_registry_lifecycle() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = format!("itest-dir-{nanos}/sub");
    let read_perm = format!("itest.dir.{nanos}.read");
    let write_perm = format!("itest.dir.{nanos}.write");

    // Directories reference permission codes by FK: register the
    // permissions first (same order app registration uses).
    for perm in [&read_perm, &write_perm] {
        let resp = c
            .post(format!("{}/api/v1/odo/auth/authz/permission/create", auth_base()))
            .headers(auth_header(&admin))
            .json(&serde_json::json!({"code": perm, "description": "itest directory perm"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    // Invalid paths are rejected.
    for bad in ["/leading", "trailing/", "has space", "dot/../dot"] {
        let resp = c
            .post(dir_url("/create"))
            .headers(auth_header(&admin))
            .json(&serde_json::json!({"path": bad, "read_perm": read_perm, "write_perm": write_perm}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "path {bad:?} should be rejected");
    }

    // Register.
    let resp = c
        .post(dir_url("/create"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({
            "path": path,
            "read_perm": read_perm,
            "write_perm": write_perm,
            "description": "integration test directory",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "create: {:?}", resp.text().await);
    let row: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(row["path"].as_str(), Some(path.as_str()));

    // Unknown permission codes -> 400.
    let resp = c
        .post(dir_url("/create"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": format!("{path}-x"), "read_perm": "no.such.perm", "write_perm": "no.such.perm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Duplicate -> 409.
    let resp = c
        .post(dir_url("/create"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": path, "read_perm": read_perm, "write_perm": write_perm}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    // Listed.
    let resp = c
        .post(dir_url("/list"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let rows: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert!(rows.iter().any(|r| r["path"].as_str() == Some(path.as_str())));

    // Delete; second delete 404s.
    let resp = c
        .post(dir_url("/delete"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": path}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(dir_url("/delete"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": path}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // Cleanup the itest permissions.
    for perm in [&read_perm, &write_perm] {
        let resp = c
            .post(format!("{}/api/v1/odo/auth/authz/permission/delete", auth_base()))
            .headers(auth_header(&admin))
            .json(&serde_json::json!({"code": perm}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }
}

#[tokio::test]
async fn directory_upload_mapping_conflicts() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let entity = format!("itest-map-{nanos}");

    // category without entity_type is a client error.
    let resp = c
        .post(dir_url("/create"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({
            "path": format!("itest-map-{nanos}/orphan"),
            "read_perm": "odo.auth.session",
            "write_perm": "odo.auth.session",
            "category": "photo",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "category requires entity_type");

    // First mapping registers; a second directory claiming the same
    // (entity_type, category) slot conflicts.
    for (path, expect) in [
        (format!("itest-map-{nanos}/a"), 200),
        (format!("itest-map-{nanos}/b"), 409),
    ] {
        let resp = c
            .post(dir_url("/create"))
            .headers(auth_header(&admin))
            .json(&serde_json::json!({
                "path": path,
                "read_perm": "odo.auth.session",
                "write_perm": "odo.auth.session",
                "entity_type": entity,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), expect);
    }

    // Uploads for the new entity route into the registered path.
    let file = create_test_file(&format!("itest-map-{nanos}.txt"), b"mapped");
    let resp = upload_file(
        &staff_token(&c).await,
        &file,
        "document",
        &entity,
        None,
    )
    .await;
    std::fs::remove_file(&file).ok();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    assert!(
        data["relative_path"]
            .as_str()
            .unwrap()
            .contains(&format!("itest-map-{nanos}/a"))
    );

    // Cleanup: delete both directories (the mapped one now holds a file,
    // so its delete must refuse; delete the file first).
    let id = data["id"].as_i64().unwrap();
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/delete", asset_base()))
        .headers(auth_header(&staff_token(&c).await))
        .json(&serde_json::json!({"id": id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(dir_url("/delete"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": format!("itest-map-{nanos}/a")}))
        .send()
        .await
        .unwrap();
    assert!(matches!(resp.status().as_u16(), 200 | 409));
}

// ---------------------------------------------------------------------------
// files/sign (signed retrieval URLs)
// ---------------------------------------------------------------------------

async fn upload_and_sign(
    c: &reqwest::Client,
    token: &str,
    name: &str,
    content: &[u8],
) -> (String, String) {
    let path = create_test_file(name, content);
    let upload_resp = upload_file(token, &path, "document", "e2e-file", None).await;
    std::fs::remove_file(&path).ok();
    assert_eq!(upload_resp.status(), 200);
    let relative_path = upload_resp.json::<serde_json::Value>().await.unwrap()["relative_path"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/sign", asset_base()))
        .headers(auth_header(token))
        .json(&serde_json::json!({"paths": [relative_path]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let data: serde_json::Value = resp.json().await.unwrap();
    let signed = &data["urls"][0];
    assert_eq!(signed["path"].as_str(), Some(relative_path.as_str()));
    assert!(signed["expires_at"].as_str().is_some_and(|s| !s.is_empty()));
    (relative_path, signed["url"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn signed_url_retrieves_without_jwt() {
    let c = client();
    let token = staff_token(&c).await;
    let content: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    let (_, url) = upload_and_sign(&c, &token, "odo_test_signed.txt", &content).await;

    let resp = c
        .get(format!("{}{}", asset_base(), url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), content.as_slice());

    // Range requests work on signed URLs too.
    let resp = c
        .get(format!("{}{}", asset_base(), url))
        .header("Range", "bytes=100-199")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &content[100..200]);
}

#[tokio::test]
async fn signed_url_tampering_rejected() {
    let c = client();
    let token = staff_token(&c).await;
    let (_, url_a) = upload_and_sign(&c, &token, "odo_test_signed_a.txt", b"file a").await;
    let (path_b, _) = upload_and_sign(&c, &token, "odo_test_signed_b.txt", b"file b").await;

    let (path_a, query) = url_a.split_once('?').unwrap();
    let path_a = path_a.trim_start_matches("/api/v1/odo/asset/files/");
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
            .unwrap()
            .to_string()
    };
    let (expires, uid, sig) = (param("expires"), param("uid"), param("sig"));
    let mut flipped_sig = sig.clone();
    let last = if flipped_sig.pop() == Some('0') {
        '1'
    } else {
        '0'
    };
    flipped_sig.push(last);

    for (label, path, expires, uid, sig) in [
        (
            "other file",
            path_b.as_str(),
            expires.clone(),
            uid.clone(),
            sig.clone(),
        ),
        (
            "altered sig",
            path_a,
            expires.clone(),
            uid.clone(),
            flipped_sig,
        ),
        (
            "altered uid",
            path_a,
            expires.clone(),
            format!("{}1", uid),
            sig.clone(),
        ),
        (
            "extended expiry",
            path_a,
            (expires.parse::<i64>().unwrap() + 300).to_string(),
            uid.clone(),
            sig.clone(),
        ),
        ("expired", path_a, "1".to_string(), uid.clone(), sig.clone()),
    ] {
        let resp = c
            .get(format!(
                "{}/api/v1/odo/asset/files/{path}?expires={expires}&uid={uid}&sig={sig}",
                asset_base()
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401, "{label} should be rejected");
    }
}

#[tokio::test]
async fn sign_requires_auth() {
    let resp = client()
        .post(format!("{}/api/v1/odo/asset/files/sign", asset_base()))
        .json(&serde_json::json!({"paths": ["e2e-assets/files/x.txt"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn sign_rejects_empty_path() {
    let c = client();
    let token = staff_token(&c).await;
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/sign", asset_base()))
        .headers(auth_header(&token))
        .json(&serde_json::json!({"paths": ["e2e-assets/files/x.txt", ".."]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn sign_denied_without_read_permission() {
    let c = client();
    let admin = odo_admin_token(&c).await;
    let staff = staff_token(&c).await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = format!("itest-sign-{nanos}");
    let perm = format!("itest.sign.{nanos}.read");

    let resp = c
        .post(format!(
            "{}/api/v1/odo/auth/authz/permission/create",
            auth_base()
        ))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"code": perm, "description": "itest sign perm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = c
        .post(dir_url("/create"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": dir, "read_perm": perm, "write_perm": perm}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // One path the caller may read and one it may not: the whole request
    // is denied.
    let resp = c
        .post(format!("{}/api/v1/odo/asset/files/sign", asset_base()))
        .headers(auth_header(&staff))
        .json(&serde_json::json!({"paths": ["e2e-assets/files/x.txt", format!("{dir}/x.jpg")]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    let resp = c
        .post(dir_url("/delete"))
        .headers(auth_header(&admin))
        .json(&serde_json::json!({"path": dir}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
