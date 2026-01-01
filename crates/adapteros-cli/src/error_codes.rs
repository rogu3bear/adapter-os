//! Error code registry for aosctl
//!
//! Structured error codes with human-readable explanations and fixes.
//! Categories:
//! - E1xxx: crypto/signing errors
//! - E2xxx: policy/determinism violations
//! - E3xxx: kernels/build/manifest issues
//! - E4xxx: telemetry/chain problems
//! - E5xxx: artifacts/CAS errors
//! - E6xxx: adapters/DIR issues
//! - E7xxx: node/cluster problems
//! - E8xxx: CLI/config errors
//! - E9xxx: OS/environment issues

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Macro to define ECode enum and implementations from a single source of truth.
///
/// This ensures `as_str()`, `parse()`, and `category()` are always in sync.
macro_rules! define_ecodes {
    (
        $(
            $category:literal => [ $($variant:ident),+ $(,)? ]
        ),+ $(,)?
    ) => {
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
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[allow(non_camel_case_types)]
        pub enum ECode {
            $($($variant,)+)+
        }

        impl ECode {
            /// Get the string representation of this error code
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($(ECode::$variant => stringify!($variant),)+)+
                }
            }

            /// Parse a string into an ECode
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($(stringify!($variant) => Some(ECode::$variant),)+)+
                    _ => None,
                }
            }

            /// Get the category for this error code
            pub const fn category(self) -> &'static str {
                match self {
                    $($(ECode::$variant)|+ => $category,)+
                }
            }
        }
    };
}

// Single source of truth for all error codes and their categories
define_ecodes! {
    "Crypto/Signing" => [E1001, E1002, E1003, E1004],
    "Policy/Determinism" => [E2001, E2002, E2003, E2004],
    "Kernels/Build/Manifest" => [E3001, E3002, E3003, E3004, E3005, E3006, E3007, E3008, E3009],
    "Telemetry/Chain" => [E4001, E4002, E4003],
    "Artifacts/CAS" => [E5001, E5002, E5003, E5004],
    "Adapters/DIR" => [E6001, E6002, E6003, E6004, E6005, E6006, E6007, E6008, E6009],
    "Node/Cluster" => [E7001, E7002],
    "CLI/Config" => [E8001, E8002, E8003, E8004, E8005, E8006, E8007, E8008, E8009, E8010, E8011, E8012, E8013],
    "OS/Environment" => [E9001, E9002, E9003, E9004, E9005, E9006, E9007, E9008, E9009],
}

