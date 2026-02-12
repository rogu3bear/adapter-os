//! Standalone checkpoint signature verification
//!
//! Enables external verification of training checkpoint integrity without
//! importing the full worker crate. Given a `.ckpt` file and its `.sig`
//! sidecar, this module verifies the BLAKE3 hash and Ed25519 signature.
//!
//! ## Sidecar Format (v1)
//!
//! The `.sig` file is JSON with the following fields:
//!
//! | Field            | Type   | Description                                    |
//! |------------------|--------|------------------------------------------------|
//! | `schema_version` | `u8`   | Always `1` for this version                    |
//! | `blake3_hash`    | `str`  | Hex-encoded BLAKE3 hash of the `.ckpt` content |
//! | `signature`      | `str`  | Hex-encoded Ed25519 signature over the hash    |
//! | `public_key`     | `str`  | Hex-encoded Ed25519 public key of the signer   |
//! | `signed_at`      | `str`  | ISO 8601 timestamp of signing                  |
//!
//! The signature is computed over the raw 32-byte BLAKE3 hash (not the hex
//! encoding). Verification recomputes the hash from the `.ckpt` content
//! and checks the Ed25519 signature against the embedded public key.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_crypto::checkpoint_verify::verify_checkpoint_file;
//! use std::path::Path;
//!
//! let result = verify_checkpoint_file(Path::new("model_epoch_0005.ckpt"));
//! match result {
//!     Ok(report) => println!("Valid: signed by {}", report.signer_key_id),
//!     Err(e) => eprintln!("Integrity failure: {}", e),
//! }
//! ```

use crate::{compute_key_id, PublicKey, Signature};
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Expected sidecar schema version.
const EXPECTED_SCHEMA_VERSION: u8 = 1;

/// On-disk representation of the checkpoint signature sidecar.
///
/// This duplicates the shape from `adapteros-lora-worker` intentionally so
/// that verification does not require a dependency on the worker crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckpointSigFile {
    schema_version: u8,
    blake3_hash: B3Hash,
    signature: Signature,
    public_key: PublicKey,
    signed_at: String,
}

/// Successful verification report.
#[derive(Debug, Clone)]
pub struct CheckpointVerifyReport {
    /// BLAKE3 hash of the checkpoint content
    pub blake3_hash: B3Hash,
    /// Key ID of the signer (kid-{blake3(pubkey)[..32]})
    pub signer_key_id: String,
    /// ISO 8601 timestamp when the checkpoint was signed
    pub signed_at: String,
    /// Schema version of the sidecar
    pub schema_version: u8,
}

/// Derive the `.sig` sidecar path from a checkpoint path.
fn sig_path_for(ckpt_path: &Path) -> std::path::PathBuf {
    let mut p = ckpt_path.as_os_str().to_owned();
    p.push(".sig");
    std::path::PathBuf::from(p)
}

/// Verify a checkpoint file's BLAKE3 + Ed25519 signature from its sidecar.
///
/// Reads `{path}` and `{path}.sig`, recomputes the BLAKE3 hash, and verifies
/// the Ed25519 signature. Returns a [`CheckpointVerifyReport`] on success.
///
/// # Errors
///
/// Returns `AosError::CheckpointIntegrity` if:
/// - The `.sig` sidecar is missing or unreadable
/// - The sidecar JSON is malformed
/// - The schema version is unsupported
/// - The BLAKE3 hash does not match the checkpoint content
/// - The Ed25519 signature is invalid
pub fn verify_checkpoint_file(ckpt_path: &Path) -> Result<CheckpointVerifyReport> {
    let sig_path = sig_path_for(ckpt_path);

    // Read checkpoint content
    let content = std::fs::read(ckpt_path).map_err(|e| {
        AosError::CheckpointIntegrity(format!(
            "Failed to read checkpoint {}: {}",
            ckpt_path.display(),
            e
        ))
    })?;

    // Read sidecar
    let sig_json = std::fs::read(&sig_path).map_err(|e| {
        AosError::CheckpointIntegrity(format!(
            "Failed to read signature sidecar {}: {}",
            sig_path.display(),
            e
        ))
    })?;

    verify_checkpoint_bytes(&content, &sig_json)
}

