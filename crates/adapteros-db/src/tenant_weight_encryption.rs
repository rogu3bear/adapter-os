//! Per-tenant weight directory encryption
//!
//! Provides tenant-scoped Data Encryption Keys (DEKs) for encrypting adapter
//! weight files at rest. Each tenant gets an independent DEK derived via
//! `HKDF(BLAKE3(tenant_id), "aos-weight-encryption")`, matching the pattern
//! established in `crypto_at_rest.rs`.
//!
//! ## Design
//!
//! - **DEK derivation**: `derive_seed(&B3Hash::hash(tenant_id), "aos-weight-encryption")`
//! - **Cipher**: ChaCha20Poly1305 (same as existing at-rest encryption)
//! - **File format**: `[12-byte nonce][ciphertext+tag]` (prepended nonce)
//! - **Migration path**: Unencrypted weights coexist via `EncryptionStatus::Plaintext`
//! - **Metadata**: Key fingerprint (BLAKE3 of DEK), algorithm, per-file tracking
//!
//! ## Schema
//! See migration `20260211140000_tenant_weight_encryption_keys.sql`.

use crate::Db;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use base64::Engine;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// HKDF label for weight encryption DEK derivation.
const WEIGHT_ENCRYPTION_LABEL: &str = "aos-weight-encryption";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Encryption status for a weight file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionStatus {
    Plaintext,
    Encrypted,
}

impl EncryptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Encrypted => "encrypted",
        }
    }

    pub fn from_str_checked(s: &str) -> Result<Self> {
        match s {
            "plaintext" => Ok(Self::Plaintext),
            "encrypted" => Ok(Self::Encrypted),
            other => Err(AosError::Crypto(format!(
                "Invalid encryption status: {}",
                other
            ))),
        }
    }
}

/// Per-tenant encryption key metadata (stored in `tenant_weight_encryption_keys`).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TenantWeightKey {
    pub id: String,
    pub tenant_id: String,
    pub key_fingerprint: String,
    pub algorithm: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub metadata: Option<String>,
}

/// Per-file encryption metadata (stored in `encrypted_weight_files`).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EncryptedWeightFile {
    pub id: String,
    pub adapter_id: String,
    pub tenant_id: String,
    pub file_path: String,
    pub encryption_status: String,
    pub key_fingerprint: Option<String>,
    pub algorithm: Option<String>,
    pub nonce_b64: Option<String>,
    pub original_digest_hex: String,
    pub encrypted_at: Option<String>,
}

/// Result of sealing a weight file.
#[derive(Debug, Clone)]
pub struct SealedWeight {
    /// The encrypted bytes (nonce || ciphertext+tag).
    pub sealed_bytes: Vec<u8>,
    /// BLAKE3 fingerprint of the DEK used.
    pub key_fingerprint: String,
    /// Base64-encoded nonce for metadata tracking.
    pub nonce_b64: String,
    /// BLAKE3 digest of the original plaintext.
    pub original_digest_hex: String,
}

// ---------------------------------------------------------------------------
// DEK derivation
// ---------------------------------------------------------------------------

/// Derive a tenant-scoped Data Encryption Key for weight files.
///
/// Uses HKDF-SHA256 with `BLAKE3(tenant_id)` as IKM and a domain-separated
/// label, matching the pattern in `crypto_at_rest.rs`.
pub fn derive_tenant_weight_dek(tenant_id: &str) -> [u8; 32] {
    derive_seed(&B3Hash::hash(tenant_id.as_bytes()), WEIGHT_ENCRYPTION_LABEL)
}

/// Compute the fingerprint of a DEK (BLAKE3 hash, hex-encoded).
pub fn dek_fingerprint(dek: &[u8; 32]) -> String {
    hex::encode(B3Hash::hash(dek).as_bytes())
}

// ---------------------------------------------------------------------------
// Seal / Unseal
// ---------------------------------------------------------------------------

/// Encrypt weight data using a tenant-scoped DEK.
///
/// Returns a `SealedWeight` containing the ciphertext with prepended nonce,
/// plus metadata for database tracking.
pub fn seal_weight(tenant_id: &str, plaintext: &[u8]) -> Result<SealedWeight> {
    let dek = derive_tenant_weight_dek(tenant_id);
    let fingerprint = dek_fingerprint(&dek);
    let original_digest = hex::encode(B3Hash::hash(plaintext).as_bytes());

    let key = Key::from_slice(&dek);
    let cipher = ChaCha20Poly1305::new(key);

    // Random nonce for each encryption (weight files are not idempotent —
    // different encryptions of the same plaintext produce different ciphertext).
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AosError::Crypto(format!("Weight encryption failed: {}", e)))?;

    let mut sealed = Vec::with_capacity(12 + ciphertext.len());
    sealed.extend_from_slice(&nonce_bytes);
    sealed.extend_from_slice(&ciphertext);

    Ok(SealedWeight {
        sealed_bytes: sealed,
        key_fingerprint: fingerprint,
        nonce_b64: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
        original_digest_hex: original_digest,
    })
}