impl fmt::Display for ECode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ECode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ECode::parse(s).ok_or(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCode {
    /// The typed error code
    pub ecode: ECode,
    /// String representation (for backward compatibility)
    #[serde(rename = "code")]
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
    ($ecode:expr, $title:expr, $cause:expr, $fix:expr $(, docs = [$($doc:expr),*])?) => {
        ErrorCode {
            ecode: $ecode,
            code: $ecode.as_str(),
            category: $ecode.category(),
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
            ECode::E1001,
            "Invalid Signature",
            "The Ed25519 signature verification failed for an artifact or bundle.",
            "1. Verify the public key is correct\n\
             2. Check that the bundle hasn't been modified\n\
             3. Re-sign the bundle: aosctl sign-bundle <bundle>\n\
             4. Verify signature: aosctl verify <bundle>",
            docs = ["docs/ARCHITECTURE.md", "crates/adapteros-crypto/"]
        ),
        error_code!(
            ECode::E1002,
            "Missing Public Key",
            "No public key found for signature verification.",
            "1. Ensure public_key.hex is present in the bundle\n\
             2. Check key distribution from CA/CI\n\
             3. For dev: generate keypair with aos-secd",
            docs = ["docs/control-plane.md"]
        ),
        error_code!(
            ECode::E1003,
            "Key Rotation Required",
            "Signing key age exceeds policy threshold (>120 days).",
            "1. Generate new keypair: aos-secd rotate-keys\n\
             2. Re-sign all artifacts with new key\n\
             3. Update public key distribution\n\
             4. Verify rotation: aosctl diag --system",
            docs = ["docs/control-plane.md"]
        ),
        error_code!(
            ECode::E1004,
            "Invalid Hash Format",
            "The provided BLAKE3 hash is malformed or has incorrect length.",
            "1. Verify hash is hex-encoded BLAKE3\n\
             2. Expected format: b3:hexstring\n\
             3. Recompute hash: aosctl hash <file>",
            docs = ["crates/adapteros-core/src/hash.rs"]
        ),
        // E2xxx: Policy/Determinism Violations
        error_code!(
            ECode::E2001,
            "Determinism Violation Detected",
            "Replay produced different outputs for identical inputs.",
            "1. Check kernel compilation flags (no fast-math)\n\
             2. Verify RNG seed derivation matches\n\
             3. Review retrieval tie-breaker ordering\n\
             4. Run: aosctl replay --verbose <bundle>\n\
             5. Compare: diff old_trace new_trace",
            docs = ["docs/ARCHITECTURE.md", "tests/determinism.rs"]
        ),
        error_code!(
            ECode::E2002,
            "Policy Violation",
            "Operation violates configured policy pack constraints.",
            "1. Review policy pack: cat configs/cp.toml\n\
             2. Check specific violation in trace\n\
             3. Adjust policy or fix operation\n\
             4. Re-audit: aosctl audit <cpid>",
            docs = ["docs/ARCHITECTURE.md", "crates/adapteros-policy/"]
        ),
        error_code!(
            ECode::E2003,
            "Egress Violation",
            "Attempted network access while serving in deny_all mode.",
            "1. Verify PF rules are active: aosctl diag --system\n\
             2. Check for DNS/network calls in adapters\n\
             3. Review egress policy configuration\n\
             4. Validate offline operation mode",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        error_code!(
            ECode::E2004,
            "Refusal Threshold Not Met",
            "Evidence below minimum confidence threshold for factual claim.",
            "1. Check abstain_threshold in policy\n\
             2. Verify RAG retrieval returned sufficient spans\n\
             3. Review evidence quality\n\
             4. Consider retraining or updating index",
            docs = ["docs/ARCHITECTURE.md", "crates/adapteros-lora-rag/"]
        ),
        // E3xxx: Kernels/Build/Manifest Issues
        error_code!(
            ECode::E3001,
            "Kernel Manifest Signature Invalid",
            "The Metal kernel manifest signature verification failed.",
            "1. Rebuild kernels: cd metal && ./build.sh\n\
             2. Verify CI signing key\n\
             3. Check kernel hash: aosctl verify-kernel\n\
             4. Ensure toolchain.toml matches build environment",
            docs = ["metal/build.sh", "docs/metal/phase4-metal-kernels.md"]
        ),
        error_code!(
            ECode::E3002,
            "Kernel Hash Mismatch",
            "Loaded kernel hash doesn't match Plan manifest.",
            "1. Verify kernel .metallib present and unmodified\n\
             2. Check Plan manifest kernel_hash field\n\
             3. Rebuild Plan: aosctl build-plan <manifest>\n\
             4. Refuse serving if mismatch persists",
            docs = ["crates/mplora-kernel-mtl/", "crates/mplora-plan/"]
        ),
        error_code!(
            ECode::E3003,
            "Invalid Manifest",
            "Manifest JSON is malformed or missing required fields.",
            "1. Validate JSON: jq . < manifest.json\n\
             2. Check required fields: model_id, adapters, policy\n\
             3. Review manifest examples: manifests/\n\
             4. Use example: manifests/qwen7b.yaml",
            docs = ["manifests/"]
        ),
        error_code!(
            ECode::E3004,
            "Metal Device Not Found",
            "No compatible Metal GPU device detected.",
            "1. Verify macOS system with Apple Silicon\n\
             2. Check Metal support: system_profiler SPDisplaysDataType\n\
             3. Update macOS if needed\n\
             4. For dev: use --mock-metal flag",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        // E4xxx: Telemetry/Chain Problems
        error_code!(
            ECode::E4001,
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
            ECode::E4002,
            "Telemetry Write Failed",
            "Cannot write telemetry events to bundle.",
            "1. Check disk space: df -h var/telemetry\n\
             2. Verify write permissions\n\
             3. Check bundle size limits in policy\n\
             4. Review telemetry configuration",
            docs = ["crates/mplora-telemetry/"]
        ),
        error_code!(
            ECode::E4003,
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
            ECode::E5001,
            "Artifact Not Found in CAS",
            "Content-addressed artifact missing from store.",
            "1. Verify hash: echo <hash>\n\
             2. Check CAS root: ls -la var/cas/\n\
             3. Re-import artifact: aosctl import <bundle>\n\
             4. Verify SBOM completeness",
            docs = ["crates/mplora-artifacts/"]
        ),
        error_code!(
            ECode::E5002,
            "SBOM Incomplete",
            "SBOM missing required artifacts or metadata.",
            "1. Validate SBOM: jq . < sbom.json\n\
             2. Check all artifacts listed have hashes\n\
             3. Regenerate SBOM: aosctl generate-sbom <dir>\n\
             4. Re-sign bundle after fixing",
            docs = ["crates/mplora-sbom/", "crates/mplora-artifacts/"]
        ),
        error_code!(
            ECode::E5003,
            "Bundle Extraction Failed",
            "Failed to extract artifact bundle.",
            "1. Verify bundle is valid ZIP format\n\
             2. Check disk space\n\
             3. Verify file permissions\n\
             4. Re-download or recreate bundle",
            docs = ["crates/mplora-artifacts/"]
        ),
        error_code!(
            ECode::E5004,
            "Hash Mismatch",
            "Computed artifact hash doesn't match expected value.",
            "1. Verify artifact file integrity\n\
             2. Recompute: aosctl hash <file>\n\
             3. Check for file corruption or tampering\n\
             4. Re-import from trusted source",
            docs = ["crates/adapteros-core/src/hash.rs"]
        ),
        // E6xxx: Adapters/DIR Issues
        error_code!(
            ECode::E6001,
            "Adapter Not Found in Registry",
            "Specified adapter ID not registered or not allowed by ACL.",
            "1. List adapters: aosctl list-adapters\n\
             2. Register: aosctl register-adapter <id> <hash>\n\
             3. Verify ACL permissions\n\
             4. Check tenant isolation",
            docs = ["crates/mplora-registry/"]
        ),
        error_code!(
            ECode::E6002,
            "Adapter Eviction Occurred",
            "Adapter evicted due to memory pressure or low activation.",
            "1. Check memory headroom: aosctl diag --system\n\
             2. Review eviction policy in manifest\n\
             3. Pin critical adapters: aosctl pin-adapter\n\
             4. Reduce K or adapter count",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        error_code!(
            ECode::E6003,
            "Router Skew Detected",
            "Router gate distribution exceeds entropy floor.",
            "1. Check router calibration\n\
             2. Verify entropy_floor setting\n\
             3. Review adapter activation patterns\n\
             4. Rebuild Plan if needed",
            docs = ["crates/mplora-router/"]
        ),
        error_code!(
            ECode::E6004,
            "Adapter Quality Below Threshold",
            "Adapter quality delta below minimum threshold for retention.",
            "1. Review min_quality_delta in policy\n\
             2. Check adapter performance metrics\n\
             3. Retrain adapter with better data\n\
             4. Adjust quality threshold if appropriate",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        error_code!(
            ECode::E6005,
            "Adapter Socket Connection Failed",
            "Cannot connect to worker socket for adapter operations.",
            "1. Check if worker is running: aosctl serve status\n\
             2. Start worker if needed: aosctl serve start\n\
             3. Verify socket path: ./var/run/aos/<tenant>/worker.sock\n\
             4. Check tenant isolation and permissions",
            docs = ["crates/adapteros-client/"]
        ),
        error_code!(
            ECode::E6006,
            "Invalid Adapter ID Format",
            "Adapter ID contains invalid characters or exceeds length limit.",
            "1. Use only alphanumeric characters, hyphens, and underscores\n\
             2. Keep adapter ID under 64 characters\n\
             3. Avoid special characters and spaces\n\
             4. Examples: 'python-general', 'adapter_2', 'rust-helper'",
            docs = ["crates/adapteros-cli/src/commands/adapter.rs"]
        ),
        error_code!(
            ECode::E6007,
            "Adapter Command Failed",
            "Adapter lifecycle command (promote/demote/pin/unpin) failed.",
            "1. Check adapter exists: aosctl adapter list\n\
             2. Verify adapter is in correct state for operation\n\
             3. Check worker logs for detailed error\n\
             4. Ensure adapter is not locked or in use",
            docs = ["crates/adapteros-lora-worker/src/adapter_hotswap.rs"]
        ),
        error_code!(
            ECode::E6008,
            "Kernel Version Mismatch",
            "Adapter kernel version does not match runtime kernel version.",
            "1. Check adapter kernel version: aosctl adapter info <id>\n\
             2. Verify runtime kernel version: aosctl diag --system\n\
             3. Retrain adapter with current kernel version\n\
             4. Update adapter metadata to match kernel version",
            docs = [
                "crates/adapteros-lora-kernel-mtl/",
                "crates/adapteros-lora-kernel-coreml/"
            ]
        ),
        error_code!(
            ECode::E6009,
            "Base Model Mismatch",
            "Adapters in stack target different base models.",
            "1. List adapters in stack: aosctl stack info <id>\n\
             2. Check base model for each adapter: aosctl adapter info <id>\n\
             3. Remove incompatible adapters from stack\n\
             4. Ensure all adapters target the same base model",
            docs = ["crates/adapteros-lora-lifecycle/", "docs/ARCHITECTURE.md"]
        ),
        // E7xxx: Node/Cluster Problems
        error_code!(
            ECode::E7001,
            "Node Unavailable",
            "Worker node not responding or unreachable.",
            "1. Check node status: aosctl node-status\n\
             2. Verify UDS socket: ls -la /var/run/aos/\n\
             3. Restart worker if needed\n\
             4. Check logs: tail -f var/logs/worker.log",
            docs = ["crates/mplora-node/"]
        ),
        error_code!(
            ECode::E7002,
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
            ECode::E8001,
            "Invalid Configuration",
            "Configuration file malformed or missing required fields.",
            "1. Check config syntax: cat configs/cp.toml\n\
             2. Validate TOML: taplo check configs/cp.toml\n\
             3. Review example: configs/cp.toml.example\n\
             4. Check environment variables",
            docs = ["configs/"]
        ),
        error_code!(
            ECode::E8002,
            "Missing Required Argument",
            "Command requires argument that was not provided.",
            "1. Run: aosctl <command> --help\n\
             2. Check command syntax in manual\n\
             3. Review examples: aosctl tutorial",
            docs = []
        ),
        error_code!(
            ECode::E8003,
            "Database Connection Failed",
            "Cannot connect to control plane database.",
            "1. Check database file exists: ls var/aos-cp.sqlite3\n\
             2. Verify permissions\n\
             3. Initialize if needed: aosctl init-cp\n\
             4. Check DATABASE_URL environment variable",
            docs = ["crates/mplora-db/"]
        ),
        error_code!(
            ECode::E8004,
            "Required Config File Missing",
            "The specified configuration file does not exist.",
            "1. Verify the config file path is correct\n\
             2. Create the config file: cp configs/cp.toml.example configs/cp.toml\n\
             3. Or remove the --config flag to use compiled defaults",
            docs = ["configs/"]
        ),
        error_code!(
            ECode::E8005,
            "Config File Permission Denied",
            "Configuration file exists but cannot be read due to permissions.",
            "1. Check file permissions: ls -la <config-file>\n\
             2. Fix permissions: chmod 644 <config-file>\n\
             3. Ensure the process user has read access",
            docs = ["docs/CONFIGURATION.md"]
        ),
        error_code!(
            ECode::E8006,
            "Config File Parse Error",
            "Configuration file contains invalid TOML syntax.",
            "1. Validate TOML syntax: taplo check <config-file>\n\
             2. Check for typos, missing quotes, or invalid values\n\
             3. Review example: configs/cp.toml.example",
            docs = ["configs/"]
        ),
        error_code!(
            ECode::E8007,
            "Empty Environment Variable",
            "An environment variable was set but contains only whitespace.",
            "1. Check the variable value: echo $AOS_<VAR>\n\
             2. Either set a valid value or unset the variable\n\
             3. Empty values are ignored; unset to use defaults",
            docs = ["docs/CONFIGURATION.md"]
        ),
        error_code!(
            ECode::E8008,
            "Invalid Secret Value",
            "A required secret is blank, whitespace, or uses a placeholder value.",
            "1. Generate a secure secret: openssl rand -base64 32\n\
             2. Set the secret in environment or config file\n\
             3. Never use placeholder values like 'changeme' in production",
            docs = ["docs/CONFIGURATION.md"]
        ),
        error_code!(
            ECode::E8009,
            "Deprecated Flag Used",
            "A deprecated CLI flag was used that will be removed in a future version.",
            "1. Replace the deprecated flag with its replacement\n\
             2. Check release notes for migration guide\n\
             3. Update any scripts using this flag",
            docs = ["docs/CLI_REFERENCE.md"]
        ),
        error_code!(
            ECode::E8010,
            "Output Format Mismatch",
            "CLI output format version doesn't match expected schema.",
            "1. Update client consuming CLI output\n\
             2. Check --format-version flag\n\
             3. Review API changelog for schema changes",
            docs = ["docs/CLI_REFERENCE.md"]
        ),
        error_code!(
            ECode::E8011,
            "Write Permission Denied",
            "CLI cannot write to the specified output directory.",
            "1. Check directory permissions: ls -la <dir>\n\
             2. Use --output to specify a writable location\n\
             3. Run with appropriate permissions",
            docs = ["docs/TROUBLESHOOTING.md"]
        ),
        error_code!(
            ECode::E8012,
            "Invalid Input Encoding",
            "Input contains binary data but UTF-8 was expected.",
            "1. Check input file encoding: file <input>\n\
             2. Use --binary flag for binary input\n\
             3. Convert input to UTF-8: iconv -f <encoding> -t UTF-8",
            docs = ["docs/CLI_REFERENCE.md"]
        ),
        error_code!(
            ECode::E8013,
            "Invalid Retry Attempt",
            "Attempted to retry a non-retriable error.",
            "1. Check the original error type\n\
             2. Non-retriable errors include: auth, validation, policy violations\n\
             3. Fix the underlying issue before retrying",
            docs = ["docs/TROUBLESHOOTING.md"]
        ),
        // E3xxx extensions: Build/Toolchain errors
        error_code!(
            ECode::E3005,
            "Toolchain Version Mismatch",
            "Build toolchain differs from CI-verified version.",
            "1. Check rust-toolchain.toml: cat rust-toolchain.toml\n\
             2. Run: rustup override set <version>\n\
             3. Verify: rustc --version",
            docs = ["rust-toolchain.toml"]
        ),
        error_code!(
            ECode::E3006,
            "Stale Build Cache",
            "Build cache may hide compilation errors.",
            "1. Clean build cache: cargo clean\n\
             2. Rebuild: cargo build --release\n\
             3. Verify CI matches local build",
            docs = ["docs/BUILD.md"]
        ),
        error_code!(
            ECode::E3007,
            "Lint Target Missing",
            "Cannot run lints without building target first.",
            "1. Build target: cargo build --target <target>\n\
             2. Then run lints: cargo clippy --target <target>",
            docs = ["docs/BUILD.md"]
        ),
        error_code!(
            ECode::E3008,
            "Cargo.lock Out of Sync",
            "Cargo.lock doesn't match Cargo.toml dependencies.",
            "1. Update lock file: cargo update\n\
             2. Or: cargo generate-lockfile\n\
             3. Commit the updated Cargo.lock",
            docs = ["Cargo.lock"]
        ),
        error_code!(
            ECode::E3009,
            "Workspace Member Path Invalid",
            "Workspace member references an invalid path.",
            "1. Check workspace members in Cargo.toml\n\
             2. Verify member directory exists\n\
             3. Update path if directory was moved",
            docs = ["Cargo.toml"]
        ),
        // E9xxx: OS/Environment Issues
        error_code!(
            ECode::E9001,
            "Insufficient Memory",
            "System memory below minimum threshold for operation.",
            "1. Check memory: aosctl diag --system\n\
             2. Close other applications\n\
             3. Reduce adapter count or K value\n\
             4. Consider larger model tier or machine",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        error_code!(
            ECode::E9002,
            "Permission Denied",
            "Insufficient permissions for operation.",
            "1. Check file/directory permissions\n\
             2. Verify tenant UID/GID mapping\n\
             3. Run with appropriate privileges if needed\n\
             4. Check isolation policy",
            docs = ["docs/ARCHITECTURE.md"]
        ),
        error_code!(
            ECode::E9003,
            "Service Not Running",
            "Required system service (aos-secd) not running.",
            "1. Check service: ps aux | grep aos-secd\n\
             2. Start service: launchctl load scripts/aos-secd.plist\n\
             3. Check logs: tail -f /var/log/aos-secd.log\n\
             4. Verify launchd configuration",
            docs = ["scripts/aos-secd.plist"]
        ),
        error_code!(
            ECode::E9004,
            "Disk Space Insufficient",
            "Insufficient disk space for operation.",
            "1. Check space: df -h\n\
             2. Clean old telemetry bundles: aosctl gc-bundles\n\
             3. Remove unused adapters\n\
             4. Archive old CPs",
            docs = ["scripts/gc_bundles.sh"]
        ),
        error_code!(
            ECode::E9005,
            "CPU Throttled",
            "Process CPU usage exceeded configured limits, causing throttling.",
            "1. Check CPU-intensive operations: aosctl diag --system\n\
             2. Reduce concurrent inference requests\n\
             3. Increase max_cpu_time_per_request in config\n\
             4. Consider scaling horizontally",
            docs = ["docs/CONFIGURATION.md"]
        ),
        error_code!(
            ECode::E9006,
            "Out of Memory",
            "Process memory usage exceeded limits, triggering OOM condition.",
            "1. Check memory usage: aosctl diag --system\n\
             2. Reduce max_concurrent_requests\n\
             3. Evict unused adapters: aosctl adapter evict\n\
             4. Increase system memory or container limits\n\
             5. Consider quantized model variants",
            docs = ["docs/ARCHITECTURE.md", "crates/adapteros-lora-worker/"]
        ),
        error_code!(
            ECode::E9007,
            "File Descriptor Limit Reached",
            "Process exhausted available file descriptors.",
            "1. Check current usage: lsof -p $(pgrep aos) | wc -l\n\
             2. Increase ulimit: ulimit -n 65536\n\
             3. Check for file/socket leaks\n\
             4. Reduce connection pool sizes if applicable",
            docs = ["docs/TROUBLESHOOTING.md"]
        ),
        error_code!(
            ECode::E9008,
            "Thread Pool Saturated",
            "All worker threads are busy, causing request queuing.",
            "1. Check thread pool status: aosctl diag --system\n\
             2. Reduce concurrent_requests setting\n\
             3. Scale horizontally for higher throughput\n\
             4. Check for blocking operations in hot paths",
            docs = ["docs/CONFIGURATION.md"]
        ),
        error_code!(
            ECode::E9009,
            "GPU Device Unavailable",
            "Metal/GPU device became unavailable during operation.",
            "1. Check GPU status: system_profiler SPDisplaysDataType\n\
             2. Verify no other processes hold exclusive GPU access\n\
             3. Check Activity Monitor for GPU memory pressure\n\
             4. Restart the worker process: aosctl serve restart\n\
             5. Use --backend=coreml for ANE fallback if available",
            docs = ["docs/BACKEND_SELECTION.md"]
        ),
    ]
}

/// Get error code info by typed ECode (compile-time checked)
pub fn get(ecode: ECode) -> ErrorCode {
    TYPED_REGISTRY
        .get(&ecode)
        .cloned()
        .expect("All ECode variants must have corresponding ErrorCode entries")
}

/// Find error code by code string (e.g., "E3001")
///
/// For compile-time checked lookups with known codes, use `get(ECode::E3001)` instead.
/// This function is appropriate for runtime lookups with dynamic/user-provided codes.
#[deprecated(
    since = "0.12.0",
    note = "Use get(ECode::E3001) for compile-time checked lookups. \
            This function remains valid for runtime/dynamic string lookups."
)]
pub fn find_by_code(code: &str) -> Option<ErrorCode> {
    ECode::parse(code).map(get)
}

/// Alias for find_by_code
#[deprecated(
    since = "0.12.0",
    note = "Use get(ECode::E3001) for compile-time checked lookups. \
            Use find_by_code() for runtime/dynamic string lookups."
)]
pub fn get_error_code(code: &str) -> Option<ErrorCode> {
    #[allow(deprecated)]
    find_by_code(code)
}

/// Registry of error codes for fast lookup by string
pub static REGISTRY: Lazy<HashMap<&'static str, ErrorCode>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for code in all_error_codes() {
        m.insert(code.code, code);
    }
    m
});

