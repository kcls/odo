//! odo-register: apply an app-registration manifest to the odo platform.
//!
//! Apps install their platform data (permissions, roles, grants,
//! notification templates, asset directories, SAML attribute->role maps)
//! and dev/test fixtures (users, role assignments) by describing them in a
//! JSON manifest and running this tool with the `odo-registration` machine
//! account. Installations use the same mechanism for their own org
//! structure. Upsert-only semantics: nothing is ever updated or deleted,
//! and items that already exist are left as they are. Re-running is
//! always safe.
//!
//! Every section reads what is already registered before it writes, so
//! each item is reported as created, unchanged, or present with values
//! other than the manifest's (`~`; registration does not change those).
//! `--plan` stops there: it makes the same reads, prints what a run would
//! do, and writes nothing.
//!
//! Ordering within a manifest is fixed and dependency-correct:
//! org unit types -> org units -> permissions -> roles -> grants ->
//! templates -> directories -> SAML IdPs -> SPs -> attributes -> maps ->
//! users -> user role assignments. (Directories reference permission
//! codes; maps and assignments reference roles; assignments reference org
//! units.) A plan cannot look up what an earlier section has yet to
//! create, so such items are reported as "create, after <dependency>".
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
//!   odo-register [--plan [--detailed-exitcode]] <manifest.json> [more...]
//!
//! Exit status: 0 success, 1 failure, 2 usage. With --plan
//! --detailed-exitcode, a plan that would create anything exits 3 instead
//! of 0, so a caller can skip an apply that would change nothing.
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
use std::collections::{HashMap, HashSet};
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

/// The largest page the admin list endpoints return (odo-service
/// MAX_PAGE_LIMIT); asking for more is silently clamped.
const PAGE_LIMIT: usize = 200;

const UNIT_TYPE_LIST: &str = "/api/v1/odo/org/admin/unit-type/list";
const IDP_LIST: &str = "/api/v1/odo/auth/saml/admin/idp/list";
const ATTRIBUTE_LIST: &str = "/api/v1/odo/auth/saml/admin/attribute/list";

/// `--plan --detailed-exitcode`: the plan would create something. Not 2,
/// which usage errors already return.
const EXIT_PLAN_HAS_CREATES: u8 = 3;

/// Every user status (auth.usr's check constraint). User search filters on
/// exactly one, so finding a username means asking for each.
const USER_STATUSES: [&str; 4] = ["active", "inactive", "suspended", "deleted"];

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
        let status = resp.status().as_u16();
        let body = read_body(resp).await;
        if !(200..300).contains(&status) {
            return Err(http_error(
                &format!("login as {username} failed"),
                status,
                &body,
            ));
        }
        let token = body["access_token"]
            .as_str()
            .ok_or("login response had no access_token")?
            .to_string();
        Ok(Self { http, base, token })
    }

    fn api(&self, path: &str) -> String {
        format!("{}{path}", self.base)
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
        Ok((status, read_body(resp).await))
    }

    /// Create; 409 (already registered) is success. Anything else fails.
    async fn upsert(&self, label: &str, url: String, body: &Value) -> Result<Outcome, String> {
        match self.post(url, body).await? {
            (200, _) => Ok(Outcome::Created),
            (409, _) => Ok(Outcome::Exists),
            (status, detail) => Err(http_error(label, status, &detail)),
        }
    }

    /// POST that requires a 200; for reads that happen to be POSTs.
    async fn post_ok(&self, url: String, body: &Value) -> Result<Value, String> {
        let display = url.clone();
        match self.post(url, body).await? {
            (200, body) => Ok(body),
            (status, detail) => Err(http_error(&display, status, &detail)),
        }
    }

    /// POST read that may legitimately find nothing: 404 is `None`.
    async fn post_found(&self, url: String, body: &Value) -> Result<Option<Value>, String> {
        let display = url.clone();
        match self.post(url, body).await? {
            (200, body) => Ok(Some(body)),
            (404, _) => Ok(None),
            (status, detail) => Err(http_error(&display, status, &detail)),
        }
    }

    /// Every row of a paginated admin list (`{rows, total}`). Reading only
    /// the first page would report the rest as missing.
    async fn list_all(&self, path: &str) -> Result<Vec<Value>, String> {
        let mut rows: Vec<Value> = Vec::new();
        loop {
            let page = self
                .post_ok(
                    self.api(path),
                    &json!({"limit": PAGE_LIMIT, "offset": rows.len()}),
                )
                .await?;
            let batch = page["rows"]
                .as_array()
                .ok_or(format!("{path}: response had no rows"))?;
            if batch.is_empty() {
                break;
            }
            rows.extend(batch.iter().cloned());
            if rows.len() as u64 >= page["total"].as_u64().unwrap_or(0) {
                break;
            }
        }
        Ok(rows)
    }

    /// The array under `key` in an unpaginated list response.
    async fn list_key(&self, path: &str, key: &str) -> Result<Vec<Value>, String> {
        let body = self.post_ok(self.api(path), &json!({})).await?;
        body[key]
            .as_array()
            .cloned()
            .ok_or(format!("{path}: response had no {key}"))
    }

    async fn get(&self, url: String) -> Result<Value, String> {
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("{url}: {e}"))?;
        let status = resp.status().as_u16();
        let body = read_body(resp).await;
        if !(200..300).contains(&status) {
            return Err(http_error(&url, status, &body));
        }
        // A success that is not JSON is still a failure for a JSON API:
        // callers would read every field as missing.
        if let Value::String(_) = body {
            return Err(http_error(&format!("{url}: expected JSON"), status, &body));
        }
        Ok(body)
    }
}

/// Read a response body as JSON, keeping it as text when it is not JSON.
/// Error bodies from the gateway or a proxy are often plain text or HTML,
/// and dropping them would hide the one thing that explains the failure.
async fn read_body(resp: reqwest::Response) -> Value {
    let text = resp.text().await.unwrap_or_default();
    parse_body(&text)
}

fn parse_body(text: &str) -> Value {
    if text.trim().is_empty() {
        return Value::Null;
    }
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.trim().to_string()))
}

/// Upper bound on how much of a response body an error message carries, so
/// an HTML error page does not flood the terminal.
const ERROR_BODY_MAX_CHARS: usize = 1000;

