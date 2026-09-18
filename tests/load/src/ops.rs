//! API operations: login/token handling, data discovery, and the weighted
//! op mix the workers draw from.
//!
//! Discovery over hard-coding: setup fetches the org tree, unit types,
//! users, and permission codes from the live system and samples from
//! those, so the harness keeps working as seed data and permission names
//! evolve.

use anyhow::{Context, Result, bail};
use rand::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock};

/// A logged-in user whose token workers share; re-login on 401 is
/// single-flighted so a stampede of expired requests logs in once.
pub struct Identity {
    pub username: String,
    password: String,
    token: RwLock<String>,
    refresh_lock: Mutex<()>,
    pub refreshes: AtomicU64,
}

impl Identity {
    pub async fn login(
        client: &reqwest::Client,
        base: &str,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let token = Self::fetch_token(client, base, username, password).await?;
        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
            token: RwLock::new(token),
            refresh_lock: Mutex::new(()),
            refreshes: AtomicU64::new(0),
        })
    }

    async fn fetch_token(
        client: &reqwest::Client,
        base: &str,
        username: &str,
        password: &str,
    ) -> Result<String> {
        let resp = client
            .post(format!("{base}/api/v1/odo/auth/login"))
            .json(&json!({"username": username, "password": password}))
            .send()
            .await
            .with_context(|| format!("login request for {username}"))?;
        if !resp.status().is_success() {
            bail!("login failed for {username}: HTTP {}", resp.status());
        }
        let body: Value = resp.json().await.context("login response body")?;
        body["access_token"]
            .as_str()
            .map(str::to_string)
            .context("login response missing access_token")
    }

    pub async fn token(&self) -> String {
        self.token.read().await.clone()
    }

    /// Re-login unless another task already replaced the token we failed with.
    pub async fn refresh(&self, client: &reqwest::Client, base: &str, stale: &str) -> Result<()> {
        let _guard = self.refresh_lock.lock().await;
        if *self.token.read().await != stale {
            return Ok(()); // someone else already refreshed
        }
        let fresh = Self::fetch_token(client, base, &self.username, &self.password).await?;
        *self.token.write().await = fresh;
        self.refreshes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// Live data sampled at startup so ops hit real ids instead of constants.
pub struct Discovery {
    pub root_id: i64,
    pub unit_ids: Vec<i64>,
    /// A unit_type id usable for creating churn units under the root.
    pub child_unit_type: Option<i64>,
    pub user_ids: Vec<i64>,
    pub perm_codes: Vec<String>,
    pub search_terms: Vec<String>,
}

fn collect_unit_ids(
    node: &Value,
    ids: &mut Vec<i64>,
    root_child_type: &mut Option<i64>,
    depth: usize,
) {
    if let Some(id) = node["id"].as_i64() {
        ids.push(id);
    }
    if depth == 1 && root_child_type.is_none() {
        *root_child_type = node["unit_type"]["id"].as_i64();
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            collect_unit_ids(child, ids, root_child_type, depth + 1);
        }
    }
}

pub async fn discover(ctx: &Ctx) -> Result<Discovery> {
    // Org tree -> every active unit id + the root + a plausible child type.
    let tree: Value = ctx.get_json("/api/v1/odo/org/tree", &ctx.odo_admin).await?;
    let mut unit_ids = Vec::new();
    let mut child_unit_type = None;
    collect_unit_ids(&tree, &mut unit_ids, &mut child_unit_type, 0);
    let root_id = tree["id"].as_i64().context("org tree root id")?;
    if unit_ids.is_empty() {
        bail!("org tree discovery returned no units");
    }

    // Users -> ids for user/get and realistic search terms.
    let users: Value = ctx
        .post_json(
            "/api/v1/odo/auth/user/search",
            &json!({"keywords": "e2e", "limit": 100}),
            &ctx.odo_admin,
        )
        .await?;
    let user_rows = users.as_array().cloned().unwrap_or_default();
    let user_ids: Vec<i64> = user_rows.iter().filter_map(|u| u["id"].as_i64()).collect();
    let mut search_terms: Vec<String> = user_rows
        .iter()
        .filter_map(|u| u["username"].as_str())
        .filter_map(|n| n.split(['.', '@']).next())
        .map(str::to_string)
        .collect();
    search_terms.sort();
    search_terms.dedup();
    if search_terms.is_empty() {
        search_terms.push("e2e".to_string());
    }

    // Permission codes -> user-has-perm probes use real, current codes.
    let perms: Value = ctx
        .post_json(
            "/api/v1/odo/auth/authz/permission/list",
            &json!({}),
            &ctx.odo_admin,
        )
        .await?;
    let perm_codes: Vec<String> = perms["rows"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|p| p["code"].as_str().map(str::to_string))
        .collect();
    if perm_codes.is_empty() {
        bail!("permission list discovery returned no codes");
    }

    Ok(Discovery {
        root_id,
        unit_ids,
        child_unit_type,
        user_ids,
        perm_codes,
        search_terms,
    })
}