/// Registry of error codes for fast lookup by typed ECode
pub static TYPED_REGISTRY: Lazy<HashMap<ECode, ErrorCode>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for code in all_error_codes() {
        m.insert(code.ecode, code);
    }
    m
});

/// Find error code by AosError variant name
/// Returns typed ECode for compile-time safety
pub fn ecode_for_aos_error(error_name: &str) -> Option<ECode> {
    match error_name {
        "InvalidHash" => Some(ECode::E1004),
        "InvalidCPID" => Some(ECode::E8001),
        "Crypto" => Some(ECode::E1001),
        "PolicyViolation" => Some(ECode::E2002),
        "InvalidManifest" => Some(ECode::E3003),
        "Kernel" => Some(ECode::E3002),
        "Telemetry" => Some(ECode::E4002),
        "DeterminismViolation" => Some(ECode::E2001),
        "EgressViolation" => Some(ECode::E2003),
        "Artifact" => Some(ECode::E5001),
        "Registry" => Some(ECode::E6001),
        "Worker" => Some(ECode::E7001),
        "Node" => Some(ECode::E7001),
        "Job" => Some(ECode::E7002),
        "Config" => Some(ECode::E8001),
        "Database" => Some(ECode::E8003),
        "Io" | "Parse" => Some(ECode::E9002),
        "CpuThrottled" => Some(ECode::E9005),
        "OutOfMemory" => Some(ECode::E9006),
        "FileDescriptorExhausted" => Some(ECode::E9007),
        "ThreadPoolSaturated" => Some(ECode::E9008),
        "GpuUnavailable" => Some(ECode::E9009),
        "ToolchainMismatch" => Some(ECode::E3005),
        "StaleBuildCache" => Some(ECode::E3006),
        "LintTargetMissing" => Some(ECode::E3007),
        "LockfileOutOfSync" => Some(ECode::E3008),
        "WorkspaceMemberPathInvalid" => Some(ECode::E3009),
        "DeprecatedFlag" => Some(ECode::E8009),
        "OutputFormatMismatch" => Some(ECode::E8010),
        "CliWritePermissionDenied" => Some(ECode::E8011),
        "InvalidInputEncoding" => Some(ECode::E8012),
        "InvalidRetryAttempt" => Some(ECode::E8013),
        "RateLimiterNotConfigured" | "InvalidRateLimitConfig" | "ThunderingHerdRejected" => {
            Some(ECode::E9008)
        }
        _ => None,
    }
}

