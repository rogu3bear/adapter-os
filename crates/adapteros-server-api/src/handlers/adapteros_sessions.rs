//! Session capability token minting endpoints.
//!
//! Provides `POST /v1/adapteros/sessions/mint` to issue session tokens
//! (aos_sess_v1) that bind to a specific adapter stack.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::{
    derive_kid_from_str, encode_ed25519_public_key_pem, AuthMode, Claims, PrincipalType, JWT_ISSUER,
};
use crate::session_tokens::{SessionTokenLockPayload, SESSION_TOKEN_PREFIX};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::{B3Hash, BackendKind};
use adapteros_id::{IdPrefix, TypedId};
use adapteros_types::coreml::CoreMLMode;
use axum::{extract::State, Extension, Json};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm as JwtAlgorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Default session token TTL: 15 minutes (same as access tokens)
const DEFAULT_SESSION_TOKEN_TTL_SECS: u64 = 15 * 60;

/// Maximum session token TTL: 24 hours
const MAX_SESSION_TOKEN_TTL_SECS: u64 = 24 * 60 * 60;

/// Request body for minting a session capability token.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MintSessionTokenRequest {
    /// Stack ID to bind the session token to (required)
    pub stack_id: String,
    /// Token TTL in seconds (default: 900 = 15 minutes, max: 86400 = 24 hours)
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    /// Backend profile to enforce (optional)
    #[serde(default)]
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode to enforce (optional)
    #[serde(default)]
    pub coreml_mode: Option<CoreMLMode>,
}

/// Response from minting a session capability token.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MintSessionTokenResponse {
    /// The minted session token (prefixed with aos_sess_v1)
    pub token: String,
    /// Token expiration time (ISO 8601)
    pub expires_at: String,
    /// Stack ID the token is bound to
    pub stack_id: String,
    /// JWT ID for revocation tracking
    pub jti: String,
}

/// JWT claims for session capability tokens.
///
/// This combines standard access token claims with a nested session_lock.
/// When parsed by middleware, it validates as Claims for auth, and
/// `decode_session_token_lock` extracts the session_lock.
#[derive(Debug, Clone, Serialize)]
struct SessionTokenClaims {
    // Standard JWT claims
    sub: String,
    #[serde(default)]
    email: String,
    role: String,
    #[serde(default)]
    roles: Vec<String>,
    tenant_id: String,
    #[serde(default)]
    admin_tenants: Vec<String>,
    exp: i64,
    iat: i64,
    jti: String,
    nbf: i64,
    iss: String,
    #[serde(default)]
    auth_mode: AuthMode,
    #[serde(default)]
    principal_type: Option<PrincipalType>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    mfa_level: Option<String>,
    #[serde(default)]
    rot_id: Option<String>,
    // Session lock fields (nested object for decode_session_token_lock)
    session_lock: SessionTokenLockPayload,
}

/// Encode a raw Ed25519 private key into PKCS#8 DER format for JWT signing.
fn encode_ed25519_pkcs8_der(raw_key: &[u8; 32]) -> Vec<u8> {
    // PKCS#8 header for Ed25519 (16 bytes prefix)
    let pkcs8_prefix: [u8; 16] = [
        0x30, 0x2e, // SEQUENCE, 46 bytes total
        0x02, 0x01, 0x00, // INTEGER 0 (version)
        0x30, 0x05, // SEQUENCE, 5 bytes
        0x06, 0x03, 0x2b, 0x65, 0x70, // OID 1.3.101.112 (Ed25519)
        0x04, 0x22, // OCTET STRING, 34 bytes
        0x04, 0x20, // OCTET STRING, 32 bytes (the key)
    ];

    let mut der = Vec::with_capacity(48);
    der.extend_from_slice(&pkcs8_prefix);
    der.extend_from_slice(raw_key);
    der
}

