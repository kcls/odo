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
    saml_idps: Vec<Value>,
    #[serde(default)]
    saml_sps: Vec<SamlSpSpec>,
    #[serde(default)]
    saml_idp_attributes: Vec<SamlAttrSpec>,
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

/// A SAML attribute an IdP asserts, which role maps then match against.
///
/// `idp_entity_id` names the owning IdP, resolved at apply time so a
/// manifest never carries a database key. Omit it when the install has a
/// single IdP and the attribute belongs to it.
#[derive(Deserialize)]
struct SamlAttrSpec {
    key: String,
    label: String,
    #[serde(default)]
    idp_entity_id: Option<String>,
    #[serde(default)]
    is_location: Option<bool>,
    #[serde(default)]
    normalizer: Option<String>,
}

#[derive(Deserialize)]
struct SamlMaps {
    attr_key: String,
    maps: Vec<Value>,
}

/// A SAML service provider: this installation's own SAML identity.
///
/// No signing material: odo never signs anything, so the SP has no key
/// or certificate of its own (odo:006_drop_sp_signing_material). The
/// IdP's certificate, which incoming assertions are verified against,
/// is `idp_x509_cert` and belongs to the SP row rather than here --
/// odo-auth fetches it from the IdP's metadata.
///
/// `idp_entity_id` names the IdP this SP belongs to. It is resolved to an
/// id at apply time, so a manifest never carries a database key.
#[derive(Deserialize)]
struct SamlSpSpec {
    entity_id: String,
    acs_url: String,
    #[serde(default)]
    idp_entity_id: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    slo_url: Option<String>,
    #[serde(default)]
    metadata_url: Option<String>,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
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

    let mut t = Tally::new();
    for idp in &manifest.saml_idps {
        let label = format!("saml idp {}", idp["name"]);
        t.add(
            client
                .upsert(
                    &label,
                    format!("{}/api/v1/odo/auth/saml/admin/idp/create", client.base),
                    idp,
                )
                .await?,
        );
    }
    t.report("saml idps");

