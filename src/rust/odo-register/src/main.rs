//! odo-register: apply an app-registration manifest to the odo platform.
//!
//! Apps install their platform data (permissions, roles, grants,
//! notification templates, asset directories, SAML attribute->role maps)
//! and dev/test fixtures (users, role assignments) by describing them in a
//! JSON manifest and running this tool with the `odo-registration` machine
//! account. Upsert-only semantics: rows that already exist (409 conflicts)
//! count as OK; nothing is ever deleted. Re-running is always safe.
//!
//! Ordering within a manifest is fixed and dependency-correct:
//! permissions -> roles -> grants -> templates -> directories -> SAML maps
//! -> users -> user role assignments. (Directories reference permission
//! codes; maps and assignments reference roles.)
//!
//! SAML maps apply to every active IdP that defines the manifest's
//! `attr_key`; installs without SSO (or without the attribute) skip them
//! with a notice. User role assignments resolve org units by tree code.
//!
//! Usage:
//!   odo-register <manifest.json> [more-manifests...]
//!
//! All calls go through the Envoy gateway (every endpoint this tool uses
//! is gateway-routed), so one base URL suffices.
//!
//! Environment (all optional):
//!   ODO_URL         gateway base, default http://localhost:30080
//!   REGISTRATION_USERNAME / REGISTRATION_PASSWORD
//!                   default odo-registration / the seed's dev-only value
//!
//! This is the interim client-side mechanism until odo exposes a
//! declarative app-manifest registration endpoint; the manifest format is
//! already shaped for that future API.

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::ExitCode;

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    permissions: Vec<Value>,
    #[serde(default)]
    roles: Vec<Value>,
    #[serde(default)]
    role_permissions: Vec<Value>,
    #[serde(default)]
    notification_templates: Vec<Value>,
    #[serde(default)]
    asset_directories: Vec<Value>,
    #[serde(default)]
    saml_attr_role_maps: Option<SamlMaps>,
    #[serde(default)]
    users: Vec<Value>,
    #[serde(default)]
    user_role_assignments: Vec<Assignment>,
}

#[derive(Deserialize)]
struct SamlMaps {
    attr_key: String,
    maps: Vec<Value>,
}

#[derive(Deserialize)]
struct Assignment {
    usr_uuid: String,
    role: String,
    org_unit_code: String,
}

struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

#[derive(Debug)]
enum Outcome {
    Created,
    Exists,
}

