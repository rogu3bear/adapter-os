//! Kernel manifest verification
//!
//! Verifies signed kernel manifests at runtime to ensure determinism
//! and prevent tampering with kernel binaries.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::signature::{PublicKey, Signature};
use adapteros_telemetry::{unified_events::TelemetryEventBuilder, TelemetryWriter};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Kernel manifest containing build metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelManifest {
    pub kernel_hash: String,
    pub xcrun_version: String,
    pub sdk_version: String,
    pub rust_version: String,
    pub build_timestamp: String,
    pub toolchain_metadata: ToolchainMetadata,
}

/// Toolchain metadata for reproducibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainMetadata {
    pub xcode_version: String,
    pub sdk_version: String,
    pub rust_version: String,
    pub metal_version: String,
}

/// Signature metadata for manifest verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSignature {
    pub signature: String,
    pub public_key: String,
    pub algorithm: String,
    pub canonical_json: String,
}

/// Manifest verifier with embedded public key
pub struct ManifestVerifier {
    /// Embedded public key for verification
    public_key: PublicKey,
    /// Telemetry writer for logging verification events
    telemetry: Option<Arc<TelemetryWriter>>,
}

impl ManifestVerifier {
    /// Create a new manifest verifier with embedded public key
    pub fn new(telemetry: Option<Arc<TelemetryWriter>>) -> Result<Self> {
        // Load public key (allow env override)
        let public_key_pem = crate::keys::resolve_public_key_pem();
        let public_key = PublicKey::from_pem(&public_key_pem)
            .map_err(|e| AosError::Crypto(format!("Failed to load embedded public key: {}", e)))?;

        Ok(Self {
            public_key,
            telemetry,
        })
    }

    /// Verify manifest signature and kernel hash
    pub fn verify_manifest(
        &self,
        manifest_json: &str,
        signature_data: &str,
        actual_kernel_hash: B3Hash,
    ) -> Result<KernelManifest> {
        // Parse signature metadata
        let sig_metadata: ManifestSignature =
            serde_json::from_str(signature_data).map_err(AosError::Serialization)?;

        // Verify signature algorithm
        if sig_metadata.algorithm != "Ed25519" {
            return Err(AosError::Crypto(format!(
                "Unsupported signature algorithm: {}",
                sig_metadata.algorithm
            )));
        }

        // Decode signature
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(&sig_metadata.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature base64: {}", e)))?;

        if signature_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid signature length: {}",
                signature_bytes.len()
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&signature_bytes);

        let signature = Signature::from_bytes(&sig_array)
            .map_err(|e| AosError::Crypto(format!("Invalid signature format: {}", e)))?;

        // Verify signature against canonical JSON
        self.public_key
            .verify(sig_metadata.canonical_json.as_bytes(), &signature)
            .map_err(|e| AosError::Crypto(format!("Signature verification failed: {}", e)))?;

        // Verify Metal kernel signature before execution
        let kernel_bytes = include_bytes!("../../../metal/aos_kernels.metallib");
        let kernel_sig = hex::decode("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")
            .map_err(|e| AosError::Crypto(format!("Invalid kernel signature hex: {}", e)))?;
        let mut kernel_sig_array = [0u8; 64];
        kernel_sig_array.copy_from_slice(&kernel_sig);
        let kernel_signature = adapteros_crypto::Signature::from_bytes(&kernel_sig_array)
            .map_err(|e| AosError::Crypto(format!("Invalid kernel signature format: {}", e)))?;

        adapteros_crypto::verify_signature(&self.public_key, kernel_bytes, &kernel_signature)
            .map_err(|e| {
                AosError::Crypto(format!("Metal kernel signature verification failed: {}", e))
            })?;

        // Parse manifest
        let manifest: KernelManifest =
            serde_json::from_str(manifest_json).map_err(AosError::Serialization)?;

        // Verify kernel hash matches
        let expected_hash = B3Hash::from_hex(&manifest.kernel_hash)
            .map_err(|e| AosError::Crypto(format!("Invalid kernel hash in manifest: {}", e)))?;

        if actual_kernel_hash != expected_hash {
            return Err(AosError::DeterminismViolation(format!(
                "Kernel hash mismatch!\n  Expected: {}\n  Actual:   {}\n  \
                This indicates the embedded kernel does not match the manifest.\n  \
                Rebuild with: cargo clean && cargo build",
                expected_hash.to_hex(),
                actual_kernel_hash.to_hex()
            )));
        }

        // Log verification success to telemetry
        self.log_verification_event(&manifest, actual_kernel_hash, true)?;

        Ok(manifest)
    }