    if !manifest.saml_sps.is_empty() {
        // SPs reference their IdP by row id, which a manifest must not
        // carry. Resolve entity_id -> id against what is deployed, which
        // includes anything the loop above just created.
        let (status, idps) = client
            .post(
                format!("{}/api/v1/odo/auth/saml/admin/idp/list", client.base),
                &json!({}),
            )
            .await?;
        if status != 200 {
            return Err(format!("saml idp list: {status}"));
        }
        let by_entity: HashMap<&str, i64> = idps["idps"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|i| Some((i["entity_id"].as_str()?, i["id"].as_i64()?)))
                    .collect()
            })
            .unwrap_or_default();

        let mut t = Tally::new();
        for sp in &manifest.saml_sps {
            let mut body = json!({
                "entity_id": sp.entity_id,
                "acs_url": sp.acs_url,
            });
            let obj = body.as_object_mut().expect("json object");
            if let Some(v) = &sp.label { obj.insert("label".into(), json!(v)); }
            if let Some(v) = &sp.slo_url { obj.insert("slo_url".into(), json!(v)); }
            if let Some(v) = &sp.metadata_url { obj.insert("metadata_url".into(), json!(v)); }
            if let Some(v) = &sp.callback_url { obj.insert("callback_url".into(), json!(v)); }
            if let Some(v) = sp.is_active { obj.insert("is_active".into(), json!(v)); }

            if let Some(entity) = &sp.idp_entity_id {
                match by_entity.get(entity.as_str()) {
                    Some(id) => { obj.insert("idp".into(), json!(id)); }
                    None => {
                        println!(
                            "saml sp {}: no IdP with entity_id '{}' - skipped",
                            sp.entity_id, entity
                        );
                        continue;
                    }
                }
            }

            let label = format!("saml sp {}", sp.entity_id);
            t.add(
                client
                    .upsert(
                        &label,
                        format!("{}/api/v1/odo/auth/saml/admin/sp/create", client.base),
                        &body,
                    )
                    .await?,
            );
        }
        t.report("saml sps");
    }

    if !manifest.saml_idp_attributes.is_empty() {
        // Same entity_id -> id resolution the SPs use. Re-read rather
        // than reuse: this run may have just created the IdP.
        let (status, idps) = client
            .post(
                format!("{}/api/v1/odo/auth/saml/admin/idp/list", client.base),
                &json!({}),
            )
            .await?;
        if status != 200 {
            return Err(format!("saml idp list: {status}"));
        }
        let rows = idps["idps"].as_array().cloned().unwrap_or_default();
        let by_entity: HashMap<&str, i64> = rows
            .iter()
            .filter_map(|i| Some((i["entity_id"].as_str()?, i["id"].as_i64()?)))
            .collect();

        let mut t = Tally::new();
        for attr in &manifest.saml_idp_attributes {
            // With one IdP and no entity named, the attribute is its.
            // Naming it is required as soon as there is more than one,
            // since guessing would silently attach it to the wrong IdP.
            let idp_id = match &attr.idp_entity_id {
                Some(entity) => match by_entity.get(entity.as_str()) {
                    Some(id) => *id,
                    None => {
                        println!(
                            "saml attribute {}: no IdP with entity_id '{}' - skipped",
                            attr.key, entity
                        );
                        continue;
                    }
                },
                None if rows.len() == 1 => rows[0]["id"].as_i64().unwrap_or_default(),
                None => {
                    println!(
                        "saml attribute {}: {} IdPs configured and no idp_entity_id - skipped",
                        attr.key,
                        rows.len()
                    );
                    continue;
                }
            };

            let mut body = json!({
                "idp": idp_id,
                "key": attr.key,
                "label": attr.label,
            });
            let obj = body.as_object_mut().expect("json object");
            if let Some(v) = attr.is_location { obj.insert("is_location".into(), json!(v)); }
            if let Some(v) = &attr.normalizer { obj.insert("normalizer".into(), json!(v)); }

            // The unique constraint is on (idp, key, normalizer), and a
            // NULL normalizer never equals itself in Postgres -- so an
            // attribute without one can be created over and over and the
            // database will not complain. Check before creating rather
            // than rely on a 409 that will not come.
            let (status, existing) = client
                .post(
                    format!("{}/api/v1/odo/auth/saml/admin/attribute/list", client.base),
                    &json!({}),
                )
                .await?;
            if status != 200 {
                return Err(format!("saml attribute list: {status}"));
            }
            let already = existing["attributes"]
                .as_array()
                .map(|a| {
                    a.iter().any(|e| {
                        e["idp"].as_i64() == Some(idp_id)
                            && e["key"].as_str() == Some(attr.key.as_str())
                            && e["normalizer"].as_str() == attr.normalizer.as_deref()
                    })
                })
                .unwrap_or(false);
            if already {
                t.add(Outcome::Exists);
                continue;
            }

            let label = format!("saml attribute {}", attr.key);
            t.add(
                client
                    .upsert(
                        &label,
                        format!("{}/api/v1/odo/auth/saml/admin/attribute/create", client.base),
                        &body,
                    )
                    .await?,
            );
        }
        t.report("saml attributes");
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The SAML keys must accept a real site manifest, and must carry no
    /// signing material for any to land in.
    #[test]
    fn saml_manifest_parses_without_a_private_key() {
        let m: Manifest = serde_json::from_str(
            r#"{
              "saml_idps": [{
                "name": "Example IdP",
                "entity_id": "https://idp.example.org/",
                "sso_url": "https://idp.example.org/saml2",
                "is_active": true,
                "session_lifetime_hours": 8,
                "allow_idp_initiated": false,
                "attribute_mapping": {}
              }],
              "saml_sps": [{
                "idp_entity_id": "https://idp.example.org/",
                "entity_id": "https://site.example.org",
                "acs_url": "https://site.example.org/api/v1/odo/auth/saml/acs",
                "callback_url": "https://site.example.org/login/callback",
                "is_active": true,
                "x509_cert": "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----"
              }]
            }"#,
        )
        .expect("manifest parses");

        assert_eq!(m.saml_idps.len(), 1);
        assert_eq!(m.saml_sps.len(), 1);
        let sp = &m.saml_sps[0];
        assert_eq!(sp.idp_entity_id.as_deref(), Some("https://idp.example.org/"));
        assert_eq!(sp.entity_id, "https://site.example.org");
    }

    /// The manifest actually shipped for bizapps02 must parse, and must
    /// not contain a private key.
    #[test]
    fn the_shipped_bizapps02_manifest_parses_and_has_no_key() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../odo-deploy/sites/bizapps02.demo.kclseg.org",
            "/data/manifests/025-saml-sso.json"
        );
        let Ok(raw) = std::fs::read_to_string(path) else {
            // odo-deploy is a private sibling checkout; skip where absent.
            return;
        };
        assert!(
            !raw.contains("PRIVATE KEY"),
            "a private key reached a committed manifest"
        );
        let m: Manifest = serde_json::from_str(&raw).expect("shipped manifest parses");
        assert_eq!(m.saml_idps.len(), 1);
        assert_eq!(m.saml_sps.len(), 1);
        assert_eq!(m.saml_sps[0].entity_id, "https://bizapps02.demo.kclseg.org");
    }

    /// Attributes parse, and the IdP may be named or left implicit.
    #[test]
    fn saml_idp_attributes_parse() {
        let m: Manifest = serde_json::from_str(
            r#"{"saml_idp_attributes": [
                {"key": "Title", "label": "Job Title"},
                {"key": "Branch", "label": "Home Branch",
                 "idp_entity_id": "https://idp.example.org/",
                 "is_location": true, "normalizer": "split_slash_last"}
            ]}"#,
        )
        .expect("manifest parses");
        assert_eq!(m.saml_idp_attributes.len(), 2);
        assert_eq!(m.saml_idp_attributes[0].key, "Title");
        assert!(m.saml_idp_attributes[0].idp_entity_id.is_none());
        assert_eq!(m.saml_idp_attributes[1].is_location, Some(true));
        assert_eq!(
            m.saml_idp_attributes[1].normalizer.as_deref(),
            Some("split_slash_last")
        );
    }

    /// Every site manifest that maps roles off an attribute must also
    /// declare it. Without the attribute the maps silently apply to
    /// nothing, which looks like a successful run and leaves SSO users
    /// with no roles.
    #[test]
    fn every_site_declares_the_attribute_its_maps_need() {
        let sites = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../odo-deploy/sites");
        let Ok(entries) = std::fs::read_dir(sites) else {
            return; // odo-deploy is a private sibling checkout; skip when absent.
        };
        let mut checked = 0;
        for site in entries.filter_map(Result::ok) {
            let dir = site.path().join("data/manifests");
            if !dir.is_dir() {
                continue;
            }
            let mut declared: Vec<String> = Vec::new();
            let mut needed: Vec<String> = Vec::new();
            for f in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
                let raw = std::fs::read_to_string(f.path()).unwrap();
                let m: Manifest = serde_json::from_str(&raw)
                    .unwrap_or_else(|e| panic!("{}: {e}", f.path().display()));
                declared.extend(m.saml_idp_attributes.iter().map(|a| a.key.clone()));
                if let Some(maps) = &m.saml_attr_role_maps {
                    needed.push(maps.attr_key.clone());
                }
            }
            for key in &needed {
                assert!(
                    declared.contains(key),
                    "{}: maps roles off attribute '{key}' but no manifest declares it",
                    site.file_name().to_string_lossy()
                );
            }
            checked += 1;
        }
        assert!(checked > 0, "no site manifests found to check");
    }

    /// Signing material in a manifest is a leftover from before
    /// odo:006_drop_sp_signing_material. serde ignores unknown fields, so
    /// an old manifest still loads and the material simply goes nowhere;
    /// this records that rather than leaving it to chance.
    #[test]
    fn stale_signing_material_in_a_manifest_is_ignored() {
        let m: Manifest = serde_json::from_str(
            r#"{"saml_sps": [{
                "entity_id": "https://site.example.org",
                "acs_url": "https://site.example.org/acs",
                "x509_cert": "x",
                "private_key": "-----BEGIN PRIVATE KEY-----"
            }]}"#,
        )
        .expect("manifest parses");
        // Neither field exists on SamlSpSpec any more, so nothing
        // carries them to the API.
        assert_eq!(m.saml_sps.len(), 1);
    }
}
