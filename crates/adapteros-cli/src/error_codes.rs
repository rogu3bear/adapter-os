//! Error code registry for aosctl
//!
//! Structured error codes with human-readable explanations and fixes.
//! Categories:
//! - E1xxx: crypto/signing errors
//! - E2xxx: policy/determinism violations
//! - E3xxx: kernels/build/manifest issues
//! - E4xxx: telemetry/chain problems
//! - E5xxx: artifacts/CAS errors
//! - E6xxx: adapters/MPLoRA issues
//! - E7xxx: node/cluster problems
//! - E8xxx: CLI/config errors
//! - E9xxx: OS/environment issues

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCode {
    pub code: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub cause: &'static str,
    pub fix: &'static str,
    #[serde(skip)]
    pub related_docs: &'static [&'static str],
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        )?;
        writeln!(f, "Error Code: {}", self.code)?;
        writeln!(f, "Category: {}", self.category)?;
        writeln!(
            f,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        )?;
        writeln!(f)?;
        writeln!(f, "📋 {}", self.title)?;
        writeln!(f)?;
        writeln!(f, "🔍 Cause:")?;
        writeln!(f, "   {}", self.cause)?;
        writeln!(f)?;
        writeln!(f, "🔧 Fix:")?;
        for line in self.fix.lines() {
            writeln!(f, "   {}", line)?;
        }
        if !self.related_docs.is_empty() {
            writeln!(f)?;
            writeln!(f, "📚 Related Documentation:")?;
            for doc in self.related_docs {
                writeln!(f, "   - {}", doc)?;
            }
        }
        writeln!(f)?;
        writeln!(
            f,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        )?;
        Ok(())
    }
}

macro_rules! error_code {
    ($code:expr, $cat:expr, $title:expr, $cause:expr, $fix:expr $(, docs = [$($doc:expr),*])?) => {
        ErrorCode {
            code: $code,
            category: $cat,
            title: $title,
            cause: $cause,
            fix: $fix,
            related_docs: &[$($($doc),*)?],
        }
    };
}

