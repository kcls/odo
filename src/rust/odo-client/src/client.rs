//! HTTP client that propagates request context (request_id, auth token)
//! to downstream odo services.

use crate::context::RequestContext;
use crate::error::{LocalError, LocalResult};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct ServiceClient {
    base_url: String,
    http: reqwest::Client,
}

impl ServiceClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Ensure auth tokens and request_id's are propagated in client calls.
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(id) = RequestContext::current().map(|c| c.request_id)
            && let Ok(val) = HeaderValue::from_str(&id)
        {
            headers.insert("x-request-id", val);
        }

        if let Some(token) = RequestContext::auth_token()
            && let Ok(val) = HeaderValue::from_str(&format!("Bearer {token}"))
        {
            headers.insert(AUTHORIZATION, val);
        }

        headers
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> LocalResult<T> {
        let resp = self
            .http
            .get(self.url(path))
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| LocalError::internal(format!("HTTP request failed: {e}")))?;

        Self::parse_response(resp).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> LocalResult<T> {
        let resp = self
            .http
            .post(self.url(path))
            .headers(self.headers())
            .json(body)
            .send()
            .await
            .map_err(|e| LocalError::internal(format!("HTTP request failed: {e}")))?;

        Self::parse_response(resp).await
    }

    async fn parse_response<T: DeserializeOwned>(resp: reqwest::Response) -> LocalResult<T> {
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => LocalError::unauthenticated(),
                403 => LocalError::permission_denied(body, None::<i32>),
                404 => LocalError::not_found("resource"),
                400 => LocalError::invalid_input(body),
                _ => LocalError::internal(format!("HTTP {status}: {body}")),
            });
        }

        resp.json::<T>()
            .await
            .map_err(|e| LocalError::internal(format!("Failed to parse response: {e}")))
    }
}

/// Specialized client for the odo-org service.
pub struct OrgServiceClient {
    client: ServiceClient,
}

impl From<ServiceClient> for OrgServiceClient {
    fn from(client: ServiceClient) -> OrgServiceClient {
        OrgServiceClient { client }
    }
}

impl OrgServiceClient {
    /// Returns the IDs of an org unit and all its ancestors, root first.
    pub async fn ancestor_ids(&self, org_unit: i32) -> LocalResult<Vec<i32>> {
        self.id_list(&format!("/api/v1/odo/org/unit/{org_unit}/ancestors"))
            .await
    }

    /// Returns the IDs of an org unit and all its descendants.
    pub async fn descendant_ids(&self, org_unit: i32) -> LocalResult<Vec<i32>> {
        self.id_list(&format!("/api/v1/odo/org/unit/{org_unit}/descendants"))
            .await
    }

    async fn id_list(&self, path: &str) -> LocalResult<Vec<i32>> {
        let resp: serde_json::Value = self.client.get(path).await?;
        let ids = resp
            .as_array()
            .ok_or_else(|| LocalError::internal("unexpected id-list response"))?
            .iter()
            .filter_map(|v| v["id"].as_i64().map(|id| id as i32))
            .collect();
        Ok(ids)
    }

    /// Returns the full org unit detail document, including addresses,
    /// operating hours, and future closures. See odo-org's `OrgUnitDetailResponse`.
    pub async fn get_unit_detail(&self, org_unit: i32) -> LocalResult<serde_json::Value> {
        self.client
            .get(&format!("/api/v1/odo/org/unit/{org_unit}"))
            .await
    }

