//! Migration Signature Verification
//!
//! Verifies Ed25519 signatures on database migrations before applying them.
//! Per Artifacts Ruleset #13: All migrations must be signed.
//! Per Build Ruleset #15: Signature verification gates CAB promotion.
//!
//! ## Security Model
//!
//! - Migrations are signed with Ed25519 private key
//! - Public key is embedded in signatures.json
//! - File hashes use BLAKE3 (fallback to SHA256)
//! - Tampering with any migration blocks database initialization
//!
//! ## Usage
//!
//! ```no_run
//! use adapteros_db::migration_verify::MigrationVerifier;
//!
//! let verifier = MigrationVerifier::new("migrations")?;
//! verifier.verify_all()?;
//! ```

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// Migration signature metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSignature {
    /// File hash (BLAKE3 or SHA256)
    pub hash: String,
    /// Ed25519 signature (base64-encoded)
    pub signature: String,
    /// Signature algorithm (always "ed25519")
    pub algorithm: String,
    /// Hash algorithm ("blake3" or "sha256")
    pub hash_algorithm: String,
}

/// Signatures file schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignaturesSchema {
    /// Schema version
    pub schema_version: String,
    /// Timestamp when signatures were generated
    pub signed_at: String,
    /// Ed25519 public key (base64-encoded PEM)
    pub public_key: String,
    /// Migration filename -> signature mapping
    pub signatures: HashMap<String, MigrationSignature>,
}

/// Migration verifier
#[derive(Debug)]
pub struct MigrationVerifier {
    /// Path to migrations directory
    migrations_dir: PathBuf,
    /// Loaded signatures
    signatures: SignaturesSchema,
}

impl MigrationVerifier {
    /// Create a new migration verifier
    ///
    /// Loads signatures.json from the migrations directory.
    pub fn new<P: AsRef<Path>>(migrations_dir: P) -> Result<Self> {
        let migrations_dir = migrations_dir.as_ref().to_path_buf();

        // Load signatures file
        let signatures_path = migrations_dir.join("signatures.json");
        if !signatures_path.exists() {
            return Err(AosError::Validation(format!(
                "Migration signatures not found: {}. Run scripts/sign_migrations.sh",
                signatures_path.display()
            )));
        }

        let signatures_content = fs::read_to_string(&signatures_path)
            .map_err(|e| AosError::Io(format!("Failed to read signatures file: {}", e)))?;

        let signatures: SignaturesSchema = serde_json::from_str(&signatures_content)
            .map_err(|e| AosError::Validation(format!("Invalid signatures.json format: {}", e)))?;

        info!(
            "Loaded {} migration signatures (schema v{})",
            signatures.signatures.len(),
            signatures.schema_version
        );

        Ok(Self {
            migrations_dir,
            signatures,
        })
    }

    /// Verify all migration signatures
    ///
    /// Returns an error if any signature is invalid or if files have been tampered with.
    pub fn verify_all(&self) -> Result<()> {
        info!("Verifying migration signatures...");

        let migration_files = self.list_migration_files()?;

        if migration_files.is_empty() {
            warn!(
                "No migration files found in {}",
                self.migrations_dir.display()
            );
            return Ok(());
        }

        let mut verified_count = 0;
        let mut errors = Vec::new();

        for migration_file in &migration_files {
            let filename = migration_file
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AosError::Validation("Invalid migration filename".to_string()))?;

            match self.verify_migration(migration_file, filename) {
                Ok(()) => {
                    debug!("✓ {}", filename);
                    verified_count += 1;
                }
                Err(e) => {
                    error!("✗ {}: {}", filename, e);
                    errors.push(format!("{}: {}", filename, e));
                }
            }
        }

        if !errors.is_empty() {
            return Err(AosError::Validation(format!(
                "Migration signature verification failed:\n{}",
                errors.join("\n")
            )));
        }

        info!(
            "✓ All {} migration signatures verified successfully",
            verified_count
        );