/// Find error code by AosError variant name (backward compatible)
///
/// For compile-time checked lookups, use `ecode_for_aos_error()` to get the typed ECode,
/// then `get()` to retrieve the full ErrorCode.
#[deprecated(
    since = "0.12.0",
    note = "Use ecode_for_aos_error() for typed ECode, then get() for full ErrorCode"
)]
pub fn find_by_aos_error(error_name: &str) -> Option<ErrorCode> {
    ecode_for_aos_error(error_name).map(get)
}

/// Map AosError variant names to ECode (typed version)
pub fn map_aos_error_to_ecode(name: &str) -> ECode {
    match name {
        "PolicyViolation" => ECode::E2001,
        "InvalidHash" => ECode::E3002,
        "ManifestMissing" => ECode::E3003,
        "TelemetryGap" => ECode::E4002,
        "SignatureInvalid" => ECode::E1001,
        "AdapterIncompatible" => ECode::E6003,
        _ => ECode::E9001, // Default to OS/env
    }
}

// Backward compatible - keep old function signature
#[deprecated(note = "Use map_aos_error_to_ecode for typed return")]
pub fn map_aos_error(name: &str) -> &'static str {
    map_aos_error_to_ecode(name).as_str()
}

/// Adapter kernel version does not match runtime kernel version
pub const KERNEL_VERSION_MISMATCH: &str = "E_KERNEL_VERSION_MISMATCH";