/// Shared execution context: HTTP client, base URL, identities, and the
/// discovered data pool.
pub struct Ctx {
    pub client: reqwest::Client,
    pub base: String,
    /// e2e.odo.admin — holds the odo admin read/write perms.
    pub odo_admin: Arc<Identity>,
    pub uniq: AtomicU64,
    seed: u64,
}

impl Ctx {
    pub async fn new(base: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("building HTTP client")?;
        let odo_admin = Arc::new(Identity::login(&client, &base, "e2e.odo.admin", "test123!").await?);
        // Upload routing is registry data: ensure the harness's own
        // mapping exists (odo.auth.session as the perm — universal).
        let resp = client
            .post(format!("{base}/api/v1/odo/asset/directory/create"))
            .bearer_auth(odo_admin.token().await)
            .json(&json!({
                "path": "load-tests/files",
                "read_perm": "odo.auth.session",
                "write_perm": "odo.auth.session",
                "entity_type": "load-test",
            }))
            .send()
            .await
            .context("registering the load-test upload mapping")?;
        if !(resp.status().is_success() || resp.status() == reqwest::StatusCode::CONFLICT) {
            bail!("load-test upload mapping: {}", resp.status());
        }
        Ok(Self {
            client,
            base,
            odo_admin,
            uniq: AtomicU64::new(0),
            seed: rand::rng().random(),
        })
    }

    pub fn token_refreshes(&self) -> u64 {
        self.odo_admin.refreshes.load(Ordering::Relaxed)
    }

    /// Unique, prefix-identifiable suffix for write-churn artifacts.
    pub fn uniq_suffix(&self) -> String {
        format!(
            "{:x}-{}",
            self.seed,
            self.uniq.fetch_add(1, Ordering::Relaxed)
        )
    }

    async fn send(
        &self,
        req: reqwest::RequestBuilder,
        identity: &Identity,
    ) -> Result<reqwest::Response> {
        let token = identity.token().await;
        let cloned = req.try_clone();
        let resp = req.bearer_auth(&token).send().await?;
        // One retry on 401: refresh the shared token and replay.
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED
            && let Some(retry) = cloned
        {
            identity.refresh(&self.client, &self.base, &token).await?;
            return Ok(retry.bearer_auth(identity.token().await).send().await?);
        }
        Ok(resp)
    }

    async fn json_of(resp: reqwest::Response, what: &str) -> Result<Value> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "{what}: HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        resp.json().await.with_context(|| format!("{what}: body"))
    }

    pub async fn get_json(&self, path: &str, identity: &Identity) -> Result<Value> {
        let resp = self
            .send(self.client.get(format!("{}{}", self.base, path)), identity)
            .await?;
        Self::json_of(resp, path).await
    }

    pub async fn post_json(&self, path: &str, body: &Value, identity: &Identity) -> Result<Value> {
        let resp = self
            .send(
                self.client
                    .post(format!("{}{}", self.base, path))
                    .json(body),
                identity,
            )
            .await?;
        Self::json_of(resp, path).await
    }
}

/// The outcome the metrics layer records for one HTTP call. Transport
/// errors are represented as status 0 rather than an Err so the hot path
/// never bubbles.
pub struct CallResult {
    pub status: u16,
    pub ok: bool,
    pub micros: u64,
}