/// Decrypt weight data using a tenant-scoped DEK.
///
/// Expects the sealed format: `[12-byte nonce][ciphertext+tag]`.
pub fn unseal_weight(tenant_id: &str, sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 12 {
        return Err(AosError::Crypto(
            "Sealed weight payload too short (need at least 12 bytes for nonce)".into(),
        ));
    }

    let (nonce_bytes, ciphertext) = sealed.split_at(12);
    let dek = derive_tenant_weight_dek(tenant_id);
    let key = Key::from_slice(&dek);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AosError::Crypto(format!("Weight decryption failed: {}", e)))
}

/// Decrypt and verify integrity against the original digest.
pub fn unseal_weight_verified(
    tenant_id: &str,
    sealed: &[u8],
    expected_digest_hex: &str,
) -> Result<Vec<u8>> {
    let plaintext = unseal_weight(tenant_id, sealed)?;
    let actual_digest = hex::encode(B3Hash::hash(&plaintext).as_bytes());

    if actual_digest != expected_digest_hex {
        return Err(AosError::Crypto(format!(
            "Weight integrity check failed: expected digest {}, got {}",
            expected_digest_hex, actual_digest
        )));
    }

    Ok(plaintext)
}

// ---------------------------------------------------------------------------
// Database operations
// ---------------------------------------------------------------------------

impl Db {
    /// Register a tenant's weight encryption key metadata.
    pub async fn register_tenant_weight_key(
        &self,
        id: &str,
        tenant_id: &str,
        key_fingerprint: &str,
        algorithm: &str,
        created_at: &str,
        metadata: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO tenant_weight_encryption_keys
                (id, tenant_id, key_fingerprint, algorithm, created_at, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(key_fingerprint)
        .bind(algorithm)
        .bind(created_at)
        .bind(metadata)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to register tenant weight key: {}", e)))?;

        Ok(())
    }

    /// Get the active (non-revoked) weight encryption key for a tenant.
    pub async fn get_active_tenant_weight_key(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantWeightKey>> {
        let row = sqlx::query_as::<_, TenantWeightKey>(
            r#"
            SELECT id, tenant_id, key_fingerprint, algorithm,
                   created_at, revoked_at, metadata
            FROM tenant_weight_encryption_keys
            WHERE tenant_id = ? AND revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get active tenant weight key: {}", e))
        })?;

        Ok(row)
    }

    /// Revoke a tenant weight encryption key.
    pub async fn revoke_tenant_weight_key(&self, key_id: &str, revoked_at: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE tenant_weight_encryption_keys
            SET revoked_at = ?
            WHERE id = ?
            "#,
        )
        .bind(revoked_at)
        .bind(key_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to revoke tenant weight key: {}", e)))?;

        Ok(())
    }

    /// Record encryption metadata for a weight file.
    ///
    /// Uses `INSERT OR REPLACE` so re-encryption of the same file updates
    /// the record in place (keyed on `(adapter_id, file_path)` UNIQUE).
    pub async fn record_encrypted_weight_file(
        &self,
        id: &str,
        adapter_id: &str,
        tenant_id: &str,
        file_path: &str,
        status: EncryptionStatus,
        key_fingerprint: Option<&str>,
        algorithm: Option<&str>,
        nonce_b64: Option<&str>,
        original_digest_hex: &str,
        encrypted_at: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO encrypted_weight_files
                (id, adapter_id, tenant_id, file_path, encryption_status,
                 key_fingerprint, algorithm, nonce_b64, original_digest_hex, encrypted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(file_path)
        .bind(status.as_str())
        .bind(key_fingerprint)
        .bind(algorithm)
        .bind(nonce_b64)
        .bind(original_digest_hex)
        .bind(encrypted_at)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to record encrypted weight file: {}", e))
        })?;

        Ok(())
    }

