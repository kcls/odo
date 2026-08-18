use reqwest::Client;
use serde_json::Value;
use std::sync::OnceLock;

pub fn auth_base() -> &'static str {
    static VAL: OnceLock<String> = OnceLock::new();
    VAL.get_or_init(|| {
        std::env::var("ODO_AUTH_URL").unwrap_or_else(|_| "http://localhost:30042".to_string())
    })
}

pub fn org_base() -> &'static str {
    static VAL: OnceLock<String> = OnceLock::new();
    VAL.get_or_init(|| {
        std::env::var("ODO_ORG_URL").unwrap_or_else(|_| "http://localhost:30040".to_string())
    })
}

pub fn notify_base() -> &'static str {
    static VAL: OnceLock<String> = OnceLock::new();
    VAL.get_or_init(|| {
        std::env::var("ODO_NOTIFY_URL").unwrap_or_else(|_| "http://localhost:30044".to_string())
    })
}

pub fn asset_base() -> &'static str {
    static VAL: OnceLock<String> = OnceLock::new();
    VAL.get_or_init(|| {
        std::env::var("ODO_ASSET_URL").unwrap_or_else(|_| "http://localhost:30046".to_string())
    })
}

// Test users are defined by src/test-data (odo platform fixtures). Rows
// carry pinned UUIDs; database ids are resolved at runtime (login JWT sub
// for the caller, get_user-by-uuid for other users) - never hard-coded.
pub struct TestUser {
    pub username: &'static str,
    pub password: &'static str,
    pub email: &'static str,
    pub uuid: &'static str,
}

/// A valid login with no admin permissions (holds only e2e-test-role).
pub const STAFF: TestUser = TestUser {
    username: "e2e.odo.staff",
    password: "test123!",
    email: "e2e.odo.staff@odo.example.org",
    uuid: "e2e00000-0000-4000-a000-000000000001",
};

/// SAML user (no local account; cannot local-login). Useful as a target id.
pub const SSO: TestUser = TestUser {
    username: "e2e.odo.sso",
    password: "",
    email: "e2e.odo.sso@example.com",
    uuid: "e2e00000-0000-4000-a000-000000000002",
};

/// odo-admin @ root.
pub const ODO_ADMIN: TestUser = TestUser {
    username: "e2e.odo.admin",
    password: "test123!",
    email: "e2e.odo.admin@odo.example.org",
    uuid: "e2e00000-0000-4000-a000-000000000003",
};

/// Soft-deleted fixture user (only resolvable with with_deleted).
pub const DELETED_USER_UUID: &str = "e2e00000-0000-4000-a000-000000000004";

/// Mutation guinea pig for user-admin tests (local; nothing logs in as it).
pub const MUTABLE: TestUser = TestUser {
    username: "e2e.odo.mutable",
    password: "",
    email: "e2e.odo.mutable@odo.example.org",
    uuid: "e2e00000-0000-4000-a000-000000000005",
};

/// Shared machine account for background jobs (platform seed).
pub const NOTIFY_SERVICE_USERNAME: &str = "odo-notify-service";
pub const NOTIFY_SERVICE_PASSWORD: &str = "odo-notify-service-dev-only";

/// Machine account apps use to register their seed data (platform seed).
pub const REGISTRATION_USERNAME: &str = "odo-registration";
pub const REGISTRATION_PASSWORD: &str = "odo-registration-dev-only";

pub fn client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("failed to build HTTP client")
}

pub async fn login(client: &Client, user: &TestUser) -> Value {
    let resp = client
        .post(format!("{}/api/v1/odo/auth/login", auth_base()))
        .json(&serde_json::json!({
            "username": user.username,
            "password": user.password,
        }))
        .send()
        .await
        .expect("login request failed");

    assert_eq!(resp.status(), 200, "login failed for {}", user.username);
    resp.json().await.expect("failed to parse login response")
}

pub async fn login_token(client: &Client, user: &TestUser) -> String {
    let data = login(client, user).await;
    data["access_token"]
        .as_str()
        .expect("missing access_token")
        .to_string()
}

static STAFF_TOKEN: OnceLock<String> = OnceLock::new();

pub async fn staff_token(client: &Client) -> String {
    if let Some(t) = STAFF_TOKEN.get() {
        return t.clone();
    }
    let t = login_token(client, &STAFF).await;
    let _ = STAFF_TOKEN.set(t.clone());
    t
}

static ODO_ADMIN_TOKEN: OnceLock<String> = OnceLock::new();

pub async fn odo_admin_token(client: &Client) -> String {
    if let Some(t) = ODO_ADMIN_TOKEN.get() {
        return t.clone();
    }
    let t = login_token(client, &ODO_ADMIN).await;
    let _ = ODO_ADMIN_TOKEN.set(t.clone());
    t
}

