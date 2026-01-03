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