/// Perform one timed HTTP call, retrying once through a token refresh on
/// 401. Each call is timed individually so churn cycles report honest
/// per-step latencies.
async fn call(
    ctx: &Ctx,
    identity: &Identity,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> CallResult {
    let build = |token: String| {
        let mut req = ctx
            .client
            .request(method.clone(), format!("{}{}", ctx.base, path));
        if let Some(b) = &body {
            req = req.json(b);
        }
        req.bearer_auth(token)
    };

    let t0 = std::time::Instant::now();
    let token = identity.token().await;
    let mut resp = build(token.clone()).send().await;

    if let Ok(r) = &resp
        && r.status() == reqwest::StatusCode::UNAUTHORIZED
        && identity
            .refresh(&ctx.client, &ctx.base, &token)
            .await
            .is_ok()
    {
        resp = build(identity.token().await).send().await;
    }

    match resp {
        Ok(r) => {
            let status = r.status();
            // Drain the body so the timing covers the full response and the
            // connection returns to the pool.
            let _ = r.bytes().await;
            CallResult {
                status: status.as_u16(),
                ok: status.is_success(),
                micros: t0.elapsed().as_micros() as u64,
            }
        }
        Err(_) => CallResult {
            status: 0,
            ok: false,
            micros: t0.elapsed().as_micros() as u64,
        },
    }
}

/// One weighted operation in the mix. `steps` ops record each HTTP call
/// under its own name (create/delete cycles).
pub struct Op {
    pub name: &'static str,
    pub weight: u32,
}

pub const READ_OPS: &[Op] = &[
    Op {
        name: "auth.user_has_perm",
        weight: 12,
    },
    Op {
        name: "auth.user_search",
        weight: 8,
    },
    Op {
        name: "auth.user_get",
        weight: 8,
    },
    Op {
        name: "auth.role_list",
        weight: 5,
    },
    Op {
        name: "auth.permission_list",
        weight: 4,
    },
    Op {
        name: "org.unit_detail",
        weight: 12,
    },
    Op {
        name: "org.descendants",
        weight: 8,
    },
    Op {
        name: "org.ancestors",
        weight: 5,
    },
    Op {
        name: "org.label_batch",
        weight: 8,
    },
    Op {
        name: "org.tree",
        weight: 4,
    },
    Op {
        name: "org.root",
        weight: 4,
    },
    Op {
        name: "notify.inbox_list",
        weight: 8,
    },
    Op {
        name: "notify.email_group_list",
        weight: 5,
    },
    Op {
        name: "notify.template_list",
        weight: 5,
    },
    Op {
        name: "asset.api_doc",
        weight: 3,
    },
];

pub const WRITE_OPS: &[Op] = &[
    Op {
        name: "auth.role_churn",
        weight: 5,
    },
    Op {
        name: "notify.template_churn",
        weight: 5,
    },
    Op {
        name: "org.unit_churn",
        weight: 2,
    },
    Op {
        name: "asset.directory_churn",
        weight: 2,
    },
    Op {
        name: "asset.file_churn",
        weight: 6,
    },
];

/// Execute one op. Multi-step churn ops return one result per HTTP call,
/// each tagged with its own recording name.
pub async fn execute(
    ctx: &Ctx,
    disc: &Discovery,
    op: &'static str,
    rng: &mut SmallRng,
) -> Vec<(&'static str, CallResult)> {
    const GET: reqwest::Method = reqwest::Method::GET;
    const POST: reqwest::Method = reqwest::Method::POST;

    let unit = || {
        *disc
            .unit_ids
            .choose(&mut rand::rng())
            .expect("non-empty unit ids")
    };

    match op {
        // ---------------- reads ----------------
        "auth.user_has_perm" => {
            let perm = disc
                .perm_codes
                .choose(rng)
                .expect("non-empty perms")
                .clone();
            let org_unit = if rng.random_bool(0.5) {
                Some(unit())
            } else {
                None
            };
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/auth/authz/user-has-perm",
                    Some(json!({"perm": perm, "org_unit": org_unit})),
                )
                .await,
            )]
        }
        "auth.user_search" => {
            let term = disc
                .search_terms
                .choose(rng)
                .expect("non-empty terms")
                .clone();
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/auth/user/search",
                    Some(json!({"keywords": term, "limit": 20})),
                )
                .await,
            )]
        }
        "auth.user_get" => {
            let id = disc.user_ids.choose(rng).copied().unwrap_or(1);
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/auth/user/get",
                    Some(json!({"id": id})),
                )
                .await,
            )]
        }
        "auth.role_list" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/auth/authz/role/list",
                    Some(json!({})),
                )
                .await,
            )]
        }
        "auth.permission_list" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/auth/authz/permission/list",
                    Some(json!({})),
                )
                .await,
            )]
        }
        "org.unit_detail" => {
            let path = format!("/api/v1/odo/org/unit/{}", unit());
            vec![(op, call_path(ctx, &ctx.odo_admin, GET, path, None).await)]
        }
        "org.descendants" => {
            let path = format!("/api/v1/odo/org/unit/{}/descendants", unit());
            vec![(op, call_path(ctx, &ctx.odo_admin, GET, path, None).await)]
        }
        "org.ancestors" => {
            let path = format!("/api/v1/odo/org/unit/{}/ancestors", unit());
            vec![(op, call_path(ctx, &ctx.odo_admin, GET, path, None).await)]
        }
        "org.label_batch" => {
            let n = rng.random_range(3..=10.min(disc.unit_ids.len()));
            let ids: Vec<i64> = disc.unit_ids.choose_multiple(rng, n).copied().collect();
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/org/unit/label-batch",
                    Some(json!({"ids": ids})),
                )
                .await,
            )]
        }
        "org.tree" => {
            vec![(
                op,
                call(ctx, &ctx.odo_admin, GET, "/api/v1/odo/org/tree", None).await,
            )]
        }
        "org.root" => {
            vec![(
                op,
                call(ctx, &ctx.odo_admin, GET, "/api/v1/odo/org/root", None).await,
            )]
        }
        "notify.inbox_list" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/notify/inbox/list",
                    Some(json!({"limit": 20, "offset": 0})),
                )
                .await,
            )]
        }
        "notify.email_group_list" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/notify/email-group/list",
                    Some(json!({})),
                )
                .await,
            )]
        }
        "notify.template_list" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    POST,
                    "/api/v1/odo/notify/template/list",
                    Some(json!({})),
                )
                .await,
            )]
        }
        "asset.api_doc" => {
            vec![(
                op,
                call(
                    ctx,
                    &ctx.odo_admin,
                    GET,
                    "/api/v1/odo/asset/api-doc/openapi.json",
                    None,
                )
                .await,
            )]
        }

        // ---------------- writes (churn cycles) ----------------
        "auth.role_churn" => {
            let code = format!("loadtest-role-{}", ctx.uniq_suffix());
            let mut out = Vec::new();
            let created = call(
                ctx,
                &ctx.odo_admin,
                POST,
                "/api/v1/odo/auth/authz/role/create",
                Some(json!({"code": code, "label": "Load test role"})),
            )
            .await;
            let created_ok = created.ok;
            out.push(("auth.role_create", created));
            if created_ok {
                out.push((
                    "auth.role_delete",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/auth/authz/role/delete",
                        Some(json!({"code": code})),
                    )
                    .await,
                ));
            }
            out
        }
        "notify.template_churn" => {
            let code = format!("loadtest-tmpl-{}", ctx.uniq_suffix());
            let mut out = Vec::new();
            let created = json_step(
                ctx,
                &ctx.odo_admin,
                "/api/v1/odo/notify/template/create",
                json!({
                    "code": code,
                    "name": "Load test template",
                    "subject_template": "load test {{n}}",
                    "body_template": "generated by load-tests; safe to delete",
                    "is_active": false,
                }),
            )
            .await;
            let (res, body) = created;
            let id = body["id"].as_i64();
            out.push(("notify.template_create", res));
            if let Some(id) = id {
                out.push((
                    "notify.template_delete",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/notify/template/delete",
                        Some(json!({"id": id})),
                    )
                    .await,
                ));
            }
            out
        }
        "org.unit_churn" => {
            let Some(unit_type) = disc.child_unit_type else {
                return vec![];
            };
            let suffix = ctx.uniq_suffix();
            let mut out = Vec::new();
            let created = json_step(
                ctx,
                &ctx.odo_admin,
                "/api/v1/odo/org/admin/unit/create",
                json!({
                    "label": format!("LoadTest {suffix}"),
                    // Short seed + full counter: truncating the counter (as a
                    // slice of the suffix would) collides under load.
                    "code": format!("LT{suffix}"),
                    "parent": disc.root_id,
                    "unit_type": unit_type,
                }),
            )
            .await;
            let (res, body) = created;
            let id = body["id"].as_i64().or_else(|| body["unit"]["id"].as_i64());
            out.push(("org.unit_create", res));
            if let Some(id) = id {
                out.push((
                    "org.unit_delete",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/org/admin/unit/delete",
                        Some(json!({"id": id})),
                    )
                    .await,
                ));
            }
            out
        }
        "asset.directory_churn" => {
            // Platform-only write churn: register and remove an asset
            // directory (odo-admin holds odo.asset.directory.write). The
            // perm codes just have to exist; odo.auth.session is universal.
            let mut out = Vec::new();
            let path = format!("load-tests/churn-{}", ctx.uniq_suffix());
            let res = call(
                ctx,
                &ctx.odo_admin,
                POST,
                "/api/v1/odo/asset/directory/create",
                Some(json!({
                    "path": path,
                    "read_perm": "odo.auth.session",
                    "write_perm": "odo.auth.session",
                })),
            )
            .await;
            let created = res.ok;
            out.push(("asset.directory_create", res));
            if created {
                out.push((
                    "asset.directory_delete",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/asset/directory/delete",
                        Some(json!({"path": path})),
                    )
                    .await,
                ));
            }
            out
        }
        "asset.file_churn" => {
            let mut out = Vec::new();
            let form = reqwest::multipart::Form::new()
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(LOAD_TEST_FILE.to_vec())
                        .file_name(format!("loadtest-{}.txt", ctx.uniq_suffix()))
                        .mime_str("text/plain")
                        .expect("static mime"),
                )
                .text("category", "document")
                // Routes via the load-test mapping registered at startup.
                .text("entity_type", "load-test");
            let t0 = std::time::Instant::now();
            // multipart bodies aren't replayable; no 401 retry on upload.
            let upload = ctx
                .client
                .post(format!("{}/api/v1/odo/asset/upload", ctx.base))
                .multipart(form)
                .bearer_auth(ctx.odo_admin.token().await)
                .send()
                .await;
            let (res, body) = match upload {
                Ok(resp) => {
                    let status = resp.status();
                    let body: Value = if status.is_success() {
                        resp.json().await.unwrap_or(Value::Null)
                    } else {
                        let _ = resp.bytes().await;
                        Value::Null
                    };
                    (
                        CallResult {
                            status: status.as_u16(),
                            ok: status.is_success(),
                            micros: t0.elapsed().as_micros() as u64,
                        },
                        body,
                    )
                }
                Err(_) => (
                    CallResult {
                        status: 0,
                        ok: false,
                        micros: t0.elapsed().as_micros() as u64,
                    },
                    Value::Null,
                ),
            };
            let id = body["id"].as_i64();
            let rel = body["relative_path"].as_str().map(str::to_string);
            out.push(("asset.upload", res));
            if let Some(id) = id {
                out.push((
                    "asset.files_get",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/asset/files/get",
                        Some(json!({"ids": [id]})),
                    )
                    .await,
                ));
                if let Some(rel) = rel {
                    let path = format!("/api/v1/odo/asset/files/{rel}");
                    out.push((
                        "asset.retrieve",
                        call_path(ctx, &ctx.odo_admin, GET, path, None).await,
                    ));
                }
                out.push((
                    "asset.file_delete",
                    call(
                        ctx,
                        &ctx.odo_admin,
                        POST,
                        "/api/v1/odo/asset/files/delete",
                        Some(json!({"id": id})),
                    )
                    .await,
                ));
            }
            out
        }
        other => unreachable!("unknown op {other}"),
    }
}


