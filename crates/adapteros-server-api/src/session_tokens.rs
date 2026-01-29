//! Session token parsing and enforcement helpers.
//!
//! Session tokens are versioned bearer tokens prefixed with `aos_sess_v1` and
//! carry adapter-lock constraints for inference endpoints.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::state::AppState;
use adapteros_core::stack::compute_stack_hash;
use adapteros_core::{B3Hash, BackendKind};
use adapteros_types::coreml::CoreMLMode;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;

pub const SESSION_TOKEN_PREFIX: &str = "aos_sess_v1";

#[derive(Debug, Clone)]
pub struct SessionTokenContext {
    pub lock: SessionTokenLockPayload,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionTokenLockPayload {
    #[serde(default, alias = "stack_id", alias = "stack")]
    pub stack_id: Option<String>,
    #[serde(
        default,
        alias = "stack_hash_b3",
        alias = "stack_hash",
        alias = "stack_manifest_digest_b3",
        alias = "stack_manifest_hash_b3",
        alias = "stack_digest_b3",
        alias = "stack_digest"
    )]
    pub stack_hash_b3: Option<String>,
    #[serde(default, alias = "adapter_ids", alias = "adapters")]
    pub adapter_ids: Option<Vec<String>>,
    #[serde(default, alias = "pinned_adapter_ids", alias = "pinned_adapters", alias = "pinned")]
    pub pinned_adapter_ids: Option<Vec<String>>,
    #[serde(default, alias = "backend", alias = "backend_profile")]
    pub backend_profile: Option<BackendKind>,
    #[serde(default, alias = "coreml_mode", alias = "compute_mode", alias = "coreml")]
    pub coreml_mode: Option<CoreMLMode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionTokenPayload {
    #[serde(default)]
    session_lock: Option<SessionTokenLockPayload>,
    #[serde(flatten)]
    flat: SessionTokenLockPayload,
}

impl SessionTokenLockPayload {
    fn overlay(mut self, other: SessionTokenLockPayload) -> Self {
        if other.stack_id.is_some() {
            self.stack_id = other.stack_id;
        }
        if other.stack_hash_b3.is_some() {
            self.stack_hash_b3 = other.stack_hash_b3;
        }
        if other.adapter_ids.is_some() {
            self.adapter_ids = other.adapter_ids;
        }
        if other.pinned_adapter_ids.is_some() {
            self.pinned_adapter_ids = other.pinned_adapter_ids;
        }
        if other.backend_profile.is_some() {
            self.backend_profile = other.backend_profile;
        }
        if other.coreml_mode.is_some() {
            self.coreml_mode = other.coreml_mode;
        }
        self
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSessionTokenLock {
    pub stack_id: Option<String>,
    pub stack_hash_b3: B3Hash,
    pub adapter_ids: Vec<String>,
    pub pinned_adapter_ids: Vec<String>,
    pub backend_profile: Option<BackendKind>,
    pub coreml_mode: Option<CoreMLMode>,
}

pub fn strip_session_token_prefix(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if !trimmed.starts_with(SESSION_TOKEN_PREFIX) {
        return None;
    }
    let rest = trimmed[SESSION_TOKEN_PREFIX.len()..]
        .trim_start_matches(|c| matches!(c, '.' | ':' | '_' | '-'));
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

pub fn decode_session_token_lock(token: &str) -> Result<SessionTokenLockPayload, String> {
    let payload = decode_jwt_payload(token)?;
    let mut lock = payload.flat;
    if let Some(nested) = payload.session_lock {
        lock = lock.overlay(nested);
    }
    Ok(lock)
}

fn decode_jwt_payload(token: &str) -> Result<SessionTokenPayload, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("session token must be a JWT".to_string());
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| "session token payload is not valid base64url".to_string())?;
    serde_json::from_slice::<SessionTokenPayload>(&payload_bytes)
        .map_err(|_| "session token payload is not valid JSON".to_string())
}

pub fn ensure_no_adapter_overrides(fields: &[(&'static str, bool)]) -> Result<(), ApiError> {
    let blocked: Vec<&'static str> = fields
        .iter()
        .filter_map(|(name, present)| present.then_some(*name))
        .collect();
    if blocked.is_empty() {
        return Ok(());
    }
    Err(ApiError::forbidden("session token forbids adapter overrides")
        .with_details(format!("blocked_fields: {}", blocked.join(", "))))
}

pub async fn resolve_session_token_lock(
    state: &AppState,
    claims: &Claims,
    lock: &SessionTokenLockPayload,
) -> Result<ResolvedSessionTokenLock, ApiError> {
    let stack_hash_raw = lock.stack_hash_b3.as_deref().ok_or_else(|| {
        ApiError::forbidden("session token missing stack_hash_b3")
            .with_details("stack_hash_b3 is required for session tokens")
    })?;
    let expected_stack_hash = parse_b3_hash(stack_hash_raw, "stack_hash_b3")?;

    let pinned_adapter_ids = lock.pinned_adapter_ids.clone().ok_or_else(|| {
        ApiError::forbidden("session token missing pinned_adapter_ids")
            .with_details("pinned_adapter_ids is required for session tokens")
    })?;

    let adapter_ids = if let Some(ids) = lock.adapter_ids.clone() {
        ids
    } else if let Some(stack_id) = lock.stack_id.as_deref() {
        let stack = state
            .db
            .get_stack(&claims.tenant_id, stack_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| ApiError::not_found("Stack"))?;
        serde_json::from_str::<Vec<String>>(&stack.adapter_ids_json)
            .map_err(|e| ApiError::bad_request("stack adapter list invalid").with_details(e.to_string()))?
    } else {
        return Err(ApiError::forbidden("session token missing stack binding")
            .with_details("stack_id or adapter_ids is required for session tokens"));
    };

    if adapter_ids.is_empty() {
        return Err(ApiError::forbidden("session token adapter set is empty"));
    }

    let missing_pins: Vec<String> = pinned_adapter_ids
        .iter()
        .filter(|id| !adapter_ids.iter().any(|candidate| candidate == *id))
        .cloned()
        .collect();
    if !missing_pins.is_empty() {
        return Err(ApiError::forbidden("session token pins outside adapter set")
            .with_details(format!(
                "pinned adapters not in adapter set: {}",
                missing_pins.join(", ")
            )));
    }

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
        let hash = parse_b3_hash(&adapter.hash_b3, "adapter hash")?;
        pairs.push((adapter_id.clone(), hash));
    }

    let actual_stack_hash = compute_stack_hash(pairs);
    if actual_stack_hash != expected_stack_hash {
        return Err(ApiError::forbidden("session token stack hash mismatch").with_details(
            format!(
                "expected {}, got {}",
                expected_stack_hash.to_hex(),
                actual_stack_hash.to_hex()
            ),
        ));
    }

    Ok(ResolvedSessionTokenLock {
        stack_id: lock.stack_id.clone(),
        stack_hash_b3: expected_stack_hash,
        adapter_ids,
        pinned_adapter_ids,
        backend_profile: lock.backend_profile,
        coreml_mode: lock.coreml_mode,
    })
}

fn parse_b3_hash(raw: &str, field: &'static str) -> Result<B3Hash, ApiError> {
    let cleaned = raw.trim().strip_prefix("b3:").unwrap_or(raw.trim());
    B3Hash::from_hex(cleaned).ok_or_else(|| {
        ApiError::bad_request(format!("invalid {}", field))
            .with_details(format!("expected hex BLAKE3 digest, got '{}'", raw))
    })
}