    /// Batch lookup of `(id -> label)` for a set of org-unit ids.
    /// Unknown ids are silently dropped on the server, so the returned
    /// map may be smaller than the input. Empty input short-circuits
    /// without an HTTP call.
    pub async fn fetch_labels(
        &self,
        ids: &[i32],
    ) -> LocalResult<std::collections::HashMap<i32, String>> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        #[derive(serde::Serialize)]
        struct Req<'a> {
            ids: &'a [i32],
        }
        #[derive(serde::Deserialize)]
        struct Entry {
            id: i32,
            label: String,
        }
        #[derive(serde::Deserialize)]
        struct Resp {
            labels: Vec<Entry>,
        }
        let resp: Resp = self
            .client
            .post("/api/v1/odo/org/unit/label-batch", &Req { ids })
            .await?;
        Ok(resp.labels.into_iter().map(|e| (e.id, e.label)).collect())
    }

    /// Returns the ID of the root org unit. Prefer this over hardcoding
    /// `1` — the root ID is not guaranteed.
    pub async fn root_id(&self) -> LocalResult<i32> {
        let resp: serde_json::Value = self.client.get("/api/v1/odo/org/root").await?;
        resp["id"]
            .as_i64()
            .map(|id| id as i32)
            .ok_or_else(|| LocalError::internal("odo-org /root response missing id"))
    }

    // ---- uuid variants (durable references) --------------------------------

    /// Returns the uuid of the root org unit.
    pub async fn root_uuid(&self) -> LocalResult<uuid::Uuid> {
        let resp: serde_json::Value = self.client.get("/api/v1/odo/org/root").await?;
        resp["uuid"]
            .as_str()
            .and_then(|u| u.parse().ok())
            .ok_or_else(|| LocalError::internal("odo-org /root response missing uuid"))
    }

    /// Uuids of an org unit and all its ancestors, root first.
    pub async fn ancestor_uuids(&self, org_unit: &uuid::Uuid) -> LocalResult<Vec<uuid::Uuid>> {
        self.uuid_list(&format!("/api/v1/odo/org/unit/uuid/{org_unit}/ancestors"))
            .await
    }

    /// Uuids of an org unit and all its descendants.
    pub async fn descendant_uuids(&self, org_unit: &uuid::Uuid) -> LocalResult<Vec<uuid::Uuid>> {
        self.uuid_list(&format!("/api/v1/odo/org/unit/uuid/{org_unit}/descendants"))
            .await
    }

    async fn uuid_list(&self, path: &str) -> LocalResult<Vec<uuid::Uuid>> {
        let resp: serde_json::Value = self.client.get(path).await?;
        let uuids = resp
            .as_array()
            .ok_or_else(|| LocalError::internal("unexpected unit-list response"))?
            .iter()
            .filter_map(|v| v["uuid"].as_str().and_then(|u| u.parse().ok()))
            .collect();
        Ok(uuids)
    }

    /// Full org unit detail document, looked up by uuid.
    pub async fn get_unit_detail_by_uuid(
        &self,
        org_unit: &uuid::Uuid,
    ) -> LocalResult<serde_json::Value> {
        self.client
            .get(&format!("/api/v1/odo/org/unit/uuid/{org_unit}"))
            .await
    }

    /// Batch lookup of `(uuid -> label)` for a set of org-unit uuids.
    /// Unknown uuids are silently dropped on the server, so the returned
    /// map may be smaller than the input.
    pub async fn fetch_labels_by_uuid(
        &self,
        uuids: &[uuid::Uuid],
    ) -> LocalResult<std::collections::HashMap<uuid::Uuid, String>> {
        if uuids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        #[derive(serde::Serialize)]
        struct Req<'a> {
            uuids: &'a [uuid::Uuid],
        }
        #[derive(serde::Deserialize)]
        struct Entry {
            uuid: uuid::Uuid,
            label: String,
        }
        #[derive(serde::Deserialize)]
        struct Resp {
            labels: Vec<Entry>,
        }
        let resp: Resp = self
            .client
            .post("/api/v1/odo/org/unit/label-batch", &Req { uuids })
            .await?;
        Ok(resp.labels.into_iter().map(|e| (e.uuid, e.label)).collect())
    }
}

/// Specialized client to handle common auth service requests
pub struct AuthServiceClient {
    client: ServiceClient,
}

impl From<ServiceClient> for AuthServiceClient {
    fn from(client: ServiceClient) -> AuthServiceClient {
        AuthServiceClient { client }
    }
}

impl AuthServiceClient {
    /// Pass-thru POST
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> LocalResult<T> {
        self.client.post(path, body).await
    }

    /// Pass-thru GET
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> LocalResult<T> {
        self.client.get(path).await
    }

    /// Returns true if the user has the provided permission at the provided org unit.
    ///
    /// If no org unit is provided, the permission check occurs against the root org unit.
    pub async fn user_has_permission(
        &self,
        perm: &str,
        org_unit: impl Into<Option<i32>>,
    ) -> LocalResult<bool> {
        let mut params = serde_json::json!({"perm": perm});

        if let Some(org_unit_id) = org_unit.into() {
            params["org_unit"] = org_unit_id.into();
        }

        let perm_result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/user-has-perm", &params)
            .await?;

        Ok(perm_result["has_perm"].as_bool().unwrap_or(false))
    }