/// ~1KB deterministic payload for asset churn.
static LOAD_TEST_FILE: &[u8] = &[b'x'; 1024];

async fn call_path(
    ctx: &Ctx,
    identity: &Identity,
    method: reqwest::Method,
    path: String,
    body: Option<Value>,
) -> CallResult {
    call(ctx, identity, method, &path, body).await
}

/// Timed POST that also returns the parsed body (create steps needing ids).
async fn json_step(ctx: &Ctx, identity: &Identity, path: &str, body: Value) -> (CallResult, Value) {
    let t0 = std::time::Instant::now();
    let token = identity.token().await;
    let resp = ctx
        .client
        .post(format!("{}{}", ctx.base, path))
        .json(&body)
        .bearer_auth(token)
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            let parsed: Value = if status.is_success() {
                r.json().await.unwrap_or(Value::Null)
            } else {
                let _ = r.bytes().await;
                Value::Null
            };
            (
                CallResult {
                    status: status.as_u16(),
                    ok: status.is_success(),
                    micros: t0.elapsed().as_micros() as u64,
                },
                parsed,
            )
        }
        Err(_) => (
            CallResult {
                status: 0,
                ok: false,
                micros: t0.elapsed().as_micros() as u64,
            },
            Value::Null,
        ),
    }
}

