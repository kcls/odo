//! Asset directory registry administration.
//!
//! `asset.directory` maps path prefixes to the permissions guarding them
//! (see `require_directory_access` in handler.rs). Rows are registered by
//! applications (e.g. Current registers `current` and `current/patron`
//! alongside the permissions they reference) or by admins. The registry is
//! deliberately small and flat: `path` is the primary key.

use axum::Json;
use axum::extract::State;
use odo_client::error::{ApiResult, LocalError};
use odo_entity::asset::{directory, file_upload};
use sea_orm::prelude::*;
use sea_orm::{QueryOrder, Set, SqlErr};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::AppState;

const READ_PERM: &str = "odo.asset.directory.read";
const WRITE_PERM: &str = "odo.asset.directory.write";

#[derive(Serialize, ToSchema)]
pub struct DirectoryRow {
    pub path: String,
    pub read_perm: String,
    pub write_perm: String,
    pub description: Option<String>,
    /// Uploads with this entity_type route under `path` (absent = a pure
    /// path-protection row with no upload routing).
    pub entity_type: Option<String>,
    /// Exact upload category this mapping serves; absent = the
    /// entity_type's catch-all.
    pub category: Option<String>,
}

impl From<directory::Model> for DirectoryRow {
    fn from(m: directory::Model) -> Self {
        Self {
            path: m.path,
            read_perm: m.read_perm,
            write_perm: m.write_perm,
            description: m.description,
            entity_type: m.entity_type,
            category: m.category,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ListDirectoriesRequest {}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/directory/list",
    request_body = ListDirectoriesRequest,
    responses((status = 200, body = Vec<DirectoryRow>)),
    security(("bearer" = []))
)]
pub async fn list_directories(
    State(state): State<Arc<AppState>>,
    Json(_params): Json<ListDirectoriesRequest>,
) -> ApiResult<Json<Vec<DirectoryRow>>> {
    state.auth_client.permission_required(READ_PERM, None).await?;

    let rows = directory::Entity::find()
        .order_by_asc(directory::Column::Path)
        .all(&state.db)
        .await?;

    Ok(Json(rows.into_iter().map(DirectoryRow::from).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateDirectoryRequest {
    /// Path prefix (no leading/trailing slash), e.g. `current/patron`.
    path: String,
    /// Permission code required to read files under this prefix.
    read_perm: String,
    /// Permission code required to upload/delete files under this prefix.
    write_perm: String,
    #[serde(default)]
    description: Option<String>,
    /// Route uploads with this entity_type under `path`. Optional: omit
    /// for a pure path-protection row.
    #[serde(default)]
    pub entity_type: Option<String>,
    /// Restrict the mapping to one upload category; omit for the
    /// entity_type's catch-all. Requires entity_type.
    #[serde(default)]
    pub category: Option<String>,
}

/// Path prefixes are plain relative segments: no whitespace, no traversal,
/// no leading/trailing separators.
fn clean_path(path: &str) -> Result<String, LocalError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(LocalError::invalid_input("path may not be empty"));
    }
    if path.len() > 200 {
        return Err(LocalError::invalid_input("path may not exceed 200 characters"));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(LocalError::invalid_input("path may not start or end with '/'"));
    }
    if path.chars().any(char::is_whitespace) {
        return Err(LocalError::invalid_input("path may not contain whitespace"));
    }
    if path.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..") {
        return Err(LocalError::invalid_input("path may not contain empty or relative segments"));
    }
    Ok(path.to_string())
}

fn clean_perm(value: &str, field: &str) -> Result<String, LocalError> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(LocalError::invalid_input(format!(
            "{field} must be a permission code without whitespace"
        )));
    }
    Ok(value.to_string())
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/directory/create",
    request_body = CreateDirectoryRequest,
    responses((status = 200, body = DirectoryRow)),
    security(("bearer" = []))
)]
pub async fn create_directory(
    State(state): State<Arc<AppState>>,
    Json(params): Json<CreateDirectoryRequest>,
) -> ApiResult<Json<DirectoryRow>> {
    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let path = clean_path(&params.path)?;
    let read_perm = clean_perm(&params.read_perm, "read_perm")?;
    let write_perm = clean_perm(&params.write_perm, "write_perm")?;

    let entity_type = params
        .entity_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let category = params
        .category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if category.is_some() && entity_type.is_none() {
        return Err(LocalError::invalid_input("category requires entity_type").into());
    }

    if directory::Entity::find_by_id(&path).one(&state.db).await?.is_some() {
        return Err(LocalError::conflict(
            "DIRECTORY_EXISTS",
            Some("path"),
            "A directory with this path is already registered.",
        )
        .into());
    }

    tracing::info!(path = %path, "CreateAssetDirectory");

    let model = directory::ActiveModel {
        path: Set(path),
        read_perm: Set(read_perm),
        write_perm: Set(write_perm),
        description: Set(params.description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty())),
        entity_type: Set(entity_type),
        category: Set(category),
    };
    // read_perm/write_perm reference authz.permission(code): permissions
    // register before the directories that use them.
    let created = model.insert(&state.db).await.map_err(|e| {
        if matches!(e.sql_err(), Some(SqlErr::ForeignKeyConstraintViolation(_))) {
            LocalError::invalid_input(
                "read_perm and write_perm must reference existing permission codes (register the permissions first)",
            )
        } else if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
            // The path pkey is pre-checked above, so a unique violation
            // here is the (entity_type, category) upload-mapping index.
            LocalError::conflict(
                "UPLOAD_MAPPING_EXISTS",
                Some("entity_type"),
                "Another directory already routes this entity_type/category.",
            )
        } else {
            e.into()
        }
    })?;

    Ok(Json(created.into()))
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteDirectoryRequest {
    path: String,
}

#[derive(Serialize, ToSchema)]
pub struct DeleteDirectoryResponse {
    pub deleted: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/asset/directory/delete",
    request_body = DeleteDirectoryRequest,
    responses((status = 200, body = DeleteDirectoryResponse)),
    security(("bearer" = []))
)]
pub async fn delete_directory(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DeleteDirectoryRequest>,
) -> ApiResult<Json<DeleteDirectoryResponse>> {
    state.auth_client.permission_required(WRITE_PERM, None).await?;

    let existing = directory::Entity::find_by_id(params.path.trim())
        .one(&state.db)
        .await?
        .ok_or_else(|| LocalError::not_found(format!("directory {}", params.path)))?;

    // Refuse while active files live under the prefix: the registry row is
    // what guards access to them.
    let in_use = file_upload::Entity::find()
        .filter(file_upload::Column::DeletedAt.is_null())
        .filter(
            file_upload::Column::RelativePath
                .like(format!("{}/%", existing.path.replace('%', ""))),
        )
        .count(&state.db)
        .await?;
    if in_use > 0 {
        return Err(LocalError::conflict(
            "DIRECTORY_IN_USE",
            Some("path"),
            "Active files exist under this directory; delete them first.",
        )
        .into());
    }

    tracing::info!(path = %existing.path, "DeleteAssetDirectory");
    directory::Entity::delete_by_id(existing.path).exec(&state.db).await?;

    Ok(Json(DeleteDirectoryResponse { deleted: true }))
}
