//! JWT token management: generation, validation, and verification.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{LocalError, LocalResult};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Claims {
    pub sub: String,
    /// Stable uuid of the user (dual-claim phase of the uuid migration).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub sub_uuid: Option<String>,
    pub email: String,
    pub auth_method: String,
    pub session_id: String,
    /// Stable uuid of the caller's working org unit (uuid migration
    /// 1d.3: this claim carries the uuid; the legacy integer claim and
    /// the `org_unit_uuid` alias are gone).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub org_unit: Option<String>,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub token_type: String,
}

impl Claims {
    pub fn user_id(&self) -> LocalResult<i64> {
        self.sub
            .parse::<i64>()
            .map_err(|_| LocalError::internal("invalid user_id in token"))
    }
}

/// Verification keys: a single fixed key, or a shared set kept fresh from
/// a JWKS endpoint by a background refresh task (see `jwks_verifier`).
enum VerifyKeys {
    Fixed(DecodingKey),
    Shared(std::sync::Arc<std::sync::RwLock<Vec<DecodingKey>>>),
}

pub struct TokenManager {
    encoding_key: Option<EncodingKey>,
    verify_keys: VerifyKeys,
    algorithm: Algorithm,
    issuer: String,
    access_expire_minutes: i64,
    refresh_expire_days: i64,
    jwks_json: Option<String>,
}

impl TokenManager {
    /// Create a TokenManager that signs and verifies HS256 tokens.
    pub fn new_hmac(
        secret: &str,
        issuer: &str,
        access_expire_minutes: i64,
        refresh_expire_days: i64,
    ) -> Self {
        Self {
            encoding_key: Some(EncodingKey::from_secret(secret.as_bytes())),
            verify_keys: VerifyKeys::Fixed(DecodingKey::from_secret(secret.as_bytes())),
            algorithm: Algorithm::HS256,
            issuer: issuer.to_string(),
            access_expire_minutes,
            refresh_expire_days,
            jwks_json: None,
        }
    }

    /// Create a verify-only TokenManager for HS256.
    pub fn hmac_verifier(secret: &str) -> Self {
        Self {
            encoding_key: None,
            verify_keys: VerifyKeys::Fixed(DecodingKey::from_secret(secret.as_bytes())),
            algorithm: Algorithm::HS256,
            issuer: String::new(),
            access_expire_minutes: 0,
            refresh_expire_days: 0,
            jwks_json: None,
        }
    }

    /// Create a TokenManager that signs and verifies RS256 tokens.
    pub fn new_rsa(
        private_key_pem: &str,
        public_key_pem: &str,
        issuer: &str,
        access_expire_minutes: i64,
        refresh_expire_days: i64,
    ) -> Self {
        let jwks_json = build_jwks(public_key_pem).ok();

        Self {
            encoding_key: Some(
                EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
                    .expect("invalid RSA private key PEM"),
            ),
            verify_keys: VerifyKeys::Fixed(
                DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
                    .expect("invalid RSA public key PEM"),
            ),
            algorithm: Algorithm::RS256,
            issuer: issuer.to_string(),
            access_expire_minutes,
            refresh_expire_days,
            jwks_json,
        }
    }

    /// Create a verify-only TokenManager for RS256.
    pub fn rsa_verifier(public_key_pem: &str) -> Self {
        Self {
            encoding_key: None,
            verify_keys: VerifyKeys::Fixed(
                DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
                    .expect("invalid RSA public key PEM"),
            ),
            algorithm: Algorithm::RS256,
            issuer: String::new(),
            access_expire_minutes: 0,
            refresh_expire_days: 0,
            jwks_json: None,
        }
    }