/// Verify checkpoint content bytes against a signature sidecar's JSON bytes.
///
/// This is the core verification function. It does not touch the filesystem,
/// making it suitable for use with in-memory data or streaming verification.
pub fn verify_checkpoint_bytes(content: &[u8], sig_json: &[u8]) -> Result<CheckpointVerifyReport> {
    let sig: CheckpointSigFile = serde_json::from_slice(sig_json).map_err(|e| {
        AosError::CheckpointIntegrity(format!("Failed to parse signature sidecar: {}", e))
    })?;

    // Schema version check
    if sig.schema_version != EXPECTED_SCHEMA_VERSION {
        return Err(AosError::CheckpointIntegrity(format!(
            "Unsupported signature schema version: expected {}, got {}",
            EXPECTED_SCHEMA_VERSION, sig.schema_version
        )));
    }

    // Recompute BLAKE3 hash
    let actual_hash = B3Hash::hash(content);
    if actual_hash != sig.blake3_hash {
        return Err(AosError::CheckpointIntegrity(format!(
            "BLAKE3 hash mismatch: sidecar says {}, content hashes to {}",
            sig.blake3_hash.to_hex(),
            actual_hash.to_hex()
        )));
    }

    // Verify Ed25519 signature
    sig.public_key
        .verify(sig.blake3_hash.as_bytes(), &sig.signature)
        .map_err(|e| {
            AosError::CheckpointIntegrity(format!("Ed25519 signature verification failed: {}", e))
        })?;

    let signer_key_id = compute_key_id(&sig.public_key);

    Ok(CheckpointVerifyReport {
        blake3_hash: actual_hash,
        signer_key_id,
        signed_at: sig.signed_at,
        schema_version: sig.schema_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Keypair;

    fn make_signed_pair(content: &[u8], keypair: &Keypair) -> Vec<u8> {
        let blake3_hash = B3Hash::hash(content);
        let signature = keypair.sign(blake3_hash.as_bytes());
        let sig = CheckpointSigFile {
            schema_version: 1,
            blake3_hash,
            signature,
            public_key: keypair.public_key(),
            signed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        serde_json::to_vec_pretty(&sig).unwrap()
    }

    #[test]
    fn test_verify_valid() {
        let keypair = Keypair::generate();
        let content = b"checkpoint json content";
        let sig_json = make_signed_pair(content, &keypair);

        let report = verify_checkpoint_bytes(content, &sig_json).unwrap();
        assert_eq!(report.blake3_hash, B3Hash::hash(content));
        assert!(report.signer_key_id.starts_with("kid-"));
        assert_eq!(report.schema_version, 1);
    }

    #[test]
    fn test_verify_tampered_content() {
        let keypair = Keypair::generate();
        let content = b"original checkpoint content";
        let sig_json = make_signed_pair(content, &keypair);

        let tampered = b"tampered checkpoint content";
        let err = verify_checkpoint_bytes(tampered, &sig_json).unwrap_err();
        assert!(err.to_string().contains("BLAKE3 hash mismatch"));
    }

    #[test]
    fn test_verify_wrong_key() {
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        let content = b"some content";

        // Sign with keypair1 but replace public key with keypair2's
        let blake3_hash = B3Hash::hash(content);
        let signature = keypair1.sign(blake3_hash.as_bytes());
        let sig = CheckpointSigFile {
            schema_version: 1,
            blake3_hash,
            signature,
            public_key: keypair2.public_key(), // wrong key
            signed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let sig_json = serde_json::to_vec(&sig).unwrap();

        let err = verify_checkpoint_bytes(content, &sig_json).unwrap_err();
        assert!(err.to_string().contains("signature verification failed"));
    }

    #[test]
    fn test_verify_bad_schema_version() {
        let keypair = Keypair::generate();
        let content = b"content";
        let blake3_hash = B3Hash::hash(content);
        let sig = CheckpointSigFile {
            schema_version: 99,
            blake3_hash,
            signature: keypair.sign(blake3_hash.as_bytes()),
            public_key: keypair.public_key(),
            signed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let sig_json = serde_json::to_vec(&sig).unwrap();

        let err = verify_checkpoint_bytes(content, &sig_json).unwrap_err();
        assert!(err.to_string().contains("schema version"));
    }

    #[test]
    fn test_verify_malformed_sidecar() {
        let err = verify_checkpoint_bytes(b"content", b"not json").unwrap_err();
        assert!(err.to_string().contains("parse signature sidecar"));
    }

    #[test]
    fn test_file_roundtrip() {
        let temp_dir = tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .unwrap();
        let ckpt_path = temp_dir.path().join("test.ckpt");
        let sig_path = temp_dir.path().join("test.ckpt.sig");

        let keypair = Keypair::generate();
        let content = b"checkpoint file content for roundtrip test";

        std::fs::write(&ckpt_path, content).unwrap();
        let sig_json = make_signed_pair(content, &keypair);
        std::fs::write(&sig_path, &sig_json).unwrap();

        let report = verify_checkpoint_file(&ckpt_path).unwrap();
        assert_eq!(report.blake3_hash, B3Hash::hash(content));
    }
}