    /// Returns Ok if the permission has been granted, permission-denied error otherwise.
    pub async fn permission_required(
        &self,
        perm: &str,
        org_unit: impl Into<Option<i32>>,
    ) -> LocalResult<()> {
        let org_unit: Option<i32> = org_unit.into();
        let has_perm = self.user_has_permission(perm, org_unit).await?;

        if has_perm {
            Ok(())
        } else {
            Err(LocalError::permission_denied(perm, org_unit))
        }
    }

    /// Retrieve a user by id.
    pub async fn get_user(&self, user_id: i32) -> LocalResult<serde_json::Value> {
        self.post(
            "/api/v1/odo/auth/user/get",
            &serde_json::json!({"id": user_id}),
        )
        .await
    }

    /// Returns true if the user has the permission at the org unit given
    /// by uuid (durable references). None checks at the root.
    pub async fn user_has_permission_uuid(
        &self,
        perm: &str,
        org_unit: Option<&uuid::Uuid>,
    ) -> LocalResult<bool> {
        let mut params = serde_json::json!({"perm": perm});
        if let Some(u) = org_unit {
            params["org_unit_uuid"] = u.to_string().into();
        }
        let perm_result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/user-has-perm", &params)
            .await?;
        Ok(perm_result["has_perm"].as_bool().unwrap_or(false))
    }

    /// Ok if the permission is granted at the uuid-referenced org unit,
    /// permission-denied error otherwise.
    pub async fn permission_required_uuid(
        &self,
        perm: &str,
        org_unit: Option<&uuid::Uuid>,
    ) -> LocalResult<()> {
        if self.user_has_permission_uuid(perm, org_unit).await? {
            Ok(())
        } else {
            Err(LocalError::permission_denied(perm, None))
        }
    }

    /// Retrieve a user by stable uuid; `with_deleted` resolves soft-deleted
    /// users too (flagged), for rendering historical references.
    pub async fn get_user_by_uuid(
        &self,
        user_uuid: &uuid::Uuid,
        with_deleted: bool,
    ) -> LocalResult<serde_json::Value> {
        self.post(
            "/api/v1/odo/auth/user/get",
            &serde_json::json!({
                "uuid": user_uuid.to_string(),
                "options": {"with_deleted": with_deleted},
            }),
        )
        .await
    }

    /// Returns true if the current user holds `role` at the given org unit.
    ///
    /// Role grants propagate from the granting unit down to its descendants,
    /// so a grant at an ancestor of `org_unit` satisfies the check.
    ///
    /// `org_unit = None` defers to the root org unit on the server side
    /// (grants at root propagate everywhere).
    pub async fn user_has_role(
        &self,
        role: &str,
        org_unit: impl Into<Option<i32>>,
    ) -> LocalResult<bool> {
        let mut params = serde_json::json!({"role": role});

        if let Some(ou) = org_unit.into() {
            params["org_unit"] = ou.into();
        }

        let result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/user-has-role", &params)
            .await?;

        Ok(result["has_role"].as_bool().unwrap_or(false))
    }

    /// Uuid variant of [`user_has_role`]: org unit referenced by uuid.
    pub async fn user_has_role_uuid(
        &self,
        role: &str,
        org_unit: Option<&uuid::Uuid>,
    ) -> LocalResult<bool> {
        let mut params = serde_json::json!({"role": role});
        if let Some(u) = org_unit {
            params["org_unit_uuid"] = u.to_string().into();
        }
        let result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/user-has-role", &params)
            .await?;
        Ok(result["has_role"].as_bool().unwrap_or(false))
    }