/// Adapters in stack target different base models
pub const BASE_MODEL_MISMATCH: &str = "E_BASE_MODEL_MISMATCH";

/// Numeric exit codes for CLI commands
///
/// Categories:
/// - 1-9: General errors
/// - 10-19: Configuration errors
/// - 20-29: Database errors
/// - 30-39: Network errors
/// - 40-49: Crypto errors
/// - 50-59: Policy errors
/// - 60-69: Validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    // General errors (1-9)
    Success = 0,
    GeneralError = 1,
    InternalError = 2,
    NotFound = 3,
    Timeout = 4,
    ResourceExhaustion = 5,
    Unavailable = 6,
    FeatureDisabled = 7,
    Other = 8,

    // Configuration errors (10-19)
    Config = 10,
    InvalidManifest = 11,
    Parse = 12,
    Toolchain = 13,
    DeprecatedFlag = 14,
    OutputFormat = 15,
    InputEncoding = 16,
    InvalidRetry = 17,
    RateLimitConfig = 18,

    // Database errors (20-29)
    Database = 20,
    Sqlite = 21,
    Sqlx = 22,
    Registry = 23,

    // Network errors (30-39)
    Network = 30,
    Http = 31,
    UdsConnection = 32,
    WorkerNotResponding = 33,
    CircuitBreakerOpen = 34,
    InvalidResponse = 35,

    // Crypto errors (40-49)
    Crypto = 40,
    InvalidHash = 41,
    Encryption = 42,
    Decryption = 43,
    InvalidSealedData = 44,

    // Policy errors (50-59)
    PolicyViolation = 50,
    Policy = 51,
    DeterminismViolation = 52,
    EgressViolation = 53,
    IsolationViolation = 54,
    PerformanceViolation = 55,
    Quarantined = 56,
    PolicyHashMismatch = 57,

    // Validation errors (60-69)
    Validation = 60,
    InvalidCPID = 61,
    AdapterHashMismatch = 62,
    KernelLayoutMismatch = 63,
    RngError = 64,

    // Auth errors (70-79)
    Auth = 70,
    Authz = 71,

    // Worker/Job errors (80-89)
    Worker = 80,
    Job = 81,
    Node = 82,
    Lifecycle = 83,

    // Subsystem errors (90-99)
    Io = 90,
    Serialization = 91,
    Memory = 92,
    MemoryPressure = 93,
    Kernel = 94,
    Mtl = 95,
    CoreML = 96,
    Mlx = 97,
    Routing = 98,
    Platform = 99,

    // Domain errors (100-119)
    Telemetry = 100,
    Artifact = 101,
    Plan = 102,
    Replay = 103,
    Verification = 104,
    Rag = 105,
    Git = 106,
    Training = 107,
    Autograd = 108,
    Quantization = 109,
    ChatTemplate = 110,
    BaseLLM = 111,
    Promotion = 112,
    Anomaly = 113,
    System = 114,
    DeterministicExecutor = 115,

    // Model Hub errors (120-129)
    DownloadFailed = 120,
    CacheCorruption = 121,
    HealthCheckFailed = 122,
    ModelNotFound = 123,
    ModelAcquisitionInProgress = 124,
}

