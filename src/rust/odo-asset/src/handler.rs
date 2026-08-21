use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use odo_client::context::RequestContext;
use odo_entity::asset::directory as asset_directory;
use odo_entity::asset::file_upload;
use odo_client::error::{ApiResult, LocalError};
use sea_orm::{Condition, Set};
use sea_orm::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

use crate::AppState;

// --- Directory mapping & permissions ---
//
// Two layers:
//
//   1. The (entity_type, category) -> relative path routing is data in
//      code. All files now land under `current/...`. Old layouts
//      (`incident-tracker/photos`, `patrons/photos`, ...) are mapped to the
//      new paths by Envoy rewrite rules for retrieval backwards-compat — see
//      k8s/infrastructure/envoy/odo-routes.yaml.
//
//   2. *Which* directories are permission-protected, and the read/write
//      permission guarding each, is database-driven via `asset.directory`.
//      A target path is gated by the row whose `path` is its longest prefix
//      (so `current/patron/...` is guarded by the `current/patron` row while
//      everything else under `current` falls back to the `current` row).

/// Resolve the upload target from the registered directory mappings: an
/// exact (entity_type, category) row wins; a (entity_type, NULL) row is
/// the catch-all. No mapping registered for the entity_type is a client
/// error — apps register their upload routing with their directories.
fn resolve_target_directory<'a>(
    dirs: &'a [asset_directory::Model],
    entity_type: Option<&str>,
    category: &str,
) -> Result<&'a str, LocalError> {
    let Some(entity_type) = entity_type.filter(|s| !s.is_empty()) else {
        return Err(LocalError::invalid_input("entity_type is required"));
    };
    let candidates = dirs
        .iter()
        .filter(|d| d.entity_type.as_deref() == Some(entity_type));
    let mut catch_all = None;
    for d in candidates {
        match d.category.as_deref() {
            Some(c) if c == category => return Ok(&d.path),
            None => catch_all = Some(d.path.as_str()),
            _ => {}
        }
    }
    catch_all.ok_or_else(|| {
        LocalError::invalid_input(format!(
            "no asset directory registered for entity type: {entity_type}"
        ))
    })
}

/// Access mode for a directory permission lookup.
#[derive(Clone, Copy)]
enum Access {
    Read,
    Write,
}

/// Resolve the permission guarding `target_path` via longest-prefix match
/// against `asset.directory`, then verify the calling user holds it (at the
/// caller's token org unit) by asking odo-auth.
///
/// A path with no matching `asset.directory` row is unprotected and allowed —
/// only configured directories are gated. Matching is on whole path segments
/// so `current/patronage` is not matched by a `current/patron` row.
async fn require_directory_access(
    state: &AppState,
    target_path: &str,
    access: Access,
) -> Result<(), LocalError> {
    let dirs = asset_directory::Entity::find().all(&state.db).await?;

    let best = dirs
        .iter()
        .filter(|d| path_has_prefix(target_path, &d.path))
        .max_by_key(|d| d.path.len());

    let Some(dir) = best else {
        // No configured directory covers this path; nothing to enforce.
        return Ok(());
    };

    let perm = match access {
        Access::Read => &dir.read_perm,
        Access::Write => &dir.write_perm,
    };

    // Check at the caller's currently-selected org unit (from their JWT),
    // falling back to root (None) when the token carries no org unit.
    let org_unit = RequestContext::claims()
        .and_then(|c| c.org_unit)
        .and_then(|ou| sea_orm::prelude::Uuid::parse_str(&ou).ok());

    state
        .auth_client
        .permission_required_uuid(perm, org_unit.as_ref())
        .await
}

/// True when `prefix` matches `path` on whole path segments (a `current`
/// prefix matches `current` and `current/photos` but not `currentish`).
fn path_has_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn is_allowed_extension(category: &str, ext: &str) -> bool {
    let allowed = match category {
        "photo" => &[".jpg", ".jpeg", ".png", ".gif", ".webp", ".heic"][..],
        "document" => &[".pdf", ".doc", ".docx", ".txt", ".rtf", ".odt"][..],
        "video" => &[".mp4", ".avi", ".mov", ".wmv", ".flv", ".webm"][..],
        _ => return false,
    };
    allowed.contains(&ext)
}