/// "<context>: <status> <body>", with the body as compact JSON or raw text.
fn http_error(context: &str, status: u16, body: &Value) -> String {
    let detail = match body {
        Value::Null => "(empty body)".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let detail = if detail.chars().count() > ERROR_BODY_MAX_CHARS {
        let cut: String = detail.chars().take(ERROR_BODY_MAX_CHARS).collect();
        format!("{cut}... (truncated)")
    } else {
        detail
    };
    format!("{context}: {status} {detail}")
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// ---------------------------------------------------------------------------
// Deciding and reporting
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// Read only: report what an apply would do.
    Plan,
    Apply,
}

/// A field whose registered value is not the manifest's.
#[derive(Debug, PartialEq)]
struct FieldDiff {
    field: String,
    have: Value,
    want: Value,
}

/// What happens to one manifest item.
#[derive(Debug, PartialEq)]
enum Action {
    /// Not registered: created by an apply, or to be created by one. In a
    /// plan, `after` names what this run creates first and the item
    /// depends on, which is why the plan could not look the item up.
    Create {
        after: Option<String>,
    },
    /// The create came back 409 although the read beforehand did not show
    /// the item, e.g. a soft-deleted template still holding its code.
    ExistedUnseen,
    Unchanged,
    /// Registered with other values. Registration never updates a row, so
    /// these stay as they are.
    Differs(Vec<FieldDiff>),
    /// Not applicable on this install; the reason says why.
    Skipped(String),
}

/// Whether a live value matches the manifest's. Strings compare without
/// leading and trailing whitespace: stored templates have turned up without
/// the leading newline their manifest bodies carry, a difference nobody
/// can see in a rendered email and not worth reporting on every run.
fn same_value(want: &Value, have: &Value) -> bool {
    match (want, have) {
        (Value::String(w), Value::String(h)) => w.trim() == h.trim(),
        _ => want == have,
    }
}

/// Fields of `want` (manifest) whose value differs from `have` (live row).
/// Nulls in the manifest mean "not specified", and fields the row does not
/// carry cannot be compared (passwords, for one), so both are passed over.
fn diff_fields(want: &Value, have: &Value) -> Vec<FieldDiff> {
    let (Some(want), Some(have)) = (want.as_object(), have.as_object()) else {
        return Vec::new();
    };
    want.iter()
        .filter(|(_, w)| !w.is_null())
        .filter_map(|(field, w)| {
            let h = have.get(field)?;
            (!same_value(w, h)).then(|| FieldDiff {
                field: field.clone(),
                have: h.clone(),
                want: w.clone(),
            })
        })
        .collect()
}

fn compare(want: &Value, have: &Value) -> Action {
    let diffs = diff_fields(want, have);
    if diffs.is_empty() {
        Action::Unchanged
    } else {
        Action::Differs(diffs)
    }
}

/// An object without its null fields.
fn strip_nulls(v: &Value) -> Value {
    Value::Object(
        v.as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, v)| !v.is_null())
            .collect(),
    )
}

/// Longest value shown in a diff line; template bodies would otherwise
/// fill the screen.
const DIFF_VALUE_MAX_CHARS: usize = 60;

fn show_value(v: &Value) -> String {
    let text = v.to_string();
    if text.chars().count() > DIFF_VALUE_MAX_CHARS {
        let cut: String = text.chars().take(DIFF_VALUE_MAX_CHARS).collect();
        format!("{cut}...")
    } else {
        text
    }
}

/// Join two "after" notes; an item can wait on more than one thing.
fn and_after(a: Option<String>, b: String) -> Option<String> {
    Some(match a {
        Some(a) => format!("{a}, {b}"),
        None => b,
    })
}

#[derive(Default, Debug, PartialEq)]
struct Counts {
    create: usize,
    existed_unseen: usize,
    unchanged: usize,
    differs: usize,
    skipped: usize,
}

impl Counts {
    fn add(&mut self, other: &Counts) {
        self.create += other.create;
        self.existed_unseen += other.existed_unseen;
        self.unchanged += other.unchanged;
        self.differs += other.differs;
        self.skipped += other.skipped;
    }

