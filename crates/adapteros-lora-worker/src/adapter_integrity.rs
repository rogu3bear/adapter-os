//! Worker-side adapter integrity verification.
//!
//! Verifies that on-disk adapter bytes and manifest metadata match
//! the control plane intent (hash, base model, tier/scope).

use crate::galaxy_loader::AdapterLoadOutcome;
use adapteros_core::B3Hash;
use adapteros_manifest::{AdapterScope, AdapterTier};
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DEFAULT_VERIFY_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterIntegrityMode {
    Off,
    Warn,
    Enforce,
}

impl AdapterIntegrityMode {
    pub fn from_env() -> Self {
        match std::env::var("AOS_ADAPTER_VERIFY_MODE") {
            Ok(value) => match value.to_ascii_lowercase().as_str() {
                "off" | "disable" | "disabled" => Self::Off,
                "warn" => Self::Warn,
                "enforce" | "strict" | "reject" => Self::Enforce,
                _ => Self::default_for_build(),
            },
            Err(_) => Self::default_for_build(),
        }
    }

    fn default_for_build() -> Self {
        if cfg!(debug_assertions) {
            Self::Warn
        } else {
            Self::Enforce
        }
    }

    pub fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    pub fn is_enforce(self) -> bool {
        matches!(self, Self::Enforce)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterIntegrityReason {
    MissingAdapter,
    ManifestParseFailed,
    AdapterIdMismatch,
    BaseModelMismatch,
    TierViolation,
    ScopeViolation,
    HashMismatch,
    VerifyTimeout,
    StackHashMismatch,
}

impl AdapterIntegrityReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingAdapter => "missing_adapter",
            Self::ManifestParseFailed => "manifest_parse_failed",
            Self::AdapterIdMismatch => "adapter_id_mismatch",
            Self::BaseModelMismatch => "base_model_mismatch",
            Self::TierViolation => "tier_violation",
            Self::ScopeViolation => "scope_violation",
            Self::HashMismatch => "hash_mismatch",
            Self::VerifyTimeout => "verify_timeout",
            Self::StackHashMismatch => "stack_hash_mismatch",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterIntegrityError {
    pub adapter_id: String,
    pub reason: AdapterIntegrityReason,
    pub message: String,
    pub expected: Option<B3Hash>,
    pub actual: Option<B3Hash>,
}

impl std::fmt::Display for AdapterIntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AdapterIntegrityError {}

#[derive(Debug, Clone)]
pub struct AdapterManifestInfo {
    pub adapter_id: String,
    pub base_model: Option<String>,
    pub base_model_id: Option<String>,
    pub tier: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdapterVerification {
    pub weights_hash: B3Hash,
    pub manifest_hash: B3Hash,
    pub manifest_info: AdapterManifestInfo,
}

#[derive(Debug, Clone)]
pub struct ExpectedAdapterMetadata {
    pub tier: Option<AdapterTier>,
    pub scope: Option<AdapterScope>,
}

#[derive(Debug, Clone)]
struct FileIdentity {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct AdapterIntegrityCacheEntry {
    identity: FileIdentity,
    expected_hash: B3Hash,
    outcome: Result<AdapterVerification, AdapterIntegrityError>,
}

pub struct AdapterIntegrityVerifier {
    tenant_id: String,
    base_model: String,
    expected: HashMap<String, ExpectedAdapterMetadata>,
    cache: RwLock<HashMap<String, AdapterIntegrityCacheEntry>>,
    mode: AdapterIntegrityMode,
    timeout: Duration,
}

impl AdapterIntegrityVerifier {
    pub fn new(
        tenant_id: String,
        base_model: String,
        expected: HashMap<String, ExpectedAdapterMetadata>,
    ) -> Self {
        Self {
            tenant_id,
            base_model,
            expected,
            cache: RwLock::new(HashMap::new()),
            mode: AdapterIntegrityMode::from_env(),
            timeout: verify_timeout(),
        }
    }

    pub fn disabled(tenant_id: String) -> Self {
        Self {
            tenant_id,
            base_model: String::new(),
            expected: HashMap::new(),
            cache: RwLock::new(HashMap::new()),
            mode: AdapterIntegrityMode::Off,
            timeout: verify_timeout(),
        }
    }

    pub fn mode(&self) -> AdapterIntegrityMode {
        self.mode
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    pub fn expected_metadata(&self, adapter_id: &str) -> Option<ExpectedAdapterMetadata> {
        self.expected.get(adapter_id).cloned()
    }

    pub async fn verify_outcome(
        &self,
        adapter_id: &str,
        expected_hash: B3Hash,
        outcome: &AdapterLoadOutcome,
    ) -> Result<AdapterVerification, AdapterIntegrityError> {
        let identity = file_identity(outcome.backing.path())?;
        if let Some(cached) = self.cache_hit(adapter_id, expected_hash, &identity) {
            return cached;
        }

        let expected_meta = self.expected_metadata(adapter_id);
        let base_model = self.base_model.clone();
        let adapter_id = adapter_id.to_string();
        let adapter_id_for_task = adapter_id.clone();
        let adapter_id_for_timeout = adapter_id.clone();
        let manifest_range = outcome.view.manifest_range.clone();
        let payload_range = outcome.view.payload_range.clone();
        let backing = outcome.backing.clone();

        let verification = verify_with_timeout(
            self.timeout,
            move || {
                let manifest_bytes = backing.slice(&manifest_range);
                let payload_bytes = backing.slice(&payload_range);
                verify_manifest_and_hashes(
                    &adapter_id_for_task,
                    expected_hash,
                    &base_model,
                    expected_meta,
                    manifest_bytes,
                    payload_bytes,
                )
            },
            adapter_id_for_timeout,
        )
        .await;

        let should_cache = match &verification {
            Ok(_) => true,
            Err(err) => !matches!(err.reason, AdapterIntegrityReason::VerifyTimeout),
        };
        if should_cache {
            self.store_cache(adapter_id, expected_hash, identity, verification.clone());
        }

        verification
    }

    fn cache_hit(
        &self,
        adapter_id: &str,
        expected_hash: B3Hash,
        identity: &FileIdentity,
    ) -> Option<Result<AdapterVerification, AdapterIntegrityError>> {
        let cache = self.cache.read();
        let entry = cache.get(adapter_id)?;
        if entry.expected_hash != expected_hash {
            return None;
        }
        if !identity_matches(identity, &entry.identity) {
            return None;
        }
        Some(entry.outcome.clone())
    }

    fn store_cache(
        &self,
        adapter_id: String,
        expected_hash: B3Hash,
        identity: FileIdentity,
        outcome: Result<AdapterVerification, AdapterIntegrityError>,
    ) {
        let mut cache = self.cache.write();
        cache.insert(
            adapter_id,
            AdapterIntegrityCacheEntry {
                identity,
                expected_hash,
                outcome,
            },
        );
    }
}

fn verify_timeout() -> Duration {
    std::env::var("AOS_ADAPTER_VERIFY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(DEFAULT_VERIFY_TIMEOUT_MS))
}

fn identity_matches(lhs: &FileIdentity, rhs: &FileIdentity) -> bool {
    lhs.path == rhs.path && lhs.len == rhs.len && lhs.modified == rhs.modified
}

fn file_identity(path: &Path) -> Result<FileIdentity, AdapterIntegrityError> {
    let metadata = std::fs::metadata(path).map_err(|e| AdapterIntegrityError {
        adapter_id: path.to_string_lossy().to_string(),
        reason: AdapterIntegrityReason::MissingAdapter,
        message: format!("Adapter file metadata unavailable: {e}"),
        expected: None,
        actual: None,
    })?;
    let modified = metadata.modified().ok();
    Ok(FileIdentity {
        path: path.to_path_buf(),
        len: metadata.len(),
        modified,
    })
}

async fn verify_with_timeout<F>(
    timeout: Duration,
    verify_fn: F,
    adapter_id: String,
) -> Result<AdapterVerification, AdapterIntegrityError>
where
    F: FnOnce() -> Result<AdapterVerification, AdapterIntegrityError> + Send + 'static,
{
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(verify_fn)).await {
        Ok(join) => match join {
            Ok(result) => result,
            Err(e) => Err(AdapterIntegrityError {
                adapter_id,
                reason: AdapterIntegrityReason::ManifestParseFailed,
                message: format!("Adapter verification task failed: {e}"),
                expected: None,
                actual: None,
            }),
        },
        Err(_) => Err(AdapterIntegrityError {
            adapter_id,
            reason: AdapterIntegrityReason::VerifyTimeout,
            message: "Adapter verification timed out".to_string(),
            expected: None,
            actual: None,
        }),
    }
}

fn verify_manifest_and_hashes(
    adapter_id: &str,
    expected_hash: B3Hash,
    expected_base_model: &str,
    expected_meta: Option<ExpectedAdapterMetadata>,
    manifest_bytes: &[u8],
    payload_bytes: &[u8],
) -> Result<AdapterVerification, AdapterIntegrityError> {
    let manifest_hash = B3Hash::hash(manifest_bytes);
    let actual_hash = B3Hash::hash(payload_bytes);
    let manifest_info = match parse_manifest_info(adapter_id, manifest_bytes) {
        Ok(info) => info,
        Err(mut err) => {
            err.actual = Some(actual_hash);
            return Err(err);
        }
    };

    if manifest_info.adapter_id != adapter_id {
        return Err(AdapterIntegrityError {
            adapter_id: adapter_id.to_string(),
            reason: AdapterIntegrityReason::AdapterIdMismatch,
            message: format!(
                "Adapter manifest adapter_id '{}' does not match requested '{}'",
                manifest_info.adapter_id, adapter_id
            ),
            expected: None,
            actual: Some(actual_hash),
        });
    }

    if !expected_base_model.is_empty() {
        let base_model_matches = manifest_info
            .base_model
            .as_deref()
            .is_some_and(|actual| actual == expected_base_model)
            || manifest_info
                .base_model_id
                .as_deref()
                .is_some_and(|actual| actual == expected_base_model);

        if !base_model_matches {
            let actual = manifest_info
                .base_model
                .as_deref()
                .or(manifest_info.base_model_id.as_deref())
                .unwrap_or("<missing>");
            return Err(AdapterIntegrityError {
                adapter_id: adapter_id.to_string(),
                reason: AdapterIntegrityReason::BaseModelMismatch,
                message: format!(
                    "Adapter base_model '{}' does not match worker base '{}'",
                    actual, expected_base_model
                ),
                expected: None,
                actual: Some(actual_hash),
            });
        }
    }

    if let Some(expected) = expected_meta {
        if let Some(expected_tier) = expected.tier {
            let actual_tier = manifest_info.tier.as_deref().and_then(normalize_tier);
            if actual_tier != Some(expected_tier) {
                return Err(AdapterIntegrityError {
                    adapter_id: adapter_id.to_string(),
                    reason: AdapterIntegrityReason::TierViolation,
                    message: format!(
                        "Adapter tier '{:?}' does not match expected '{:?}'",
                        manifest_info.tier.as_deref(),
                        expected_tier
                    ),
                    expected: None,
                    actual: Some(actual_hash),
                });
            }
        }

        if let Some(expected_scope) = expected.scope {
            let actual_scope = manifest_info.scope.as_deref().and_then(normalize_scope);
            if actual_scope.as_ref() != Some(&expected_scope) {
                return Err(AdapterIntegrityError {
                    adapter_id: adapter_id.to_string(),
                    reason: AdapterIntegrityReason::ScopeViolation,
                    message: format!(
                        "Adapter scope '{:?}' does not match expected '{:?}'",
                        manifest_info.scope.as_deref(),
                        expected_scope
                    ),
                    expected: None,
                    actual: Some(actual_hash),
                });
            }
        }
    }

    if actual_hash != expected_hash {
        return Err(AdapterIntegrityError {
            adapter_id: adapter_id.to_string(),
            reason: AdapterIntegrityReason::HashMismatch,
            message: format!(
                "Adapter hash mismatch: expected {}, got {}",
                expected_hash.to_hex(),
                actual_hash.to_hex()
            ),
            expected: Some(expected_hash),
            actual: Some(actual_hash),
        });
    }

    Ok(AdapterVerification {
        weights_hash: actual_hash,
        manifest_hash,
        manifest_info,
    })
}

fn parse_manifest_info(
    adapter_id: &str,
    manifest_bytes: &[u8],
) -> Result<AdapterManifestInfo, AdapterIntegrityError> {
    let value: Value =
        serde_json::from_slice(manifest_bytes).map_err(|e| AdapterIntegrityError {
            adapter_id: adapter_id.to_string(),
            reason: AdapterIntegrityReason::ManifestParseFailed,
            message: format!("Adapter manifest parse failed: {e}"),
            expected: None,
            actual: None,
        })?;

    let adapter_id = value
        .get("adapter_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AdapterIntegrityError {
            adapter_id: adapter_id.to_string(),
            reason: AdapterIntegrityReason::ManifestParseFailed,
            message: "Adapter manifest missing adapter_id".to_string(),
            expected: None,
            actual: None,
        })?;

    let base_model = value
        .get("base_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_model_id = value
        .get("base_model_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let metadata = value.get("metadata");
    let tier = value
        .get("tier")
        .and_then(|v| v.as_str())
        .or_else(|| {
            metadata
                .and_then(|m| m.get("tier"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string());
    let scope = value
        .get("scope")
        .and_then(|v| v.as_str())
        .or_else(|| {
            metadata
                .and_then(|m| m.get("scope"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string());

    Ok(AdapterManifestInfo {
        adapter_id,
        base_model,
        base_model_id,
        tier,
        scope,
    })
}

fn normalize_tier(value: &str) -> Option<AdapterTier> {
    match value.to_ascii_lowercase().as_str() {
        "persistent" | "warm" => Some(AdapterTier::Persistent),
        "ephemeral" => Some(AdapterTier::Ephemeral),
        _ => None,
    }
}

fn normalize_scope(value: &str) -> Option<AdapterScope> {
    match value.to_ascii_lowercase().as_str() {
        "global" => Some(AdapterScope::Global),
        "tenant" => Some(AdapterScope::Tenant),
        "repo" | "repository" | "project" => Some(AdapterScope::Repo),
        "commit" => Some(AdapterScope::Commit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Helper: create a manifest JSON for testing
    fn make_manifest(
        adapter_id: &str,
        base_model: Option<&str>,
        tier: Option<&str>,
        scope: Option<&str>,
    ) -> Vec<u8> {
        let mut obj = serde_json::json!({
            "adapter_id": adapter_id,
        });
        if let Some(bm) = base_model {
            obj["base_model"] = serde_json::json!(bm);
        }
        if let Some(t) = tier {
            obj["tier"] = serde_json::json!(t);
        }
        if let Some(s) = scope {
            obj["scope"] = serde_json::json!(s);
        }
        serde_json::to_vec(&obj).unwrap()
    }

    // ========================================================================
    // Hash Mismatch Detection Tests
    // ========================================================================

    #[test]
    fn test_hash_mismatch_detected() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, None);
        let payload = b"original payload content";
        let expected_hash = B3Hash::hash(payload);

        // Tamper with payload (single byte change)
        let tampered_payload = b"original payload contenT"; // last byte changed
        let actual_hash = B3Hash::hash(tampered_payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            None,
            &manifest,
            tampered_payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::HashMismatch);
        assert_eq!(err.expected, Some(expected_hash));
        assert_eq!(err.actual, Some(actual_hash));
        assert!(err.message.contains("hash mismatch"));
    }

    #[test]
    fn test_hash_match_succeeds() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, None);
        let payload = b"valid payload content";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            None,
            &manifest,
            payload,
        );

        let verification = result.unwrap();
        assert_eq!(verification.weights_hash, expected_hash);
        assert_eq!(verification.manifest_info.adapter_id, adapter_id);
    }

    #[test]
    fn test_hash_includes_both_hashes_in_error() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, None);
        let original = b"original";
        let tampered = b"tampered";
        let expected_hash = B3Hash::hash(original);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            None,
            &manifest,
            tampered,
        );

        let err = result.unwrap_err();
        // Both hashes must be present for forensic investigation
        assert!(err.expected.is_some(), "Expected hash missing from error");
        assert!(err.actual.is_some(), "Actual hash missing from error");
        assert_ne!(err.expected, err.actual, "Hashes should differ");
    }

    // ========================================================================
    // Adapter ID Mismatch Tests
    // ========================================================================

    #[test]
    fn test_adapter_id_mismatch_detected() {
        let manifest = make_manifest("wrong-adapter-id", Some("Qwen2.5"), None, None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            "expected-adapter-id",
            expected_hash,
            "Qwen2.5",
            None,
            &manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::AdapterIdMismatch);
        assert!(err.message.contains("wrong-adapter-id"));
        assert!(err.message.contains("expected-adapter-id"));
    }

    // ========================================================================
    // Base Model Mismatch Tests
    // ========================================================================

    #[test]
    fn test_base_model_mismatch_detected() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Llama-3-8B"), None, None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5-7B-Instruct", // Different base model
            None,
            &manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::BaseModelMismatch);
        assert!(err.message.contains("Llama-3-8B"));
        assert!(err.message.contains("Qwen2.5-7B-Instruct"));
    }

    #[test]
    fn test_base_model_id_alternative_field() {
        let adapter_id = "test-adapter";
        // Use base_model_id instead of base_model
        let manifest_json = serde_json::json!({
            "adapter_id": adapter_id,
            "base_model_id": "Qwen2.5"
        });
        let manifest = serde_json::to_vec(&manifest_json).unwrap();
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5", // Matches base_model_id
            None,
            &manifest,
            payload,
        );

        assert!(result.is_ok(), "base_model_id should be accepted");
    }

    #[test]
    fn test_empty_expected_base_model_skips_check() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("AnyModel"), None, None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "", // Empty = skip base model check
            None,
            &manifest,
            payload,
        );

        assert!(result.is_ok(), "Empty base model should skip check");
    }

    // ========================================================================
    // Tier Violation Tests
    // ========================================================================

    #[test]
    fn test_tier_violation_ephemeral_vs_persistent() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), Some("ephemeral"), None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let expected_meta = ExpectedAdapterMetadata {
            tier: Some(AdapterTier::Persistent),
            scope: None,
        };

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            Some(expected_meta),
            &manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::TierViolation);
    }

    #[test]
    fn test_tier_normalization_case_insensitive() {
        let adapter_id = "test-adapter";
        // PERSISTENT in uppercase
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), Some("PERSISTENT"), None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let expected_meta = ExpectedAdapterMetadata {
            tier: Some(AdapterTier::Persistent),
            scope: None,
        };

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            Some(expected_meta),
            &manifest,
            payload,
        );

        assert!(
            result.is_ok(),
            "Tier normalization should be case-insensitive"
        );
    }

    #[test]
    fn test_tier_warm_alias_for_persistent() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), Some("warm"), None);
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let expected_meta = ExpectedAdapterMetadata {
            tier: Some(AdapterTier::Persistent),
            scope: None,
        };

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            Some(expected_meta),
            &manifest,
            payload,
        );

        assert!(result.is_ok(), "'warm' should map to Persistent tier");
    }

    // ========================================================================
    // Scope Violation Tests (Multi-Tenant Isolation Critical)
    // ========================================================================

    #[test]
    fn test_scope_violation_tenant_vs_global() {
        let adapter_id = "test-adapter";
        let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, Some("tenant"));
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let expected_meta = ExpectedAdapterMetadata {
            tier: None,
            scope: Some(AdapterScope::Global),
        };

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            Some(expected_meta),
            &manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::ScopeViolation);
    }

    #[test]
    fn test_scope_repo_aliases() {
        // "repo", "repository", "project" should all map to AdapterScope::Repo
        for alias in ["repo", "repository", "project"] {
            let adapter_id = "test-adapter";
            let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, Some(alias));
            let payload = b"payload";
            let expected_hash = B3Hash::hash(payload);

            let expected_meta = ExpectedAdapterMetadata {
                tier: None,
                scope: Some(AdapterScope::Repo),
            };

            let result = verify_manifest_and_hashes(
                adapter_id,
                expected_hash,
                "Qwen2.5",
                Some(expected_meta),
                &manifest,
                payload,
            );

            assert!(result.is_ok(), "Scope alias '{}' should map to Repo", alias);
        }
    }

    #[test]
    fn test_all_scope_values() {
        let scopes = [
            ("global", AdapterScope::Global),
            ("tenant", AdapterScope::Tenant),
            ("repo", AdapterScope::Repo),
            ("commit", AdapterScope::Commit),
        ];

        for (scope_str, expected_scope) in scopes {
            let adapter_id = "test-adapter";
            let manifest = make_manifest(adapter_id, Some("Qwen2.5"), None, Some(scope_str));
            let payload = b"payload";
            let expected_hash = B3Hash::hash(payload);

            let expected_meta = ExpectedAdapterMetadata {
                tier: None,
                scope: Some(expected_scope.clone()),
            };

            let result = verify_manifest_and_hashes(
                adapter_id,
                expected_hash,
                "Qwen2.5",
                Some(expected_meta),
                &manifest,
                payload,
            );

            assert!(
                result.is_ok(),
                "Scope '{}' should match {:?}",
                scope_str,
                expected_scope
            );
        }
    }

    // ========================================================================
    // Manifest Parse Error Tests
    // ========================================================================

    #[test]
    fn test_manifest_parse_invalid_json() {
        let adapter_id = "test-adapter";
        let invalid_manifest = b"not valid json {{{";
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            adapter_id,
            expected_hash,
            "Qwen2.5",
            None,
            invalid_manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::ManifestParseFailed);
        assert!(err.message.contains("parse failed"));
    }

    #[test]
    fn test_manifest_missing_adapter_id() {
        let manifest_json = serde_json::json!({
            "base_model": "Qwen2.5"
            // Missing adapter_id
        });
        let manifest = serde_json::to_vec(&manifest_json).unwrap();
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let result = verify_manifest_and_hashes(
            "test-adapter",
            expected_hash,
            "Qwen2.5",
            None,
            &manifest,
            payload,
        );

        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::ManifestParseFailed);
        assert!(err.message.contains("missing adapter_id"));
    }

    #[test]
    fn test_manifest_metadata_nested_tier_scope() {
        // Tier and scope can be nested under "metadata"
        let manifest_json = serde_json::json!({
            "adapter_id": "test-adapter",
            "base_model": "Qwen2.5",
            "metadata": {
                "tier": "persistent",
                "scope": "global"
            }
        });
        let manifest = serde_json::to_vec(&manifest_json).unwrap();
        let payload = b"payload";
        let expected_hash = B3Hash::hash(payload);

        let expected_meta = ExpectedAdapterMetadata {
            tier: Some(AdapterTier::Persistent),
            scope: Some(AdapterScope::Global),
        };

        let result = verify_manifest_and_hashes(
            "test-adapter",
            expected_hash,
            "Qwen2.5",
            Some(expected_meta),
            &manifest,
            payload,
        );

        assert!(
            result.is_ok(),
            "Nested metadata tier/scope should be accepted"
        );
    }

    // ========================================================================
    // Mode Configuration Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_mode_from_env_off_variants() {
        for variant in ["off", "disable", "disabled", "OFF", "DISABLE"] {
            std::env::set_var("AOS_ADAPTER_VERIFY_MODE", variant);
            let mode = AdapterIntegrityMode::from_env();
            assert_eq!(
                mode,
                AdapterIntegrityMode::Off,
                "Variant '{}' should be Off",
                variant
            );
        }
        std::env::remove_var("AOS_ADAPTER_VERIFY_MODE");
    }

    #[test]
    #[serial]
    fn test_mode_from_env_enforce_variants() {
        for variant in ["enforce", "strict", "reject", "ENFORCE"] {
            std::env::set_var("AOS_ADAPTER_VERIFY_MODE", variant);
            let mode = AdapterIntegrityMode::from_env();
            assert_eq!(
                mode,
                AdapterIntegrityMode::Enforce,
                "Variant '{}' should be Enforce",
                variant
            );
        }
        std::env::remove_var("AOS_ADAPTER_VERIFY_MODE");
    }

    #[test]
    #[serial]
    fn test_mode_from_env_warn() {
        std::env::set_var("AOS_ADAPTER_VERIFY_MODE", "warn");
        let mode = AdapterIntegrityMode::from_env();
        assert_eq!(mode, AdapterIntegrityMode::Warn);
        std::env::remove_var("AOS_ADAPTER_VERIFY_MODE");
    }

    #[test]
    fn test_mode_is_off() {
        assert!(AdapterIntegrityMode::Off.is_off());
        assert!(!AdapterIntegrityMode::Warn.is_off());
        assert!(!AdapterIntegrityMode::Enforce.is_off());
    }

    #[test]
    fn test_mode_is_enforce() {
        assert!(!AdapterIntegrityMode::Off.is_enforce());
        assert!(!AdapterIntegrityMode::Warn.is_enforce());
        assert!(AdapterIntegrityMode::Enforce.is_enforce());
    }

    // ========================================================================
    // Verifier Construction Tests
    // ========================================================================

    #[test]
    fn test_verifier_disabled_mode() {
        let verifier = AdapterIntegrityVerifier::disabled("test-tenant".to_string());
        assert_eq!(verifier.mode(), AdapterIntegrityMode::Off);
        assert_eq!(verifier.tenant_id(), "test-tenant");
    }

    #[test]
    fn test_verifier_expected_metadata_lookup() {
        let mut expected = HashMap::new();
        expected.insert(
            "adapter-1".to_string(),
            ExpectedAdapterMetadata {
                tier: Some(AdapterTier::Persistent),
                scope: Some(AdapterScope::Tenant),
            },
        );

        let verifier =
            AdapterIntegrityVerifier::new("tenant-1".to_string(), "Qwen2.5".to_string(), expected);

        let meta = verifier.expected_metadata("adapter-1");
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().tier, Some(AdapterTier::Persistent));

        let missing = verifier.expected_metadata("nonexistent");
        assert!(missing.is_none());
    }

    // ========================================================================
    // File Identity Tests
    // ========================================================================

    #[test]
    fn test_file_identity_missing_file() {
        let result = file_identity(Path::new("/nonexistent/path/to/adapter.aos"));
        let err = result.unwrap_err();
        assert_eq!(err.reason, AdapterIntegrityReason::MissingAdapter);
    }

    #[test]
    fn test_identity_matches_same_file() {
        let id1 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let id2 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        assert!(identity_matches(&id1, &id2));
    }

    #[test]
    fn test_identity_mismatch_different_size() {
        let id1 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let id2 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 2048, // Different size
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        assert!(!identity_matches(&id1, &id2));
    }

    #[test]
    fn test_identity_mismatch_different_mtime() {
        let id1 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let id2 = FileIdentity {
            path: PathBuf::from("/test/path.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)), // Different mtime
        };
        assert!(!identity_matches(&id1, &id2));
    }

    #[test]
    fn test_identity_mismatch_different_path() {
        let id1 = FileIdentity {
            path: PathBuf::from("/test/path1.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        let id2 = FileIdentity {
            path: PathBuf::from("/test/path2.aos"),
            len: 1024,
            modified: Some(SystemTime::UNIX_EPOCH),
        };
        assert!(!identity_matches(&id1, &id2));
    }

    // ========================================================================
    // Verify Timeout Configuration Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_verify_timeout_from_env() {
        std::env::set_var("AOS_ADAPTER_VERIFY_TIMEOUT_MS", "100");
        let timeout = verify_timeout();
        assert_eq!(timeout, Duration::from_millis(100));
        std::env::remove_var("AOS_ADAPTER_VERIFY_TIMEOUT_MS");
    }

    #[test]
    #[serial]
    fn test_verify_timeout_default() {
        std::env::remove_var("AOS_ADAPTER_VERIFY_TIMEOUT_MS");
        let timeout = verify_timeout();
        assert_eq!(timeout, Duration::from_millis(DEFAULT_VERIFY_TIMEOUT_MS));
    }

    #[test]
    #[serial]
    fn test_verify_timeout_invalid_env() {
        std::env::set_var("AOS_ADAPTER_VERIFY_TIMEOUT_MS", "not-a-number");
        let timeout = verify_timeout();
        assert_eq!(timeout, Duration::from_millis(DEFAULT_VERIFY_TIMEOUT_MS));
        std::env::remove_var("AOS_ADAPTER_VERIFY_TIMEOUT_MS");
    }

    // ========================================================================
    // Reason String Conversion Tests
    // ========================================================================

    #[test]
    fn test_reason_as_str() {
        assert_eq!(
            AdapterIntegrityReason::MissingAdapter.as_str(),
            "missing_adapter"
        );
        assert_eq!(
            AdapterIntegrityReason::ManifestParseFailed.as_str(),
            "manifest_parse_failed"
        );
        assert_eq!(
            AdapterIntegrityReason::AdapterIdMismatch.as_str(),
            "adapter_id_mismatch"
        );
        assert_eq!(
            AdapterIntegrityReason::BaseModelMismatch.as_str(),
            "base_model_mismatch"
        );
        assert_eq!(
            AdapterIntegrityReason::TierViolation.as_str(),
            "tier_violation"
        );
        assert_eq!(
            AdapterIntegrityReason::ScopeViolation.as_str(),
            "scope_violation"
        );
        assert_eq!(
            AdapterIntegrityReason::HashMismatch.as_str(),
            "hash_mismatch"
        );
        assert_eq!(
            AdapterIntegrityReason::VerifyTimeout.as_str(),
            "verify_timeout"
        );
        assert_eq!(
            AdapterIntegrityReason::StackHashMismatch.as_str(),
            "stack_hash_mismatch"
        );
    }

    // ========================================================================
    // Error Display Tests
    // ========================================================================

    #[test]
    fn test_error_display() {
        let err = AdapterIntegrityError {
            adapter_id: "test".to_string(),
            reason: AdapterIntegrityReason::HashMismatch,
            message: "Test error message".to_string(),
            expected: None,
            actual: None,
        };
        assert_eq!(format!("{}", err), "Test error message");
    }

    // ========================================================================
    // Normalize Function Unit Tests
    // ========================================================================

    #[test]
    fn test_normalize_tier_all_values() {
        assert_eq!(normalize_tier("persistent"), Some(AdapterTier::Persistent));
        assert_eq!(normalize_tier("PERSISTENT"), Some(AdapterTier::Persistent));
        assert_eq!(normalize_tier("warm"), Some(AdapterTier::Persistent));
        assert_eq!(normalize_tier("ephemeral"), Some(AdapterTier::Ephemeral));
        assert_eq!(normalize_tier("EPHEMERAL"), Some(AdapterTier::Ephemeral));
        assert_eq!(normalize_tier("invalid"), None);
    }

    #[test]
    fn test_normalize_scope_all_values() {
        assert_eq!(normalize_scope("global"), Some(AdapterScope::Global));
        assert_eq!(normalize_scope("GLOBAL"), Some(AdapterScope::Global));
        assert_eq!(normalize_scope("tenant"), Some(AdapterScope::Tenant));
        assert_eq!(normalize_scope("repo"), Some(AdapterScope::Repo));
        assert_eq!(normalize_scope("repository"), Some(AdapterScope::Repo));
        assert_eq!(normalize_scope("project"), Some(AdapterScope::Repo));
        assert_eq!(normalize_scope("commit"), Some(AdapterScope::Commit));
        assert_eq!(normalize_scope("invalid"), None);
    }
}