impl Client {
    async fn login(username: &str, password: &str) -> Result<Self, String> {
        let base = env_or("ODO_URL", "http://localhost:30080");
        let http = reqwest::Client::new();
        let resp = http
            .post(format!("{base}/api/v1/odo/auth/login"))
            .json(&json!({"username": username, "password": password}))
            .send()
            .await
            .map_err(|e| format!("login request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("login as {username} failed: {}", resp.status()));
        }
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        let token = body["access_token"]
            .as_str()
            .ok_or("login response had no access_token")?
            .to_string();
        Ok(Self { http, base, token })
    }

    async fn post(&self, url: String, body: &Value) -> Result<(u16, Value), String> {
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("{url}: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp.json().await.unwrap_or(Value::Null);
        Ok((status, body))
    }

    /// Create; 409 (already registered) is success. Anything else fails.
    async fn upsert(&self, label: &str, url: String, body: &Value) -> Result<Outcome, String> {
        match self.post(url, body).await? {
            (200, _) => Ok(Outcome::Created),
            (409, _) => Ok(Outcome::Exists),
            (status, detail) => Err(format!("{label}: {status} {detail}")),
        }
    }

    async fn get(&self, url: String) -> Result<Value, String> {
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("{url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("{url}: {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

struct Tally {
    created: usize,
    exists: usize,
}

impl Tally {
    fn new() -> Self {
        Self {
            created: 0,
            exists: 0,
        }
    }
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Created => self.created += 1,
            Outcome::Exists => self.exists += 1,
        }
    }
    fn report(&self, section: &str) {
        if self.created + self.exists > 0 {
            println!(
                "{section}: {} created, {} already registered",
                self.created, self.exists
            );
        }
    }
}

/// Walk the org tree into a code -> uuid map.
fn walk_tree(node: &Value, map: &mut HashMap<String, String>) {
    if let (Some(code), Some(uuid)) = (node["code"].as_str(), node["uuid"].as_str()) {
        map.insert(code.to_string(), uuid.to_string());
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            walk_tree(child, map);
        }
    }
}

async fn apply(client: &Client, manifest: &Manifest) -> Result<(), String> {
    let mut t = Tally::new();
    for p in &manifest.permissions {
        let label = format!("permission {}", p["code"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/auth/authz/permission/create", client.base),
                    p,
                )
                .await?,
        );
    }
    t.report("permissions");

    let mut t = Tally::new();
    for r in &manifest.roles {
        let label = format!("role {}", r["code"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/auth/authz/role/create", client.base),
                    r,
                )
                .await?,
        );
    }
    t.report("roles");

    let mut t = Tally::new();
    for g in &manifest.role_permissions {
        let label = format!("grant {} <- {}", g["role"], g["perm"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!(
                        "{}/api/v1/odo/auth/authz/role-permission/create",
                        client.base
                    ),
                    g,
                )
                .await?,
        );
    }
    t.report("role grants");

    let mut t = Tally::new();
    for tmpl in &manifest.notification_templates {
        let label = format!("template {}", tmpl["code"]);
        // Strip explicit nulls: the create API treats absent and null the
        // same, but this keeps request logs tidy.
        let body = Value::Object(
            tmpl.as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, v)| !v.is_null())
                .collect(),
        );
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/notify/template/create", client.base),
                    &body,
                )
                .await?,
        );
    }
    t.report("notification templates");

    let mut t = Tally::new();
    for d in &manifest.asset_directories {
        let label = format!("directory {}", d["path"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/asset/directory/create", client.base),
                    d,
                )
                .await?,
        );
    }
    t.report("asset directories");

    if let Some(saml) = &manifest.saml_attr_role_maps {
        // Resolve the attribute id per active IdP; skip with a notice when
        // the install has no SSO or the IdP lacks the attribute.
        let (status, attrs) = client
            .post(
                format!("{}/api/v1/odo/auth/saml/admin/attribute/list", client.base),
                &json!({}),
            )
            .await?;
        if status != 200 {
            return Err(format!("saml attribute list: {status}"));
        }
        let targets: Vec<&Value> = attrs["attributes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|attr| attr["key"].as_str() == Some(saml.attr_key.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        if targets.is_empty() {
            println!(
                "saml maps: no IdP defines attribute '{}' - skipped {} maps",
                saml.attr_key,
                saml.maps.len()
            );
        }
        for attr in targets {
            let mut t = Tally::new();
            for m in &saml.maps {
                let mut body = m.as_object().cloned().unwrap_or_default();
                body.insert("attr".into(), attr["id"].clone());
                let label = format!("saml map {} <- {}", m["role"], m["attr_value"]);
                t.add(
                    client
                        .upsert(
                            &label,
                            format!(
                                "{}/api/v1/odo/auth/saml/admin/attr-role-map/create",
                                client.base
                            ),
                            &Value::Object(body),
                        )
                        .await?,
                );
            }
            t.report(&format!(
                "saml maps (idp {})",
                attr["idp_name"].as_str().unwrap_or("?")
            ));
        }
    }

    let mut t = Tally::new();
    for u in &manifest.users {
        let label = format!("user {}", u["username"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/auth/user/create", client.base),
                    u,
                )
                .await?,
        );
    }
    t.report("users");

    if !manifest.user_role_assignments.is_empty() {
        let tree = client
            .get(format!("{}/api/v1/odo/org/tree", client.base))
            .await?;
        let mut units = HashMap::new();
        // The tree endpoint returns either a bare root node or {tree: [...]}.
        if let Some(roots) = tree["tree"].as_array() {
            for root in roots {
                walk_tree(root, &mut units);
            }
        } else {
            walk_tree(&tree, &mut units);
        }

        let mut t = Tally::new();
        for a in &manifest.user_role_assignments {
            let org_unit_uuid = units.get(&a.org_unit_code).ok_or(format!(
                "org unit code '{}' not in the tree",
                a.org_unit_code
            ))?;
            let label = format!("assignment {} @ {}", a.role, a.org_unit_code);
            t.add(
                client
                    .upsert(
                        &label,
                        format!("{}/api/v1/odo/auth/authz/user-role/create", client.base),
                        &json!({
                            "usr_uuid": a.usr_uuid,
                            "role": a.role,
                            "org_unit_uuid": org_unit_uuid,
                        }),
                    )
                    .await?,
            );
        }
        t.report("role assignments");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: odo-register <manifest.json> [more-manifests...]");
        return ExitCode::from(2);
    }

    let username = env_or("REGISTRATION_USERNAME", "odo-registration");
    let password = env_or("REGISTRATION_PASSWORD", "odo-registration-dev-only");

    let client = match Client::login(&username, &password).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    };

    for path in &paths {
        println!("applying {path}");
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("FAILED reading {path}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let manifest: Manifest = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("FAILED parsing {path}: {e}");
                return ExitCode::FAILURE;
            }
        };
        if let Err(e) = apply(&client, &manifest).await {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    }
    println!("registration complete");
    ExitCode::SUCCESS
}