fn get_mime_type(ext: &str) -> &'static str {
    match ext {
        ".jpg" | ".jpeg" => "image/jpeg",
        ".png" => "image/png",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".heic" => "image/heic",
        ".pdf" => "application/pdf",
        ".doc" => "application/msword",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".txt" => "text/plain",
        ".rtf" => "application/rtf",
        ".odt" => "application/vnd.oasis.opendocument.text",
        ".mp4" => "video/mp4",
        ".avi" => "video/x-msvideo",
        ".mov" => "video/quicktime",
        ".wmv" => "video/x-ms-wmv",
        ".flv" => "video/x-flv",
        ".webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn sanitize_filename(filename: &str) -> String {
    let name = StdPath::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(50)
        .collect();

    if safe.is_empty() {
        "file".to_string()
    } else {
        safe
    }
}

fn generate_file_id() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::rng().random();
    hex::encode(bytes)
}

// --- Upload ---

#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadResponse {
    id: i32,
    /// Stable, DB-independent identity (see odo-durable-references).
    uuid: String,
    filename: String,
    original_name: String,
    relative_path: String,
    size: i64,
    mime_type: String,
    uploaded_by: i32,
    uploaded_at: String,
    /// Echo of the multipart `category` field ("photo" | "document" |
    /// "video"). The DB doesn't persist this — it's derived from the
    /// destination directory. UI consumers use it to filter rendered
    /// attachment lists by kind.
    category: String,
    entity_type: Option<String>,
    entity_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/upload",
    responses((status = 200, description = "File uploaded")),
    security(("bearer" = [])),
    tag = "asset"
)]
pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadResponse>> {
    let user_id = RequestContext::user_id().ok_or(LocalError::unauthenticated())? as i32;

    let mut file_data: Option<(String, Vec<u8>)> = None;
    let mut category = String::from("document");
    let mut entity_type: Option<String> = None;
    let mut entity_id: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| LocalError::invalid_input(format!("Failed to parse upload: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field
                    .file_name()
                    .ok_or(LocalError::invalid_input("Missing filename"))?
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| LocalError::invalid_input(format!("Failed to read file: {e}")))?;
                file_data = Some((filename, data.to_vec()));
            }
            "category" => {
                category = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "document".to_string());
            }
            "entity_type" => {
                entity_type = Some(field.text().await.unwrap_or_default());
            }
            "entity_id" => {
                entity_id = Some(field.text().await.unwrap_or_default());
            }
            _ => {}
        }
    }

    let (original_name, data) =
        file_data.ok_or(LocalError::invalid_input("Missing file in upload"))?;

    if !matches!(category.as_str(), "photo" | "document" | "video") {
        return Err(LocalError::invalid_input("Invalid file category").into());
    }

    let ext = StdPath::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    if !is_allowed_extension(&category, &ext) {
        return Err(LocalError::invalid_input(format!(
            "File type {ext} not allowed for {category}"
        ))
        .into());
    }

    // One registry fetch serves both routing and the write-perm check.
    let dirs = asset_directory::Entity::find().all(&state.db).await?;
    let target_dir = resolve_target_directory(&dirs, entity_type.as_deref(), &category)?.to_string();

    require_directory_access(&state, &target_dir, Access::Write).await?;

    let file_id = generate_file_id();
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let safe_name = format!(
        "{}_{}_{}{}",
        timestamp,
        &file_id[..8],
        sanitize_filename(&original_name),
        ext
    );

    let full_dir = PathBuf::from(&state.storage_base_path).join(&target_dir);
    fs::create_dir_all(&full_dir).await.map_err(|e| {
        error!(error = %e, path = %full_dir.display(), "Failed to create directory");
        LocalError::internal(format!("Storage error: {e}"))
    })?;

    let file_path = full_dir.join(&safe_name);

    let mut file = fs::File::create(&file_path).await.map_err(|e| {
        error!(error = %e, "Failed to create file");
        LocalError::internal(format!("Storage error: {e}"))
    })?;
    file.write_all(&data).await.map_err(|e| {
        error!(error = %e, "Failed to write file");
        LocalError::internal(format!("Storage error: {e}"))
    })?;
    file.flush().await.ok();

    let mime_type = get_mime_type(&ext).to_string();
    let relative_path = format!("{}/{}", target_dir, safe_name);
    let file_size = data.len() as i32;
    let now = Utc::now();

    let record = file_upload::ActiveModel {
        file_name: Set(original_name.clone()),
        file_type: Set(Some(mime_type.clone())),
        file_size: Set(Some(file_size)),
        storage_path: Set(file_path.to_string_lossy().to_string()),
        relative_path: Set(relative_path.clone()),
        uploaded_by: Set(user_id),
        uploaded_at: Set(now.into()),
        ..Default::default()
    };

    let inserted = file_upload::Entity::insert(record)
        .exec_with_returning(&state.db)
        .await?;

    info!(
        id = inserted.id,
        user_id = user_id,
        relative_path = %relative_path,
        "File uploaded"
    );

    Ok(Json(UploadResponse {
        id: inserted.id,
        uuid: inserted.uuid.to_string(),
        filename: safe_name,
        original_name,
        relative_path,
        size: file_size as i64,
        mime_type,
        uploaded_by: user_id,
        uploaded_at: now.to_rfc3339(),
        category,
        entity_type,
        entity_id,
    }))
}

