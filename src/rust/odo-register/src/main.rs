//! odo-register: apply an app-registration manifest to the odo platform.
//!
//! Apps install their platform data (permissions, roles, grants,
//! notification templates, asset directories, SAML attribute->role maps)
//! and dev/test fixtures (users, role assignments) by describing them in a
//! JSON manifest and running this tool with the `odo-registration` machine
//! account. Installations use the same mechanism for their own org
//! structure. Upsert-only semantics: rows that already exist (409
//! conflicts) count as OK; nothing is ever deleted. Re-running is always
//! safe.
//!
//! Ordering within a manifest is fixed and dependency-correct:
//! org unit types -> org units -> permissions -> roles -> grants ->
//! templates -> directories -> SAML maps -> users -> user role
//! assignments. (Directories reference permission codes; maps and
//! assignments reference roles; assignments reference org units.)
//!
//! SAML maps apply to every active IdP that defines the manifest's
//! `attr_key`; installs without SSO (or without the attribute) skip them
//! with a notice.
//!
//! Org structure is addressed by natural key throughout -- units by
//! `code`, unit types by `label` -- never by database id, so one manifest
//! applies to any install (see docs/tech-docs/durable-references.md). This
//! tool resolves those to ids against the live tree, so a manifest must
//! list a parent before its children.
//!
//! Usage:
//!   odo-register <manifest.json> [more-manifests...]
//!
//! All calls go through the Envoy gateway (every endpoint this tool uses
//! is gateway-routed), so one base URL suffices.
//!
//! Environment:
//!   ODO_URL         gateway base, default http://localhost:30080 (optional)
//!   REGISTRATION_USERNAME
//!                   default odo-registration (optional)
//!   REGISTRATION_PASSWORD
//!                   required. The account ships disabled with an
//!                   unknowable password, so there is no default worth
//!                   falling back to. Normally you do not set this by
//!                   hand: scripts/load-data-manifest.sh generates a password,
//!                   activates the account for the run and disables it
//!                   again afterwards.
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
    org_unit_types: Vec<OrgUnitTypeSpec>,
    #[serde(default)]
    org_units: Vec<OrgUnitSpec>,
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

/// An org unit type, parented by label. Types form their own shallow tree
/// (Root -> Region -> Branch -> ...) independent of the unit tree.
#[derive(Deserialize)]
struct OrgUnitTypeSpec {
    label: String,
    /// Parent type's label. Omit only for a root type.
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    can_have_staff: Option<bool>,
    #[serde(default)]
    can_have_patrons: Option<bool>,
}

/// An org unit, parented by code and typed by label.
#[derive(Deserialize)]
struct OrgUnitSpec {
    code: String,
    label: String,
    /// Parent unit's code. Required: units always attach below an existing
    /// unit, and the single root is seeded rather than registered.
    parent: String,
    /// Unit type's label.
    unit_type: String,
    #[serde(default)]
    timezone: Option<String>,
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