        Ok(())
    }

    /// Verify a single migration file
    fn verify_migration(&self, file_path: &Path, filename: &str) -> Result<()> {
        // Get signature for this migration
        let sig_data = self.signatures.signatures.get(filename).ok_or_else(|| {
            AosError::Validation(format!(
                "No signature found for migration: {}. Re-sign with scripts/sign_migrations.sh",
                filename
            ))
        })?;

        // Compute file hash
        let file_content = fs::read(file_path)
            .map_err(|e| AosError::Io(format!("Failed to read migration file: {}", e)))?;

        let computed_hash = match sig_data.hash_algorithm.as_str() {
            "blake3" => {
                let hash = blake3::hash(&file_content);
                hex::encode(hash.as_bytes())
            }
            "sha256" => {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&file_content);
                hex::encode(hasher.finalize())
            }
            alg => {
                return Err(AosError::Validation(format!(
                    "Unsupported hash algorithm: {}",
                    alg
                )))
            }
        };

        // Verify hash matches
        if computed_hash != sig_data.hash {
            return Err(AosError::Validation(format!(
                "File hash mismatch (file has been tampered with)\n  Expected: {}\n  Computed: {}",
                sig_data.hash, computed_hash
            )));
        }

        // Verify signature
        self.verify_signature(&computed_hash, &sig_data.signature)?;

        Ok(())
    }

    /// Verify Ed25519 signature
    fn verify_signature(&self, file_hash: &str, signature_b64: &str) -> Result<()> {
        use base64::{engine::general_purpose, Engine as _};
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        // Decode base64 to get PEM bytes
        let public_key_pem_bytes = general_purpose::STANDARD
            .decode(&self.signatures.public_key)
            .map_err(|e| AosError::Crypto(format!("Invalid public key base64 encoding: {}", e)))?;

        // Convert PEM bytes to string
        let public_key_pem = String::from_utf8(public_key_pem_bytes)
            .map_err(|e| AosError::Crypto(format!("Public key PEM is not valid UTF-8: {}", e)))?;

        // Extract raw Ed25519 public key from PEM
        let public_key_bytes = Self::extract_ed25519_public_key_from_pem(&public_key_pem)?;

        // Create Ed25519 verifying key
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|e| AosError::Crypto(format!("Invalid Ed25519 public key: {}", e)))?;

        // Decode signature from base64
        let signature_bytes = general_purpose::STANDARD
            .decode(signature_b64)
            .map_err(|e| AosError::Crypto(format!("Invalid signature encoding: {}", e)))?;

        // Validate signature length
        if signature_bytes.len() != 64 {
            return Err(AosError::Crypto(format!(
                "Invalid Ed25519 signature length: {} (expected 64)",
                signature_bytes.len()
            )));
        }

        // Create signature object
        let signature = Signature::from_bytes(
            signature_bytes
                .as_slice()
                .try_into()
                .map_err(|_| AosError::Crypto("Failed to parse signature".to_string()))?,
        );

        // Verify signature against file hash
        verifying_key
            .verify(file_hash.as_bytes(), &signature)
            .map_err(|e| {
                AosError::Crypto(format!("Ed25519 signature verification failed: {}", e))
            })?;

        debug!(
            "✓ Ed25519 signature verified (key fingerprint: {})",
            hex::encode(&public_key_bytes[..4])
        );

        Ok(())
    }

    /// Extract raw Ed25519 public key bytes from PEM format
    ///
    /// Parses OpenSSL-generated Ed25519 public key PEM and extracts the 32-byte raw key.
    /// DER structure for Ed25519 public key (SPKI format):
    /// - SEQUENCE (48 bytes total for Ed25519)
    ///   - SEQUENCE (algorithm identifier)
    ///     - OID 1.3.101.112 (Ed25519)
    ///   - BIT STRING (34 bytes)
    ///     - 0x00 (unused bits)
    ///     - 32 bytes (raw Ed25519 public key)
    fn extract_ed25519_public_key_from_pem(pem: &str) -> Result<[u8; 32]> {
        // Remove PEM header/footer and whitespace
        let pem_body = pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>();

        // Decode base64
        use base64::{engine::general_purpose, Engine as _};
        let der_bytes = general_purpose::STANDARD
            .decode(pem_body.trim())
            .map_err(|e| AosError::Crypto(format!("Failed to decode PEM base64: {}", e)))?;

        // Parse DER structure to extract raw key
        // Ed25519 public key in SPKI format is 44 bytes:
        // 30 2a (SEQUENCE, 42 bytes)
        //   30 05 (SEQUENCE, 5 bytes)
        //     06 03 2b 65 70 (OID 1.3.101.112 = Ed25519)
        //   03 21 (BIT STRING, 33 bytes)
        //     00 (unused bits)
        //     <32 bytes of Ed25519 public key>

        if der_bytes.len() < 44 {
            return Err(AosError::Crypto(format!(
                "Invalid Ed25519 DER length: {} (expected >= 44)",
                der_bytes.len()
            )));
        }

        // Extract the last 32 bytes (raw Ed25519 public key)
        // This works for standard SPKI-encoded Ed25519 keys
        let key_start = der_bytes.len() - 32;
        let raw_key_bytes: [u8; 32] = der_bytes[key_start..]
            .try_into()
            .map_err(|_| AosError::Crypto("Failed to extract Ed25519 key bytes".to_string()))?;

        Ok(raw_key_bytes)
    }

    /// List all .sql migration files in the migrations directory
    fn list_migration_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let entries = fs::read_dir(&self.migrations_dir)
            .map_err(|e| AosError::Io(format!("Failed to read migrations directory: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("sql") {
                files.push(path);
            }
        }

        // Sort for deterministic ordering
        files.sort();

        Ok(files)
    }

    /// Get count of signed migrations
    pub fn signature_count(&self) -> usize {
        self.signatures.signatures.len()
    }

    /// Get public key fingerprint for audit logs
    pub fn public_key_fingerprint(&self) -> String {
        let hash = blake3::hash(self.signatures.public_key.as_bytes());
        hex::encode(&hash.as_bytes()[..8]) // First 8 bytes as fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_signature_schema_parsing() {
        let json = r#"{
            "schema_version": "1.0",
            "signed_at": "2025-10-15T00:00:00Z",
            "public_key": "LS0tLS1CRUdJTiBQVUJMSUMgS0VZLS0tLS0=",
            "signatures": {
                "0001_init.sql": {
                    "hash": "abc123",
                    "signature": "def456",
                    "algorithm": "ed25519",
                    "hash_algorithm": "blake3"
                }
            }
        }"#;

        let schema: SignaturesSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.schema_version, "1.0");
        assert_eq!(schema.signatures.len(), 1);
    }

    #[test]
    fn test_blake3_hashing() {
        let content = b"CREATE TABLE test (id INTEGER PRIMARY KEY);";
        let hash = blake3::hash(content);
        let hash_hex = hex::encode(hash.as_bytes());

        // Hash should be deterministic
        let hash2 = blake3::hash(content);
        let hash_hex2 = hex::encode(hash2.as_bytes());

        assert_eq!(hash_hex, hash_hex2);
        assert_eq!(hash_hex.len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
    }

    #[test]
    fn test_migration_verifier_missing_signatures() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path();

        // Create a migration file without signatures.json
        fs::write(migrations_dir.join("0001_init.sql"), "CREATE TABLE test;").unwrap();

        let result = MigrationVerifier::new(migrations_dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("signatures not found"));
    }

    #[test]
    #[ignore = "Verifies actual migrations directory - run with: cargo test --release -- --ignored test_verify_actual_migrations"]
    fn test_verify_actual_migrations() {
        // This test verifies the actual migrations in the project
        // Run with: cargo test test_verify_actual_migrations -- --ignored
        let migrations_dir = "../../migrations";

        let verifier = MigrationVerifier::new(migrations_dir).expect("Failed to create verifier");

        verifier
            .verify_all()
            .expect("Migration verification failed");

        println!(
            "✓ Successfully verified {} migrations",
            verifier.signature_count()
        );
        println!(
            "✓ Public key fingerprint: {}",
            verifier.public_key_fingerprint()
        );
    }
}