/// Best-effort removal of loadtest-* artifacts left behind by crashed
/// --writes runs. Runs before and after a writes-enabled session.
pub async fn sweep_leftovers(ctx: &Ctx) -> Result<usize> {
    let mut removed = 0;

    if let Ok(roles) = ctx
        .post_json(
            "/api/v1/odo/auth/authz/role/list",
            &json!({}),
            &ctx.odo_admin,
        )
        .await
    {
        for role in roles["rows"].as_array().unwrap_or(&Vec::new()) {
            if let Some(code) = role["code"].as_str()
                && code.starts_with("loadtest-role-")
                && ctx
                    .post_json(
                        "/api/v1/odo/auth/authz/role/delete",
                        &json!({"code": code}),
                        &ctx.odo_admin,
                    )
                    .await
                    .is_ok()
            {
                removed += 1;
            }
        }
    }

    if let Ok(templates) = ctx
        .post_json(
            "/api/v1/odo/notify/template/list",
            &json!({}),
            &ctx.odo_admin,
        )
        .await
    {
        for t in templates["rows"].as_array().unwrap_or(&Vec::new()) {
            if let (Some(code), Some(id)) = (t["code"].as_str(), t["id"].as_i64())
                && code.starts_with("loadtest-tmpl-")
                && ctx
                    .post_json(
                        "/api/v1/odo/notify/template/delete",
                        &json!({"id": id}),
                        &ctx.odo_admin,
                    )
                    .await
                    .is_ok()
            {
                removed += 1;
            }
        }
    }

    if let Ok(tree) = ctx.get_json("/api/v1/odo/org/tree", &ctx.odo_admin).await
        && let Some(children) = tree["children"].as_array()
    {
        for child in children {
            if let (Some(label), Some(id)) = (child["label"].as_str(), child["id"].as_i64())
                && label.starts_with("LoadTest ")
                && ctx
                    .post_json(
                        "/api/v1/odo/org/admin/unit/delete",
                        &json!({"id": id}),
                        &ctx.odo_admin,
                    )
                    .await
                    .is_ok()
            {
                removed += 1;
            }
        }
    }

    if let Ok(dirs) = ctx
        .post_json("/api/v1/odo/asset/directory/list", &json!({}), &ctx.odo_admin)
        .await
    {
        // directory/list returns a bare array of rows.
        for d in dirs.as_array().unwrap_or(&Vec::new()) {
            if let Some(path) = d["path"].as_str()
                && path.starts_with("load-tests/churn-")
                && ctx
                    .post_json(
                        "/api/v1/odo/asset/directory/delete",
                        &json!({"path": path}),
                        &ctx.odo_admin,
                    )
                    .await
                    .is_ok()
            {
                removed += 1;
            }
        }
    }

    Ok(removed)
}
