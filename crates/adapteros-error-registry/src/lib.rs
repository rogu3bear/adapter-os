//! Unified Error Registry for AdapterOS
//!
//! This crate provides a centralized error code registry with:
//! - Compile-time exhaustiveness checking for AosError → ECode mappings
//! - Machine-readable recovery actions with safety levels
//! - Structured error metadata (cause, fix steps, related docs)
//!
//! ## Design Goals
//!
//! 1. **Single source of truth**: All error codes defined in one place
//! 2. **Compile-time safety**: No string-based lookups that can drift
//! 3. **Executable recovery**: Actions can be auto-executed based on safety level
//! 4. **Backward compatible**: Preserves existing ECode values

pub mod recovery;

pub use recovery::{FixSafety, RecoveryAction};
pub use strum::IntoEnumIterator;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::EnumIter;

// =============================================================================
// ECode: Typed Error Codes
// =============================================================================

/// Typed error codes for compile-time checking.
///
/// Categories:
/// - E1xxx: Crypto/Signing errors
/// - E2xxx: Policy/Determinism violations
/// - E3xxx: Kernels/Build/Manifest issues
/// - E4xxx: Telemetry/Chain problems
/// - E5xxx: Artifacts/CAS errors
/// - E6xxx: Adapters/DIR issues
/// - E7xxx: Node/Cluster problems
/// - E8xxx: CLI/Config errors
/// - E9xxx: OS/Environment issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
#[allow(non_camel_case_types)]
pub enum ECode {
    // E1xxx: Crypto/Signing
    E1001,
    E1002,
    E1003,
    E1004,

    // E2xxx: Policy/Determinism
    E2001,
    E2002,
    E2003,
    E2004,

    // E3xxx: Kernels/Build/Manifest
    E3001,
    E3002,
    E3003,
    E3004,
    E3005,
    E3006,
    E3007,
    E3008,
    E3009,

    // E4xxx: Telemetry/Chain
    E4001,
    E4002,
    E4003,

    // E5xxx: Artifacts/CAS
    E5001,
    E5002,
    E5003,
    E5004,

    // E6xxx: Adapters/DIR
    E6001,
    E6002,
    E6003,
    E6004,
    E6005,
    E6006,
    E6007,
    E6008,
    E6009,

    // E7xxx: Node/Cluster
    E7001,
    E7002,

    // E8xxx: CLI/Config
    E8001,
    E8002,
    E8003,
    E8004,
    E8005,
    E8006,
    E8007,
    E8008,
    E8009,
    E8010,
    E8011,
    E8012,
    E8013,

    // E9xxx: OS/Environment
    E9001,
    E9002,
    E9003,
    E9004,
    E9005,
    E9006,
    E9007,
    E9008,
    E9009,
}