    /// Uuid variant of [`users_with_role`]: users and org referenced by
    /// uuid; returns the holder uuids.
    pub async fn users_with_role_uuids(
        &self,
        role: &str,
        user_uuids: &[uuid::Uuid],
        org_unit: Option<&uuid::Uuid>,
    ) -> LocalResult<Vec<uuid::Uuid>> {
        if user_uuids.is_empty() {
            return Ok(Vec::new());
        }
        let mut params = serde_json::json!({
            "role": role,
            "user_uuids": user_uuids,
        });
        if let Some(u) = org_unit {
            params["org_unit_uuid"] = u.to_string().into();
        }
        let result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/users-with-role", &params)
            .await?;
        Ok(result["user_uuids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Returns the subset of `user_ids` that hold `role`.
    ///
    /// Single round-trip regardless of input size. Same propagation
    /// semantics as [`user_has_role`]: grants at ancestors of the target
    /// `org_unit` satisfy the check; `org_unit = None` defers to root.
    pub async fn users_with_role(
        &self,
        role: &str,
        user_ids: &[i32],
        org_unit: impl Into<Option<i32>>,
    ) -> LocalResult<Vec<i32>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut params = serde_json::json!({
            "role": role,
            "user_ids": user_ids,
        });

        if let Some(ou) = org_unit.into() {
            params["org_unit"] = ou.into();
        }

        let result: serde_json::Value = self
            .client
            .post("/api/v1/odo/auth/authz/users-with-role", &params)
            .await?;

        Ok(result["user_ids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_i64().map(|n| n as i32))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// Specialized client for the odo-asset service.
///
/// Lets services consume asset metadata without reading
/// `asset.file_upload` across schema boundaries themselves.
pub struct AssetServiceClient {
    client: ServiceClient,
}

impl From<ServiceClient> for AssetServiceClient {
    fn from(client: ServiceClient) -> AssetServiceClient {
        AssetServiceClient { client }
    }
}

/// File metadata as returned by odo-asset's `/files/get` endpoint.
/// Matches `odo_asset::handler::FileMetadata` field-for-field; redefined
/// here so callers don't have to depend on the odo-asset crate.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileUploadMetadata {
    pub id: i32,
    /// Stable uuid (durable references).
    pub uuid: uuid::Uuid,
    pub file_name: String,
    pub file_type: Option<String>,
    pub file_size: Option<i32>,
    pub storage_path: String,
    pub relative_path: String,
    pub uploaded_by: i32,
    /// Stable uuid of the uploader (durable references).
    #[serde(default)]
    pub uploaded_by_uuid: Option<uuid::Uuid>,
    pub uploaded_at: String,
}

#[derive(Debug, serde::Deserialize)]
struct GetFilesResponse {
    files: Vec<FileUploadMetadata>,
}

#[derive(Debug, serde::Deserialize)]
struct DeleteFileResponse {
    #[allow(dead_code)]
    success: bool,
    #[allow(dead_code)]
    id: i32,
    file_removed: bool,
}

impl AssetServiceClient {
    /// Batch-look-up file metadata by id. Missing or soft-deleted ids
    /// are silently omitted from the response, so callers should not
    /// assume `out.len() == ids.len()`.
    pub async fn get_files(&self, ids: &[i32]) -> LocalResult<Vec<FileUploadMetadata>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let resp: GetFilesResponse = self
            .client
            .post(
                "/api/v1/odo/asset/files/get",
                &serde_json::json!({"ids": ids}),
            )
            .await?;
        Ok(resp.files)
    }

    /// Soft-deletes a file_upload row and best-effort removes the
    /// on-disk file. Returns whether the on-disk file was actually
    /// removed (the DB row is always marked deleted on success).
    pub async fn delete_file(&self, id: i32) -> LocalResult<bool> {
        let resp: DeleteFileResponse = self
            .client
            .post(
                "/api/v1/odo/asset/files/delete",
                &serde_json::json!({"id": id}),
            )
            .await?;
        Ok(resp.file_removed)
    }

    /// Batch-look-up file metadata by stable uuid. Missing or soft-deleted
    /// uuids are silently omitted from the response.
    pub async fn get_files_by_uuid(
        &self,
        uuids: &[uuid::Uuid],
    ) -> LocalResult<Vec<FileUploadMetadata>> {
        if uuids.is_empty() {
            return Ok(Vec::new());
        }
        let resp: GetFilesResponse = self
            .client
            .post(
                "/api/v1/odo/asset/files/get",
                &serde_json::json!({"uuids": uuids}),
            )
            .await?;
        Ok(resp.files)
    }

    /// Soft-delete a file referenced by stable uuid.
    pub async fn delete_file_by_uuid(&self, uuid: &uuid::Uuid) -> LocalResult<bool> {
        let resp: DeleteFileResponse = self
            .client
            .post(
                "/api/v1/odo/asset/files/delete",
                &serde_json::json!({"uuid": uuid.to_string()}),
            )
            .await?;
        Ok(resp.file_removed)
    }
}