    /// Returns (access_token, refresh_token, refresh_expires_at_ms)
    #[allow(clippy::too_many_arguments)]
    pub fn generate_token_pair(
        &self,
        user_id: i64,
        user_uuid: Option<String>,
        email: &str,
        auth_method: &str,
        session_id: &str,
        org_unit: Option<String>,
    ) -> LocalResult<(String, String, i64)> {
        let encoding_key = self
            .encoding_key
            .as_ref()
            .ok_or_else(|| LocalError::internal("TokenManager has no signing key"))?;

        let now = Utc::now();
        let header = Header::new(self.algorithm);

        let access_claims = Claims {
            sub: user_id.to_string(),
            sub_uuid: user_uuid,
            email: email.to_string(),
            auth_method: auth_method.to_string(),
            session_id: session_id.to_string(),
            org_unit,
            iat: now.timestamp(),
            exp: (now + Duration::minutes(self.access_expire_minutes)).timestamp(),
            iss: self.issuer.clone(),
            token_type: "access".to_string(),
        };

        let access_token = encode(&header, &access_claims, encoding_key)
            .map_err(|e| LocalError::internal(format!("Failed to encode access token: {e}")))?;

        let refresh_exp = (now + Duration::days(self.refresh_expire_days)).timestamp();
        let refresh_claims = Claims {
            token_type: "refresh".to_string(),
            exp: refresh_exp,
            ..access_claims
        };

        let refresh_token = encode(&header, &refresh_claims, encoding_key)
            .map_err(|e| LocalError::internal(format!("Failed to encode refresh token: {e}")))?;

        Ok((access_token, refresh_token, refresh_exp * 1000))
    }

    /// Validate a token (access or refresh) and return its claims.
    pub fn validate_token(&self, token: &str) -> LocalResult<Claims> {
        let mut validation = Validation::new(self.algorithm);
        validation.set_required_spec_claims(&["exp", "sub"]);

        let map_err = |e: jsonwebtoken::errors::Error| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature
            | jsonwebtoken::errors::ErrorKind::InvalidToken
            | jsonwebtoken::errors::ErrorKind::InvalidSignature => LocalError::unauthenticated(),
            _ => LocalError::internal(format!("JWT verification failed: {e}")),
        };

        let token_data = match &self.verify_keys {
            VerifyKeys::Fixed(key) => decode::<Claims>(token, key, &validation).map_err(map_err)?,
            VerifyKeys::Shared(keys) => {
                // Tokens carry no `kid`, so try each key in the set (in
                // practice one, briefly two during a rotation). The set is
                // kept fresh by the background JWKS refresh task.
                let keys = keys.read().unwrap();
                let mut last_err = jsonwebtoken::errors::ErrorKind::InvalidToken.into();
                let mut decoded = None;
                for key in keys.iter() {
                    match decode::<Claims>(token, key, &validation) {
                        Ok(data) => {
                            decoded = Some(data);
                            break;
                        }
                        Err(e) => last_err = e,
                    }
                }
                decoded.ok_or(last_err).map_err(map_err)?
            }
        };

        Ok(token_data.claims)
    }

    /// Validate an access token specifically (rejects refresh tokens).
    pub fn verify(&self, token: &str) -> LocalResult<Claims> {
        let claims = self.validate_token(token)?;
        if claims.token_type != "access" {
            return Err(LocalError::unauthenticated());
        }
        Ok(claims)
    }

    /// Returns the JWKS JSON for the public key (RS256 only).
    pub fn jwks(&self) -> Option<&str> {
        self.jwks_json.as_deref()
    }

    pub fn access_expire_seconds(&self) -> i64 {
        self.access_expire_minutes * 60
    }

    pub fn refresh_expire_seconds(&self) -> i64 {
        self.refresh_expire_days * 24 * 60 * 60
    }
}

/// One key set fetched from a JWKS endpoint.
#[derive(Deserialize)]
struct JwksDoc {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

/// Parse a JWKS document into decoding keys (RSA keys only).
fn parse_jwks(json: &str) -> LocalResult<Vec<DecodingKey>> {
    let doc: JwksDoc = serde_json::from_str(json)
        .map_err(|e| LocalError::internal(format!("invalid JWKS document: {e}")))?;

    let keys: Vec<DecodingKey> = doc
        .keys
        .iter()
        .filter(|k| k.kty == "RSA")
        .filter_map(|k| {
            let (n, e) = (k.n.as_deref()?, k.e.as_deref()?);
            DecodingKey::from_rsa_components(n, e).ok()
        })
        .collect();

    if keys.is_empty() {
        return Err(LocalError::internal("JWKS document contains no usable RSA keys"));
    }
    Ok(keys)
}

/// Fetch and parse a JWKS endpoint.
pub async fn fetch_jwks(url: &str) -> LocalResult<Vec<DecodingKey>> {
    let body = reqwest::get(url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| LocalError::internal(format!("JWKS fetch from {url} failed: {e}")))?
        .text()
        .await
        .map_err(|e| LocalError::internal(format!("JWKS fetch from {url} failed: {e}")))?;
    parse_jwks(&body)
}

impl TokenManager {
    /// Create a verify-only RS256 TokenManager whose keys come from a JWKS
    /// endpoint (odo-auth's `/.well-known/jwks.json`). Retries the initial
    /// fetch until the endpoint is reachable, then spawns a background task
    /// that refreshes the key set every `refresh_secs`, so signing-key
    /// rotations are picked up without a restart. A refresh failure keeps
    /// the previous keys and logs a warning.
    pub async fn jwks_verifier(jwks_url: &str, refresh_secs: u64) -> Self {
        let keys = loop {
            match fetch_jwks(jwks_url).await {
                Ok(keys) => break keys,
                Err(e) => {
                    tracing::warn!(error = %e, url = jwks_url, "JWKS not available yet; retrying");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        };
        tracing::info!(url = jwks_url, keys = keys.len(), "JWKS loaded");

        let shared = std::sync::Arc::new(std::sync::RwLock::new(keys));

        let refresh_keys = shared.clone();
        let refresh_url = jwks_url.to_string();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(refresh_secs)).await;
                match fetch_jwks(&refresh_url).await {
                    Ok(keys) => *refresh_keys.write().unwrap() = keys,
                    Err(e) => {
                        tracing::warn!(error = %e, url = refresh_url, "JWKS refresh failed; keeping previous keys");
                    }
                }
            }
        });