impl ECode {
    /// Get the string representation (e.g., "E1001")
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E1001 => "E1001",
            Self::E1002 => "E1002",
            Self::E1003 => "E1003",
            Self::E1004 => "E1004",
            Self::E2001 => "E2001",
            Self::E2002 => "E2002",
            Self::E2003 => "E2003",
            Self::E2004 => "E2004",
            Self::E3001 => "E3001",
            Self::E3002 => "E3002",
            Self::E3003 => "E3003",
            Self::E3004 => "E3004",
            Self::E3005 => "E3005",
            Self::E3006 => "E3006",
            Self::E3007 => "E3007",
            Self::E3008 => "E3008",
            Self::E3009 => "E3009",
            Self::E4001 => "E4001",
            Self::E4002 => "E4002",
            Self::E4003 => "E4003",
            Self::E5001 => "E5001",
            Self::E5002 => "E5002",
            Self::E5003 => "E5003",
            Self::E5004 => "E5004",
            Self::E6001 => "E6001",
            Self::E6002 => "E6002",
            Self::E6003 => "E6003",
            Self::E6004 => "E6004",
            Self::E6005 => "E6005",
            Self::E6006 => "E6006",
            Self::E6007 => "E6007",
            Self::E6008 => "E6008",
            Self::E6009 => "E6009",
            Self::E7001 => "E7001",
            Self::E7002 => "E7002",
            Self::E8001 => "E8001",
            Self::E8002 => "E8002",
            Self::E8003 => "E8003",
            Self::E8004 => "E8004",
            Self::E8005 => "E8005",
            Self::E8006 => "E8006",
            Self::E8007 => "E8007",
            Self::E8008 => "E8008",
            Self::E8009 => "E8009",
            Self::E8010 => "E8010",
            Self::E8011 => "E8011",
            Self::E8012 => "E8012",
            Self::E8013 => "E8013",
            Self::E9001 => "E9001",
            Self::E9002 => "E9002",
            Self::E9003 => "E9003",
            Self::E9004 => "E9004",
            Self::E9005 => "E9005",
            Self::E9006 => "E9006",
            Self::E9007 => "E9007",
            Self::E9008 => "E9008",
            Self::E9009 => "E9009",
        }
    }

    /// Parse from string (e.g., "E1001" -> Some(ECode::E1001))
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "E1001" => Some(Self::E1001),
            "E1002" => Some(Self::E1002),
            "E1003" => Some(Self::E1003),
            "E1004" => Some(Self::E1004),
            "E2001" => Some(Self::E2001),
            "E2002" => Some(Self::E2002),
            "E2003" => Some(Self::E2003),
            "E2004" => Some(Self::E2004),
            "E3001" => Some(Self::E3001),
            "E3002" => Some(Self::E3002),
            "E3003" => Some(Self::E3003),
            "E3004" => Some(Self::E3004),
            "E3005" => Some(Self::E3005),
            "E3006" => Some(Self::E3006),
            "E3007" => Some(Self::E3007),
            "E3008" => Some(Self::E3008),
            "E3009" => Some(Self::E3009),
            "E4001" => Some(Self::E4001),
            "E4002" => Some(Self::E4002),
            "E4003" => Some(Self::E4003),
            "E5001" => Some(Self::E5001),
            "E5002" => Some(Self::E5002),
            "E5003" => Some(Self::E5003),
            "E5004" => Some(Self::E5004),
            "E6001" => Some(Self::E6001),
            "E6002" => Some(Self::E6002),
            "E6003" => Some(Self::E6003),
            "E6004" => Some(Self::E6004),
            "E6005" => Some(Self::E6005),
            "E6006" => Some(Self::E6006),
            "E6007" => Some(Self::E6007),
            "E6008" => Some(Self::E6008),
            "E6009" => Some(Self::E6009),
            "E7001" => Some(Self::E7001),
            "E7002" => Some(Self::E7002),
            "E8001" => Some(Self::E8001),
            "E8002" => Some(Self::E8002),
            "E8003" => Some(Self::E8003),
            "E8004" => Some(Self::E8004),
            "E8005" => Some(Self::E8005),
            "E8006" => Some(Self::E8006),
            "E8007" => Some(Self::E8007),
            "E8008" => Some(Self::E8008),
            "E8009" => Some(Self::E8009),
            "E8010" => Some(Self::E8010),
            "E8011" => Some(Self::E8011),
            "E8012" => Some(Self::E8012),
            "E8013" => Some(Self::E8013),
            "E9001" => Some(Self::E9001),
            "E9002" => Some(Self::E9002),
            "E9003" => Some(Self::E9003),
            "E9004" => Some(Self::E9004),
            "E9005" => Some(Self::E9005),
            "E9006" => Some(Self::E9006),
            "E9007" => Some(Self::E9007),
            "E9008" => Some(Self::E9008),
            "E9009" => Some(Self::E9009),
            _ => None,
        }
    }

    /// Get the category name
    pub const fn category(self) -> &'static str {
        match self {
            Self::E1001 | Self::E1002 | Self::E1003 | Self::E1004 => "Crypto/Signing",
            Self::E2001 | Self::E2002 | Self::E2003 | Self::E2004 => "Policy/Determinism",
            Self::E3001
            | Self::E3002
            | Self::E3003
            | Self::E3004
            | Self::E3005
            | Self::E3006
            | Self::E3007
            | Self::E3008
            | Self::E3009 => "Kernels/Build/Manifest",
            Self::E4001 | Self::E4002 | Self::E4003 => "Telemetry/Chain",
            Self::E5001 | Self::E5002 | Self::E5003 | Self::E5004 => "Artifacts/CAS",
            Self::E6001
            | Self::E6002
            | Self::E6003
            | Self::E6004
            | Self::E6005
            | Self::E6006
            | Self::E6007
            | Self::E6008
            | Self::E6009 => "Adapters/DIR",
            Self::E7001 | Self::E7002 => "Node/Cluster",
            Self::E8001
            | Self::E8002
            | Self::E8003
            | Self::E8004
            | Self::E8005
            | Self::E8006
            | Self::E8007
            | Self::E8008
            | Self::E8009
            | Self::E8010
            | Self::E8011
            | Self::E8012
            | Self::E8013 => "CLI/Config",
            Self::E9001
            | Self::E9002
            | Self::E9003
            | Self::E9004
            | Self::E9005
            | Self::E9006
            | Self::E9007
            | Self::E9008
            | Self::E9009 => "OS/Environment",
        }
    }

    /// Get the associated recovery action
    pub fn recovery_action(self) -> RecoveryAction {
        match self {
            // Crypto - usually requires manual re-signing
            Self::E1001 | Self::E1002 | Self::E1003 => RecoveryAction::Resign,
            Self::E1004 => RecoveryAction::ValidationFix,

            // Policy - requires policy adjustment
            Self::E2001 | Self::E2002 | Self::E2003 | Self::E2004 => RecoveryAction::PolicyAdjust,

            // Kernels/Build - rebuild or CLI commands
            Self::E3001 | Self::E3002 => RecoveryAction::CliCommand {
                command: "verify-kernel",
                args: &[],
            },
            Self::E3003 => RecoveryAction::ValidationFix,
            Self::E3004 => RecoveryAction::Manual,
            Self::E3005 | Self::E3006 | Self::E3007 | Self::E3008 | Self::E3009 => {
                RecoveryAction::CliCommand {
                    command: "build",
                    args: &["--clean"],
                }
            }

            // Telemetry - can retry or repair
            Self::E4001 => RecoveryAction::RepairHashes {
                entity_type: "telemetry",
            },
            Self::E4002 | Self::E4003 => RecoveryAction::Retry {
                max_attempts: 3,
                base_backoff_ms: 1000,
            },

            // Artifacts - repair or re-import
            Self::E5001 | Self::E5002 | Self::E5003 | Self::E5004 => RecoveryAction::RepairHashes {
                entity_type: "artifact",
            },

            // Adapters - various recoveries
            Self::E6001 => RecoveryAction::CliCommand {
                command: "adapter",
                args: &["list"],
            },
            Self::E6002 => RecoveryAction::EvictCache { target_mb: None },
            Self::E6003 | Self::E6004 => RecoveryAction::PolicyAdjust,
            Self::E6005 => RecoveryAction::RestartComponent {
                component: "worker",
            },
            Self::E6006 => RecoveryAction::ValidationFix,
            Self::E6007 => RecoveryAction::Retry {
                max_attempts: 2,
                base_backoff_ms: 500,
            },
            Self::E6008 | Self::E6009 => RecoveryAction::Manual,

            // Node/Cluster - restart or retry
            Self::E7001 => RecoveryAction::RestartComponent {
                component: "worker",
            },
            Self::E7002 => RecoveryAction::Retry {
                max_attempts: 3,
                base_backoff_ms: 2000,
            },

            // CLI/Config - config changes or validation
            Self::E8001 | Self::E8004 | Self::E8005 | Self::E8006 | Self::E8007 | Self::E8008 => {
                RecoveryAction::ConfigChange {
                    setting: "configs/cp.toml",
                    suggested_value: None,
                }
            }
            Self::E8002 => RecoveryAction::Manual,
            Self::E8003 => RecoveryAction::RunMigrations,
            Self::E8009 | Self::E8010 | Self::E8011 | Self::E8012 | Self::E8013 => {
                RecoveryAction::ValidationFix
            }

            // OS/Environment - resource management
            Self::E9001 | Self::E9006 => RecoveryAction::EvictCache {
                target_mb: Some(512),
            },
            Self::E9002 => RecoveryAction::Manual,
            Self::E9003 => RecoveryAction::RestartComponent {
                component: "aos-secd",
            },
            Self::E9004 => RecoveryAction::CliCommand {
                command: "gc-bundles",
                args: &[],
            },
            Self::E9005 | Self::E9007 | Self::E9008 => RecoveryAction::Retry {
                max_attempts: 3,
                base_backoff_ms: 5000,
            },
            Self::E9009 => RecoveryAction::RestartComponent {
                component: "worker",
            },
        }
    }

    /// Get HTTP status code for API responses
    pub const fn http_status(self) -> u16 {
        match self {
            // 400 Bad Request - validation errors
            Self::E1004 | Self::E3003 | Self::E5002 | Self::E6006 | Self::E8002 | Self::E8012 => {
                400
            }

            // 401 Unauthorized
            // (none currently)

            // 403 Forbidden - policy violations
            Self::E2001 | Self::E2002 | Self::E2003 | Self::E2004 => 403,

            // 404 Not Found
            Self::E5001 | Self::E6001 => 404,

            // 409 Conflict - hash mismatches
            Self::E1001 | Self::E3001 | Self::E3002 | Self::E5004 | Self::E6008 | Self::E6009 => {
                409
            }

            // 503 Service Unavailable - resource exhaustion
            Self::E9001
            | Self::E9003
            | Self::E9004
            | Self::E9005
            | Self::E9006
            | Self::E9007
            | Self::E9008
            | Self::E9009
            | Self::E6002
            | Self::E6005
            | Self::E7001 => 503,

            // 504 Gateway Timeout
            Self::E4002 | Self::E4003 => 504,

            // 500 Internal Server Error - everything else
            _ => 500,
        }
    }

    /// Check if this error is retryable
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::E4002
                | Self::E4003
                | Self::E6002
                | Self::E6005
                | Self::E6007
                | Self::E7001
                | Self::E7002
                | Self::E9005
                | Self::E9007
                | Self::E9008
        )
    }
}