// --- Retrieval ---

#[derive(Deserialize)]
pub struct FileQueryParams {
    pub token: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/odo/asset/files/{path}",
    params(("path" = String, Path, description = "Relative file path")),
    responses((status = 200, description = "File contents")),
    security(("bearer" = [])),
    tag = "asset"
)]
pub async fn retrieve(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    Query(params): Query<FileQueryParams>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Accept token from Authorization header or ?token= query param
    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or(params.token)
    else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"code": "UNAUTHENTICATED", "message": "Missing authorization"})),
        )
            .into_response();
    };

    let Ok(claims) = state.tokens.verify(&token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"code": "UNAUTHENTICATED", "message": "Invalid token"})),
        )
            .into_response();
    };

    let user_id = claims.user_id().unwrap_or(0);

    // Prevent path traversal
    let clean_path: PathBuf = StdPath::new(&path)
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .collect();

    let full_path = PathBuf::from(&state.storage_base_path).join(&clean_path);

    if !full_path.starts_with(&state.storage_base_path) {
        warn!(user_id = user_id, path = %path, "Path traversal attempt");
        return (StatusCode::BAD_REQUEST, "Invalid file path").into_response();
    }

    // Enforce the read permission for the file's directory. This route runs
    // outside the request_tracing/require_auth layers (it self-validates the
    // token to support `?token=`), so establish a RequestContext from the
    // verified token first — that's what lets `auth_client` forward the
    // caller's JWT and lets `require_directory_access` read the org unit.
    let rel_path = clean_path.to_string_lossy().replace('\\', "/");
    let ctx = RequestContext::generate().with_auth(token.clone(), claims.clone());
    if let Err(e) = ctx
        .scope(require_directory_access(&state, &rel_path, Access::Read))
        .await
    {
        warn!(user_id = user_id, path = %path, "Asset read denied");
        return odo_client::error::ApiError(e).into_response();
    }

    let Ok(metadata) = fs::metadata(&full_path).await else {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    };

    if !metadata.is_file() {
        return (StatusCode::BAD_REQUEST, "Cannot serve directories").into_response();
    }

    let contents = match fs::read(&full_path).await {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Failed to read file");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
        }
    };

    let ext = full_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();
    let content_type = get_mime_type(&ext);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Length", contents.len().to_string());

    if content_type.starts_with("image/") || content_type == "application/pdf" {
        builder = builder.header("Cache-Control", "public, max-age=86400");
    } else {
        builder = builder.header("Cache-Control", "no-cache");
        if let Some(filename) = full_path.file_name().and_then(|n| n.to_str()) {
            builder = builder.header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename),
            );
        }
    }

    match builder.body(Body::from(contents)) {
        Ok(resp) => resp,
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Batch get
// ---------------------------------------------------------------------------
//
// Lets callers (e.g. `current`) decorate UI rows with file metadata
// without reading `asset.file_upload` across schemas themselves.
// Soft-deleted files are filtered out by default so callers don't accidentally
// surface tombstones; pass `include_deleted` to resolve them too (flagged with
// deleted_at) for historical decoration.

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct GetFilesRequest {
    /// Lookup by database id.
    #[serde(default)]
    pub ids: Vec<i32>,
    /// Lookup by stable uuid (durable references). May be mixed with `ids`;
    /// unparseable/unknown uuids are silently dropped like unknown ids.
    #[serde(default)]
    pub uuids: Vec<String>,
    /// Opt in to also return soft-deleted files (flagged with deleted_at).
    /// Default false keeps this active-only, so write-time attachment
    /// validation still rejects deleted ids.
    #[serde(default)]
    pub include_deleted: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileMetadata {
    pub id: i32,
    /// Stable, DB-independent identity (see odo-durable-references).
    pub uuid: String,
    pub file_name: String,
    pub file_type: Option<String>,
    pub file_size: Option<i32>,
    pub storage_path: String,
    pub relative_path: String,
    pub uploaded_by: i32,
    /// Stable uuid of the uploader (durable references).
    pub uploaded_by_uuid: Option<String>,
    pub uploaded_at: String,
    /// RFC3339 soft-delete timestamp; null for active files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GetFilesResponse {
    pub files: Vec<FileMetadata>,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/files/get",
    request_body = GetFilesRequest,
    responses((status = 200, body = GetFilesResponse, description = "File metadata for the requested ids (missing/deleted ids are silently omitted)")),
    security(("bearer" = [])),
    tag = "asset"
)]
pub async fn get_files(
    State(state): State<Arc<AppState>>,
    Json(params): Json<GetFilesRequest>,
) -> ApiResult<Json<GetFilesResponse>> {
    // Authentication is enforced by the route's middleware; no extra
    // permission gate here — file metadata is not sensitive on its own
    // (the storage path can't be turned into a URL without going
    // through `/files/{path}` which re-validates auth).
    let uuids: Vec<sea_orm::prelude::Uuid> = params
        .uuids
        .iter()
        .filter_map(|u| u.parse().ok())
        .collect();
    if params.ids.is_empty() && uuids.is_empty() {
        return Ok(Json(GetFilesResponse { files: Vec::new() }));
    }

    let mut cond = Condition::any();
    if !params.ids.is_empty() {
        cond = cond.add(file_upload::Column::Id.is_in(params.ids));
    }
    if !uuids.is_empty() {
        cond = cond.add(file_upload::Column::Uuid.is_in(uuids));
    }
    let mut query = file_upload::Entity::find().filter(cond);
    if !params.include_deleted {
        query = query.filter(file_upload::Column::DeletedAt.is_null());
    }
    let rows = query.all(&state.db).await?;

    // Resolve uploader uuids in one query (odo-internal join).
    let uploader_ids: Vec<i32> = rows.iter().map(|m| m.uploaded_by).collect();
    let uploader_uuids: std::collections::HashMap<i32, String> =
        odo_entity::auth::usr::Entity::find()
            .filter(odo_entity::auth::usr::Column::Id.is_in(uploader_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|u| (u.id, u.uuid.to_string()))
            .collect();

    let files = rows
        .into_iter()
        .map(|m| FileMetadata {
            id: m.id,
            uuid: m.uuid.to_string(),
            file_name: m.file_name,
            file_type: m.file_type,
            file_size: m.file_size,
            storage_path: m.storage_path,
            relative_path: m.relative_path,
            uploaded_by_uuid: uploader_uuids.get(&m.uploaded_by).cloned(),
            uploaded_by: m.uploaded_by,
            uploaded_at: m.uploaded_at.to_rfc3339(),
            deleted_at: m.deleted_at.map(|d| d.to_rfc3339()),
        })
        .collect();

    Ok(Json(GetFilesResponse { files }))
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------
//
// Soft-deletes the row (sets `deleted_at`) and best-effort removes the
// on-disk file. We do soft-delete rather than hard-delete so callers
// referencing the id (activity logs, audit trails) don't break.
//
// The on-disk delete is best-effort: failures are logged but don't
// fail the request, since the row is already marked deleted. The
// alternative — making the DB write conditional on the file delete —
// would leave the DB and disk out of sync the other way after partial
// failures.

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeleteFileRequest {
    /// Database id of the file; alternatively pass `uuid`.
    #[serde(default)]
    pub id: Option<i32>,
    /// Stable uuid of the file (durable references).
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteFileResponse {
    pub success: bool,
    pub id: i32,
    /// True when the on-disk file was unlinked. False when the file was
    /// missing or couldn't be removed (the DB row is deleted either way).
    pub file_removed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/files/delete",
    request_body = DeleteFileRequest,
    responses((status = 200, body = DeleteFileResponse, description = "Soft-deletes the file_upload row and unlinks the on-disk file")),
    security(("bearer" = [])),
    tag = "asset"
)]
pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeleteFileRequest>,
) -> ApiResult<Json<DeleteFileResponse>> {
    let row = match (params.id, params.uuid.as_deref()) {
        (Some(id), _) => file_upload::Entity::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| LocalError::not_found(format!("file_upload {id}")))?,
        (None, Some(raw)) => {
            let uuid: sea_orm::prelude::Uuid = raw
                .parse()
                .map_err(|_| LocalError::invalid_input("invalid uuid"))?;
            file_upload::Entity::find()
                .filter(file_upload::Column::Uuid.eq(uuid))
                .one(&state.db)
                .await?
                .ok_or_else(|| LocalError::not_found(format!("file_upload {raw}")))?
        }
        (None, None) => {
            return Err(LocalError::invalid_input("id or uuid required").into());
        }
    };

    if row.deleted_at.is_some() {
        return Err(LocalError::invalid_input(format!(
            "file_upload {} is already deleted",
            row.id
        ))
        .into());
    }

    let file_id = row.id;
    let storage_path = row.storage_path.clone();

    let mut active: file_upload::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;

    // Best-effort on-disk removal. Path traversal isn't a concern here
    // because storage_path was written by us at upload time.
    let file_removed = match fs::remove_file(&storage_path).await {
        Ok(()) => true,
        Err(e) => {
            warn!(
                error = %e,
                storage_path = %storage_path,
                id = params.id,
                "soft-delete: row marked deleted but on-disk file removal failed",
            );
            false
        }
    };

    info!(id = file_id, file_removed, "File soft-deleted");

    Ok(Json(DeleteFileResponse {
        success: true,
        id: file_id,
        file_removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- resolve_target_directory (registry-driven upload routing) ---

    fn dir_row(path: &str, entity_type: Option<&str>, category: Option<&str>) -> asset_directory::Model {
        asset_directory::Model {
            path: path.to_string(),
            read_perm: "r".to_string(),
            write_perm: "w".to_string(),
            description: None,
            entity_type: entity_type.map(str::to_string),
            category: category.map(str::to_string),
        }
    }

    fn sample_registry() -> Vec<asset_directory::Model> {
        vec![
            // Pure protection row: no upload routing.
            dir_row("current", None, None),
            dir_row("current/photos", Some("incident"), Some("photo")),
            dir_row("current/videos", Some("incident"), Some("video")),
            dir_row("current/documents", Some("incident"), None),
            dir_row("current/patron/photos", Some("patron"), None),
        ]
    }

    #[test]
    fn target_dir_exact_category_wins() {
        let dirs = sample_registry();
        assert_eq!(
            resolve_target_directory(&dirs, Some("incident"), "photo").unwrap(),
            "current/photos"
        );
        assert_eq!(
            resolve_target_directory(&dirs, Some("incident"), "video").unwrap(),
            "current/videos"
        );
    }

    #[test]
    fn target_dir_catch_all_for_unmapped_category() {
        let dirs = sample_registry();
        assert_eq!(
            resolve_target_directory(&dirs, Some("incident"), "document").unwrap(),
            "current/documents"
        );
        assert_eq!(
            resolve_target_directory(&dirs, Some("patron"), "photo").unwrap(),
            "current/patron/photos"
        );
    }

    #[test]
    fn target_dir_unregistered_entity_is_client_error() {
        let dirs = sample_registry();
        assert!(resolve_target_directory(&dirs, Some("spaceship"), "photo").is_err());
    }

    #[test]
    fn target_dir_missing_entity_is_client_error() {
        let dirs = sample_registry();
        assert!(resolve_target_directory(&dirs, None, "photo").is_err());
        assert!(resolve_target_directory(&dirs, Some(""), "photo").is_err());
    }

    #[test]
    fn target_dir_protection_rows_do_not_route() {
        // Only rows with entity_type participate in routing; an entity
        // with no mapping errors even when protection rows cover paths.
        let dirs = vec![dir_row("current", None, None)];
        assert!(resolve_target_directory(&dirs, Some("incident"), "photo").is_err());
    }

    // --- path_has_prefix (longest-prefix permission matching) ---

    #[test]
    fn prefix_matches_exact_and_segment_boundary() {
        assert!(path_has_prefix("current", "current"));
        assert!(path_has_prefix("current/photos", "current"));
        assert!(path_has_prefix("current/patron/photos", "current"));
        assert!(path_has_prefix("current/patron/photos", "current/patron"));
    }

    #[test]
    fn prefix_rejects_partial_segment() {
        // A `current/patron` row must not match `current/patronage/...`.
        assert!(!path_has_prefix("current/patronage/x", "current/patron"));
        // A `current` row must not match `currentish/...`.
        assert!(!path_has_prefix("currentish/x", "current"));
    }

    #[test]
    fn prefix_longest_wins() {
        // Simulates the resolution: among matching prefixes, the longest
        // (most specific) should be selected.
        let prefixes = ["current", "current/patron"];
        let path = "current/patron/photos/x.jpg";
        let best = prefixes
            .iter()
            .filter(|p| path_has_prefix(path, p))
            .max_by_key(|p| p.len())
            .copied();
        assert_eq!(best, Some("current/patron"));
    }

    // --- is_allowed_extension ---

    #[test]
    fn allowed_photo_extensions() {
        assert!(is_allowed_extension("photo", ".jpg"));
        assert!(is_allowed_extension("photo", ".jpeg"));
        assert!(is_allowed_extension("photo", ".png"));
        assert!(is_allowed_extension("photo", ".gif"));
        assert!(is_allowed_extension("photo", ".webp"));
        assert!(is_allowed_extension("photo", ".heic"));
    }

    #[test]
    fn rejected_photo_extensions() {
        assert!(!is_allowed_extension("photo", ".exe"));
        assert!(!is_allowed_extension("photo", ".pdf"));
        assert!(!is_allowed_extension("photo", ".sh"));
        assert!(!is_allowed_extension("photo", ""));
    }

    #[test]
    fn allowed_document_extensions() {
        assert!(is_allowed_extension("document", ".pdf"));
        assert!(is_allowed_extension("document", ".txt"));
        assert!(is_allowed_extension("document", ".docx"));
    }

    #[test]
    fn rejected_document_extensions() {
        assert!(!is_allowed_extension("document", ".jpg"));
        assert!(!is_allowed_extension("document", ".exe"));
    }

    #[test]
    fn allowed_video_extensions() {
        assert!(is_allowed_extension("video", ".mp4"));
        assert!(is_allowed_extension("video", ".webm"));
    }

    #[test]
    fn unknown_category_rejects_all() {
        assert!(!is_allowed_extension("malware", ".jpg"));
        assert!(!is_allowed_extension("", ".pdf"));
    }

    // --- sanitize_filename ---

    #[test]
    fn sanitize_preserves_safe_chars() {
        assert_eq!(sanitize_filename("my-file_2024.txt"), "my-file_2024");
    }

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_filename("my file (1).txt"), "my_file__1_");
    }

    #[test]
    fn sanitize_strips_path_components() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
    }

    #[test]
    fn sanitize_truncates_long_names() {
        let long = "a".repeat(100);
        let result = sanitize_filename(&format!("{long}.txt"));
        assert_eq!(result.len(), 50);
    }

    #[test]
    fn sanitize_handles_empty() {
        assert_eq!(sanitize_filename(""), "file");
    }

    // --- get_mime_type ---

    #[test]
    fn mime_types() {
        assert_eq!(get_mime_type(".jpg"), "image/jpeg");
        assert_eq!(get_mime_type(".png"), "image/png");
        assert_eq!(get_mime_type(".pdf"), "application/pdf");
        assert_eq!(get_mime_type(".mp4"), "video/mp4");
        assert_eq!(get_mime_type(".xyz"), "application/octet-stream");
    }

    // --- generate_file_id ---

    #[test]
    fn file_id_is_hex_and_unique() {
        let a = generate_file_id();
        let b = generate_file_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32); // 16 bytes = 32 hex chars
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