/// All registered error codes
pub fn all_error_codes() -> Vec<ErrorCode> {
    vec![
        // E1xxx: Crypto/Signing Errors
        error_code!(
            "E1001",
            "Crypto/Signing",
            "Invalid Signature",
            "The Ed25519 signature verification failed for an artifact or bundle.",
            "1. Verify the public key is correct\n\
             2. Check that the bundle hasn't been modified\n\
             3. Re-sign the bundle: aosctl sign-bundle <bundle>\n\
             4. Verify signature: aosctl verify <bundle>",
            docs = ["docs/architecture.md", "crates/adapteros-crypto/"]
        ),
        error_code!(
            "E1002",
            "Crypto/Signing",
            "Missing Public Key",
            "No public key found for signature verification.",
            "1. Ensure public_key.hex is present in the bundle\n\
             2. Check key distribution from CA/CI\n\
             3. For dev: generate keypair with aos-secd",
            docs = ["docs/control-plane.md"]
        ),
        error_code!(
            "E1003",
            "Crypto/Signing",
            "Key Rotation Required",
            "Signing key age exceeds policy threshold (>120 days).",
            "1. Generate new keypair: aos-secd rotate-keys\n\
             2. Re-sign all artifacts with new key\n\
             3. Update public key distribution\n\
             4. Verify rotation: aosctl diag --system",
            docs = ["docs/control-plane.md"]
        ),
        error_code!(
            "E1004",
            "Crypto/Signing",
            "Invalid Hash Format",
            "The provided BLAKE3 hash is malformed or has incorrect length.",
            "1. Verify hash is hex-encoded BLAKE3\n\
             2. Expected format: b3:hexstring\n\
             3. Recompute hash: aosctl hash <file>",
            docs = ["crates/adapteros-core/src/hash.rs"]
        ),
        // E2xxx: Policy/Determinism Violations
        error_code!(
            "E2001",
            "Policy/Determinism",
            "Determinism Violation Detected",
            "Replay produced different outputs for identical inputs.",
            "1. Check kernel compilation flags (no fast-math)\n\
             2. Verify RNG seed derivation matches\n\
             3. Review retrieval tie-breaker ordering\n\
             4. Run: aosctl replay --verbose <bundle>\n\
             5. Compare: diff old_trace new_trace",
            docs = ["docs/architecture.md", "tests/determinism.rs"]
        ),
        error_code!(
            "E2002",
            "Policy/Determinism",
            "Policy Violation",
            "Operation violates configured policy pack constraints.",
            "1. Review policy pack: cat configs/cp.toml\n\
             2. Check specific violation in trace\n\
             3. Adjust policy or fix operation\n\
             4. Re-audit: aosctl audit <cpid>",
            docs = ["docs/architecture.md", "crates/mplora-policy/"]
        ),
        error_code!(
            "E2003",
            "Policy/Determinism",
            "Egress Violation",
            "Attempted network access while serving in deny_all mode.",
            "1. Verify PF rules are active: aosctl diag --system\n\
             2. Check for DNS/network calls in adapters\n\
             3. Review egress policy configuration\n\
             4. Validate offline operation mode",
            docs = ["docs/architecture.md"]
        ),
        error_code!(
            "E2004",
            "Policy/Determinism",
            "Refusal Threshold Not Met",
            "Evidence below minimum confidence threshold for factual claim.",
            "1. Check abstain_threshold in policy\n\
             2. Verify RAG retrieval returned sufficient spans\n\
             3. Review evidence quality\n\
             4. Consider retraining or updating index",
            docs = ["docs/architecture.md", "crates/mplora-rag/"]
        ),
        // E3xxx: Kernels/Build/Manifest Issues
        error_code!(
            "E3001",
            "Kernels/Build/Manifest",
            "Kernel Manifest Signature Invalid",
            "The Metal kernel manifest signature verification failed.",
            "1. Rebuild kernels: cd metal && ./build.sh\n\
             2. Verify CI signing key\n\
             3. Check kernel hash: aosctl verify-kernel\n\
             4. Ensure toolchain.toml matches build environment",
            docs = ["metal/build.sh", "docs/metal/phase4-metal-kernels.md"]
        ),
        error_code!(
            "E3002",
            "Kernels/Build/Manifest",
            "Kernel Hash Mismatch",
            "Loaded kernel hash doesn't match Plan manifest.",
            "1. Verify kernel .metallib present and unmodified\n\
             2. Check Plan manifest kernel_hash field\n\
             3. Rebuild Plan: aosctl build-plan <manifest>\n\
             4. Refuse serving if mismatch persists",
            docs = ["crates/mplora-kernel-mtl/", "crates/mplora-plan/"]
        ),
        error_code!(
            "E3003",
            "Kernels/Build/Manifest",
            "Invalid Manifest",
            "Manifest JSON is malformed or missing required fields.",
            "1. Validate JSON: jq . < manifest.json\n\
             2. Check required fields: model_id, adapters, policy\n\
             3. Review manifest schema: docs/code-intelligence/code-manifest-v4.md\n\
             4. Use example: manifests/qwen7b.yaml",
            docs = ["docs/code-intelligence/code-manifest-v4.md", "manifests/"]
        ),
        error_code!(
            "E3004",
            "Kernels/Build/Manifest",
            "Metal Device Not Found",
            "No compatible Metal GPU device detected.",
            "1. Verify macOS system with Apple Silicon\n\
             2. Check Metal support: system_profiler SPDisplaysDataType\n\
             3. Update macOS if needed\n\
             4. For dev: use --mock-metal flag",
            docs = ["docs/architecture.md"]
        ),
        // E4xxx: Telemetry/Chain Problems
        error_code!(
            "E4001",
            "Telemetry/Chain",
            "Telemetry Bundle Chain Broken",
            "Merkle root hash mismatch in telemetry bundle chain.",
            "1. Verify bundle signatures: aosctl verify-telemetry <dir>\n\
             2. Check for missing bundles\n\
             3. Review bundle rotation logs\n\
             4. Restore from backup if corruption detected",
            docs = [
                "crates/mplora-telemetry/",
                "tests/telemetry_bundle_rotation.rs"
            ]
        ),
        error_code!(
            "E4002",
            "Telemetry/Chain",
            "Telemetry Write Failed",
            "Cannot write telemetry events to bundle.",
            "1. Check disk space: df -h var/telemetry\n\
             2. Verify write permissions\n\
             3. Check bundle size limits in policy\n\
             4. Review telemetry configuration",
            docs = ["crates/mplora-telemetry/"]
        ),
        error_code!(
            "E4003",
            "Telemetry/Chain",
            "Bundle Rotation Failed",
            "Failed to rotate telemetry bundle at threshold.",
            "1. Check disk space and inodes\n\
             2. Verify bundle signing works\n\
             3. Review retention policy settings\n\
             4. Manually trigger: aosctl rotate-bundle",
            docs = ["tests/telemetry_bundle_rotation.rs"]
        ),
        // E5xxx: Artifacts/CAS Errors
        error_code!(
            "E5001",
            "Artifacts/CAS",
            "Artifact Not Found in CAS",
            "Content-addressed artifact missing from store.",
            "1. Verify hash: echo <hash>\n\
             2. Check CAS root: ls -la var/cas/\n\
             3. Re-import artifact: aosctl import <bundle>\n\
             4. Verify SBOM completeness",
            docs = ["crates/mplora-artifacts/"]
        ),
        error_code!(
            "E5002",
            "Artifacts/CAS",
            "SBOM Incomplete",
            "SBOM missing required artifacts or metadata.",
            "1. Validate SBOM: jq . < sbom.json\n\
             2. Check all artifacts listed have hashes\n\
             3. Regenerate SBOM: aosctl generate-sbom <dir>\n\
             4. Re-sign bundle after fixing",
            docs = ["crates/mplora-sbom/", "crates/mplora-artifacts/"]
        ),
        error_code!(
            "E5003",
            "Artifacts/CAS",
            "Bundle Extraction Failed",
            "Failed to extract artifact bundle.",
            "1. Verify bundle is valid ZIP format\n\
             2. Check disk space\n\
             3. Verify file permissions\n\
             4. Re-download or recreate bundle",
            docs = ["crates/mplora-artifacts/"]
        ),
        error_code!(
            "E5004",
            "Artifacts/CAS",
            "Hash Mismatch",
            "Computed artifact hash doesn't match expected value.",
            "1. Verify artifact file integrity\n\
             2. Recompute: aosctl hash <file>\n\
             3. Check for file corruption or tampering\n\
             4. Re-import from trusted source",
            docs = ["crates/adapteros-core/src/hash.rs"]
        ),
        // E6xxx: Adapters/MPLoRA Issues
        error_code!(
            "E6001",
            "Adapters/MPLoRA",
            "Adapter Not Found in Registry",
            "Specified adapter ID not registered or not allowed by ACL.",
            "1. List adapters: aosctl list-adapters\n\
             2. Register: aosctl register-adapter <id> <hash>\n\
             3. Verify ACL permissions\n\
             4. Check tenant isolation",
            docs = ["crates/mplora-registry/"]
        ),
        error_code!(
            "E6002",
            "Adapters/MPLoRA",
            "Adapter Eviction Occurred",
            "Adapter evicted due to memory pressure or low activation.",
            "1. Check memory headroom: aosctl diag --system\n\
             2. Review eviction policy in manifest\n\
             3. Pin critical adapters: aosctl pin-adapter\n\
             4. Reduce K or adapter count",
            docs = ["docs/architecture.md"]
        ),
        error_code!(
            "E6003",
            "Adapters/MPLoRA",
            "Router Skew Detected",
            "Router gate distribution exceeds entropy floor.",
            "1. Check router calibration\n\
             2. Verify entropy_floor setting\n\
             3. Review adapter activation patterns\n\
             4. Rebuild Plan if needed",
            docs = ["crates/mplora-router/"]
        ),
        error_code!(
            "E6004",
            "Adapters/MPLoRA",
            "Adapter Quality Below Threshold",
            "Adapter quality delta below minimum threshold for retention.",
            "1. Review min_quality_delta in policy\n\
             2. Check adapter performance metrics\n\
             3. Retrain adapter with better data\n\
             4. Adjust quality threshold if appropriate",
            docs = ["docs/architecture.md"]
        ),
        error_code!(
            "E6005",
            "Adapters/MPLoRA",
            "Adapter Socket Connection Failed",
            "Cannot connect to worker socket for adapter operations.",
            "1. Check if worker is running: aosctl serve status\n\
             2. Start worker if needed: aosctl serve start\n\
             3. Verify socket path: /var/run/aos/<tenant>/aos.sock\n\
             4. Check tenant isolation and permissions",
            docs = ["crates/adapteros-client/"]
        ),
        error_code!(
            "E6006",
            "Adapters/MPLoRA",
            "Invalid Adapter ID Format",
            "Adapter ID contains invalid characters or exceeds length limit.",
            "1. Use only alphanumeric characters, hyphens, and underscores\n\
             2. Keep adapter ID under 64 characters\n\
             3. Avoid special characters and spaces\n\
             4. Examples: 'python-general', 'adapter_2', 'rust-helper'",
            docs = ["crates/adapteros-cli/src/commands/adapter.rs"]
        ),
        error_code!(
            "E6007",
            "Adapters/MPLoRA",
            "Adapter Command Failed",
            "Adapter lifecycle command (promote/demote/pin/unpin) failed.",
            "1. Check adapter exists: aosctl adapter list\n\
             2. Verify adapter is in correct state for operation\n\
             3. Check worker logs for detailed error\n\
             4. Ensure adapter is not locked or in use",
            docs = ["crates/adapteros-lora-worker/src/adapter_hotswap.rs"]
        ),
        // E7xxx: Node/Cluster Problems
        error_code!(
            "E7001",
            "Node/Cluster",
            "Node Unavailable",
            "Worker node not responding or unreachable.",
            "1. Check node status: aosctl node-status\n\
             2. Verify UDS socket: ls -la /var/run/aos/\n\
             3. Restart worker if needed\n\
             4. Check logs: tail -f var/logs/worker.log",
            docs = ["crates/mplora-node/"]
        ),
        error_code!(
            "E7002",
            "Node/Cluster",
            "Job Execution Failed",
            "Async job (scan, train, etc.) failed to complete.",
            "1. Check job status: aosctl job-status <id>\n\
             2. Review job logs for specific error\n\
             3. Verify resource availability\n\
             4. Retry job if transient failure",
            docs = ["crates/mplora-orchestrator/"]
        ),
        // E8xxx: CLI/Config Errors
        error_code!(
            "E8001",
            "CLI/Config",
            "Invalid Configuration",
            "Configuration file malformed or missing required fields.",
            "1. Check config syntax: cat configs/cp.toml\n\
             2. Validate TOML: taplo check configs/cp.toml\n\
             3. Review example: configs/cp.toml.example\n\
             4. Check environment variables",
            docs = ["configs/"]
        ),
        error_code!(
            "E8002",
            "CLI/Config",
            "Missing Required Argument",
            "Command requires argument that was not provided.",
            "1. Run: aosctl <command> --help\n\
             2. Check command syntax in manual\n\
             3. Review examples: aosctl tutorial",
            docs = []
        ),
        error_code!(
            "E8003",
            "CLI/Config",
            "Database Connection Failed",
            "Cannot connect to control plane database.",
            "1. Check database file exists: ls var/aos-cp.sqlite3\n\
             2. Verify permissions\n\
             3. Initialize if needed: aosctl init-cp\n\
             4. Check DATABASE_URL environment variable",
            docs = ["crates/mplora-db/"]
        ),
        // E9xxx: OS/Environment Issues
        error_code!(
            "E9001",
            "OS/Environment",
            "Insufficient Memory",
            "System memory below minimum threshold for operation.",
            "1. Check memory: aosctl diag --system\n\
             2. Close other applications\n\
             3. Reduce adapter count or K value\n\
             4. Consider larger model tier or machine",
            docs = ["docs/architecture.md"]
        ),
        error_code!(
            "E9002",
            "OS/Environment",
            "Permission Denied",
            "Insufficient permissions for operation.",
            "1. Check file/directory permissions\n\
             2. Verify tenant UID/GID mapping\n\
             3. Run with appropriate privileges if needed\n\
             4. Check isolation policy",
            docs = ["docs/architecture.md"]
        ),
        error_code!(
            "E9003",
            "OS/Environment",
            "Service Not Running",
            "Required system service (aos-secd) not running.",
            "1. Check service: ps aux | grep aos-secd\n\
             2. Start service: launchctl load scripts/aos-secd.plist\n\
             3. Check logs: tail -f /var/log/aos-secd.log\n\
             4. Verify launchd configuration",
            docs = ["scripts/aos-secd.plist"]
        ),
        error_code!(
            "E9004",
            "OS/Environment",
            "Disk Space Insufficient",
            "Insufficient disk space for operation.",
            "1. Check space: df -h\n\
             2. Clean old telemetry bundles: aosctl gc-bundles\n\
             3. Remove unused adapters\n\
             4. Archive old CPs",
            docs = ["scripts/gc_bundles.sh"]
        ),
    ]
}