impl std::fmt::Display for ECode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ECode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

// =============================================================================
// ErrorMetadata: Structured error information
// =============================================================================

/// Complete error metadata for an error code
#[derive(Debug, Clone, Serialize)]
pub struct ErrorMetadata {
    /// The typed error code
    pub ecode: ECode,
    /// Human-readable title
    pub title: &'static str,
    /// Explanation of why this error occurs
    pub cause: &'static str,
    /// Step-by-step fix instructions
    #[serde(skip)]
    pub fix_steps: &'static [&'static str],
    /// Related documentation paths
    #[serde(skip)]
    pub related_docs: &'static [&'static str],
    /// Associated recovery action
    pub recovery: RecoveryAction,
    /// HTTP status code for API responses
    pub http_status: u16,
    /// Whether this error is retryable
    pub is_retryable: bool,
}

// =============================================================================
// HasECode trait: For compile-time exhaustiveness checking
// =============================================================================

/// Trait for errors that have an associated error code.
///
/// Implementing this trait provides compile-time guarantees that all error
/// variants map to an ECode.
pub trait HasECode {
    /// Get the error code for this error
    fn ecode(&self) -> ECode;

    /// Get the recovery action for this error
    fn recovery_action(&self) -> RecoveryAction {
        self.ecode().recovery_action()
    }

