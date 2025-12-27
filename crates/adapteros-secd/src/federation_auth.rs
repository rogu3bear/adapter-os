use adapteros_boot::jti_cache::JtiCacheStore;
use adapteros_core::time;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const CLOCK_SKEW_TOLERANCE_SECS: i64 = 5;

/// Claims carried by federation JWTs/messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationClaims {
    pub iss: String,
    pub aud: String,
    pub jti: String,
    pub exp: i64,
    pub iat: i64,
}

/// Errors produced during federation token validation
#[derive(Debug, Error)]
pub enum FederationAuthError {
    #[error("Invalid federation token format")]
    InvalidFormat,
    #[error("Token decode failed: {0}")]
    Decode(String),
    #[error("Invalid signature length: {0}")]
    InvalidSignatureLength(usize),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Expired token")]
    Expired,
    #[error("Token not yet valid")]
    NotYetValid,
    #[error("Replay detected for jti {0}")]
    ReplayDetected(String),
}

pub type FederationAuthResult<T> = std::result::Result<T, FederationAuthError>;

/// Validate a federation JWT and enforce nonce replay defense using the shared JTI cache.
///
/// This mirrors worker-auth semantics but is scoped to federation control-plane traffic.
pub fn validate_federation_token(
    token: &str,
    verifying_key: &VerifyingKey,
    jti_cache: &mut JtiCacheStore,
) -> FederationAuthResult<FederationClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(FederationAuthError::InvalidFormat);
    }

    let message = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| FederationAuthError::Decode(e.to_string()))?;

    if sig_bytes.len() != 64 {
        return Err(FederationAuthError::InvalidSignatureLength(sig_bytes.len()));
    }

    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| FederationAuthError::InvalidSignatureLength(sig_bytes.len()))?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| FederationAuthError::InvalidSignature)?;

    let claims_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| FederationAuthError::Decode(e.to_string()))?;
    let claims: FederationClaims = serde_json::from_slice(&claims_bytes)
        .map_err(|e| FederationAuthError::Decode(e.to_string()))?;

    let now = time::unix_timestamp_secs() as i64;
    if claims.exp <= now {
        return Err(FederationAuthError::Expired);
    }

    if claims.iat > now + CLOCK_SKEW_TOLERANCE_SECS {
        return Err(FederationAuthError::NotYetValid);
    }

    if jti_cache.check_and_add(&claims.jti, claims.exp) {
        return Err(FederationAuthError::ReplayDetected(claims.jti));
    }

    Ok(claims)
}