impl From<&adapteros_core::AosError> for ExitCode {
    fn from(error: &adapteros_core::AosError) -> Self {
        use adapteros_core::AosError;
        match error {
            // General errors (1-9)
            AosError::Internal(_) => ExitCode::InternalError,
            AosError::NotFound(_) => ExitCode::NotFound,
            AosError::Timeout { .. } => ExitCode::Timeout,
            AosError::ResourceExhaustion(_) => ExitCode::ResourceExhaustion,
            AosError::Unavailable(_) => ExitCode::Unavailable,
            AosError::FeatureDisabled { .. } => ExitCode::FeatureDisabled,
            AosError::WithContext { source, .. } => ExitCode::from(source.as_ref()),

            // Configuration errors (10-19)
            AosError::Config(_) => ExitCode::Config,
            AosError::InvalidManifest(_) => ExitCode::InvalidManifest,
            AosError::Parse(_) => ExitCode::Parse,
            AosError::Toolchain(_) => ExitCode::Toolchain,
            // Build/Toolchain errors (Category 20)
            AosError::ToolchainMismatch { .. } => ExitCode::Toolchain,
            AosError::StaleBuildCache { .. } => ExitCode::Toolchain,
            AosError::LintTargetMissing { .. } => ExitCode::Toolchain,
            AosError::LockfileOutOfSync { .. } => ExitCode::Toolchain,
            AosError::WorkspaceMemberPathInvalid { .. } => ExitCode::Toolchain,
            // CLI errors (Category 21)
            AosError::DeprecatedFlag { .. } => ExitCode::DeprecatedFlag,
            AosError::OutputFormatMismatch { .. } => ExitCode::OutputFormat,
            AosError::CliWritePermissionDenied { .. } => ExitCode::Io,
            AosError::InvalidInputEncoding { .. } => ExitCode::InputEncoding,
            AosError::InvalidRetryAttempt { .. } => ExitCode::InvalidRetry,
            // Rate limiting errors (Category 23)
            AosError::RateLimiterNotConfigured { .. } => ExitCode::RateLimitConfig,
            AosError::InvalidRateLimitConfig { .. } => ExitCode::RateLimitConfig,
            AosError::ThunderingHerdRejected { .. } => ExitCode::ResourceExhaustion,

            // Database errors (20-29)
            AosError::Database(_) | AosError::DatabaseError { .. } => ExitCode::Database,
            AosError::Sqlite(_) => ExitCode::Sqlite,
            AosError::Sqlx(_) => ExitCode::Sqlx,
            AosError::Registry(_) => ExitCode::Registry,

            // Network errors (30-39)
            AosError::Network(_) => ExitCode::Network,
            AosError::Http(_) => ExitCode::Http,
            AosError::UdsConnectionFailed { .. } => ExitCode::UdsConnection,
            AosError::WorkerNotResponding { .. } => ExitCode::WorkerNotResponding,
            AosError::CircuitBreakerOpen { .. } | AosError::CircuitBreakerHalfOpen { .. } => {
                ExitCode::CircuitBreakerOpen
            }
            AosError::InvalidResponse { .. } => ExitCode::InvalidResponse,
            AosError::Federation(_) => ExitCode::Network, // Federation errors treated as network errors

            // Crypto errors (40-49)
            AosError::Crypto(_) => ExitCode::Crypto,
            AosError::InvalidHash(_) => ExitCode::InvalidHash,
            AosError::EncryptionFailed { .. } => ExitCode::Encryption,
            AosError::DecryptionFailed { .. } => ExitCode::Decryption,
            AosError::InvalidSealedData { .. } => ExitCode::InvalidSealedData,

            // Policy errors (50-59)
            AosError::PolicyViolation(_) => ExitCode::PolicyViolation,
            AosError::Policy(_) => ExitCode::Policy,
            AosError::DeterminismViolation(_) => ExitCode::DeterminismViolation,
            AosError::EgressViolation(_) => ExitCode::EgressViolation,
            AosError::IsolationViolation(_) => ExitCode::IsolationViolation,
            AosError::PerformanceViolation(_) => ExitCode::PerformanceViolation,
            AosError::Quarantined(_) => ExitCode::Quarantined,
            AosError::PolicyHashMismatch { .. } => ExitCode::PolicyHashMismatch,

            // Validation errors (60-69)
            AosError::Validation(_) => ExitCode::Validation,
            AosError::InvalidCPID(_) => ExitCode::InvalidCPID,
            AosError::AdapterHashMismatch { .. } | AosError::AdapterLayerHashMismatch { .. } => {
                ExitCode::AdapterHashMismatch
            }
            AosError::KernelLayoutMismatch { .. } => ExitCode::KernelLayoutMismatch,
            AosError::RngError { .. } => ExitCode::RngError,
            AosError::SegmentHashMismatch { .. } => ExitCode::Validation,
            AosError::MissingSegment { .. } => ExitCode::Validation,
            AosError::MissingCanonicalSegment => ExitCode::Validation,

            // Auth errors (70-79)
            AosError::Auth(_) => ExitCode::Auth,
            AosError::Authz(_) => ExitCode::Authz,

            // Worker/Job errors (80-89)
            AosError::Worker(_) => ExitCode::Worker,
            AosError::Job(_) => ExitCode::Job,
            AosError::Node(_) => ExitCode::Node,
            AosError::Lifecycle(_) => ExitCode::Lifecycle,
            AosError::AdapterNotLoaded { .. } => ExitCode::Worker,

            // Subsystem errors (90-99)
            AosError::Io(_) => ExitCode::Io,
            AosError::Serialization(_) => ExitCode::Serialization,
            AosError::Memory(_) => ExitCode::Memory,
            AosError::MemoryPressure(_) => ExitCode::MemoryPressure,
            AosError::Kernel(_) => ExitCode::Kernel,
            AosError::Mtl(_) => ExitCode::Mtl,
            AosError::CoreML(_) => ExitCode::CoreML,
            AosError::Mlx(_) => ExitCode::Mlx,

            // Domain errors (100-119)
            AosError::Telemetry(_) => ExitCode::Telemetry,
            AosError::Artifact(_) => ExitCode::Artifact,
            AosError::Plan(_) => ExitCode::Plan,
            AosError::Replay(_) => ExitCode::Replay,
            AosError::Verification(_) => ExitCode::Verification,
            AosError::Rag(_) => ExitCode::Rag,
            AosError::Git(_) => ExitCode::Git,
            AosError::Training(_) => ExitCode::Training,
            AosError::Autograd(_) => ExitCode::Autograd,
            AosError::Quantization(_) => ExitCode::Quantization,
            AosError::ChatTemplate(_) => ExitCode::ChatTemplate,
            AosError::BaseLLM(_) => ExitCode::BaseLLM,
            AosError::Promotion(_) => ExitCode::Promotion,
            AosError::Anomaly(_) => ExitCode::Anomaly,
            AosError::System(_) => ExitCode::System,
            AosError::DeterministicExecutor(_) => ExitCode::DeterministicExecutor,
            AosError::Routing(_) => ExitCode::Routing,

            // Model Hub errors (120-129)
            AosError::DownloadFailed { .. } => ExitCode::DownloadFailed,
            AosError::CacheCorruption { .. } => ExitCode::CacheCorruption,
            AosError::HealthCheckFailed { .. } => ExitCode::HealthCheckFailed,
            AosError::ModelNotFound { .. } => ExitCode::ModelNotFound,
            AosError::ModelAcquisitionInProgress { .. } => ExitCode::ModelAcquisitionInProgress,
            AosError::Platform(_) => ExitCode::Platform,
            AosError::AdapterNotInManifest { .. } => ExitCode::Validation,
            AosError::AdapterNotInEffectiveSet { .. } => ExitCode::Validation,
            _ => ExitCode::Other,
        }
    }
}