    /// Get encryption metadata for a specific weight file.
    pub async fn get_weight_file_encryption(
        &self,
        adapter_id: &str,
        file_path: &str,
    ) -> Result<Option<EncryptedWeightFile>> {
        let row = sqlx::query_as::<_, EncryptedWeightFile>(
            r#"
            SELECT id, adapter_id, tenant_id, file_path, encryption_status,
                   key_fingerprint, algorithm, nonce_b64, original_digest_hex, encrypted_at
            FROM encrypted_weight_files
            WHERE adapter_id = ? AND file_path = ?
            "#,
        )
        .bind(adapter_id)
        .bind(file_path)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get weight file encryption: {}", e)))?;

        Ok(row)
    }

    /// List all weight files for an adapter with their encryption status.
    pub async fn list_adapter_weight_files(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<EncryptedWeightFile>> {
        let rows = sqlx::query_as::<_, EncryptedWeightFile>(
            r#"
            SELECT id, adapter_id, tenant_id, file_path, encryption_status,
                   key_fingerprint, algorithm, nonce_b64, original_digest_hex, encrypted_at
            FROM encrypted_weight_files
            WHERE adapter_id = ?
            ORDER BY file_path ASC
            "#,
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list adapter weight files: {}", e)))?;

        Ok(rows)
    }

    /// Count plaintext (unencrypted) weight files for a tenant.
    ///
    /// Useful for migration progress tracking.
    pub async fn count_plaintext_weight_files(&self, tenant_id: &str) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM encrypted_weight_files
            WHERE tenant_id = ? AND encryption_status = 'plaintext'
            "#,
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to count plaintext weight files: {}", e))
        })?;

        Ok(row.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dek_derivation_deterministic() {
        let dek1 = derive_tenant_weight_dek("tenant-abc");
        let dek2 = derive_tenant_weight_dek("tenant-abc");
        assert_eq!(dek1, dek2, "Same tenant must produce same DEK");
    }

    #[test]
    fn test_dek_derivation_tenant_isolation() {
        let dek_a = derive_tenant_weight_dek("tenant-a");
        let dek_b = derive_tenant_weight_dek("tenant-b");
        assert_ne!(
            dek_a, dek_b,
            "Different tenants must produce different DEKs"
        );
    }

    #[test]
    fn test_dek_differs_from_at_rest_key() {
        // Ensure the weight encryption DEK differs from the crypto_at_rest DEK
        // for the same tenant (different HKDF labels).
        let weight_dek = derive_tenant_weight_dek("tenant-x");
        let at_rest_dek = derive_seed(&B3Hash::hash("tenant-x".as_bytes()), "aos-crypto-at-rest");
        assert_ne!(
            weight_dek, at_rest_dek,
            "Weight DEK must differ from at-rest DEK (different labels)"
        );
    }

    #[test]
    fn test_dek_fingerprint_stable() {
        let dek = derive_tenant_weight_dek("tenant-fp");
        let fp1 = dek_fingerprint(&dek);
        let fp2 = dek_fingerprint(&dek);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64, "BLAKE3 hex fingerprint should be 64 chars");
    }