    fn summary(&self, mode: Mode) -> String {
        let created = match mode {
            Mode::Plan => "to create",
            Mode::Apply => "created",
        };
        let parts: Vec<String> = [
            (self.create, created),
            (self.existed_unseen, "already registered"),
            (self.unchanged, "unchanged"),
            (self.differs, "differ"),
            (self.skipped, "skipped"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// One manifest section's items and what happens to each.
struct Section {
    title: String,
    items: Vec<(String, Action)>,
}

impl Section {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    fn push(&mut self, item: impl Into<String>, action: Action) {
        self.items.push((item.into(), action));
    }

    fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for (_, action) in &self.items {
            match action {
                Action::Create { .. } => c.create += 1,
                Action::ExistedUnseen => c.existed_unseen += 1,
                Action::Unchanged => c.unchanged += 1,
                Action::Differs(_) => c.differs += 1,
                Action::Skipped(_) => c.skipped += 1,
            }
        }
        c
    }

    /// One line per item worth reading: everything but the unchanged,
    /// which the table only counts.
    fn details(&self, mode: Mode) -> Vec<String> {
        let mut lines = Vec::new();
        for (item, action) in &self.items {
            match action {
                Action::Create { after } => match (mode, after) {
                    (Mode::Plan, Some(after)) => {
                        lines.push(format!("+ {item} (after {after})"));
                    }
                    _ => lines.push(format!("+ {item}")),
                },
                Action::ExistedUnseen => lines.push(format!(
                    "= {item}: already registered, though the read beforehand did not show it"
                )),
                Action::Unchanged => {}
                Action::Differs(diffs) => {
                    lines.push(format!("~ {item}"));
                    for d in diffs {
                        lines.push(format!(
                            "    {}: {} -> {}",
                            d.field,
                            show_value(&d.have),
                            show_value(&d.want)
                        ));
                    }
                }
                Action::Skipped(reason) => lines.push(format!("- {item}: {reason}")),
            }
        }
        lines
    }
}

/// The longest fixed section title ("notification templates"). Every table
/// is at least this wide, so the tables of several manifests line up.
const MIN_TITLE_WIDTH: usize = 22;

/// How a count is read out of `Counts`, for one table column.
type Column = (&'static str, fn(&Counts) -> usize);

/// Section totals as an aligned grid, one row per section, so a long run
/// can be scanned down a column. Zero is "-" to keep the eye on the rest.
/// The "existed" column appears only when some section has one, since it
/// takes a create that unexpectedly met an existing row.
fn table(sections: &[Section], mode: Mode) -> Vec<String> {
    if sections.is_empty() {
        return Vec::new();
    }
    let counts: Vec<Counts> = sections.iter().map(Section::counts).collect();
    let created = match mode {
        Mode::Plan => "create",
        Mode::Apply => "created",
    };
    let mut columns: Vec<Column> = vec![
        (created, |c| c.create),
        ("unchanged", |c| c.unchanged),
        ("differ", |c| c.differs),
        ("skipped", |c| c.skipped),
    ];
    if counts.iter().any(|c| c.existed_unseen > 0) {
        columns.insert(1, ("existed", |c| c.existed_unseen));
    }

    let title_width = sections
        .iter()
        .map(|s| s.title.chars().count())
        .chain(std::iter::once(MIN_TITLE_WIDTH))
        .max()
        .unwrap_or_default();
    let widths: Vec<usize> = columns
        .iter()
        .map(|(name, get)| {
            counts
                .iter()
                .map(|c| get(c).to_string().len())
                .chain(std::iter::once(name.len()))
                .max()
                .unwrap_or_default()
        })
        .collect();

    let row = |title: &str, cells: Vec<String>| {
        let mut line = format!("{title:<title_width$}");
        for (cell, width) in cells.iter().zip(&widths) {
            line.push_str(&format!("  {cell:>width$}"));
        }
        line.trim_end().to_string()
    };
    let mut lines = vec![row(
        "section",
        columns.iter().map(|(name, _)| name.to_string()).collect(),
    )];
    for (section, c) in sections.iter().zip(&counts) {
        let cells = columns
            .iter()
            .map(|(_, get)| match get(c) {
                0 => "-".to_string(),
                n => n.to_string(),
            })
            .collect();
        lines.push(row(&section.title, cells));
    }
    lines
}

/// One manifest's report: the table, then each section's details under its
/// title, then any notes.
fn report(sections: &[Section], notes: &[String], mode: Mode) -> Vec<String> {
    let mut lines: Vec<String> = table(sections, mode)
        .into_iter()
        .map(|l| format!("  {l}"))
        .collect();
    for section in sections {
        let details = section.details(mode);
        if details.is_empty() {
            continue;
        }
        lines.push(String::new());
        lines.push(format!("  {}", section.title));
        lines.extend(details.into_iter().map(|l| format!("    {l}")));
    }
    if !notes.is_empty() {
        lines.push(String::new());
        lines.extend(notes.iter().map(|n| format!("  {n}")));
    }
    lines
}

/// What this run creates, by natural key. A plan creates nothing, so later
/// sections (and later manifests in the same run) check here for what
/// they depend on before declaring it missing.
#[derive(Default)]
struct Planned {
    unit_types: HashSet<String>,
    units: HashSet<String>,
    permissions: HashSet<String>,
    roles: HashSet<String>,
    idps: HashSet<String>,
    attr_keys: HashSet<String>,
    users: HashSet<String>,
}

/// An org unit as the tree reports it: the uuid is what callers store, the
/// id is what the create API wants for a parent. The rest is for comparing
/// with the manifest.
struct UnitRef {
    id: i64,
    uuid: String,
    label: Option<String>,
    parent: Option<i64>,
    unit_type: Option<i64>,
    timezone: Option<String>,
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
                label: node["label"].as_str().map(str::to_string),
                parent: node["parent"].as_i64(),
                unit_type: node["unit_type"].as_i64(),
                timezone: node["timezone"].as_str().map(str::to_string),
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
    let tree = client.get(client.api("/api/v1/odo/org/tree")).await?;
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

/// Rows keyed by one of their string fields.
fn index_by(rows: Vec<Value>, key: &str) -> HashMap<String, Value> {
    rows.into_iter()
        .filter_map(|r| Some((r[key].as_str()?.to_string(), r)))
        .collect()
}

/// An item listed again in the same manifest section. Registration would
/// create it once and skip the repeat, so the plan says so rather than
/// counting it twice.
fn repeated() -> Action {
    Action::Skipped("repeats an earlier entry in this manifest".into())
}

/// A SAML map's role and attribute value: with the attribute, its key.
fn map_key(m: &Value) -> (&str, &str) {
    (
        m["role"].as_str().unwrap_or_default(),
        m["attr_value"].as_str().unwrap_or_default(),
    )
}

/// One run over one or more manifests, planning or applying.
struct Run<'a> {
    client: &'a Client,
    mode: Mode,
    planned: Planned,
    totals: Counts,
    /// The current manifest's finished sections and notes, printed as one
    /// report when it is done (or has failed).
    sections: Vec<Section>,
    notes: Vec<String>,
}

impl<'a> Run<'a> {
    fn new(client: &'a Client, mode: Mode) -> Self {
        Self {
            client,
            mode,
            planned: Planned::default(),
            totals: Counts::default(),
            sections: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn finish(&mut self, section: Section) {
        if section.items.is_empty() {
            return;
        }
        self.totals.add(&section.counts());
        self.sections.push(section);
    }

    /// The report for everything finished since the last call.
    fn take_report(&mut self) -> Vec<String> {
        let lines = report(&self.sections, &self.notes, self.mode);
        self.sections.clear();
        self.notes.clear();
        lines
    }

    /// Create a missing item through its create endpoint, or, in a plan,
    /// record that it would be. A 409 here means the item exists after all.
    async fn create(
        &self,
        section: &mut Section,
        item: String,
        after: Option<String>,
        path: &str,
        body: &Value,
    ) -> Result<(), String> {
        if self.mode == Mode::Plan {
            section.push(item, Action::Create { after });
            return Ok(());
        }
        let label = format!("{} {item}", section.title);
        let action = match self
            .client
            .upsert(&label, self.client.api(path), body)
            .await?
        {
            Outcome::Created => Action::Create { after: None },
            Outcome::Exists => Action::ExistedUnseen,
        };
        section.push(item, action);
        Ok(())
    }

    /// The common case: items keyed by one string field, compared field
    /// for field with the row of the same key, created when missing.
    /// Returns the keys of the items that are (or would be) created.
    async fn keyed_section(
        &mut self,
        title: &str,
        specs: &[Value],
        key: &str,
        existing: Vec<Value>,
        create_path: &str,
        body_of: fn(&Value) -> Value,
    ) -> Result<Vec<String>, String> {
        let existing = index_by(existing, key);
        let mut section = Section::new(title);
        let mut created = Vec::new();
        let mut seen = HashSet::new();
        for spec in specs {
            let id = spec[key]
                .as_str()
                .ok_or(format!("{title}: an item has no \"{key}\""))?
                .to_string();
            if !seen.insert(id.clone()) {
                section.push(id, repeated());
                continue;
            }
            let body = body_of(spec);
            if let Some(row) = existing.get(&id) {
                section.push(id, compare(&body, row));
                continue;
            }
            self.create(&mut section, id.clone(), None, create_path, &body)
                .await?;
            created.push(id);
        }
        self.finish(section);
        Ok(created)
    }

    /// Create org unit types, resolving each parent label to the id of a
    /// type that already exists or was created earlier in this run.
    async fn unit_types(&mut self, specs: &[OrgUnitTypeSpec]) -> Result<(), String> {
        let rows = self.client.list_all(UNIT_TYPE_LIST).await?;
        let mut ids: HashMap<String, i64> = rows
            .iter()
            .filter_map(|r| Some((r["label"].as_str()?.to_string(), r["id"].as_i64()?)))
            .collect();
        let by_label = index_by(rows, "label");

        let mut section = Section::new("org unit types");
        for spec in specs {
            if let Some(row) = by_label.get(&spec.label) {
                let want = json!({
                    "parent_label": spec.parent,
                    "can_have_staff": spec.can_have_staff,
                    "can_have_patrons": spec.can_have_patrons,
                });
                section.push(spec.label.clone(), compare(&want, row));
                continue;
            }

            let mut body = json!({"label": spec.label});
            let mut after = None;
            if let Some(parent) = &spec.parent {
                if let Some(id) = ids.get(parent) {
                    body["parent"] = json!(id);
                } else if self.planned.unit_types.contains(parent) {
                    after = Some(format!("org unit type {parent}"));
                } else {
                    return Err(format!(
                        "org unit type '{}': parent type '{parent}' does not exist yet. \
                         List a parent type before the types beneath it.",
                        spec.label
                    ));
                }
            }
            if let Some(v) = spec.can_have_staff {
                body["can_have_staff"] = json!(v);
            }
            if let Some(v) = spec.can_have_patrons {
                body["can_have_patrons"] = json!(v);
            }
            self.planned.unit_types.insert(spec.label.clone());

            if self.mode == Mode::Plan {
                section.push(spec.label.clone(), Action::Create { after });
                continue;
            }
            // Not create(): the new id is needed for any child type below.
            let label = format!("org unit type {}", spec.label);
            let created = self
                .client
                .post_ok(
                    self.client.api("/api/v1/odo/org/admin/unit-type/create"),
                    &body,
                )
                .await
                .map_err(|e| format!("{label}: {e}"))?;
            let id = created["id"]
                .as_i64()
                .ok_or(format!("{label}: create response had no id"))?;
            ids.insert(spec.label.clone(), id);
            section.push(spec.label.clone(), Action::Create { after: None });
        }
        self.finish(section);
        Ok(())
    }

    /// Create org units, resolving parents by code and types by label.
    /// Units created here join the map, so a manifest can build a whole
    /// subtree in one pass as long as parents come first.
    async fn units(&mut self, specs: &[OrgUnitSpec]) -> Result<(), String> {
        let mut units = load_units(self.client).await?;
        let types = self.client.list_all(UNIT_TYPE_LIST).await?;
        let type_ids: HashMap<String, i64> = types
            .iter()
            .filter_map(|r| Some((r["label"].as_str()?.to_string(), r["id"].as_i64()?)))
            .collect();
        let type_labels: HashMap<i64, String> =
            type_ids.iter().map(|(l, id)| (*id, l.clone())).collect();
        let mut codes: HashMap<i64, String> =
            units.iter().map(|(code, u)| (u.id, code.clone())).collect();

        let mut section = Section::new("org units");
        for spec in specs {
            if let Some(unit) = units.get(&spec.code) {
                let have = json!({
                    "label": unit.label,
                    "parent": unit.parent.and_then(|p| codes.get(&p)),
                    "unit_type": unit.unit_type.and_then(|t| type_labels.get(&t)),
                    "timezone": unit.timezone,
                });
                let want = json!({
                    "label": spec.label,
                    "parent": spec.parent,
                    "unit_type": spec.unit_type,
                    "timezone": spec.timezone,
                });
                section.push(spec.code.clone(), compare(&want, &have));
                continue;
            }

            let mut after = None;
            let parent = match units.get(&spec.parent) {
                Some(p) => Some(p.id),
                None if self.planned.units.contains(&spec.parent) => {
                    after = and_after(after, format!("org unit {}", spec.parent));
                    None
                }
                None => {
                    return Err(format!(
                        "org unit '{}': parent code '{}' is not in the tree. List a parent \
                         before its children; the root unit is seeded, not registered.",
                        spec.code, spec.parent
                    ));
                }
            };
            let unit_type = match type_ids.get(&spec.unit_type) {
                Some(t) => Some(*t),
                None if self.planned.unit_types.contains(&spec.unit_type) => {
                    after = and_after(after, format!("org unit type {}", spec.unit_type));
                    None
                }
                None => {
                    return Err(format!(
                        "org unit '{}': unknown unit type '{}'. Register it under \
                         org_unit_types first.",
                        spec.code, spec.unit_type
                    ));
                }
            };
            self.planned.units.insert(spec.code.clone());

            if self.mode == Mode::Plan {
                section.push(spec.code.clone(), Action::Create { after });
                continue;
            }
            let mut body = json!({
                "label": spec.label,
                "code": spec.code,
                "parent": parent,
                "unit_type": unit_type,
            });
            if let Some(tz) = &spec.timezone {
                body["timezone"] = json!(tz);
            }
            let label = format!("org unit {}", spec.code);
            let created = self
                .client
                .post_ok(self.client.api("/api/v1/odo/org/admin/unit/create"), &body)
                .await
                .map_err(|e| format!("{label}: {e}"))?;
            let id = created["id"]
                .as_i64()
                .ok_or(format!("{label}: create response had no id"))?;
            // The create response carries no uuid; nothing in this run
            // needs one, and assignments re-read the tree.
            codes.insert(id, spec.code.clone());
            units.insert(
                spec.code.clone(),
                UnitRef {
                    id,
                    uuid: String::new(),
                    label: Some(spec.label.clone()),
                    parent,
                    unit_type,
                    timezone: spec.timezone.clone(),
                },
            );
            section.push(spec.code.clone(), Action::Create { after: None });
        }
        self.finish(section);
        Ok(())
    }

    async fn grants(&mut self, specs: &[Value]) -> Result<(), String> {
        let perms: HashSet<String> = self
            .client
            .list_all("/api/v1/odo/auth/authz/permission/list")
            .await?
            .iter()
            .filter_map(|r| r["code"].as_str().map(str::to_string))
            .collect();
        // role -> its grants by perm, or None when the role does not exist.
        let mut roles: HashMap<String, Option<HashMap<String, Value>>> = HashMap::new();

        let mut section = Section::new("role grants");
        for g in specs {
            let (Some(role), Some(perm)) = (g["role"].as_str(), g["perm"].as_str()) else {
                return Err(format!("role grant without role and perm: {g}"));
            };
            let item = format!("{role} <- {perm}");

            if !roles.contains_key(role) {
                let found = self
                    .client
                    .post_found(
                        self.client.api("/api/v1/odo/auth/authz/role/get"),
                        &json!({"code": role}),
                    )
                    .await?;
                let grants = found.map(|detail| {
                    index_by(
                        detail["grants"].as_array().cloned().unwrap_or_default(),
                        "perm",
                    )
                });
                roles.insert(role.to_string(), grants);
            }

            let mut after = None;
            match &roles[role] {
                Some(grants) => {
                    if let Some(row) = grants.get(perm) {
                        let want = json!({"min_depth": g["min_depth"]});
                        section.push(item, compare(&want, row));
                        continue;
                    }
                }
                None if self.planned.roles.contains(role) => {
                    after = and_after(after, format!("role {role}"));
                }
                None => {
                    return Err(format!(
                        "role grant {item}: role '{role}' does not exist, and no \
                         manifest in this run creates it"
                    ));
                }
            }
            if !perms.contains(perm) {
                if self.planned.permissions.contains(perm) {
                    after = and_after(after, format!("permission {perm}"));
                } else {
                    return Err(format!(
                        "role grant {item}: permission '{perm}' does not exist, and no \
                         manifest in this run creates it"
                    ));
                }
            }
            self.create(
                &mut section,
                item,
                after,
                "/api/v1/odo/auth/authz/role-permission/create",
                g,
            )
            .await?;
            if let Some(Some(grants)) = roles.get_mut(role) {
                grants.insert(perm.to_string(), g.clone());
            }
        }
        self.finish(section);
        Ok(())
    }

    async fn sps(&mut self, specs: &[SamlSpSpec]) -> Result<(), String> {
        // SPs reference their IdP by row id, which a manifest must not
        // carry. Resolve entity_id -> id against what is deployed, which
        // includes anything this run just created.
        let idps = self.client.list_key(IDP_LIST, "idps").await?;
        let by_entity: HashMap<&str, i64> = idps
            .iter()
            .filter_map(|i| Some((i["entity_id"].as_str()?, i["id"].as_i64()?)))
            .collect();
        // Only active SPs are unique by entity_id, so several rows can
        // share one. The active row is the one that counts.
        let mut existing: HashMap<String, Value> = HashMap::new();
        for row in self
            .client
            .list_key("/api/v1/odo/auth/saml/admin/sp/list", "sps")
            .await?
        {
            let Some(entity) = row["entity_id"].as_str().map(str::to_string) else {
                continue;
            };
            let active = row["is_active"].as_bool().unwrap_or(false);
            if active || !existing.contains_key(&entity) {
                existing.insert(entity, row);
            }
        }

        let mut section = Section::new("saml sps");
        for sp in specs {
            let mut body = json!({
                "entity_id": sp.entity_id,
                "acs_url": sp.acs_url,
                "label": sp.label,
                "slo_url": sp.slo_url,
                "metadata_url": sp.metadata_url,
                "callback_url": sp.callback_url,
                "is_active": sp.is_active,
            });
            let mut after = None;
            if let Some(entity) = &sp.idp_entity_id {
                if let Some(id) = by_entity.get(entity.as_str()) {
                    body["idp"] = json!(id);
                } else if self.planned.idps.contains(entity) {
                    after = Some(format!("saml idp {entity}"));
                } else {
                    section.push(
                        sp.entity_id.clone(),
                        Action::Skipped(format!("no IdP with entity_id '{entity}'")),
                    );
                    continue;
                }
            }
            let body = strip_nulls(&body);

            if let Some(row) = existing.get(&sp.entity_id) {
                section.push(sp.entity_id.clone(), compare(&body, row));
                continue;
            }
            self.create(
                &mut section,
                sp.entity_id.clone(),
                after,
                "/api/v1/odo/auth/saml/admin/sp/create",
                &body,
            )
            .await?;
            existing.insert(sp.entity_id.clone(), body);
        }
        self.finish(section);
        Ok(())
    }

    async fn attributes(&mut self, specs: &[SamlAttrSpec]) -> Result<(), String> {
        // Same entity_id -> id resolution the SPs use. Re-read rather
        // than reuse: this run may have just created the IdP.
        let idps = self.client.list_key(IDP_LIST, "idps").await?;
        let by_entity: HashMap<&str, i64> = idps
            .iter()
            .filter_map(|i| Some((i["entity_id"].as_str()?, i["id"].as_i64()?)))
            .collect();
        // The unique constraint is on (idp, key, normalizer), and a NULL
        // normalizer never equals itself in Postgres -- so an attribute
        // without one can be created over and over and the database will
        // not complain. Existence is decided here, never by a 409.
        let mut existing = self.client.list_key(ATTRIBUTE_LIST, "attributes").await?;

        let mut section = Section::new("saml attributes");
        for attr in specs {
            // With one IdP and no entity named, the attribute is its.
            // Naming it is required as soon as there is more than one,
            // since guessing would silently attach it to the wrong IdP.
            let idp_id: Result<i64, String> = match &attr.idp_entity_id {
                Some(entity) => match by_entity.get(entity.as_str()) {
                    Some(id) => Ok(*id),
                    None if self.planned.idps.contains(entity) => Err(format!("saml idp {entity}")),
                    None => {
                        section.push(
                            attr.key.clone(),
                            Action::Skipped(format!("no IdP with entity_id '{entity}'")),
                        );
                        continue;
                    }
                },
                None if idps.len() == 1 => Ok(idps[0]["id"].as_i64().unwrap_or_default()),
                None if idps.is_empty() && self.planned.idps.len() == 1 => {
                    let entity = self.planned.idps.iter().next().cloned().unwrap_or_default();
                    Err(format!("saml idp {entity}"))
                }
                None => {
                    section.push(
                        attr.key.clone(),
                        Action::Skipped(format!(
                            "{} IdPs configured and no idp_entity_id",
                            idps.len()
                        )),
                    );
                    continue;
                }
            };
            // Err carries the IdP this run creates first, so there is
            // nothing to look the attribute up against yet.
            let idp_id = match idp_id {
                Ok(id) => id,
                Err(after) => {
                    self.planned.attr_keys.insert(attr.key.clone());
                    section.push(attr.key.clone(), Action::Create { after: Some(after) });
                    continue;
                }
            };

            let body = strip_nulls(&json!({
                "idp": idp_id,
                "key": attr.key,
                "label": attr.label,
                "is_location": attr.is_location,
                "normalizer": attr.normalizer,
            }));
            let row = existing.iter().find(|e| {
                e["idp"].as_i64() == Some(idp_id)
                    && e["key"].as_str() == Some(attr.key.as_str())
                    && e["normalizer"].as_str() == attr.normalizer.as_deref()
            });
            if let Some(row) = row {
                section.push(attr.key.clone(), compare(&body, row));
                continue;
            }
            self.create(
                &mut section,
                attr.key.clone(),
                None,
                "/api/v1/odo/auth/saml/admin/attribute/create",
                &body,
            )
            .await?;
            self.planned.attr_keys.insert(attr.key.clone());
            existing.push(body);
        }
        self.finish(section);
        Ok(())
    }

    async fn attr_role_maps(&mut self, saml: &SamlMaps) -> Result<(), String> {
        // Resolve the attribute id per IdP; skip with a notice when the
        // install has no SSO or no IdP has the attribute.
        let attrs = self.client.list_key(ATTRIBUTE_LIST, "attributes").await?;
        let targets: Vec<&Value> = attrs
            .iter()
            .filter(|a| a["key"].as_str() == Some(saml.attr_key.as_str()))
            .collect();

        if targets.is_empty() {
            if self.planned.attr_keys.contains(&saml.attr_key) {
                let mut section = Section::new("saml maps");
                let mut seen = HashSet::new();
                for m in &saml.maps {
                    let (role, value) = map_key(m);
                    let action = if seen.insert((role, value)) {
                        Action::Create {
                            after: Some(format!("saml attribute {}", saml.attr_key)),
                        }
                    } else {
                        repeated()
                    };
                    section.push(format!("{role} <- {value}"), action);
                }
                self.finish(section);
            } else {
                self.notes.push(format!(
                    "saml maps: no IdP defines attribute '{}' - skipped {} maps",
                    saml.attr_key,
                    saml.maps.len()
                ));
            }
            return Ok(());
        }

        // (attr id, role, attr_value) is the unique key.
        let mut existing: HashMap<(i64, String, String), Value> = HashMap::new();
        for row in self
            .client
            .list_all("/api/v1/odo/auth/saml/admin/attr-role-map/list")
            .await?
        {
            if let (Some(attr), Some(role), Some(value)) = (
                row["attr"].as_i64(),
                row["role"].as_str(),
                row["attr_value"].as_str(),
            ) {
                existing.insert((attr, role.to_string(), value.to_string()), row);
            }
        }

        for attr in targets {
            let attr_id = attr["id"].as_i64().unwrap_or_default();
            let mut section = Section::new(format!(
                "saml maps (idp {})",
                attr["idp_name"].as_str().unwrap_or("?")
            ));
            let mut seen = HashSet::new();
            for m in &saml.maps {
                let (role, value) = map_key(m);
                let item = format!("{role} <- {value}");
                let key = (attr_id, role.to_string(), value.to_string());
                if !seen.insert(key.clone()) {
                    section.push(item, repeated());
                    continue;
                }
                if let Some(row) = existing.get(&key) {
                    let want = json!({"is_active": m["is_active"]});
                    section.push(item, compare(&want, row));
                    continue;
                }
                let mut body = m.as_object().cloned().unwrap_or_default();
                body.insert("attr".into(), json!(attr_id));
                let body = Value::Object(body);
                self.create(
                    &mut section,
                    item,
                    None,
                    "/api/v1/odo/auth/saml/admin/attr-role-map/create",
                    &body,
                )
                .await?;
                existing.insert(key, body);
            }
            self.finish(section);
        }
        Ok(())
    }

    /// The registered user a manifest user names, by uuid when it carries
    /// one and by username otherwise. A uuid that is not registered but
    /// whose username is taken finds that other user, which is the case
    /// worth reporting: the create would 409 and leave the manifest's
    /// uuid unregistered.
    async fn find_user(&self, username: &str, uuid: Option<&str>) -> Result<Option<Value>, String> {
        if let Some(uuid) = uuid {
            // with_deleted: a soft-deleted user still holds its uuid, and
            // a create would 409 on it.
            let found = self
                .client
                .post_found(
                    self.client.api("/api/v1/odo/auth/user/get"),
                    &json!({"uuid": uuid, "options": {"with_deleted": true}}),
                )
                .await?;
            if found.is_some() {
                return Ok(found);
            }
        }
        for status in USER_STATUSES {
            let rows = self
                .client
                .post_ok(
                    self.client.api("/api/v1/odo/auth/user/search"),
                    &json!({"keywords": username, "status": status, "limit": 50}),
                )
                .await?;
            if let Some(row) = rows
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["username"].as_str() == Some(username))
            {
                return Ok(Some(row.clone()));
            }
        }
        Ok(None)
    }

    async fn users(&mut self, specs: &[Value]) -> Result<(), String> {
        let mut section = Section::new("users");
        for u in specs {
            let username = u["username"]
                .as_str()
                .ok_or(format!("user without a username: {}", u["email"]))?;
            let uuid = u["uuid"].as_str();
            if let Some(row) = self.find_user(username, uuid).await? {
                // Never compared, and never shown in a diff.
                let mut want = u.clone();
                if let Some(obj) = want.as_object_mut() {
                    obj.remove("password");
                }
                let mut diffs = diff_fields(&want, &row);
                if !row["deleted_at"].is_null() {
                    diffs.push(FieldDiff {
                        field: "deleted_at".into(),
                        have: row["deleted_at"].clone(),
                        want: Value::Null,
                    });
                }
                let action = if diffs.is_empty() {
                    Action::Unchanged
                } else {
                    Action::Differs(diffs)
                };
                section.push(username, action);
                continue;
            }
            self.create(
                &mut section,
                username.to_string(),
                None,
                "/api/v1/odo/auth/user/create",
                u,
            )
            .await?;
            if let Some(uuid) = uuid {
                self.planned.users.insert(uuid.to_string());
            }
        }
        self.finish(section);
        Ok(())
    }

    async fn assignments(&mut self, specs: &[Assignment]) -> Result<(), String> {
        // Re-read the tree: this picks up units created above, with the
        // uuids their create responses did not carry.
        let units = load_units(self.client).await?;
        // usr_uuid -> the user's assignments, or None when there is no
        // such user.
        let mut held: HashMap<String, Option<Vec<Value>>> = HashMap::new();

        let mut section = Section::new("role assignments");
        for a in specs {
            let item = format!("{} {} @ {}", a.usr_uuid, a.role, a.org_unit_code);
            let mut after = None;
            if self.planned.roles.contains(&a.role) {
                after = and_after(after, format!("role {}", a.role));
            }

            let unit = units.get(&a.org_unit_code);
            if unit.is_none() {
                if self.planned.units.contains(&a.org_unit_code) {
                    after = and_after(after, format!("org unit {}", a.org_unit_code));
                } else {
                    return Err(format!(
                        "assignment {item}: org unit code '{}' not in the tree",
                        a.org_unit_code
                    ));
                }
            }

            if !held.contains_key(&a.usr_uuid) {
                let found = self
                    .client
                    .post_found(
                        self.client.api("/api/v1/odo/auth/authz/user-role/list"),
                        &json!({"usr_uuid": a.usr_uuid}),
                    )
                    .await?;
                let rows = found.map(|b| b["assignments"].as_array().cloned().unwrap_or_default());
                held.insert(a.usr_uuid.clone(), rows);
            }
            match (&held[&a.usr_uuid], unit) {
                (Some(rows), Some(unit)) => {
                    let exists = rows.iter().any(|r| {
                        r["role"].as_str() == Some(a.role.as_str())
                            && r["org_unit"].as_i64() == Some(unit.id)
                            && r["is_managed_by_saml"].as_bool() == Some(false)
                    });
                    if exists {
                        section.push(item, Action::Unchanged);
                        continue;
                    }
                }
                (Some(_), None) => {}
                (None, _) if self.planned.users.contains(&a.usr_uuid) => {
                    after = and_after(after, format!("user {}", a.usr_uuid));
                }
                (None, _) => {
                    return Err(format!(
                        "assignment {item}: no user with uuid '{}', and no manifest \
                         in this run creates one",
                        a.usr_uuid
                    ));
                }
            }

            let body = json!({
                "usr_uuid": a.usr_uuid,
                "role": a.role,
                "org_unit_uuid": unit.map(|u| u.uuid.as_str()),
            });
            self.create(
                &mut section,
                item,
                after,
                "/api/v1/odo/auth/authz/user-role/create",
                &body,
            )
            .await?;
            if let (Some(Some(rows)), Some(unit)) = (held.get_mut(&a.usr_uuid), unit) {
                rows.push(json!({
                    "role": a.role,
                    "org_unit": unit.id,
                    "is_managed_by_saml": false,
                }));
            }
        }
        self.finish(section);
        Ok(())
    }

    async fn manifest(&mut self, manifest: &Manifest) -> Result<(), String> {
        // Org structure first: role assignments below resolve org units by
        // code, and an app's fixtures may land in units this manifest
        // creates.
        if !manifest.org_unit_types.is_empty() {
            self.unit_types(&manifest.org_unit_types).await?;
        }
        if !manifest.org_units.is_empty() {
            self.units(&manifest.org_units).await?;
        }

        if !manifest.permissions.is_empty() {
            let existing = self
                .client
                .list_all("/api/v1/odo/auth/authz/permission/list")
                .await?;
            let created = self
                .keyed_section(
                    "permissions",
                    &manifest.permissions,
                    "code",
                    existing,
                    "/api/v1/odo/auth/authz/permission/create",
                    Value::clone,
                )
                .await?;
            self.planned.permissions.extend(created);
        }

        if !manifest.roles.is_empty() {
            let existing = self
                .client
                .list_all("/api/v1/odo/auth/authz/role/list")
                .await?;
            let created = self
                .keyed_section(
                    "roles",
                    &manifest.roles,
                    "code",
                    existing,
                    "/api/v1/odo/auth/authz/role/create",
                    Value::clone,
                )
                .await?;
            self.planned.roles.extend(created);
        }

        if !manifest.role_permissions.is_empty() {
            self.grants(&manifest.role_permissions).await?;
        }

        if !manifest.notification_templates.is_empty() {
            let existing = self
                .client
                .list_all("/api/v1/odo/notify/template/list")
                .await?;
            // Nulls stripped: the create API treats absent and null the
            // same, and a null is "not specified" when comparing too.
            self.keyed_section(
                "notification templates",
                &manifest.notification_templates,
                "code",
                existing,
                "/api/v1/odo/notify/template/create",
                strip_nulls,
            )
            .await?;
        }

        if !manifest.asset_directories.is_empty() {
            // Unpaginated: the endpoint returns a bare array.
            let existing = self
                .client
                .post_ok(
                    self.client.api("/api/v1/odo/asset/directory/list"),
                    &json!({}),
                )
                .await?
                .as_array()
                .cloned()
                .ok_or("asset directory list: response was not an array")?;
            self.keyed_section(
                "asset directories",
                &manifest.asset_directories,
                "path",
                existing,
                "/api/v1/odo/asset/directory/create",
                Value::clone,
            )
            .await?;
        }

        if !manifest.saml_idps.is_empty() {
            let existing = self.client.list_key(IDP_LIST, "idps").await?;
            let created = self
                .keyed_section(
                    "saml idps",
                    &manifest.saml_idps,
                    "entity_id",
                    existing,
                    "/api/v1/odo/auth/saml/admin/idp/create",
                    Value::clone,
                )
                .await?;
            self.planned.idps.extend(created);
        }

        if !manifest.saml_sps.is_empty() {
            self.sps(&manifest.saml_sps).await?;
        }
        if !manifest.saml_idp_attributes.is_empty() {
            self.attributes(&manifest.saml_idp_attributes).await?;
        }
        if let Some(saml) = &manifest.saml_attr_role_maps {
            self.attr_role_maps(saml).await?;
        }
        if !manifest.users.is_empty() {
            self.users(&manifest.users).await?;
        }
        if !manifest.user_role_assignments.is_empty() {
            self.assignments(&manifest.user_role_assignments).await?;
        }
        Ok(())
    }
}

/// What `--version` prints. A release build stamps the git tag in through
/// ODO_REGISTER_BUILD_VERSION so a fetched binary can be checked against
/// the checkout whose manifests it is about to apply; a local cargo build
/// has no tag and reports the crate version instead.
fn build_version() -> &'static str {
    option_env!("ODO_REGISTER_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("odo-register {}", build_version());
        return ExitCode::SUCCESS;
    }
    let mode = if args.iter().any(|a| a == "--plan") {
        Mode::Plan
    } else {
        Mode::Apply
    };
    let detailed_exitcode = args.iter().any(|a| a == "--detailed-exitcode");
    if detailed_exitcode && mode != Mode::Plan {
        eprintln!("--detailed-exitcode only applies to --plan");
        return ExitCode::from(2);
    }
    let paths: Vec<&String> = args
        .iter()
        .filter(|a| *a != "--plan" && *a != "--detailed-exitcode")
        .collect();
    if let Some(unknown) = paths.iter().find(|a| a.starts_with('-')) {
        eprintln!("unknown option: {unknown}");
        return ExitCode::from(2);
    }
    if paths.is_empty() {
        eprintln!(
            "usage: odo-register [--plan [--detailed-exitcode]] <manifest.json> [more-manifests...]"
        );
        eprintln!("       odo-register --version");
        return ExitCode::from(2);
    }

    // Parse everything before logging in, so a broken manifest fails
    // before anything is read or written.
    let mut manifests: Vec<(&String, Manifest)> = Vec::new();
    for path in paths {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("FAILED reading {path}: {e}");
                return ExitCode::FAILURE;
            }
        };
        match serde_json::from_str(&raw) {
            Ok(m) => manifests.push((path, m)),
            Err(e) => {
                eprintln!("FAILED parsing {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
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

    let mut run = Run::new(&client, mode);
    for (path, manifest) in &manifests {
        match mode {
            Mode::Plan => println!("planning {path}"),
            Mode::Apply => println!("applying {path}"),
        }
        // The report comes before any failure, so what was read (or
        // created) up to that point is still on screen.
        let result = run.manifest(manifest).await;
        for line in run.take_report() {
            println!("{line}");
        }
        println!();
        if let Err(e) = result {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    }

    if run.totals.differs > 0 {
        println!(
            "~ marks items registered with values other than the manifest's. \
             Registration never changes existing rows; change them in the admin \
             UI if the manifest is right."
        );
    }
    match mode {
        Mode::Plan => println!(
            "plan complete: {}; nothing was changed",
            run.totals.summary(mode)
        ),
        Mode::Apply => println!("registration complete: {}", run.totals.summary(mode)),
    }
    if detailed_exitcode && run.totals.create > 0 {
        return ExitCode::from(EXIT_PLAN_HAS_CREATES);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_includes_a_json_body() {
        let body =
            parse_body(r#"{"code":"PERMISSION_DENIED","message":"odo.auth.saml.read required"}"#);
        assert_eq!(
            http_error("saml idp list", 403, &body),
            r#"saml idp list: 403 {"code":"PERMISSION_DENIED","message":"odo.auth.saml.read required"}"#
        );
    }

    #[test]
    fn http_error_keeps_a_non_json_body_as_text() {
        let body = parse_body("Jwt is missing\n");
        assert_eq!(body, Value::String("Jwt is missing".into()));
        assert_eq!(http_error("x", 401, &body), "x: 401 Jwt is missing");
    }

    #[test]
    fn http_error_marks_an_empty_body() {
        assert_eq!(parse_body("  "), Value::Null);
        assert_eq!(http_error("x", 502, &Value::Null), "x: 502 (empty body)");
    }

    #[test]
    fn http_error_truncates_a_long_body() {
        let long = "é".repeat(ERROR_BODY_MAX_CHARS + 50);
        let msg = http_error("x", 500, &parse_body(&long));
        assert!(msg.ends_with("... (truncated)"), "{msg}");
        let kept = msg
            .trim_start_matches("x: 500 ")
            .trim_end_matches("... (truncated)");
        assert_eq!(kept.chars().count(), ERROR_BODY_MAX_CHARS);
    }

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
        assert_eq!(
            sp.idp_entity_id.as_deref(),
            Some("https://idp.example.org/")
        );
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

    #[test]
    fn diff_fields_reports_only_specified_comparable_fields() {
        let want = json!({
            "code": "app.read",
            "description": "New text",
            "label": null,
            "password": "secret",
        });
        let have = json!({"code": "app.read", "description": "Old text", "role_count": 2});
        assert_eq!(
            diff_fields(&want, &have),
            vec![FieldDiff {
                field: "description".into(),
                have: json!("Old text"),
                want: json!("New text"),
            }]
        );
    }

    #[test]
    fn edge_whitespace_in_strings_is_not_a_difference() {
        let want = json!({"body": "\n<html>x</html>\n", "n": 1});
        assert!(diff_fields(&want, &json!({"body": "<html>x</html>", "n": 1})).is_empty());
        assert_eq!(
            diff_fields(&want, &json!({"body": "<html>y</html>", "n": 1})).len(),
            1
        );
    }

    #[test]
    fn a_null_live_value_differs_from_a_specified_one() {
        let diffs = diff_fields(&json!({"description": "x"}), &json!({"description": null}));
        assert_eq!(diffs.len(), 1);
    }

    #[test]
    fn compare_distinguishes_unchanged_from_differs() {
        assert_eq!(
            compare(&json!({"a": 1}), &json!({"a": 1, "b": 2})),
            Action::Unchanged
        );
        assert!(matches!(
            compare(&json!({"a": 1}), &json!({"a": 2})),
            Action::Differs(_)
        ));
    }

    #[test]
    fn strip_nulls_drops_only_nulls() {
        assert_eq!(
            strip_nulls(&json!({"a": null, "b": false, "c": ""})),
            json!({"b": false, "c": ""})
        );
    }

    #[test]
    fn long_diff_values_are_cut() {
        let shown = show_value(&json!("x".repeat(500)));
        assert_eq!(shown.chars().count(), DIFF_VALUE_MAX_CHARS + 3);
        assert!(shown.ends_with("..."));
        assert_eq!(show_value(&json!(true)), "true");
    }

    fn sample_section() -> Section {
        let mut s = Section::new("roles");
        s.push("app-new", Action::Create { after: None });
        s.push(
            "app-child",
            Action::Create {
                after: Some("role app-new".into()),
            },
        );
        s.push("app-same", Action::Unchanged);
        s.push(
            "app-old",
            Action::Differs(vec![FieldDiff {
                field: "label".into(),
                have: json!("Old"),
                want: json!("New"),
            }]),
        );
        s
    }

    fn unchanged_section(title: &str, n: usize) -> Section {
        let mut s = Section::new(title);
        for i in 0..n {
            s.push(i.to_string(), Action::Unchanged);
        }
        s
    }

    #[test]
    fn a_plan_report_is_a_table_then_details() {
        let sections = vec![
            unchanged_section("permissions", 24),
            sample_section(),
            unchanged_section("notification templates", 7),
        ];
        assert_eq!(
            report(&sections, &[], Mode::Plan),
            vec![
                "  section                 create  unchanged  differ  skipped",
                "  permissions                  -         24       -        -",
                "  roles                        2          1       1        -",
                "  notification templates       -          7       -        -",
                "",
                "  roles",
                "    + app-new",
                "    + app-child (after role app-new)",
                "    ~ app-old",
                "        label: \"Old\" -> \"New\"",
            ]
        );
    }

    #[test]
    fn an_apply_report_says_created_and_drops_dependencies() {
        let lines = report(&[sample_section()], &[], Mode::Apply);
        assert_eq!(
            lines[0],
            "  section                 created  unchanged  differ  skipped"
        );
        assert!(lines.contains(&"    + app-child".to_string()));
    }

    #[test]
    fn the_existed_column_appears_only_when_needed() {
        let mut s = Section::new("templates");
        s.push("t", Action::ExistedUnseen);
        let lines = table(&[s], Mode::Apply);
        assert_eq!(
            lines[0],
            "section                 created  existed  unchanged  differ  skipped"
        );
        assert_eq!(
            lines[1],
            "templates                     -        1          -       -        -"
        );
        assert!(!table(&[sample_section()], Mode::Plan)[0].contains("existed"));
    }

    #[test]
    fn wide_counts_widen_their_column() {
        let mut s = Section::new("maps");
        for i in 0..1_000_000 {
            s.push(i.to_string(), Action::Create { after: None });
        }
        let lines = table(&[s], Mode::Plan);
        assert_eq!(
            lines[0],
            "section                  create  unchanged  differ  skipped"
        );
        assert_eq!(
            lines[1],
            "maps                    1000000          -       -        -"
        );
    }

    #[test]
    fn notes_follow_the_details() {
        let lines = report(&[], &["saml maps: skipped".into()], Mode::Plan);
        assert_eq!(lines, vec!["", "  saml maps: skipped"]);
    }

    #[test]
    fn nothing_finished_reports_nothing() {
        assert!(report(&[], &[], Mode::Plan).is_empty());
    }

    #[test]
    fn counts_add_up_across_sections() {
        let mut total = Counts::default();
        total.add(&sample_section().counts());
        let mut s = Section::new("users");
        s.push("x", Action::Skipped("no IdP".into()));
        s.push("y", Action::ExistedUnseen);
        total.add(&s.counts());
        assert_eq!(
            total.summary(Mode::Plan),
            "2 to create, 1 already registered, 1 unchanged, 1 differ, 1 skipped"
        );
        assert_eq!(Counts::default().summary(Mode::Apply), "nothing");
    }

    #[test]
    fn and_after_joins_dependencies() {
        assert_eq!(and_after(None, "a".into()).as_deref(), Some("a"));
        assert_eq!(
            and_after(Some("a".into()), "b".into()).as_deref(),
            Some("a, b")
        );
    }

    /// The tree walk keeps what a unit is compared on: its parent and type
    /// ids, which the plan turns back into codes and labels.
    #[test]
    fn walk_tree_records_parent_type_and_timezone() {
        let tree = json!({
            "id": 1, "uuid": "u1", "code": "ROOT", "label": "Root",
            "parent": null, "unit_type": 1, "timezone": null,
            "children": [{
                "id": 2, "uuid": "u2", "code": "MAIN", "label": "Main",
                "parent": 1, "unit_type": 3, "timezone": "America/Los_Angeles",
                "children": []
            }]
        });
        let mut units = HashMap::new();
        walk_tree(&tree, &mut units);
        assert_eq!(units.len(), 2);
        let main = &units["MAIN"];
        assert_eq!(
            (main.id, main.parent, main.unit_type),
            (2, Some(1), Some(3))
        );
        assert_eq!(main.label.as_deref(), Some("Main"));
        assert_eq!(main.timezone.as_deref(), Some("America/Los_Angeles"));
        assert_eq!(units["ROOT"].parent, None);
    }

    #[test]
    fn index_by_skips_rows_without_the_key() {
        let rows = vec![json!({"code": "a"}), json!({"label": "no code"})];
        let idx = index_by(rows, "code");
        assert_eq!(idx.len(), 1);
        assert!(idx.contains_key("a"));
    }
}
