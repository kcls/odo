use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use chrono::Utc;
use flate2::Compression;
use flate2::write::DeflateEncoder;
use odo_client::auth::generate_session_id;
use odo_entity::auth::{
    saml_auth_requests, saml_idp_config, saml_session, saml_sp_config, session, usr,
    usr_saml_identities,
};
use odo_client::error::{LocalError, LocalResult};
use openssl::x509::X509;
use samael::crypto::verify_signed_xml;
use samael::metadata::EntityDescriptor;
use samael::schema::Response as SamlResponse;
use sea_orm::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::{IntoActiveModel, QueryOrder, QuerySelect, Set, TransactionTrait};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{AppState, authz};
use odo_client::error::ApiResult;

// ---------------------------------------------------------------------------
// Request / response structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MetadataQuery {
    origin: String,
}

#[derive(Deserialize)]
pub struct InitiateSsoQuery {
    sp_id: i32,
    #[serde(default)]
    relay_state: Option<String>,
}

#[derive(Deserialize)]
pub struct AcsFormRequest {
    #[serde(rename = "SAMLResponse")]
    saml_response: String,
    #[serde(rename = "RelayState", default)]
    relay_state: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SlsRequest {
    #[serde(alias = "SAMLRequest")]
    saml_request: Option<String>,
    #[serde(alias = "SAMLResponse")]
    saml_response_field: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct InitiateLogoutRequest {
    session_index: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ListIdpsRequest {
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
pub struct SsoConfigsRequest {
    origin: String,
}

#[derive(Deserialize, ToSchema)]
pub struct GetIdpRequest {
    idp_id: String,
}

// ---------------------------------------------------------------------------
// OpenAPI doc fragment for SAML endpoints
// ---------------------------------------------------------------------------

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        single_logout_service,
        initiate_logout,
        list_idps,
        list_sso_configs,
        get_idp,
    ),
    components(schemas(
        SlsRequest,
        InitiateLogoutRequest,
        ListIdpsRequest,
        SsoConfigsRequest,
        GetIdpRequest,
        SloResponse,
        InitiateLogoutResponse,
        IdpResponse,
        ListIdpsResponse,
        SsoConfigItem,
        ListSsoConfigsResponse,
    )),
    tags(
        (name = "saml", description = "SAML SSO authentication and session management"),
    ),
)]
pub struct SamlApiDoc;

// ---------------------------------------------------------------------------
// Response structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct SloResponse {
    pub logout_response: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InitiateLogoutResponse {
    pub redirect_url: String,
    pub session_index: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IdpResponse {
    pub id: i32,
    pub entity_id: String,
    pub name: String,
    pub sso_url: Option<String>,
    pub slo_url: Option<String>,
    pub metadata_url: Option<String>,
    pub is_active: Option<bool>,
    pub session_lifetime_hours: Option<i32>,
    pub attribute_mapping: Option<serde_json::Value>,
    pub created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListIdpsResponse {
    pub idps: Vec<IdpResponse>,
    pub count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SsoConfigItem {
    pub sp_id: i32,
    pub label: Option<String>,
    pub idp_id: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListSsoConfigsResponse {
    pub sso_configs: Vec<SsoConfigItem>,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Parsed SAML assertion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SamlAssertionData {
    name_id: String,
    name_id_format: Option<String>,
    email: String,
    session_index: String,
    attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct UserProfileAttributes {
    first_given_name: Option<String>,
    second_given_name: Option<String>,
    family_name: Option<String>,
    display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// SHA-256 hash a token for storage in the session table.
fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    format!("{:x}", h.finalize())
}

/// Extract the Issuer entity ID from a SAML response XML string.
fn extract_issuer(xml: &str) -> LocalResult<String> {
    let response: SamlResponse = xml
        .parse()
        .map_err(|e| LocalError::invalid_input(format!("Failed to parse SAML response: {e}")))?;
    response
        .issuer
        .and_then(|i| i.value)
        .ok_or_else(|| LocalError::invalid_input("No Issuer found in SAML response"))
}

/// Verify the XML signature on a SAML response using the IdP's X.509 certificate.
/// Tries with an ID reference first, then falls back to no reference.
fn verify_saml_signature(
    xml: &str,
    x509_cert_pem: &str,
    metadata_url: Option<&str>,
) -> LocalResult<()> {
    let cert = X509::from_pem(x509_cert_pem.as_bytes())
        .map_err(|e| LocalError::internal(format!("Failed to parse X.509 certificate: {e}")))?;
    let cert_der = cert
        .to_der()
        .map_err(|e| LocalError::internal(format!("Failed to convert certificate to DER: {e}")))?;

    if let Err(e) = verify_signed_xml(xml.as_bytes(), &cert_der, Some("ID")) {
        verify_signed_xml(xml.as_bytes(), &cert_der, None).map_err(|e2| {
            let err_str = format!("{e2:?}");
            let err_with_id = format!("{e:?}");
            error!(
                error = %err_str,
                error_with_id = %err_with_id,
                metadata_url = ?metadata_url,
                "SAML signature verification failed"
            );
            LocalError::unauthenticated()
        })?;
    }
    Ok(())
}

/// Validate SAML response constraints: Destination, NotBefore/NotOnOrAfter,
/// and AudienceRestriction.
fn validate_saml_constraints(
    xml: &str,
    expected_audience: &str,
    expected_destination: &str,
) -> LocalResult<()> {
    let response: SamlResponse = xml
        .parse()
        .map_err(|e| LocalError::invalid_input(format!("Failed to parse SAML response: {e}")))?;

    if let Some(destination) = &response.destination
        && destination != expected_destination
    {
        return Err(LocalError::invalid_input(format!(
            "Destination mismatch - expected: {expected_destination}, received: {destination}"
        )));
    }

    if let Some(assertion) = response.assertion.as_ref()
        && let Some(conditions) = &assertion.conditions
    {
        let now = Utc::now();
        if let Some(nb) = &conditions.not_before
            && now < *nb
        {
            return Err(LocalError::invalid_input(format!(
                "Assertion not yet valid - NotBefore: {nb}"
            )));
        }
        if let Some(noa) = &conditions.not_on_or_after
            && now >= *noa
        {
            return Err(LocalError::invalid_input(format!(
                "Assertion expired - NotOnOrAfter: {noa}"
            )));
        }
        if let Some(audience_restrictions) = &conditions.audience_restrictions {
            let valid = audience_restrictions
                .iter()
                .any(|r| r.audience.iter().any(|a| a == expected_audience));
            if !valid {
                return Err(LocalError::invalid_input(format!(
                    "Audience restriction failed - expected: {expected_audience}"
                )));
            }
        }
    }
    Ok(())
}

/// Parse a SAML response XML into structured assertion data: NameID,
/// session index, email, and all attribute statements.
fn parse_saml_response(xml: &str) -> LocalResult<SamlAssertionData> {
    let response: SamlResponse = xml
        .parse()
        .map_err(|e| LocalError::invalid_input(format!("Failed to parse SAML response: {e}")))?;

    let assertion = response
        .assertion
        .as_ref()
        .ok_or(LocalError::invalid_input(
            "No assertion found in SAML response",
        ))?;

    let subject = assertion.subject.as_ref().ok_or(LocalError::invalid_input(
        "No subject found in SAML assertion",
    ))?;

    let name_id_obj = subject
        .name_id
        .as_ref()
        .ok_or(LocalError::invalid_input("No NameID found in subject"))?;

    let name_id = name_id_obj.value.clone();
    let name_id_format = name_id_obj.format.clone();

    let session_index = assertion
        .authn_statements
        .as_ref()
        .and_then(|stmts| stmts.first())
        .and_then(|stmt| stmt.session_index.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut attributes = HashMap::new();
    let mut email = String::new();

    if let Some(attr_statements) = &assertion.attribute_statements {
        for attr_statement in attr_statements {
            for attribute in &attr_statement.attributes {
                if let Some(attr_name) = &attribute.name
                    && let Some(value) = attribute.values.first().and_then(|v| v.value.as_ref())
                {
                    attributes.insert(attr_name.clone(), value.clone());
                    if attr_name.contains("email")
                        || attr_name.contains("Email")
                        || attr_name
                            == "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress"
                    {
                        email = value.clone();
                    }
                }
            }
        }
    }

    if email.is_empty() && name_id.contains('@') {
        email = name_id.clone();
    }

    info!(attributes = ?attributes, "SP returned SAML attributes");

    Ok(SamlAssertionData {
        name_id,
        name_id_format,
        email,
        session_index,
        attributes,
    })
}

/// Map SAML assertion attributes to user profile fields (name, display name).
fn extract_user_attributes(data: &SamlAssertionData) -> UserProfileAttributes {
    let first_given_name = data
        .attributes
        .get("http://schemas.xmlsoap.org/ws/2005/05/identity/claims/givenname")
        .or_else(|| data.attributes.get("givenName"))
        .or_else(|| data.attributes.get("firstName"))
        .or_else(|| data.attributes.get("first_name"))
        .cloned();

    let family_name = data
        .attributes
        .get("http://schemas.xmlsoap.org/ws/2005/05/identity/claims/surname")
        .or_else(|| data.attributes.get("sn"))
        .or_else(|| data.attributes.get("surname"))
        .or_else(|| data.attributes.get("lastName"))
        .or_else(|| data.attributes.get("last_name"))
        .cloned();

    let display_name = data
        .attributes
        .get("http://schemas.microsoft.com/identity/claims/displayname")
        .or_else(|| data.attributes.get("displayname"))
        .cloned();

    let second_given_name = data
        .attributes
        .get("middleName")
        .or_else(|| data.attributes.get("middle_name"))
        .cloned();

    UserProfileAttributes {
        first_given_name,
        second_given_name,
        family_name,
        display_name,
    }
}

/// Parse IdP metadata XML to extract the signing certificate (PEM) and SSO URL.
fn extract_idp_data_from_metadata(metadata_xml: &str) -> LocalResult<(String, String)> {
    let entity_descriptor = EntityDescriptor::from_str(metadata_xml)
        .map_err(|e| LocalError::internal(format!("Failed to parse metadata XML: {e}")))?;

    let idp_descriptor = entity_descriptor
        .idp_sso_descriptors
        .as_ref()
        .and_then(|d| d.first())
        .ok_or(LocalError::internal(
            "No IdP SSO Descriptor found in metadata",
        ))?;

    let signing_key = idp_descriptor
        .key_descriptors
        .iter()
        .find(|kd| kd.is_signing() || kd.key_use.is_none())
        .ok_or(LocalError::internal(
            "No signing key descriptor found in metadata",
        ))?;

    let certificate = signing_key
        .key_info
        .x509_data
        .as_ref()
        .and_then(|x509| x509.certificates.first())
        .ok_or(LocalError::internal(
            "No X.509 certificate found in key descriptor",
        ))?;

    let sso_service = idp_descriptor
        .single_sign_on_services
        .iter()
        .find(|svc| svc.binding.contains("HTTP-Redirect"))
        .or_else(|| idp_descriptor.single_sign_on_services.first())
        .ok_or(LocalError::internal(
            "No Single Sign-On Service found in metadata",
        ))?;

    let sso_url = sso_service.location.clone();

    let cert_clean: String = certificate.chars().filter(|c| !c.is_whitespace()).collect();
    let mut pem_body = String::new();
    for (i, chunk) in cert_clean.as_bytes().chunks(64).enumerate() {
        if i > 0 {
            pem_body.push('\n');
        }
        pem_body.push_str(&String::from_utf8_lossy(chunk));
    }
    let pem_cert = format!("-----BEGIN CERTIFICATE-----\n{pem_body}\n-----END CERTIFICATE-----");

    Ok((pem_cert, sso_url))
}

/// Fetch IdP metadata XML from a remote HTTPS URL.
async fn fetch_idp_metadata(metadata_url: &str) -> LocalResult<String> {
    if !metadata_url.starts_with("https://") {
        return Err(LocalError::invalid_input("Metadata URL must use HTTPS"));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| LocalError::internal(format!("Failed to create HTTP client: {e}")))?;

    let response = client
        .get(metadata_url)
        .send()
        .await
        .map_err(|e| LocalError::internal(format!("Failed to fetch metadata: {e}")))?;

    if !response.status().is_success() {
        return Err(LocalError::internal(format!(
            "Metadata fetch failed with status: {}",
            response.status()
        )));
    }

    response
        .text()
        .await
        .map_err(|e| LocalError::internal(format!("Failed to read metadata response: {e}")))
}

/// Fetch fresh IdP metadata and update the SP config with the new certificate.
async fn refresh_idp_cert(
    db: &DatabaseConnection,
    sp_id: i32,
    metadata_url: &str,
) -> LocalResult<String> {
    let metadata_xml = fetch_idp_metadata(metadata_url).await?;
    let (certificate, _sso_url) = extract_idp_data_from_metadata(&metadata_xml)?;

    let model = saml_sp_config::ActiveModel {
        id: Set(sp_id),
        idp_x509_cert: Set(Some(certificate.clone())),
        ..Default::default()
    };

    saml_sp_config::Entity::update(model).exec(db).await?;

    Ok(certificate)
}

/// Validate a SAML InResponseTo request ID: verify it exists, hasn't expired,
/// and delete it to prevent replay. Returns the associated SP config ID.
async fn validate_and_consume_request_id(
    db: &DatabaseConnection,
    request_id: &str,
) -> LocalResult<Option<i32>> {
    let now = Utc::now();

    let entry = saml_auth_requests::Entity::find()
        .filter(saml_auth_requests::Column::RequestId.eq(request_id))
        .one(db)
        .await?
        .ok_or_else(|| {
            error!(request_id = %request_id, "SAML request ID not found - possible replay attack");
            LocalError::unauthenticated()
        })?;

    if let Some(expires_at) = &entry.expires_at {
        let now_fixed: chrono::DateTime<chrono::FixedOffset> = now.into();
        if *expires_at < now_fixed {
            let _ = saml_auth_requests::Entity::delete_by_id(entry.id)
                .exec(db)
                .await;
            return Err(LocalError::unauthenticated());
        }
    }

    let sp_id = entry.sp_id;

    saml_auth_requests::Entity::delete_by_id(entry.id)
        .exec(db)
        .await?;

    info!(request_id = %request_id, sp_id = sp_id, "SAML request ID validated and consumed");
    Ok(sp_id)
}

/// Create or update a user account from SAML assertion data.
/// Updates profile attributes and last_login_at on existing users.
async fn upsert_user_account(
    db: &impl ConnectionTrait,
    email: &str,
    profile_attrs: &UserProfileAttributes,
) -> LocalResult<i32> {
    let existing = usr::Entity::find()
        .filter(usr::Column::Email.eq(email))
        .one(db)
        .await?;

    if let Some(user) = existing {
        let mut model = user.into_active_model();
        model.last_login_at = Set(Some(Utc::now().into()));

        if let Some(ref v) = profile_attrs.first_given_name {
            model.first_given_name = Set(Some(v.clone()));
        }
        if let Some(ref v) = profile_attrs.second_given_name {
            model.second_given_name = Set(Some(v.clone()));
        }
        if let Some(ref v) = profile_attrs.family_name {
            model.family_name = Set(Some(v.clone()));
        }
        if let Some(ref v) = profile_attrs.display_name {
            model.display_name = Set(v.clone());
        }

        let updated = usr::Entity::update(model).exec(db).await?;

        Ok(updated.id)
    } else {
        let username = email.split('@').next().unwrap_or(email);

        let display_name = profile_attrs.display_name.clone().unwrap_or_else(|| {
            format!(
                "{} {}",
                profile_attrs.first_given_name.as_deref().unwrap_or(""),
                profile_attrs.family_name.as_deref().unwrap_or("")
            )
        });

        info!(
            username = username,
            email = email,
            display_name = display_name,
            attribues = ?profile_attrs,
            "Creating SAML user account"
        );

        let now: chrono::DateTime<chrono::FixedOffset> = Utc::now().into();
        let new_usr = usr::ActiveModel {
            email: Set(email.to_string()),
            username: Set(Some(username.to_string())),
            auth_method: Set("saml".to_string()),
            last_login_at: Set(Some(now)),
            first_given_name: Set(profile_attrs.first_given_name.clone()),
            second_given_name: Set(profile_attrs.second_given_name.clone()),
            family_name: Set(profile_attrs.family_name.clone()),
            display_name: Set(display_name),
            ..Default::default()
        };
        let created = new_usr.insert(db).await?;
        Ok(created.id)
    }
}

/// Create or update the SAML identity record linking a user to an IdP.
async fn upsert_saml_identity(
    db: &impl ConnectionTrait,
    user_id: i32,
    idp_id: i32,
    assertion_data: &SamlAssertionData,
) -> LocalResult<()> {
    let attributes_json =
        serde_json::to_value(&assertion_data.attributes).unwrap_or(serde_json::json!({}));

    let existing = usr_saml_identities::Entity::find_by_id(user_id)
        .one(db)
        .await?;

    if let Some(ident) = existing {
        let mut model = ident.into_active_model();
        model.idp_id = Set(idp_id);
        model.name_id = Set(assertion_data.name_id.clone());
        model.attributes = Set(Some(attributes_json));
        model.session_index = Set(Some(assertion_data.session_index.clone()));
        if let Some(ref fmt) = assertion_data.name_id_format {
            model.name_id_format = Set(Some(fmt.clone()));
        }
        usr_saml_identities::Entity::update(model).exec(db).await?;
    } else {
        let model = usr_saml_identities::ActiveModel {
            user_id: Set(user_id),
            idp_id: Set(idp_id),
            name_id: Set(assertion_data.name_id.clone()),
            attributes: Set(Some(attributes_json)),
            session_index: Set(Some(assertion_data.session_index.clone())),
            name_id_format: Set(assertion_data.name_id_format.clone()),
            ..Default::default()
        };
        usr_saml_identities::Entity::insert(model).exec(db).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// GET /odo/auth/saml/metadata?origin=... — returns raw SP metadata XML.
pub async fn get_metadata(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MetadataQuery>,
) -> Result<Response, odo_client::error::ApiError> {
    let sp = saml_sp_config::Entity::find()
        .filter(saml_sp_config::Column::EntityId.eq(&params.origin))
        .filter(saml_sp_config::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("saml sp"))?;

    let x509_cert_inline = sp.x509_cert.replace('\n', "");
    let slo_url = sp.slo_url.as_deref().unwrap_or("/saml/sls");

    let metadata_xml = format!(
        r#"<?xml version="1.0"?>
<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{}">
    <SPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
        <KeyDescriptor use="signing">
            <KeyInfo xmlns="http://www.w3.org/2000/09/xmldsig#">
                <X509Data>
                    <X509Certificate>{}</X509Certificate>
                </X509Data>
            </KeyInfo>
        </KeyDescriptor>
        <SingleLogoutService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST" Location="{}"/>
        <AssertionConsumerService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST" Location="{}" index="0"/>
    </SPSSODescriptor>
</EntityDescriptor>"#,
        sp.entity_id, x509_cert_inline, slo_url, sp.acs_url
    );

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/xml")],
        metadata_xml,
    )
        .into_response())
}

/// GET /odo/auth/saml/sso/initiate?sp_id=X&relay_state=... — redirects to IdP.
pub async fn initiate_sso(
    State(state): State<Arc<AppState>>,
    Query(params): Query<InitiateSsoQuery>,
) -> Result<Response, odo_client::error::ApiError> {
    let sp = saml_sp_config::Entity::find_by_id(params.sp_id)
        .filter(saml_sp_config::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("saml sp"))?;

    let idp_id = sp.idp.ok_or(LocalError::not_found("sp has no idp"))?;

    let idp = saml_idp_config::Entity::find_by_id(idp_id)
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found(format!("IdP '{idp_id}'")))?;

    let sso_url = idp
        .sso_url
        .as_deref()
        .ok_or(LocalError::invalid_input("IdP has no SSO URL configured"))?;

    let sp_entity_id = &sp.entity_id;

    let full_relay_state = params.relay_state.map(|rs| {
        if rs.starts_with('/') {
            let base = sp_entity_id.trim_end_matches('/');
            format!("{base}{rs}")
        } else {
            rs
        }
    });

    let request_id = format!("_{}", Uuid::new_v4());

    let saml_request_xml = format!(
        r#"<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="{}" Version="2.0" IssueInstant="{}" Destination="{}" AssertionConsumerServiceURL="{}" ProtocolBinding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST">
    <saml:Issuer>{}</saml:Issuer>
    <samlp:NameIDPolicy Format="urn:oasis:names:tc:SAML:1.1:nameid-format:unspecified" AllowCreate="true"/>
</samlp:AuthnRequest>"#,
        request_id,
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        sso_url,
        sp.acs_url,
        sp_entity_id
    );

    let auth_req = saml_auth_requests::ActiveModel {
        request_id: Set(request_id.clone()),
        idp_id: Set(idp.id),
        relay_state: Set(full_relay_state.clone()),
        request_data: Set(saml_request_xml.clone()),
        acs_url: Set(Some(sp.acs_url.clone())),
        sp_id: Set(Some(sp.id)),
        ..Default::default()
    };

    saml_auth_requests::Entity::insert(auth_req)
        .exec(&state.db)
        .await
        .map_err(|e| LocalError::internal(format!("Failed to store auth request: {e}")))?;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(saml_request_xml.as_bytes())
        .map_err(|e| LocalError::internal(format!("Failed to compress SAML request: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| LocalError::internal(format!("Failed to finish compression: {e}")))?;

    let encoded_request = base64::engine::general_purpose::STANDARD.encode(&compressed);
    let encoded_saml = urlencoding::encode(&encoded_request);
    let encoded_relay = full_relay_state
        .as_deref()
        .map(|rs| urlencoding::encode(rs).to_string())
        .unwrap_or_default();

    let redirect_url = if encoded_relay.is_empty() {
        format!("{sso_url}?SAMLRequest={encoded_saml}")
    } else {
        format!("{sso_url}?SAMLRequest={encoded_saml}&RelayState={encoded_relay}")
    };

    Ok(Redirect::to(&redirect_url).into_response())
}

/// Browser-facing ACS endpoint: accepts form POST from the IdP,
/// sets a refresh cookie, and redirects to the relay state with ?sso=1.
pub async fn assertion_consumer_service_form(
    State(state): State<Arc<AppState>>,
    Form(params): Form<AcsFormRequest>,
) -> Result<axum::response::Response, odo_client::error::ApiError> {
    let result = acs_core(&state, &params.saml_response, params.relay_state.as_deref()).await?;

    if let Some(no_roles_redirect) = result.no_roles_redirect {
        return Ok(Redirect::to(&no_roles_redirect).into_response());
    }

    let base_redirect = result.relay_state.unwrap_or_else(|| "/".to_string());
    let sep = if base_redirect.contains('?') {
        "&"
    } else {
        "?"
    };
    let redirect_url = format!("{base_redirect}{sep}sso=1");

    let cookie = Cookie::build((state.cookie.name.clone(), result.refresh_token))
        .http_only(true)
        .secure(state.cookie.secure)
        .same_site(SameSite::Lax)
        .path(state.cookie.path.clone())
        .max_age(time::Duration::seconds(
            ((result.refresh_expires_at_ms - Utc::now().timestamp_millis()) / 1000).max(0),
        ))
        .build();

    let mut response = Redirect::to(&redirect_url).into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str(&cookie.to_string()) {
        response.headers_mut().insert(SET_COOKIE, val);
    }

    Ok(response)
}

struct AcsResult {
    refresh_token: String,
    refresh_expires_at_ms: i64,
    relay_state: Option<String>,
    no_roles_redirect: Option<String>,
}

/// Core SAML ACS processing: decode and validate the SAML response,
/// verify signature, enforce constraints, upsert user and SAML identity,
/// check permissions, and create an authenticated session.
async fn acs_core(
    state: &AppState,
    saml_response_b64: &str,
    relay_state: Option<&str>,
) -> Result<AcsResult, odo_client::error::ApiError> {
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(saml_response_b64)
        .map_err(|_| LocalError::invalid_input("Invalid SAMLResponse"))?;

    let decoded_xml = String::from_utf8_lossy(&decoded_bytes);
    info!("Processing SAML Response XML");

    let issuer = extract_issuer(&decoded_xml)?;

    info!(issuer = %issuer, "Extracted issuer");

    let idp = saml_idp_config::Entity::find()
        .filter(saml_idp_config::Column::EntityId.eq(&issuer))
        .filter(saml_idp_config::Column::IsActive.eq(true))
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            error!(issuer = %issuer, "Unknown or inactive IdP");
            LocalError::unauthenticated()
        })?;

    let session_lifetime_hours = idp.session_lifetime_hours.unwrap_or(8) as i64;

    // Validate InResponseTo
    let saml_resp: SamlResponse = decoded_xml
        .parse()
        .map_err(|e| LocalError::invalid_input(format!("Failed to parse SAML response: {e}")))?;

    let mut sp_id: Option<i32> = None;
    if let Some(in_response_to) = &saml_resp.in_response_to {
        sp_id = validate_and_consume_request_id(&state.db, in_response_to)
            .await
            .map_err(|e| {
                error!(error = %e, in_response_to = %in_response_to, "InResponseTo validation failed");
                LocalError::unauthenticated()
            })?;
    } else {
        warn!(issuer = %issuer, "SAML response missing InResponseTo");
    }

    let sid = sp_id.ok_or(LocalError::invalid_input(
        "SAML auth request has no associated SP",
    ))?;

    let sp = saml_sp_config::Entity::find_by_id(sid)
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found(format!("SP config '{sid}'")))?;

    let metadata_url = sp.metadata_url.as_deref().or(idp.metadata_url.as_deref());

    let mut x509_cert = sp.idp_x509_cert.clone();

    // Fetch cert from metadata if missing
    if x509_cert.is_none() {
        if let Some(url) = metadata_url {
            x509_cert = Some(refresh_idp_cert(&state.db, sp.id, url).await?);
        } else {
            return Err(LocalError::internal("IdP: no certificate or metadata URL").into());
        }
    }

    let cert_str = x509_cert.as_ref().unwrap();

    // Verify signature, retrying with refreshed cert if needed
    let verify_result = verify_saml_signature(&decoded_xml, cert_str, metadata_url);
    if verify_result.is_err() {
        if let Some(url) = metadata_url {
            let fresh_cert = refresh_idp_cert(&state.db, sp.id, url).await?;
            verify_saml_signature(&decoded_xml, &fresh_cert, Some(url))?;
        } else {
            verify_result?;
        }
    }

    validate_saml_constraints(&decoded_xml, &sp.entity_id, &sp.acs_url)?;

    let assertion_data = parse_saml_response(&decoded_xml)?;

    let profile_attrs = extract_user_attributes(&assertion_data);

    // Upsert user + SAML identity in a transaction
    let user_id = state
        .db
        .transaction::<_, i32, DbErr>(|txn| {
            let email = assertion_data.email.clone();
            let pa = profile_attrs.clone();
            let ad = assertion_data.clone();
            let idp_id = idp.id;
            Box::pin(async move {
                let uid = upsert_user_account(txn, &email, &pa)
                    .await
                    .map_err(|e| DbErr::Custom(e.to_string()))?;
                upsert_saml_identity(txn, uid, idp_id, &ad)
                    .await
                    .map_err(|e| DbErr::Custom(e.to_string()))?;
                Ok(uid)
            })
        })
        .await
        .map_err(|e| LocalError::internal(format!("User upsert failed: {e}")))?;

    // Check auth.session permission
    if !authz::user_has_perm(&state.db, user_id, "odo.auth.session", None).await? {
        info!(
            user_id = user_id,
            "User authenticated via SAML but has no roles"
        );
        let base = relay_state.unwrap_or("/login");
        let sep = if base.contains('?') { "&" } else { "?" };
        return Ok(AcsResult {
            refresh_token: String::new(),
            refresh_expires_at_ms: 0,
            relay_state: relay_state.map(|s| s.to_string()),
            no_roles_redirect: Some(format!("{base}{sep}error=no_roles")),
        });
    }

    let session_uuid = generate_session_id();
    // Working location is resolved on session restore (see refresh), so
    // the initial SAML token carries no org unit in either form.
    let user_uuid = usr::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .map(|u| u.uuid.to_string());
    let (access_token, refresh_token, refresh_expires_at_ms) = state.tokens.generate_token_pair(
        user_id as i64,
        user_uuid,
        &assertion_data.email,
        "saml",
        &session_uuid,
        None,
    )?;

    let expires_at = Utc::now() + chrono::Duration::seconds(session_lifetime_hours * 3600);

    // Create auth.session + saml_session in a transaction
    state
        .db
        .transaction::<_, (), DbErr>(|txn| {
            let at = access_token.clone();
            let rt = refresh_token.clone();
            let su = session_uuid.clone();
            let si = assertion_data.session_index.clone();
            let ni = assertion_data.name_id.clone();
            let sd =
                serde_json::to_value(&assertion_data.attributes).unwrap_or(serde_json::json!({}));
            let idp_id = idp.id;
            Box::pin(async move {
                let new_session = session::ActiveModel {
                    usr: Set(user_id),
                    uuid: Set(su),
                    token_hash: Set(hash_token(&at)),
                    refresh_token_hash: Set(Some(hash_token(&rt))),
                    auth_method: Set("saml".to_string()),
                    is_active: Set(Some(true)),
                    expires_at: Set(expires_at.into()),
                    ..Default::default()
                };
                let sess = new_session.insert(txn).await?;

                let saml_sess = saml_session::ActiveModel {
                    session_index: Set(si),
                    idp_id: Set(idp_id),
                    name_id: Set(ni),
                    session_data: Set(Some(sd)),
                    session: Set(sess.id),
                    ..Default::default()
                };
                saml_session::Entity::insert(saml_sess).exec(txn).await?;

                Ok(())
            })
        })
        .await
        .map_err(|e| LocalError::internal(format!("Session creation failed: {e}")))?;

    Ok(AcsResult {
        refresh_token,
        refresh_expires_at_ms,
        relay_state: relay_state.map(|s| s.to_string()),
        no_roles_redirect: None,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/sls",
    request_body = SlsRequest,
    responses((status = 200, body = SloResponse, description = "SAML logout processed")),
    tag = "saml"
)]
/// POST /api/v1/odo/auth/saml/sls — process a SAML Single Logout request.
pub async fn single_logout_service(
    State(state): State<Arc<AppState>>,
    Json(params): Json<SlsRequest>,
) -> ApiResult<Json<SloResponse>> {
    let saml_data =
        params
            .saml_request
            .or(params.saml_response_field)
            .ok_or(LocalError::invalid_input(
                "Missing SAMLRequest or SAMLResponse",
            ))?;

    let decoded: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(
        &base64::engine::general_purpose::STANDARD
            .decode(&saml_data)
            .unwrap_or_default(),
    ))
    .unwrap_or(serde_json::json!({}));

    let session_index = decoded["session_index"].as_str();
    let name_id = decoded["name_id"].as_str();

    if let (Some(si), Some(ni)) = (session_index, name_id) {
        let result = saml_session::Entity::delete_many()
            .filter(saml_session::Column::SessionIndex.eq(si))
            .filter(saml_session::Column::NameId.eq(ni))
            .exec(&state.db)
            .await;

        info!(
            session_index = si,
            deleted = result.is_ok(),
            "SAML session terminated"
        );
    }

    let logout_response = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "version": "2.0",
        "issue_instant": Utc::now().to_rfc3339(),
        "status": "Success",
    });

    Ok(Json(SloResponse {
        logout_response: base64::engine::general_purpose::STANDARD
            .encode(logout_response.to_string()),
        status: "success".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/logout",
    request_body = InitiateLogoutRequest,
    responses((status = 200, body = InitiateLogoutResponse, description = "SAML logout initiated")),
    security(("bearer" = [])),
    tag = "saml"
)]
/// POST /api/v1/odo/auth/saml/logout — initiate SAML logout by session index.
pub async fn initiate_logout(
    State(state): State<Arc<AppState>>,
    Json(params): Json<InitiateLogoutRequest>,
) -> ApiResult<Json<InitiateLogoutResponse>> {
    let sess = saml_session::Entity::find()
        .filter(saml_session::Column::SessionIndex.eq(&params.session_index))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("saml session"))?;

    let idp = saml_idp_config::Entity::find_by_id(sess.idp_id)
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("saml idp"))?;

    let slo_url = idp
        .slo_url
        .as_deref()
        .ok_or(LocalError::not_found("saml slo_url"))?;

    let logout_request = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "version": "2.0",
        "issue_instant": Utc::now().to_rfc3339(),
        "destination": slo_url,
        // TODO: should be the session's SP entity_id once saml_session
        // records which SP initiated the login.
        "issuer": "odo-sp",
        "name_id": sess.name_id,
        "session_index": params.session_index,
    });

    // Mark the linked auth.session as expired
    session::Entity::update_many()
        .col_expr(session::Column::IsActive, Expr::value(false))
        .col_expr(session::Column::ExpiresAt, Expr::value(Utc::now()))
        .filter(session::Column::Id.eq(sess.session))
        .exec(&state.db)
        .await
        .ok();

    let encoded = base64::engine::general_purpose::STANDARD.encode(logout_request.to_string());

    Ok(Json(InitiateLogoutResponse {
        redirect_url: format!("{slo_url}?SAMLRequest={encoded}"),
        session_index: params.session_index,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/idps",
    request_body = ListIdpsRequest,
    responses((status = 200, body = ListIdpsResponse, description = "List of configured IdPs")),
    security(("bearer" = [])),
    tag = "saml"
)]
/// POST /api/v1/odo/auth/saml/idps — list configured IdPs (requires auth).
pub async fn list_idps(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ListIdpsRequest>,
) -> ApiResult<Json<ListIdpsResponse>> {
    let mut query = saml_idp_config::Entity::find().order_by_asc(saml_idp_config::Column::Name);

    if let Some(active) = params.is_active {
        query = query.filter(saml_idp_config::Column::IsActive.eq(active));
    }

    if let Some(limit) = params.limit {
        query = query.limit(limit);
    }
    if let Some(offset) = params.offset {
        query = query.offset(offset);
    }

    let idps: Vec<IdpResponse> = query
        .all(&state.db)
        .await?
        .into_iter()
        .map(|m| IdpResponse {
            id: m.id,
            entity_id: m.entity_id,
            name: m.name,
            sso_url: m.sso_url,
            slo_url: m.slo_url,
            metadata_url: m.metadata_url,
            is_active: m.is_active,
            session_lifetime_hours: m.session_lifetime_hours,
            attribute_mapping: m.attribute_mapping,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect();

    let count = idps.len();
    Ok(Json(ListIdpsResponse { idps, count }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/sso-configs",
    request_body = SsoConfigsRequest,
    responses((status = 200, body = ListSsoConfigsResponse, description = "Active SSO configurations for origin")),
    tag = "saml"
)]
/// POST /api/v1/odo/auth/saml/sso-configs — list active SSO configs for an origin.
pub async fn list_sso_configs(
    State(state): State<Arc<AppState>>,
    Json(params): Json<SsoConfigsRequest>,
) -> ApiResult<Json<ListSsoConfigsResponse>> {
    let sso_configs: Vec<SsoConfigItem> = saml_sp_config::Entity::find()
        .filter(saml_sp_config::Column::EntityId.eq(&params.origin))
        .filter(saml_sp_config::Column::IsActive.eq(true))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|sp| SsoConfigItem {
            sp_id: sp.id,
            label: sp.label,
            idp_id: sp.idp,
        })
        .collect();

    let count = sso_configs.len();
    Ok(Json(ListSsoConfigsResponse { sso_configs, count }))
}

#[utoipa::path(
    post,
    path = "/api/v1/odo/auth/saml/idp/get",
    request_body = GetIdpRequest,
    responses((status = 200, body = IdpResponse, description = "IdP configuration")),
    security(("bearer" = [])),
    tag = "saml"
)]
/// POST /api/v1/odo/auth/saml/idp/get — get a single IdP by entity ID (requires auth).
pub async fn get_idp(
    State(state): State<Arc<AppState>>,
    Json(params): Json<GetIdpRequest>,
) -> ApiResult<Json<IdpResponse>> {
    let idp = saml_idp_config::Entity::find()
        .filter(saml_idp_config::Column::EntityId.eq(&params.idp_id))
        .one(&state.db)
        .await?
        .ok_or(LocalError::not_found("saml idp"))?;

    Ok(Json(IdpResponse {
        id: idp.id,
        entity_id: idp.entity_id,
        name: idp.name,
        sso_url: idp.sso_url,
        slo_url: idp.slo_url,
        metadata_url: idp.metadata_url,
        is_active: idp.is_active,
        session_lifetime_hours: idp.session_lifetime_hours,
        attribute_mapping: idp.attribute_mapping,
        created_at: idp.created_at,
        updated_at: idp.updated_at,
    }))
}