    #[test]
    fn test_seal_unseal_roundtrip() {
        let tenant = "tenant-roundtrip";
        let plaintext = b"weight data for lora_a matrix";
        let sealed = seal_weight(tenant, plaintext).unwrap();

        assert_ne!(
            sealed.sealed_bytes.as_slice(),
            plaintext,
            "Sealed data must differ from plaintext"
        );
        assert!(
            sealed.sealed_bytes.len() > plaintext.len(),
            "Sealed must be larger (nonce + tag)"
        );

        let decrypted = unseal_weight(tenant, &sealed.sealed_bytes).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unseal_wrong_tenant_fails() {
        let sealed = seal_weight("tenant-correct", b"secret weights").unwrap();
        let result = unseal_weight("tenant-wrong", &sealed.sealed_bytes);
        assert!(result.is_err(), "Wrong tenant must fail decryption");
    }

    #[test]
    fn test_unseal_truncated_payload() {
        let result = unseal_weight("tenant", &[0u8; 5]);
        assert!(result.is_err(), "Truncated payload must error");
    }

    #[test]
    fn test_unseal_verified_success() {
        let tenant = "tenant-verify";
        let plaintext = b"verified weight data";
        let sealed = seal_weight(tenant, plaintext).unwrap();

        let decrypted =
            unseal_weight_verified(tenant, &sealed.sealed_bytes, &sealed.original_digest_hex)
                .unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unseal_verified_bad_digest() {
        let tenant = "tenant-bad-digest";
        let plaintext = b"some data";
        let sealed = seal_weight(tenant, plaintext).unwrap();

        let result = unseal_weight_verified(
            tenant,
            &sealed.sealed_bytes,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(result.is_err(), "Bad digest must fail verification");
    }

    #[test]
    fn test_encryption_status_roundtrip() {
        assert_eq!(
            EncryptionStatus::from_str_checked("plaintext").unwrap(),
            EncryptionStatus::Plaintext
        );
        assert_eq!(
            EncryptionStatus::from_str_checked("encrypted").unwrap(),
            EncryptionStatus::Encrypted
        );
        assert!(EncryptionStatus::from_str_checked("invalid").is_err());
        assert_eq!(EncryptionStatus::Plaintext.as_str(), "plaintext");
        assert_eq!(EncryptionStatus::Encrypted.as_str(), "encrypted");
    }

    // -- Async DB tests --

    #[tokio::test]
    async fn test_register_and_get_tenant_weight_key() {
        let db = Db::new_in_memory().await.unwrap();

        let dek = derive_tenant_weight_dek("tenant-key-test");
        let fingerprint = dek_fingerprint(&dek);

        db.register_tenant_weight_key(
            "twk-001",
            "tenant-key-test",
            &fingerprint,
            "chacha20poly1305",
            "2026-02-11T14:00:00Z",
            None,
        )
        .await
        .unwrap();

        let key = db
            .get_active_tenant_weight_key("tenant-key-test")
            .await
            .unwrap()
            .expect("key should exist");

        assert_eq!(key.id, "twk-001");
        assert_eq!(key.tenant_id, "tenant-key-test");
        assert_eq!(key.key_fingerprint, fingerprint);
        assert_eq!(key.algorithm, "chacha20poly1305");
        assert!(key.revoked_at.is_none());
    }

    #[tokio::test]
    async fn test_revoke_tenant_weight_key() {
        let db = Db::new_in_memory().await.unwrap();

        db.register_tenant_weight_key(
            "twk-revoke-001",
            "tenant-revoke",
            "fp-original",
            "chacha20poly1305",
            "2026-02-11T14:00:00Z",
            None,
        )
        .await
        .unwrap();

        db.revoke_tenant_weight_key("twk-revoke-001", "2026-02-11T15:00:00Z")
            .await
            .unwrap();

        // Revoked key should not appear as active
        let active = db
            .get_active_tenant_weight_key("tenant-revoke")
            .await
            .unwrap();
        assert!(active.is_none(), "Revoked key must not be active");
    }

    #[tokio::test]
    async fn test_record_and_get_encrypted_weight_file() {
        let db = Db::new_in_memory().await.unwrap();

        let sealed = seal_weight("tenant-file-test", b"lora weights").unwrap();

        db.record_encrypted_weight_file(
            "ewf-001",
            "adapter-abc",
            "tenant-file-test",
            "/adapters/abc/lora_a.bin",
            EncryptionStatus::Encrypted,
            Some(&sealed.key_fingerprint),
            Some("chacha20poly1305"),
            Some(&sealed.nonce_b64),
            &sealed.original_digest_hex,
            Some("2026-02-11T14:30:00Z"),
        )
        .await
        .unwrap();

        let file = db
            .get_weight_file_encryption("adapter-abc", "/adapters/abc/lora_a.bin")
            .await
            .unwrap()
            .expect("file record should exist");

        assert_eq!(file.adapter_id, "adapter-abc");
        assert_eq!(file.encryption_status, "encrypted");
        assert_eq!(
            file.key_fingerprint.as_deref(),
            Some(sealed.key_fingerprint.as_str())
        );
        assert_eq!(file.original_digest_hex, sealed.original_digest_hex);
    }

    #[tokio::test]
    async fn test_record_plaintext_weight_file() {
        let db = Db::new_in_memory().await.unwrap();

        let digest = hex::encode(B3Hash::hash(b"plaintext weights").as_bytes());

        db.record_encrypted_weight_file(
            "ewf-plain-001",
            "adapter-plain",
            "tenant-plain",
            "/adapters/plain/lora_b.bin",
            EncryptionStatus::Plaintext,
            None,
            None,
            None,
            &digest,
            None,
        )
        .await
        .unwrap();

        let file = db
            .get_weight_file_encryption("adapter-plain", "/adapters/plain/lora_b.bin")
            .await
            .unwrap()
            .expect("file record should exist");

        assert_eq!(file.encryption_status, "plaintext");
        assert!(file.key_fingerprint.is_none());
        assert!(file.encrypted_at.is_none());
    }

    #[tokio::test]
    async fn test_list_adapter_weight_files() {
        let db = Db::new_in_memory().await.unwrap();

        let digest = hex::encode(B3Hash::hash(b"data").as_bytes());

        for (i, name) in ["lora_a.bin", "lora_b.bin", "config.json"]
            .iter()
            .enumerate()
        {
            db.record_encrypted_weight_file(
                &format!("ewf-list-{:03}", i),
                "adapter-list",
                "tenant-list",
                &format!("/adapters/list/{}", name),
                EncryptionStatus::Plaintext,
                None,
                None,
                None,
                &digest,
                None,
            )
            .await
            .unwrap();
        }

        let files = db.list_adapter_weight_files("adapter-list").await.unwrap();
        assert_eq!(files.len(), 3);
        // Sorted by file_path ASC
        assert!(files[0].file_path.contains("config.json"));
        assert!(files[1].file_path.contains("lora_a.bin"));
        assert!(files[2].file_path.contains("lora_b.bin"));
    }

    #[tokio::test]
    async fn test_count_plaintext_weight_files() {
        let db = Db::new_in_memory().await.unwrap();

        let digest = hex::encode(B3Hash::hash(b"data").as_bytes());

        // 2 plaintext + 1 encrypted
        db.record_encrypted_weight_file(
            "ewf-count-001",
            "adapter-count-a",
            "tenant-count",
            "/adapters/a/weights.bin",
            EncryptionStatus::Plaintext,
            None,
            None,
            None,
            &digest,
            None,
        )
        .await
        .unwrap();

        db.record_encrypted_weight_file(
            "ewf-count-002",
            "adapter-count-b",
            "tenant-count",
            "/adapters/b/weights.bin",
            EncryptionStatus::Plaintext,
            None,
            None,
            None,
            &digest,
            None,
        )
        .await
        .unwrap();

        db.record_encrypted_weight_file(
            "ewf-count-003",
            "adapter-count-c",
            "tenant-count",
            "/adapters/c/weights.bin",
            EncryptionStatus::Encrypted,
            Some("fp-xyz"),
            Some("chacha20poly1305"),
            Some("bm9uY2U="),
            &digest,
            Some("2026-02-11T15:00:00Z"),
        )
        .await
        .unwrap();

        let count = db
            .count_plaintext_weight_files("tenant-count")
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_reencrypt_updates_record() {
        let db = Db::new_in_memory().await.unwrap();

        let digest = hex::encode(B3Hash::hash(b"weights").as_bytes());

        // Record as plaintext initially
        db.record_encrypted_weight_file(
            "ewf-reenc-001",
            "adapter-reenc",
            "tenant-reenc",
            "/adapters/reenc/lora.bin",
            EncryptionStatus::Plaintext,
            None,
            None,
            None,
            &digest,
            None,
        )
        .await
        .unwrap();

        // Re-record as encrypted (INSERT OR REPLACE on UNIQUE(adapter_id, file_path))
        db.record_encrypted_weight_file(
            "ewf-reenc-002",
            "adapter-reenc",
            "tenant-reenc",
            "/adapters/reenc/lora.bin",
            EncryptionStatus::Encrypted,
            Some("fp-new"),
            Some("chacha20poly1305"),
            Some("bm9uY2U="),
            &digest,
            Some("2026-02-11T16:00:00Z"),
        )
        .await
        .unwrap();

        let file = db
            .get_weight_file_encryption("adapter-reenc", "/adapters/reenc/lora.bin")
            .await
            .unwrap()
            .expect("should exist");

        assert_eq!(file.encryption_status, "encrypted");
        assert_eq!(file.id, "ewf-reenc-002", "ID should be updated on replace");
        assert_eq!(file.key_fingerprint.as_deref(), Some("fp-new"));
    }

    #[tokio::test]
    async fn test_encryption_status_constraint() {
        let db = Db::new_in_memory().await.unwrap();

        let digest = hex::encode(B3Hash::hash(b"data").as_bytes());

        let result = db
            .record_encrypted_weight_file(
                "ewf-bad-001",
                "adapter-bad",
                "tenant-bad",
                "/bad/path",
                // Manually pass an invalid status by using a raw query
                EncryptionStatus::Encrypted, // valid status, so test constraint via raw
                None,
                None,
                None,
                &digest,
                None,
            )
            .await;

        // This should succeed since "encrypted" is valid
        assert!(result.is_ok());

        // Test invalid via raw query
        let raw_result = sqlx::query(
            r#"
            INSERT INTO encrypted_weight_files
                (id, adapter_id, tenant_id, file_path, encryption_status, original_digest_hex)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("ewf-bad-002")
        .bind("adapter-bad2")
        .bind("tenant-bad")
        .bind("/bad/path2")
        .bind("invalid_status")
        .bind(&digest)
        .execute(db.pool())
        .await;

        assert!(
            raw_result.is_err(),
            "Invalid status must violate CHECK constraint"
        );
    }
}