    /// Get the HTTP status code for this error
    fn http_status(&self) -> u16 {
        self.ecode().http_status()
    }

    /// Check if this error is retryable
    fn is_retryable(&self) -> bool {
        self.ecode().is_retryable()
    }
}

// =============================================================================
// Registry: Lazy-loaded metadata lookup
// =============================================================================

/// Get metadata for an error code
pub fn get_metadata(ecode: ECode) -> &'static ErrorMetadata {
    METADATA_REGISTRY
        .get(&ecode)
        .expect("All ECode variants must have metadata")
}

/// All registered error metadata
pub fn all_metadata() -> impl Iterator<Item = &'static ErrorMetadata> {
    METADATA_REGISTRY.values()
}

lazy_static::lazy_static! {
    static ref METADATA_REGISTRY: HashMap<ECode, ErrorMetadata> = {
        let mut m = HashMap::new();

        // E1xxx: Crypto/Signing
        m.insert(ECode::E1001, ErrorMetadata {
            ecode: ECode::E1001,
            title: "Invalid Signature",
            cause: "The Ed25519 signature verification failed for an artifact or bundle.",
            fix_steps: &[
                "Verify the public key is correct",
                "Check that the bundle hasn't been modified",
                "Re-sign the bundle: aosctl sign-bundle <bundle>",
                "Verify signature: aosctl verify <bundle>",
            ],
            related_docs: &["docs/ARCHITECTURE.md", "crates/adapteros-crypto/"],
            recovery: RecoveryAction::Resign,
            http_status: 409,
            is_retryable: false,
        });

        m.insert(ECode::E1002, ErrorMetadata {
            ecode: ECode::E1002,
            title: "Missing Public Key",
            cause: "No public key found for signature verification.",
            fix_steps: &[
                "Ensure public_key.hex is present in the bundle",
                "Check key distribution from CA/CI",
                "For dev: generate keypair with aos-secd",
            ],
            related_docs: &["docs/control-plane.md"],
            recovery: RecoveryAction::Resign,
            http_status: 500,
            is_retryable: false,
        });

        m.insert(ECode::E1003, ErrorMetadata {
            ecode: ECode::E1003,
            title: "Key Rotation Required",
            cause: "Signing key age exceeds policy threshold (>120 days).",
            fix_steps: &[
                "Generate new keypair: aos-secd rotate-keys",
                "Re-sign all artifacts with new key",
                "Update public key distribution",
                "Verify rotation: aosctl diag --system",
            ],
            related_docs: &["docs/control-plane.md"],
            recovery: RecoveryAction::Resign,
            http_status: 500,
            is_retryable: false,
        });

        m.insert(ECode::E1004, ErrorMetadata {
            ecode: ECode::E1004,
            title: "Invalid Hash Format",
            cause: "The provided BLAKE3 hash is malformed or has incorrect length.",
            fix_steps: &[
                "Verify hash is hex-encoded BLAKE3",
                "Expected format: b3:hexstring",
                "Recompute hash: aosctl hash <file>",
            ],
            related_docs: &["crates/adapteros-core/src/hash.rs"],
            recovery: RecoveryAction::ValidationFix,
            http_status: 400,
            is_retryable: false,
        });

        // E2xxx: Policy/Determinism
        m.insert(ECode::E2001, ErrorMetadata {
            ecode: ECode::E2001,
            title: "Determinism Violation Detected",
            cause: "Replay produced different outputs for identical inputs.",
            fix_steps: &[
                "Check kernel compilation flags (no fast-math)",
                "Verify RNG seed derivation matches",
                "Review retrieval tie-breaker ordering",
                "Run: aosctl replay --verbose <bundle>",
                "Compare: diff old_trace new_trace",
            ],
            related_docs: &["docs/ARCHITECTURE.md", "tests/determinism.rs"],
            recovery: RecoveryAction::PolicyAdjust,
            http_status: 403,
            is_retryable: false,
        });

        m.insert(ECode::E2002, ErrorMetadata {
            ecode: ECode::E2002,
            title: "Policy Violation",
            cause: "Operation violates configured policy pack constraints.",
            fix_steps: &[
                "Review policy pack: cat configs/cp.toml",
                "Check specific violation in trace",
                "Adjust policy or fix operation",
                "Re-audit: aosctl audit <cpid>",
            ],
            related_docs: &["docs/ARCHITECTURE.md", "crates/adapteros-policy/"],
            recovery: RecoveryAction::PolicyAdjust,
            http_status: 403,
            is_retryable: false,
        });

        m.insert(ECode::E2003, ErrorMetadata {
            ecode: ECode::E2003,
            title: "Egress Violation",
            cause: "Attempted network access while serving in deny_all mode.",
            fix_steps: &[
                "Verify PF rules are active: aosctl diag --system",
                "Check for DNS/network calls in adapters",
                "Review egress policy configuration",
                "Validate offline operation mode",
            ],
            related_docs: &["docs/ARCHITECTURE.md"],
            recovery: RecoveryAction::PolicyAdjust,
            http_status: 403,
            is_retryable: false,
        });

        m.insert(ECode::E2004, ErrorMetadata {
            ecode: ECode::E2004,
            title: "Refusal Threshold Not Met",
            cause: "Evidence below minimum confidence threshold for factual claim.",
            fix_steps: &[
                "Check abstain_threshold in policy",
                "Verify RAG retrieval returned sufficient spans",
                "Review evidence quality",
                "Consider retraining or updating index",
            ],
            related_docs: &["docs/ARCHITECTURE.md", "crates/adapteros-lora-rag/"],
            recovery: RecoveryAction::PolicyAdjust,
            http_status: 403,
            is_retryable: false,
        });

        // E9xxx: OS/Environment (adding a few key ones)
        m.insert(ECode::E9006, ErrorMetadata {
            ecode: ECode::E9006,
            title: "Out of Memory",
            cause: "Process memory usage exceeded limits, triggering OOM condition.",
            fix_steps: &[
                "Check memory usage: aosctl diag --system",
                "Reduce max_concurrent_requests",
                "Evict unused adapters: aosctl adapter evict",
                "Increase system memory or container limits",
                "Consider quantized model variants",
            ],
            related_docs: &["docs/ARCHITECTURE.md", "crates/adapteros-lora-worker/"],
            recovery: RecoveryAction::EvictCache { target_mb: Some(512) },
            http_status: 503,
            is_retryable: false,
        });

        m.insert(ECode::E8003, ErrorMetadata {
            ecode: ECode::E8003,
            title: "Database Connection Failed",
            cause: "Cannot connect to control plane database.",
            fix_steps: &[
                "Check database file exists: ls var/aos-cp.sqlite3",
                "Verify permissions",
                "Initialize if needed: aosctl db migrate",
                "Check DATABASE_URL environment variable",
            ],
            related_docs: &["crates/adapteros-db/"],
            recovery: RecoveryAction::RunMigrations,
            http_status: 500,
            is_retryable: false,
        });

        // Add more as needed - this is a subset for the initial implementation
        // The full registry would include all ECodes

        m
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn test_ecode_roundtrip() {
        for ecode in ECode::iter() {
            let s = ecode.as_str();
            let parsed = ECode::parse(s).expect("should parse");
            assert_eq!(ecode, parsed);
        }
    }

    #[test]
    fn test_ecode_categories() {
        assert_eq!(ECode::E1001.category(), "Crypto/Signing");
        assert_eq!(ECode::E2001.category(), "Policy/Determinism");
        assert_eq!(ECode::E9006.category(), "OS/Environment");
    }

    #[test]
    fn test_recovery_action_safety() {
        let action = ECode::E9006.recovery_action();
        assert!(matches!(action, RecoveryAction::EvictCache { .. }));
        assert_eq!(action.safety(), FixSafety::Safe);
    }

    #[test]
    fn test_http_status() {
        assert_eq!(ECode::E2002.http_status(), 403); // Policy violation -> Forbidden
        assert_eq!(ECode::E6001.http_status(), 404); // Not found
        assert_eq!(ECode::E9006.http_status(), 503); // OOM -> Service Unavailable
    }

    // =========================================================================
    // Compile-Time Exhaustiveness Tests
    // =========================================================================
    // These tests ensure that all error codes have required mappings.
    // The Rust compiler's exhaustive match checking is the primary mechanism,
    // but these tests provide explicit verification and catch regressions.

    /// Verify all ECode variants have a recovery action
    ///
    /// This test iterates through all ECode variants to ensure none panic
    /// when accessing their recovery action. The actual exhaustiveness is
    /// guaranteed at compile-time by the match statement in recovery_action().
    #[test]
    fn exhaustiveness_all_ecodes_have_recovery_action() {
        for ecode in ECode::iter() {
            // If recovery_action() has a non-exhaustive match, this will panic
            let _action = ecode.recovery_action();
        }
    }

    /// Verify all ECode variants have a category
    #[test]
    fn exhaustiveness_all_ecodes_have_category() {
        for ecode in ECode::iter() {
            let category = ecode.category();
            // Category should be non-empty
            assert!(!category.is_empty(), "ECode {:?} has empty category", ecode);
        }
    }

    /// Verify all ECode variants have an HTTP status
    #[test]
    fn exhaustiveness_all_ecodes_have_http_status() {
        for ecode in ECode::iter() {
            let status = ecode.http_status();
            // Status should be a valid HTTP status code
            assert!(
                (100..=599).contains(&status),
                "ECode {:?} has invalid HTTP status {}",
                ecode,
                status
            );
        }
    }

    /// Verify all ECode variants have is_retryable defined
    #[test]
    fn exhaustiveness_all_ecodes_have_retryable() {
        for ecode in ECode::iter() {
            // Just calling is_retryable ensures it compiles for all variants
            let _retryable = ecode.is_retryable();
        }
    }

    /// Verify ECode string representations are unique
    #[test]
    fn exhaustiveness_ecode_strings_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for ecode in ECode::iter() {
            let s = ecode.as_str();
            assert!(
                seen.insert(s),
                "Duplicate ECode string representation: {}",
                s
            );
        }
    }

    /// Verify ECode category prefixes match
    #[test]
    fn exhaustiveness_ecode_category_prefix_consistency() {
        for ecode in ECode::iter() {
            let code_str = ecode.as_str();
            let category = ecode.category();

            // Extract the category digit (E1xxx = 1, E2xxx = 2, etc.)
            let prefix = &code_str[1..2];
            let expected_category = match prefix {
                "1" => "Crypto/Signing",
                "2" => "Policy/Determinism",
                "3" => "Kernels/Build/Manifest",
                "4" => "Telemetry/Chain",
                "5" => "Artifacts/CAS",
                "6" => "Adapters/DIR",
                "7" => "Node/Cluster",
                "8" => "CLI/Config",
                "9" => "OS/Environment",
                _ => panic!("Unknown ECode prefix: {}", prefix),
            };

            assert_eq!(
                category, expected_category,
                "ECode {} has category '{}' but expected '{}' based on prefix",
                code_str, category, expected_category
            );
        }
    }

    /// Verify recovery action safety levels are appropriate for the error category
    #[test]
    fn exhaustiveness_recovery_safety_appropriate() {
        for ecode in ECode::iter() {
            let action = ecode.recovery_action();
            let safety = action.safety();

            // Some categories should never have "Safe" auto-execute actions
            // (e.g., Policy violations should always require confirmation or manual)
            let category = ecode.category();
            if category == "Policy/Determinism" {
                assert!(
                    safety != FixSafety::Safe,
                    "Policy error {:?} should not have Safe recovery",
                    ecode
                );
            }
        }
    }

    /// Count ECodes per category to detect missing entries
    #[test]
    fn exhaustiveness_category_counts() {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();

        for ecode in ECode::iter() {
            *counts.entry(ecode.category()).or_insert(0) += 1;
        }

        // Verify each category has at least one error code
        let expected_categories = [
            "Crypto/Signing",
            "Policy/Determinism",
            "Kernels/Build/Manifest",
            "Telemetry/Chain",
            "Artifacts/CAS",
            "Adapters/DIR",
            "Node/Cluster",
            "CLI/Config",
            "OS/Environment",
        ];

        for cat in expected_categories {
            assert!(
                counts.get(cat).copied().unwrap_or(0) > 0,
                "Category '{}' has no error codes",
                cat
            );
        }
    }
}