impl From<adapteros_core::AosError> for ExitCode {
    fn from(error: adapteros_core::AosError) -> Self {
        ExitCode::from(&error)
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        code as i32
    }
}

impl ExitCode {
    /// Convert to process exit code
    pub fn as_exit_code(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self as u8)
    }

    /// Get the category name for this exit code
    pub fn category(&self) -> &'static str {
        let code = *self as u8;
        match code {
            0 => "Success",
            1..=9 => "General",
            10..=19 => "Configuration",
            20..=29 => "Database",
            30..=39 => "Network",
            40..=49 => "Crypto",
            50..=59 => "Policy",
            60..=69 => "Validation",
            70..=79 => "Auth",
            80..=89 => "Worker/Job",
            90..=99 => "Subsystem",
            100..=119 => "Domain",
            _ => "Unknown",
        }
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
    #[allow(deprecated)] // Testing the deprecated function intentionally
    fn test_find_by_code() {
        assert!(find_by_code("E3001").is_some());
        assert!(find_by_code("E9999").is_none());
    }

    #[test]
    #[allow(deprecated)] // Testing the deprecated function intentionally
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
                "E6" => assert_eq!(code.category, "Adapters/DIR"),
                "E7" => assert_eq!(code.category, "Node/Cluster"),
                "E8" => assert_eq!(code.category, "CLI/Config"),
                "E9" => assert_eq!(code.category, "OS/Environment"),
                _ => panic!("Invalid code prefix: {}", prefix),
            }
        }
    }

    #[test]
    fn test_exit_code_categories() {
        assert_eq!(ExitCode::Success.category(), "Success");
        assert_eq!(ExitCode::GeneralError.category(), "General");
        assert_eq!(ExitCode::Config.category(), "Configuration");
        assert_eq!(ExitCode::Database.category(), "Database");
        assert_eq!(ExitCode::Network.category(), "Network");
        assert_eq!(ExitCode::Crypto.category(), "Crypto");
        assert_eq!(ExitCode::PolicyViolation.category(), "Policy");
        assert_eq!(ExitCode::Validation.category(), "Validation");
        assert_eq!(ExitCode::Auth.category(), "Auth");
        assert_eq!(ExitCode::Worker.category(), "Worker/Job");
        assert_eq!(ExitCode::Io.category(), "Subsystem");
        assert_eq!(ExitCode::Telemetry.category(), "Domain");
    }

    #[test]
    fn test_exit_code_from_aos_error() {
        use adapteros_core::AosError;

        // Test various error types map to correct exit codes
        let config_err = AosError::Config("test".to_string());
        assert_eq!(ExitCode::from(&config_err), ExitCode::Config);

        let db_err = AosError::Database("test".to_string());
        assert_eq!(ExitCode::from(&db_err), ExitCode::Database);

        let policy_err = AosError::PolicyViolation("test".to_string());
        assert_eq!(ExitCode::from(&policy_err), ExitCode::PolicyViolation);

        let crypto_err = AosError::Crypto("test".to_string());
        assert_eq!(ExitCode::from(&crypto_err), ExitCode::Crypto);

        let validation_err = AosError::Validation("test".to_string());
        assert_eq!(ExitCode::from(&validation_err), ExitCode::Validation);

        // Test new Build/Toolchain errors
        let toolchain_err = AosError::ToolchainMismatch {
            component: "rust".to_string(),
            expected: "1.75".to_string(),
            actual: "1.74".to_string(),
            ci_version: None,
        };
        assert_eq!(ExitCode::from(&toolchain_err), ExitCode::Toolchain);

        // Test new CLI errors
        let deprecated_err = AosError::DeprecatedFlag {
            flag: "old-flag".to_string(),
            replacement: "--new-flag".to_string(),
            removal_version: "2.0.0".to_string(),
        };
        assert_eq!(ExitCode::from(&deprecated_err), ExitCode::DeprecatedFlag);

        // Test rate limiting errors
        let rate_limit_err = AosError::RateLimiterNotConfigured {
            reason: "missing config".to_string(),
            limiter_name: "api".to_string(),
        };
        assert_eq!(ExitCode::from(&rate_limit_err), ExitCode::RateLimitConfig);
    }

    #[test]
    fn test_exit_code_numeric_values() {
        // Verify exit codes fall within their category ranges
        assert_eq!(ExitCode::Success as u8, 0);
        assert!((1..=9).contains(&(ExitCode::GeneralError as u8)));
        assert!((10..=19).contains(&(ExitCode::Config as u8)));
        assert!((10..=19).contains(&(ExitCode::DeprecatedFlag as u8)));
        assert!((10..=19).contains(&(ExitCode::OutputFormat as u8)));
        assert!((10..=19).contains(&(ExitCode::InputEncoding as u8)));
        assert!((10..=19).contains(&(ExitCode::InvalidRetry as u8)));
        assert!((10..=19).contains(&(ExitCode::RateLimitConfig as u8)));
        assert!((20..=29).contains(&(ExitCode::Database as u8)));
        assert!((30..=39).contains(&(ExitCode::Network as u8)));
        assert!((40..=49).contains(&(ExitCode::Crypto as u8)));
        assert!((50..=59).contains(&(ExitCode::PolicyViolation as u8)));
        assert!((60..=69).contains(&(ExitCode::Validation as u8)));
    }

    #[test]
    fn test_exit_code_to_i32() {
        let code = ExitCode::Config;
        let value: i32 = code.into();
        assert_eq!(value, 10);
    }
}