pub fn auth_header(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

// ---------------------------------------------------------------------------
// Dynamic id resolution (no hard-coded database ids in tests)
// ---------------------------------------------------------------------------

/// The caller's user id, from the JWT `sub` claim (no extra requests).
pub fn token_user_id(token: &str) -> i64 {
    use base64::Engine;
    let payload = token.split('.').nth(1).expect("JWT has a payload segment");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("JWT payload decodes");
    let claims: Value = serde_json::from_slice(&bytes).expect("JWT payload is JSON");
    claims["sub"]
        .as_str()
        .expect("sub claim present")
        .parse()
        .expect("sub is a user id")
}

static STAFF_ID: OnceLock<i64> = OnceLock::new();

/// e2e.odo.staff's database id (cached; resolved from a login token).
pub async fn staff_id(client: &Client) -> i64 {
    if let Some(id) = STAFF_ID.get() {
        return *id;
    }
    let id = token_user_id(&staff_token(client).await);
    let _ = STAFF_ID.set(id);
    id
}

/// Resolve any fixture user's database id by pinned uuid (needs a caller
/// with odo.auth.user.read; with_deleted so tombstone fixtures resolve too).
pub async fn user_id_by_uuid(client: &Client, token: &str, uuid: &str) -> i64 {
    let resp = client
        .post(format!("{}/api/v1/odo/auth/user/get", auth_base()))
        .headers(auth_header(token))
        .json(&serde_json::json!({"uuid": uuid, "options": {"with_deleted": true}}))
        .send()
        .await
        .expect("user/get by uuid");
    assert_eq!(resp.status(), 200, "user uuid {uuid} resolves");
    let data: Value = resp.json().await.expect("user/get json");
    data["id"].as_i64().expect("user id")
}

/// The org tree, fetched once and cached as (code -> (id, uuid)).
async fn org_tree_map(
    client: &Client,
    token: &str,
) -> &'static std::collections::HashMap<String, (i64, String)> {
    static TREE: OnceLock<std::collections::HashMap<String, (i64, String)>> = OnceLock::new();
    if let Some(map) = TREE.get() {
        return map;
    }
    let resp = client
        .get(format!("{}/api/v1/odo/org/tree", org_base()))
        .headers(auth_header(token))
        .send()
        .await
        .expect("org tree");
    assert_eq!(resp.status(), 200, "org tree loads");
    let tree: Value = resp.json().await.expect("org tree json");

    fn walk(node: &Value, map: &mut std::collections::HashMap<String, (i64, String)>) {
        if let (Some(code), Some(id), Some(uuid)) = (
            node["code"].as_str(),
            node["id"].as_i64(),
            node["uuid"].as_str(),
        ) {
            map.insert(code.to_string(), (id, uuid.to_string()));
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                walk(child, map);
            }
        }
    }
    let mut map = std::collections::HashMap::new();
    if let Some(roots) = tree["tree"].as_array() {
        for root in roots {
            walk(root, &mut map);
        }
    } else {
        walk(&tree, &mut map);
    }
    let _ = TREE.set(map);
    TREE.get().unwrap()
}

/// Resolve an org unit's database id by code via the org tree (cached).
pub async fn org_id_by_code(client: &Client, token: &str, code: &str) -> i64 {
    org_tree_map(client, token)
        .await
        .get(code)
        .unwrap_or_else(|| panic!("org code {code} in tree"))
        .0
}

/// Resolve an org unit's stable uuid by code via the org tree (cached).
pub async fn unit_uuid_by_code(client: &Client, token: &str, code: &str) -> String {
    org_tree_map(client, token)
        .await
        .get(code)
        .unwrap_or_else(|| panic!("org code {code} in tree"))
        .1
        .clone()
}

/// The root org unit id (code OLS in the platform seed).
pub async fn root_org_id(client: &Client, token: &str) -> i64 {
    org_id_by_code(client, token, "OLS").await
}

/// A branch org unit id (Main Street Branch in the platform seed).
pub async fn branch_org_id(client: &Client, token: &str) -> i64 {
    org_id_by_code(client, token, "MAIN").await
}

/// Upload routing is registry data: (entity_type, category) -> path on
/// asset.directory rows. The platform suite provisions its own mappings
/// (odo.auth.session as read/write perm — every login-capable user holds
/// it), so upload tests are self-contained on any cluster:
///
///   e2e-file  photo      -> e2e-assets/photos   (exact-category row)
///   e2e-file  (catch-all)-> e2e-assets/files
///   e2e-portrait         -> e2e-assets/portraits
pub async fn ensure_upload_fixtures() {
    static PROVISIONED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    PROVISIONED
        .get_or_init(|| async {
            let c = client();
            let admin = odo_admin_token(&c).await;
            for (path, entity_type, category) in [
                ("e2e-assets/photos", "e2e-file", Some("photo")),
                ("e2e-assets/files", "e2e-file", None),
                ("e2e-assets/portraits", "e2e-portrait", None),
            ] {
                let resp = c
                    .post(format!("{}/api/v1/odo/asset/directory/create", asset_base()))
                    .headers(auth_header(&admin))
                    .json(&serde_json::json!({
                        "path": path,
                        "read_perm": "odo.auth.session",
                        "write_perm": "odo.auth.session",
                        "entity_type": entity_type,
                        "category": category,
                    }))
                    .send()
                    .await
                    .expect("directory create request");
                assert!(
                    matches!(resp.status().as_u16(), 200 | 409),
                    "provisioning {path}: {}",
                    resp.status()
                );
            }
        })
        .await;
}
