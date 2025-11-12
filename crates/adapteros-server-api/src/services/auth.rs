use axum::{http::StatusCode, Json};
use crate::types::ErrorResponse;
use crate::auth::Claims;
use adapteros_core::AosError;
use adapteros_crypto::providers::keychain;
use jsonwebtoken::{decode, DecodingKey, Validation};
use tracing::info;

pub fn require_role(claims: &Claims, required: &str) -> Result<(), AosError> {
    if !claims.roles.contains(&required.to_string()) {
        Err(AosError::PolicyViolation(format!("Role {} required", required)))
    } else {
        Ok(())
    }
}

pub fn require_any_role(claims: &Claims, roles: &[&str]) -> Result<(), AosError> {
    if claims.roles.iter().any(|r| roles.contains(&r.as_str())) {
        Ok(())
    } else {
        Err(AosError::PolicyViolation("No matching role".to_string()))
    }
}

// JWT decode with Ed25519 for production
pub fn decode_jwt(token: &str, key: &DecodingKey) -> Result<Claims, AosError> {
    let validation = Validation::new(jsonwebtoken::Algorithm::EdDSA);
    decode::<Claims>(token, key, &validation)
        .map(|data| data.claims)
        .map_err(|e| AosError::Crypto(format!("JWT decode failed: {}", e)))
}