    /// Log verification event to telemetry
    fn log_verification_event(
        &self,
        manifest: &KernelManifest,
        kernel_hash: B3Hash,
        success: bool,
    ) -> Result<()> {
        if let Some(telemetry) = &self.telemetry {
            let event = TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("kernel_manifest_verify".to_string()),
                adapteros_telemetry::LogLevel::Info,
                format!(
                    "Kernel manifest verification: {}",
                    if success { "success" } else { "failure" }
                ),
            )
            .metadata(serde_json::json!({
                "kernel_hash": kernel_hash.to_hex(),
                "manifest_hash": B3Hash::hash(manifest.kernel_hash.as_bytes()).to_hex(),
                "verification_result": if success { "success" } else { "failure" },
                "build_timestamp": manifest.build_timestamp,
                "toolchain": manifest.toolchain_metadata
            }))
            .build();

            telemetry.log_event(event)?;
        }

        Ok(())
    }
}

/// Verify embedded manifest at runtime
pub fn verify_embedded_manifest(
    metallib_bytes: &[u8],
    telemetry: Option<Arc<TelemetryWriter>>,
) -> Result<KernelManifest> {
    // Load embedded manifest and signature
    let manifest_json = include_str!("../manifests/metallib_manifest.json");
    let signature_data = include_str!("../manifests/metallib_manifest.json.sig");

    // Create verifier
    let verifier = ManifestVerifier::new(telemetry)?;

    // Compute actual kernel hash
    let actual_hash = B3Hash::hash(metallib_bytes);

    // Allow development override to skip kernel signature verification
    let skip_sig = std::env::var("AOS_SKIP_KERNEL_SIGNATURE_VERIFY")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let manifest = if skip_sig {
        // Parse manifest without signature validation (DEV ONLY)
        tracing::warn!(
            "Skipping kernel signature verification due to AOS_SKIP_KERNEL_SIGNATURE_VERIFY"
        );
        let m: KernelManifest = serde_json::from_str(manifest_json)
            .map_err(AosError::Serialization)?;
        // Still compare kernel hash if present
        if let Ok(expected_hash) = B3Hash::from_hex(&m.kernel_hash) {
            if actual_hash != expected_hash {
                tracing::warn!(
                    expected = %expected_hash.to_short_hex(),
                    actual = %actual_hash.to_short_hex(),
                    "Kernel hash mismatch in dev mode"
                );
            }
        }
        m
    } else {
        // Verify manifest and kernel signature
        verifier.verify_manifest(manifest_json, signature_data, actual_hash)?
    };

    tracing::info!(
        "Kernel manifest verified: hash={}, build={}",
        actual_hash.to_short_hex(),
        manifest.build_timestamp
    );

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_verification() {
        // Create test manifest
        let manifest = KernelManifest {
            kernel_hash: "b3:test123".to_string(),
            xcrun_version: "xcrun 1.0.0".to_string(),
            sdk_version: "14.0".to_string(),
            rust_version: "1.75.0".to_string(),
            build_timestamp: "2024-01-15T10:30:00Z".to_string(),
            toolchain_metadata: ToolchainMetadata {
                xcode_version: "15.2".to_string(),
                sdk_version: "14.0".to_string(),
                rust_version: "1.75.0".to_string(),
                metal_version: "3.1".to_string(),
            },
        };

        let manifest_json =
            serde_json::to_string(&manifest).expect("Test manifest should serialize to JSON");
        let canonical_json = serde_json::to_string_pretty(&manifest)
            .expect("Test manifest should serialize to pretty JSON");

        // Note: This test would need actual signing keys to be complete
        // For now, just test the structure
        assert!(manifest_json.contains("kernel_hash"));
        assert!(canonical_json.contains("kernel_hash"));
    }
}