/// Mint a session capability token bound to a specific adapter stack.
///
/// The minted token:
/// - Has prefix `aos_sess_v1` followed by the JWT
/// - Contains stack binding (stack_id, stack_hash_b3, adapter_ids, pinned_adapter_ids)
/// - Is signed with the server's Ed25519 key
/// - Can be used for inference without adapter override permissions
#[utoipa::path(
    post,
    path = "/v1/adapteros/sessions/mint",
    request_body = MintSessionTokenRequest,
    responses(
        (status = 200, description = "Session token minted", body = MintSessionTokenResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Stack not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "sessions"
)]
pub async fn mint_session_token(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<MintSessionTokenRequest>,
) -> ApiResult<MintSessionTokenResponse> {
    // Validate stack_id
    if req.stack_id.is_empty() {
        return Err(ApiError::bad_request("stack_id is required"));
    }

    // Validate TTL
    let ttl_secs = req.ttl_seconds.unwrap_or(DEFAULT_SESSION_TOKEN_TTL_SECS);
    if ttl_secs == 0 {
        return Err(ApiError::bad_request("ttl_seconds must be positive"));
    }
    if ttl_secs > MAX_SESSION_TOKEN_TTL_SECS {
        return Err(ApiError::bad_request(format!(
            "ttl_seconds exceeds maximum ({})",
            MAX_SESSION_TOKEN_TTL_SECS
        )));
    }

    // Load the stack
    let stack = state
        .db
        .get_stack(&claims.tenant_id, &req.stack_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Stack"))?;

    // Parse adapter IDs from stack
    let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json)
        .map_err(|e| ApiError::internal(format!("Failed to parse stack adapter list: {}", e)))?;

    if adapter_ids.is_empty() {
        return Err(ApiError::bad_request("Stack has no adapters"));
    }

    // Compute stack hash
    let mut pairs = Vec::with_capacity(adapter_ids.len());
    for adapter_id in adapter_ids.iter() {
        let adapter = state
            .db
            .get_adapter_for_tenant(&claims.tenant_id, adapter_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found_msg(format!("Adapter '{}' not found", adapter_id))
            })?;

        let hash = B3Hash::from_hex(&adapter.hash_b3).map_err(|e| {
            ApiError::internal(format!("Invalid adapter hash for '{}': {}", adapter_id, e))
        })?;
        pairs.push((adapter_id.clone(), hash));
    }

    let stack_hash = adapteros_core::stack::compute_stack_hash(pairs);

    // Build session lock payload
    // Pinned adapters = full adapter set (all adapters are pinned for this session)
    let session_lock = SessionTokenLockPayload {
        stack_id: Some(req.stack_id.clone()),
        stack_hash_b3: Some(stack_hash.to_hex()),
        adapter_ids: Some(adapter_ids.clone()),
        pinned_adapter_ids: Some(adapter_ids),
        backend_profile: req.backend_profile,
        coreml_mode: req.coreml_mode,
    };

    // Generate token ID and timestamps
    let now = Utc::now();
    let exp = now + Duration::seconds(ttl_secs as i64);
    let jti = TypedId::new(IdPrefix::Tok).to_string();

    // Build session token claims
    let token_claims = SessionTokenClaims {
        sub: claims.sub.clone(),
        email: claims.email.clone(),
        role: claims.role.clone(),
        roles: claims.roles.clone(),
        tenant_id: claims.tenant_id.clone(),
        admin_tenants: claims.admin_tenants.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: jti.clone(),
        nbf: now.timestamp(),
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
        session_id: claims.session_id.clone(),
        device_id: claims.device_id.clone(),
        mfa_level: claims.mfa_level.clone(),
        rot_id: None,
        session_lock,
    };

    // Sign with Ed25519
    let mut header = Header::new(JwtAlgorithm::EdDSA);
    header.typ = Some("JWT".to_string());
    header.kid = Some(derive_kid_from_str(&encode_ed25519_public_key_pem(
        &state.ed25519_keypair.public_key().to_bytes(),
    )));

    let raw_key = state.ed25519_keypair.to_bytes();
    let der_key = encode_ed25519_pkcs8_der(&raw_key);
    let jwt = encode(&header, &token_claims, &EncodingKey::from_ed_der(&der_key))
        .map_err(|e| ApiError::internal(format!("Failed to sign session token: {}", e)))?;

    // Prefix with session token marker
    let token = format!("{}.{}", SESSION_TOKEN_PREFIX, jwt);

    Ok(Json(MintSessionTokenResponse {
        token,
        expires_at: exp.to_rfc3339(),
        stack_id: req.stack_id,
        jti,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_claims_serializes_with_lock() {
        let claims = SessionTokenClaims {
            sub: "user-123".to_string(),
            email: "test@example.com".to_string(),
            role: "viewer".to_string(),
            roles: vec!["viewer".to_string()],
            tenant_id: "tenant-abc".to_string(),
            admin_tenants: vec![],
            exp: 1234567890,
            iat: 1234567800,
            jti: "jti-xyz".to_string(),
            nbf: 1234567800,
            iss: "adapteros-server".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
            session_id: Some("sess-1".to_string()),
            device_id: None,
            mfa_level: None,
            rot_id: None,
            session_lock: SessionTokenLockPayload {
                stack_id: Some("stack-1".to_string()),
                stack_hash_b3: Some("abc123".to_string()),
                adapter_ids: Some(vec!["adapter-1".to_string()]),
                pinned_adapter_ids: Some(vec!["adapter-1".to_string()]),
                backend_profile: None,
                coreml_mode: None,
            },
        };

        let json = serde_json::to_string(&claims).expect("serialize");
        assert!(json.contains("session_lock"));
        assert!(json.contains("stack_id"));
        assert!(json.contains("stack_hash_b3"));
    }
}