        Self {
            encoding_key: None,
            verify_keys: VerifyKeys::Shared(shared),
            algorithm: Algorithm::RS256,
            issuer: String::new(),
            access_expire_minutes: 0,
            refresh_expire_days: 0,
            jwks_json: None,
        }
    }
}

fn build_jwks(public_key_pem: &str) -> LocalResult<String> {
    let public_key = rsa::RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|e| LocalError::internal(format!("Failed to parse RSA public key: {e}")))?;

    let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());

    let jwks = serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "kid": "1",
            "n": n,
            "e": e,
        }]
    });

    Ok(jwks.to_string())
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key-at-least-32-bytes-long!!";

    fn hmac_manager() -> TokenManager {
        TokenManager::new_hmac(TEST_SECRET, "test-issuer", 10, 7)
    }

    #[test]
    fn generate_and_validate_token_pair() {
        let tm = hmac_manager();
        let (access, refresh, refresh_exp_ms) = tm
            .generate_token_pair(42, None, "user@test.com", "local", "sess-1", None)
            .unwrap();

        assert!(!access.is_empty());
        assert!(!refresh.is_empty());
        assert!(refresh_exp_ms > 0);

        let access_claims = tm.validate_token(&access).unwrap();
        assert_eq!(access_claims.sub, "42");
        assert_eq!(access_claims.email, "user@test.com");
        assert_eq!(access_claims.auth_method, "local");
        assert_eq!(access_claims.session_id, "sess-1");
        assert_eq!(access_claims.token_type, "access");
        assert_eq!(access_claims.iss, "test-issuer");
        assert!(access_claims.org_unit.is_none());

        let refresh_claims = tm.validate_token(&refresh).unwrap();
        assert_eq!(refresh_claims.token_type, "refresh");
        assert_eq!(refresh_claims.sub, "42");
    }

    #[test]
    fn verify_rejects_refresh_token() {
        let tm = hmac_manager();
        let (_access, refresh, _) = tm
            .generate_token_pair(1, None, "a@b.com", "local", "s1", None)
            .unwrap();

        let result = tm.verify(&refresh);
        assert!(result.is_err());
    }

    #[test]
    fn verify_accepts_access_token() {
        let tm = hmac_manager();
        let (access, _, _) = tm
            .generate_token_pair(1, None, "a@b.com", "local", "s1", None)
            .unwrap();

        let claims = tm.verify(&access).unwrap();
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn validate_rejects_invalid_token() {
        let tm = hmac_manager();
        let result = tm.validate_token("not-a-valid-token");
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_wrong_secret() {
        let tm1 = TokenManager::new_hmac("secret-one-xxxxxxxxxxxxxxxxxxxxx", "iss", 10, 7);
        let tm2 = TokenManager::new_hmac("secret-two-xxxxxxxxxxxxxxxxxxxxx", "iss", 10, 7);

        let (access, _, _) = tm1
            .generate_token_pair(1, None, "a@b.com", "local", "s1", None)
            .unwrap();

        let result = tm2.validate_token(&access);
        assert!(result.is_err());
    }

    #[test]
    fn org_unit_round_trips() {
        let tm = hmac_manager();
        let (access, _, _) = tm
            .generate_token_pair(1, Some("e2e00000-0000-4000-a000-000000000042".into()), "a@b.com", "local", "s1", Some("e2e00000-0000-4000-a000-000000000099".into()))
            .unwrap();

        let claims = tm.validate_token(&access).unwrap();
        assert_eq!(
            claims.org_unit.as_deref(),
            Some("e2e00000-0000-4000-a000-000000000099")
        );
        assert_eq!(
            claims.sub_uuid.as_deref(),
            Some("e2e00000-0000-4000-a000-000000000042")
        );
    }

    #[test]
    fn claims_user_id_parses() {
        let claims = Claims {
            sub: "123".to_string(),
            sub_uuid: None,
            email: String::new(),
            auth_method: String::new(),
            session_id: String::new(),
            org_unit: None,
            iat: 0,
            exp: 0,
            iss: String::new(),
            token_type: String::new(),
        };
        assert_eq!(claims.user_id().unwrap(), 123);
    }

    #[test]
    fn claims_user_id_invalid() {
        let claims = Claims {
            sub: "not-a-number".to_string(),
            sub_uuid: None,
            email: String::new(),
            auth_method: String::new(),
            session_id: String::new(),
            org_unit: None,
            iat: 0,
            exp: 0,
            iss: String::new(),
            token_type: String::new(),
        };
        assert!(claims.user_id().is_err());
    }

    #[test]
    fn verifier_cannot_sign() {
        let tm = TokenManager::hmac_verifier(TEST_SECRET);
        let result = tm.generate_token_pair(1, None, "a@b.com", "local", "s1", None);
        assert!(result.is_err());
    }

    #[test]
    fn hmac_verifier_validates() {
        let signer = hmac_manager();
        let verifier = TokenManager::hmac_verifier(TEST_SECRET);

        let (access, _, _) = signer
            .generate_token_pair(1, None, "a@b.com", "local", "s1", None)
            .unwrap();

        let claims = verifier.validate_token(&access).unwrap();
        assert_eq!(claims.sub, "1");
    }

    const TEST_RSA_PUBLIC_PEM: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAvrIaMhylvYFsujfLh8hP
ThaskTkmhNFv7Ht4VpKMg53qI13lynGpU1FypgJhrD4JGcsu+yHOUU1RNtpvx3Ke
yvquc+CBvUIXmXhelcOB7O/TNKJPaVbPdf4Yrp0NJyuTwzUYJ3p5MXQB9WRJT9+p
hrv93brVflcQQSjNB2Q71BjY9eaPfXHkpdFEFkbBaxZZ4O2T/CDcJ4OMVJyTOOBM
jJHOCkv50MoKkMUIzwY/eAsdV+34venl3J//tykmtdwBVOOwYjpaJD+49p2so1nR
TKfLf9cr3fUAFdKloEecgMGtZmd0+SA+SQvQSnPM9BCU/a0Ca8m7w8rfpLGCQT05
mQIDAQAB
-----END PUBLIC KEY-----";

    #[test]
    fn parse_jwks_round_trips_build_jwks() {
        let jwks = build_jwks(TEST_RSA_PUBLIC_PEM).unwrap();
        let keys = parse_jwks(&jwks).unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn parse_jwks_skips_non_rsa_keys() {
        let jwks = r#"{"keys":[
            {"kty":"EC","crv":"P-256","x":"x","y":"y"},
            {"kty":"RSA","n":"sXchYPUFxK0UTLo","e":"AQAB"}
        ]}"#;
        let keys = parse_jwks(jwks).unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn parse_jwks_rejects_empty_or_unusable() {
        assert!(parse_jwks(r#"{"keys":[]}"#).is_err());
        assert!(parse_jwks(r#"{"keys":[{"kty":"EC"}]}"#).is_err());
        assert!(parse_jwks("not json").is_err());
    }

    #[test]
    fn generate_session_id_is_unique() {
        let a = generate_session_id();
        let b = generate_session_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36); // UUID v4 format
    }

    #[test]
    fn access_expire_seconds() {
        let tm = TokenManager::new_hmac("x".repeat(32).as_str(), "iss", 15, 7);
        assert_eq!(tm.access_expire_seconds(), 900);
    }

    #[test]
    fn refresh_expire_seconds() {
        let tm = TokenManager::new_hmac("x".repeat(32).as_str(), "iss", 10, 3);
        assert_eq!(tm.refresh_expire_seconds(), 3 * 24 * 60 * 60);
    }
}