/// Find error code by code string (e.g., "E3001")
pub fn find_by_code(code: &str) -> Option<ErrorCode> {
    all_error_codes().into_iter().find(|ec| ec.code == code)
}

/// Registry of error codes for fast lookup
pub static REGISTRY: Lazy<HashMap<&'static str, ErrorCode>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for code in all_error_codes() {
        m.insert(code.code, code);
    }
    m
});

/// Find error code by AosError variant name
pub fn find_by_aos_error(error_name: &str) -> Option<ErrorCode> {
    match error_name {
        "InvalidHash" => find_by_code("E1004"),
        "InvalidCPID" => find_by_code("E8001"),
        "Crypto" => find_by_code("E1001"),
        "PolicyViolation" => find_by_code("E2002"),
        "InvalidManifest" => find_by_code("E3003"),
        "Kernel" => find_by_code("E3002"),
        "Telemetry" => find_by_code("E4002"),
        "DeterminismViolation" => find_by_code("E2001"),
        "EgressViolation" => find_by_code("E2003"),
        "Artifact" => find_by_code("E5001"),
        "Registry" => find_by_code("E6001"),
        "Worker" => find_by_code("E7001"),
        "Node" => find_by_code("E7001"),
        "Job" => find_by_code("E7002"),
        "Config" => find_by_code("E8001"),
        "Database" => find_by_code("E8003"),
        "Io" | "Parse" => find_by_code("E9002"),
        _ => None,
    }
}