    /// POST that requires a 200; for reads that happen to be POSTs.
    async fn post_ok(&self, url: String, body: &Value) -> Result<Value, String> {
        let display = url.clone();
        match self.post(url, body).await? {
            (200, body) => Ok(body),
            (status, detail) => Err(format!("{display}: {status} {detail}")),
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

/// An org unit as the tree reports it: the uuid is what callers store, the
/// id is what the create API wants for a parent.
struct UnitRef {
    id: i64,
    uuid: String,
}

/// Walk the org tree into a code -> UnitRef map.
fn walk_tree(node: &Value, map: &mut HashMap<String, UnitRef>) {
    if let (Some(code), Some(uuid), Some(id)) = (
        node["code"].as_str(),
        node["uuid"].as_str(),
        node["id"].as_i64(),
    ) {
        map.insert(
            code.to_string(),
            UnitRef {
                id,
                uuid: uuid.to_string(),
            },
        );
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            walk_tree(child, map);
        }
    }
}

/// Fetch the whole org tree as code -> UnitRef.
async fn load_units(client: &Client) -> Result<HashMap<String, UnitRef>, String> {
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
    Ok(units)
}

/// Create org unit types, resolving each parent label to the id of a type
/// that already exists or was created earlier in this run.
async fn apply_unit_types(client: &Client, specs: &[OrgUnitTypeSpec]) -> Result<(), String> {
    // Existing types first: a label already present is "already registered",
    // and its id may be needed as a parent below.
    let page = client
        .post_ok(
            format!("{}/api/v1/odo/org/admin/unit-type/list", client.base),
            &json!({"limit": 200}),
        )
        .await?;
    let mut ids: HashMap<String, i64> = HashMap::new();
    if let Some(rows) = page["rows"].as_array() {
        for row in rows {
            if let (Some(label), Some(id)) = (row["label"].as_str(), row["id"].as_i64()) {
                ids.insert(label.to_string(), id);
            }
        }
    }

    let mut t = Tally::new();
    for spec in specs {
        if ids.contains_key(&spec.label) {
            t.add(Outcome::Exists);
            continue;
        }

        let mut body = json!({"label": spec.label});
        if let Some(parent) = &spec.parent {
            let parent_id = ids.get(parent).ok_or(format!(
                "org unit type '{}': parent type '{parent}' does not exist yet. \
                 List a parent type before the types beneath it.",
                spec.label
            ))?;
            body["parent"] = json!(parent_id);
        }
        if let Some(v) = spec.can_have_staff {
            body["can_have_staff"] = json!(v);
        }
        if let Some(v) = spec.can_have_patrons {
            body["can_have_patrons"] = json!(v);
        }

        let label = format!("org unit type {}", spec.label);
        let url = format!("{}/api/v1/odo/org/admin/unit-type/create", client.base);
        match client.post(url, &body).await? {
            (200, created) => {
                let id = created["id"]
                    .as_i64()
                    .ok_or(format!("{label}: create response had no id"))?;
                ids.insert(spec.label.clone(), id);
                t.add(Outcome::Created);
            }
            (status, detail) => return Err(format!("{label}: {status} {detail}")),
        }
    }
    t.report("org unit types");
    Ok(())
}

/// Create org units, resolving parents by code and types by label. Units
/// created here join the map, so a manifest can build a whole subtree in one
/// pass as long as parents come first.
async fn apply_units(client: &Client, specs: &[OrgUnitSpec]) -> Result<(), String> {
    let mut units = load_units(client).await?;

    let types = client
        .post_ok(
            format!("{}/api/v1/odo/org/admin/unit-type/list", client.base),
            &json!({"limit": 200}),
        )
        .await?;
    let mut type_ids: HashMap<String, i64> = HashMap::new();
    if let Some(rows) = types["rows"].as_array() {
        for row in rows {
            if let (Some(label), Some(id)) = (row["label"].as_str(), row["id"].as_i64()) {
                type_ids.insert(label.to_string(), id);
            }
        }
    }

    let mut t = Tally::new();
    for spec in specs {
        if units.contains_key(&spec.code) {
            t.add(Outcome::Exists);
            continue;
        }

        let parent = units.get(&spec.parent).ok_or(format!(
            "org unit '{}': parent code '{}' is not in the tree. List a parent \
             before its children; the root unit is seeded, not registered.",
            spec.code, spec.parent
        ))?;
        let unit_type = type_ids.get(&spec.unit_type).ok_or(format!(
            "org unit '{}': unknown unit type '{}'. Register it under \
             org_unit_types first.",
            spec.code, spec.unit_type
        ))?;

        let mut body = json!({
            "label": spec.label,
            "code": spec.code,
            "parent": parent.id,
            "unit_type": unit_type,
        });
        if let Some(tz) = &spec.timezone {
            body["timezone"] = json!(tz);
        }

        let label = format!("org unit {}", spec.code);
        let url = format!("{}/api/v1/odo/org/admin/unit/create", client.base);
        match client.post(url, &body).await? {
            (200, created) => {
                let id = created["id"]
                    .as_i64()
                    .ok_or(format!("{label}: create response had no id"))?;
                // The create response carries no uuid; nothing in this run
                // needs one, and the next run reads it from the tree.
                units.insert(
                    spec.code.clone(),
                    UnitRef {
                        id,
                        uuid: String::new(),
                    },
                );
                t.add(Outcome::Created);
            }
            (status, detail) => return Err(format!("{label}: {status} {detail}")),
        }
    }
    t.report("org units");
    Ok(())
}

async fn apply(client: &Client, manifest: &Manifest) -> Result<(), String> {
    // Org structure first: role assignments below resolve org units by code,
    // and an app's fixtures may land in units this manifest creates.
    if !manifest.org_unit_types.is_empty() {
        apply_unit_types(client, &manifest.org_unit_types).await?;
    }
    if !manifest.org_units.is_empty() {
        apply_units(client, &manifest.org_units).await?;
    }

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
        // Re-read the tree rather than reusing anything from apply_units:
        // this picks up units created above, with their generated uuids.
        let units = load_units(client).await?;

        let mut t = Tally::new();
        for a in &manifest.user_role_assignments {
            let org_unit_uuid = &units
                .get(&a.org_unit_code)
                .ok_or(format!(
                    "org unit code '{}' not in the tree",
                    a.org_unit_code
                ))?
                .uuid;
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
    // No default: the seeded account is disabled with a password nobody
    // holds, so falling back to a baked-in value would only turn a
    // configuration mistake into a confusing 401.
    let Ok(password) = std::env::var("REGISTRATION_PASSWORD") else {
        eprintln!("REGISTRATION_PASSWORD is not set.");
        eprintln!(
            "Run this through scripts/load-data-manifest.sh, which activates the \
             odo-registration account for the duration of the run."
        );
        return ExitCode::from(2);
    };

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
