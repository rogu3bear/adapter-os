//! Authentication service for the supervisor using JWT with Ed25519

use adapteros_crypto::Keypair;
use adapteros_core::Result as CoreResult;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// JWT claims for service supervisor authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // service_id or user_id
    pub role: String, // "admin", "operator", "service"
    pub exp: i64,
    pub iat: i64,
    pub jti: String, // JWT ID for tracking
    pub nbf: i64,    // Not Before
    pub permissions: Vec<String>, // Specific permissions
}

/// Authentication service for the supervisor
pub struct AuthService {
    keypair: Arc<Keypair>,
    token_ttl: Duration,
}

impl AuthService {
    /// Create a new authentication service
    pub fn new(keypair: Keypair, token_ttl_hours: i64) -> Self {
        Self {
            keypair: Arc::new(keypair),
            token_ttl: Duration::hours(token_ttl_hours),
        }
    }

    /// Generate a JWT token for a service or user
    pub fn generate_token(
        &self,
        subject: &str,
        role: &str,
        permissions: Vec<String>,
    ) -> Result<String, SupervisorError> {
        let now = Utc::now();
        let exp = now + self.token_ttl;
        let nbf = now;

        // Generate unique JWT ID
        let jti = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(subject.as_bytes());
            hasher.update(&now.timestamp().to_le_bytes());
            hex::encode(hasher.finalize().as_bytes())
        };

        let claims = Claims {
            sub: subject.to_string(),
            role: role.to_string(),
            exp: exp.timestamp(),
            iat: now.timestamp(),
            jti,
            nbf: nbf.timestamp(),
            permissions,
        };

        // Use Ed25519 algorithm
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());

        let key_bytes = self.keypair.to_bytes();
        let token = encode(&header, &claims, &EncodingKey::from_ed_der(&key_bytes))?;

        info!("Generated JWT token for subject: {}", subject);
        Ok(token)
    }

    /// Validate and decode a JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims, SupervisorError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_nbf = true;
        validation.validate_exp = true;

        let public_key_der = self.keypair.public_key().to_bytes();
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_ed_der(&public_key_der),
            &validation,
        )?;

        Ok(token_data.claims)
    }

    /// Check if a token is about to expire (within 1 hour)
    pub fn token_needs_refresh(&self, claims: &Claims) -> bool {
        let now = Utc::now().timestamp();
        let time_until_expiry = claims.exp - now;
        time_until_expiry < 3600 // Less than 1 hour remaining
    }

    /// Refresh a token with new expiry
    pub fn refresh_token(&self, claims: &Claims) -> Result<String, SupervisorError> {
        self.generate_token(&claims.sub, &claims.role, claims.permissions.clone())
    }

    /// Check if the authenticated user has the required permission
    pub fn has_permission(&self, claims: &Claims, required_permission: &str) -> bool {
        claims.permissions.contains(&required_permission.to_string()) ||
        claims.permissions.contains(&"admin".to_string()) ||
        claims.role == "admin"
    }

    /// Get permissions for a role (factory method for common roles)
    pub fn permissions_for_role(role: &str) -> Vec<String> {
        match role {
            "admin" => vec![
                "services.read".to_string(),
                "services.write".to_string(),
                "services.start".to_string(),
                "services.stop".to_string(),
                "services.restart".to_string(),
                "system.read".to_string(),
                "system.write".to_string(),
            ],
            "operator" => vec![
                "services.read".to_string(),
                "services.start".to_string(),
                "services.stop".to_string(),
                "services.restart".to_string(),
                "system.read".to_string(),
            ],
            "viewer" => vec![
                "services.read".to_string(),
                "system.read".to_string(),
            ],
            "service" => vec![
                "services.read".to_string(),
                "services.write".to_string(),
            ],
            _ => vec![],
        }
    }
}

use crate::error::SupervisorError;