/// Map AosError variant names to error codes (fallback to E9000)
#[allow(dead_code)] // TODO: Implement error mapping in future iteration
pub fn map_aos_error(name: &str) -> &'static str {
    match name {
        "PolicyViolation" => "E2001",
        "InvalidHash" => "E3002",
        "ManifestMissing" => "E3003",
        "TelemetryGap" => "E4002",
        "SignatureInvalid" => "E1001",
        "AdapterIncompatible" => "E6003",
        _ => "E9000", // OS/env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_codes_unique() {
        let codes = all_error_codes();
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert!(seen.insert(code.code), "Duplicate code: {}", code.code);
        }
    }

    #[test]
    fn test_find_by_code() {
        assert!(find_by_code("E3001").is_some());
        assert!(find_by_code("E9999").is_none());
    }

    #[test]
    fn test_find_by_aos_error() {
        assert!(find_by_aos_error("InvalidHash").is_some());
        assert!(find_by_aos_error("PolicyViolation").is_some());
        assert!(find_by_aos_error("Unknown").is_none());
    }

    #[test]
    fn test_code_categories() {
        let codes = all_error_codes();
        for code in codes {
            let prefix = &code.code[0..2];
            match prefix {
                "E1" => assert_eq!(code.category, "Crypto/Signing"),
                "E2" => assert_eq!(code.category, "Policy/Determinism"),
                "E3" => assert_eq!(code.category, "Kernels/Build/Manifest"),
                "E4" => assert_eq!(code.category, "Telemetry/Chain"),
                "E5" => assert_eq!(code.category, "Artifacts/CAS"),
                "E6" => assert_eq!(code.category, "Adapters/MPLoRA"),
                "E7" => assert_eq!(code.category, "Node/Cluster"),
                "E8" => assert_eq!(code.category, "CLI/Config"),
                "E9" => assert_eq!(code.category, "OS/Environment"),
                _ => panic!("Invalid code prefix: {}", prefix),
            }
        }
    }
}
