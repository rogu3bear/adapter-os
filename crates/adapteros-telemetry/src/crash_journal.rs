//! Crash Journal - Panic hook with signed, chained bundles
//!
//! Implements custom panic hook that captures crash information and writes
//! signed, chained bundles for audit and forensic analysis.
//!
//! Per Telemetry Ruleset #9: All security events (including crashes) logged at 100% sampling.

use ::tracing::{error, info};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{Keypair, ProviderAttestation};
use serde::{Deserialize, Serialize};
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Crash journal entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashJournalEntry {
    /// Timestamp of crash (Unix timestamp)
    pub timestamp: u64,
    /// Panic message
    pub message: String,
    /// Location where panic occurred
    pub location: Option<String>,
    /// Backtrace (if available)
    pub backtrace: Option<String>,
    /// Thread ID where panic occurred
    pub thread_id: String,
    /// Bundle hash of previous telemetry bundle (for chaining)
    pub prev_bundle_hash: Option<B3Hash>,
    /// Provider attestation at time of crash
    pub provider_attestation: Option<ProviderAttestation>,
    /// Policy hash at time of crash
    pub policy_hash: Option<String>,
    /// Crash signature (Ed25519 signature of crash data)
    pub signature: String,
    /// Public key for signature verification
    pub public_key: String,
}

/// Crash journal manager
pub struct CrashJournal {
    /// Output directory for crash journals
    output_dir: PathBuf,
    /// Signing keypair for crash bundles
    signer: Keypair,
    /// Previous bundle hash (for chaining)
    prev_bundle_hash: Arc<Mutex<Option<B3Hash>>>,
    /// Provider attestation (updated periodically)
    provider_attestation: Arc<Mutex<Option<ProviderAttestation>>>,
    /// Policy hash (updated periodically)
    policy_hash: Arc<Mutex<Option<String>>>,
}

impl CrashJournal {
    /// Create a new crash journal
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&output_dir)?;

        // Generate signing keypair for crash bundles
        let signer = Keypair::generate();

        Ok(Self {
            output_dir,
            signer,
            prev_bundle_hash: Arc::new(Mutex::new(None)),
            provider_attestation: Arc::new(Mutex::new(None)),
            policy_hash: Arc::new(Mutex::new(None)),
        })
    }

    /// Update previous bundle hash (called when telemetry bundles are finalized)
    pub fn update_prev_bundle_hash(&self, hash: B3Hash) {
        if let Ok(mut prev) = self.prev_bundle_hash.lock() {
            *prev = Some(hash);
        }
    }

    /// Update provider attestation
    pub fn update_provider_attestation(&self, attestation: ProviderAttestation) {
        if let Ok(mut att) = self.provider_attestation.lock() {
            *att = Some(attestation);
        }
    }

    /// Update policy hash
    pub fn update_policy_hash(&self, policy_hash: String) {
        if let Ok(mut hash) = self.policy_hash.lock() {
            *hash = Some(policy_hash);
        }
    }

    /// Record a crash
    #[allow(deprecated)]
    pub fn record_crash(&self, panic_info: &PanicHookInfo) -> Result<()> {
        let timestamp = adapteros_core::time::unix_timestamp_secs();

        // Extract panic message
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "Unknown panic".to_string());

        // Extract location
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));

        // Capture backtrace if available
        let backtrace = if std::backtrace::Backtrace::capture().status()
            == std::backtrace::BacktraceStatus::Captured
        {
            Some(format!("{:?}", Backtrace::capture()))
        } else {
            None
        };

        // Get thread ID
        let thread_id = format!("{:?}", std::thread::current().id());

        // Get previous bundle hash and attestation
        let (prev_hash, attestation, policy_hash) = {
            let prev = self.prev_bundle_hash.lock().unwrap();
            let att = self.provider_attestation.lock().unwrap();
            let pol = self.policy_hash.lock().unwrap();
            (*prev, att.clone(), pol.clone())
        };

        // Create crash entry
        let mut entry = CrashJournalEntry {
            timestamp,
            message,
            location,
            backtrace,
            thread_id,
            prev_bundle_hash: prev_hash,
            provider_attestation: attestation,
            policy_hash,
            signature: String::new(),
            public_key: hex::encode(self.signer.public_key().to_bytes()),
        };

        // Sign the crash entry
        let entry_json = serde_json::to_string(&entry).map_err(AosError::Serialization)?;
        let entry_hash = B3Hash::hash(entry_json.as_bytes());
        let signature = self.signer.sign(entry_hash.as_bytes());
        entry.signature = hex::encode(signature.to_bytes());

        // Write crash journal file
        let filename = format!("crash_{:016x}.json", timestamp);
        let filepath = self.output_dir.join(&filename);

        let final_json = serde_json::to_string_pretty(&entry).map_err(AosError::Serialization)?;
        std::fs::write(&filepath, final_json)
            .map_err(|e| AosError::Io(format!("Failed to write crash journal: {}", e)))?;

        // Also log crash for visibility
        info!(path = %filepath.display(), message = %entry.message, "Crash journal written");
        if let Some(ref loc) = entry.location {
            info!(location = %loc, "Crash location");
        }

        Ok(())
    }

    /// Get public key for verification
    pub fn public_key(&self) -> String {
        hex::encode(self.signer.public_key().to_bytes())
    }
}

/// Install panic hook with crash journaling
pub fn install_panic_hook(crash_journal: Arc<CrashJournal>) {
    std::panic::set_hook(Box::new(move |panic_info| {
        // Record crash
        if let Err(e) = crash_journal.record_crash(panic_info) {
            error!(error = %e, "Failed to record crash journal");
        }

        // Call default panic handler for backtrace
        #[cfg(debug_assertions)]
        {
            info!(backtrace = ?Backtrace::capture(), "Panic backtrace");
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::{PublicKey, Signature};
    use tempfile::TempDir;

    // ========================================================================
    // CrashJournal Creation Tests
    // ========================================================================

    #[test]
    fn test_crash_journal_creation() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        // Verify public key is generated
        let pubkey = journal.public_key();
        assert_eq!(
            pubkey.len(),
            64,
            "Public key should be 64 hex chars (32 bytes)"
        );

        // Verify it's valid hex
        let decoded = hex::decode(&pubkey);
        assert!(decoded.is_ok(), "Public key should be valid hex");
        assert_eq!(decoded.unwrap().len(), 32, "Decoded key should be 32 bytes");
    }

    #[test]
    fn test_crash_journal_creates_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("crash_journals");

        let journal = CrashJournal::new(&nested_path).unwrap();
        assert!(nested_path.exists(), "Output directory should be created");

        // Verify we can still get the public key
        let pubkey = journal.public_key();
        assert!(!pubkey.is_empty());
    }

    // ========================================================================
    // State Update Tests
    // ========================================================================

    #[test]
    fn test_update_prev_bundle_hash() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        let test_hash = B3Hash::hash(b"test bundle data");
        journal.update_prev_bundle_hash(test_hash);

        // Verify the hash was stored
        let stored = journal.prev_bundle_hash.lock().unwrap();
        assert_eq!(*stored, Some(test_hash));
    }

    #[test]
    fn test_update_policy_hash() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        let test_policy_hash = "policy_sha256_abc123".to_string();
        journal.update_policy_hash(test_policy_hash.clone());

        // Verify the hash was stored
        let stored = journal.policy_hash.lock().unwrap();
        assert_eq!(*stored, Some(test_policy_hash));
    }

    #[test]
    fn test_update_provider_attestation() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        let attestation = ProviderAttestation {
            provider_type: "test-provider".to_string(),
            fingerprint: "test-fingerprint".to_string(),
            policy_hash: "test-policy-hash".to_string(),
            timestamp: 1700000000,
            signature: vec![0u8; 64],
        };
        journal.update_provider_attestation(attestation.clone());

        // Verify the attestation was stored
        let stored = journal.provider_attestation.lock().unwrap();
        assert!(stored.is_some());
        let stored_att = stored.as_ref().unwrap();
        assert_eq!(stored_att.provider_type, "test-provider");
    }

    // ========================================================================
    // Public Key Tests
    // ========================================================================

    #[test]
    fn test_public_key_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        let key1 = journal.public_key();
        let key2 = journal.public_key();

        assert_eq!(key1, key2, "Public key should be consistent");
    }

    #[test]
    fn test_public_key_can_be_parsed() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        let pubkey_hex = journal.public_key();
        let pubkey_bytes = hex::decode(&pubkey_hex).unwrap();

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pubkey_bytes);

        // Should be able to create a PublicKey from these bytes
        let result = PublicKey::from_bytes(&arr);
        assert!(
            result.is_ok(),
            "Public key bytes should be valid Ed25519 key"
        );
    }

    // ========================================================================
    // Signature Verification Tests
    // ========================================================================

    #[test]
    fn test_signature_verification_workflow() {
        // This test simulates the verification workflow:
        // 1. Create a crash entry (manually, without actual panic)
        // 2. Sign it the same way record_crash does
        // 3. Verify the signature

        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        // Create a mock crash entry
        let mut entry = CrashJournalEntry {
            timestamp: 1700000000,
            message: "Test panic message".to_string(),
            location: Some("test.rs:42:1".to_string()),
            backtrace: None,
            thread_id: "TestThread".to_string(),
            prev_bundle_hash: None,
            provider_attestation: None,
            policy_hash: None,
            signature: String::new(),
            public_key: journal.public_key(),
        };

        // Sign using the same method as record_crash
        let entry_json = serde_json::to_string(&entry).unwrap();
        let entry_hash = B3Hash::hash(entry_json.as_bytes());

        // Sign with the internal signer (we need to use the keypair directly for this test)
        // Since signer is private, we'll verify through the public API by
        // simulating what record_crash does
        let signature = journal.signer.sign(entry_hash.as_bytes());
        entry.signature = hex::encode(signature.to_bytes());

        // Now verify the signature
        let pubkey_bytes: [u8; 32] = hex::decode(&entry.public_key).unwrap().try_into().unwrap();
        let pubkey = PublicKey::from_bytes(&pubkey_bytes).unwrap();

        let sig_bytes: [u8; 64] = hex::decode(&entry.signature).unwrap().try_into().unwrap();
        let sig = Signature::from_bytes(&sig_bytes).unwrap();

        // Verify against the original hash
        let verify_result = pubkey.verify(entry_hash.as_bytes(), &sig);
        assert!(
            verify_result.is_ok(),
            "Signature should verify successfully"
        );
    }

    #[test]
    fn test_signature_fails_with_tampered_data() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CrashJournal::new(temp_dir.path()).unwrap();

        // Create a crash entry
        let mut entry = CrashJournalEntry {
            timestamp: 1700000000,
            message: "Original message".to_string(),
            location: None,
            backtrace: None,
            thread_id: "TestThread".to_string(),
            prev_bundle_hash: None,
            provider_attestation: None,
            policy_hash: None,
            signature: String::new(),
            public_key: journal.public_key(),
        };

        // Sign the original entry
        let entry_json = serde_json::to_string(&entry).unwrap();
        let entry_hash = B3Hash::hash(entry_json.as_bytes());
        let signature = journal.signer.sign(entry_hash.as_bytes());
        entry.signature = hex::encode(signature.to_bytes());

        // Now tamper with the message
        entry.message = "Tampered message".to_string();

        // Compute hash of tampered entry
        let tampered_json = serde_json::to_string(&CrashJournalEntry {
            signature: String::new(),
            ..entry.clone()
        })
        .unwrap();
        let tampered_hash = B3Hash::hash(tampered_json.as_bytes());

        // Try to verify
        let pubkey_bytes: [u8; 32] = hex::decode(&entry.public_key).unwrap().try_into().unwrap();
        let pubkey = PublicKey::from_bytes(&pubkey_bytes).unwrap();

        let sig_bytes: [u8; 64] = hex::decode(&entry.signature).unwrap().try_into().unwrap();
        let sig = Signature::from_bytes(&sig_bytes).unwrap();

        // Verification should fail with tampered data
        let verify_result = pubkey.verify(tampered_hash.as_bytes(), &sig);
        assert!(
            verify_result.is_err(),
            "Signature should fail verification with tampered data"
        );
    }

    #[test]
    fn test_different_journals_have_different_keys() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let journal1 = CrashJournal::new(temp_dir1.path()).unwrap();
        let journal2 = CrashJournal::new(temp_dir2.path()).unwrap();

        assert_ne!(
            journal1.public_key(),
            journal2.public_key(),
            "Different journals should have different keypairs"
        );
    }

    #[test]
    fn test_cross_journal_signature_fails() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let journal1 = CrashJournal::new(temp_dir1.path()).unwrap();
        let journal2 = CrashJournal::new(temp_dir2.path()).unwrap();

        // Sign with journal1
        let message = b"test message";
        let hash = B3Hash::hash(message);
        let signature = journal1.signer.sign(hash.as_bytes());

        // Try to verify with journal2's public key
        let pubkey_bytes: [u8; 32] = hex::decode(journal2.public_key())
            .unwrap()
            .try_into()
            .unwrap();
        let pubkey = PublicKey::from_bytes(&pubkey_bytes).unwrap();

        let sig_bytes: [u8; 64] = signature.to_bytes();
        let sig = Signature::from_bytes(&sig_bytes).unwrap();

        let verify_result = pubkey.verify(hash.as_bytes(), &sig);
        assert!(
            verify_result.is_err(),
            "Signature from journal1 should not verify with journal2's key"
        );
    }

    // ========================================================================
    // CrashJournalEntry Serialization Tests
    // ========================================================================

    #[test]
    fn test_crash_entry_serialization() {
        let entry = CrashJournalEntry {
            timestamp: 1700000000,
            message: "Test message".to_string(),
            location: Some("file.rs:42:1".to_string()),
            backtrace: Some("backtrace here".to_string()),
            thread_id: "ThreadId(1)".to_string(),
            prev_bundle_hash: Some(B3Hash::hash(b"prev")),
            provider_attestation: None,
            policy_hash: Some("policy_hash".to_string()),
            signature: "sig_hex".to_string(),
            public_key: "pubkey_hex".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: CrashJournalEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.timestamp, entry.timestamp);
        assert_eq!(deserialized.message, entry.message);
        assert_eq!(deserialized.location, entry.location);
        assert_eq!(deserialized.thread_id, entry.thread_id);
        assert_eq!(deserialized.signature, entry.signature);
        assert_eq!(deserialized.public_key, entry.public_key);
    }

    #[test]
    fn test_crash_entry_with_merkle_chain() {
        let prev_hash = B3Hash::hash(b"previous bundle");

        let entry = CrashJournalEntry {
            timestamp: 1700000000,
            message: "Chained crash".to_string(),
            location: None,
            backtrace: None,
            thread_id: "ThreadId(1)".to_string(),
            prev_bundle_hash: Some(prev_hash),
            provider_attestation: None,
            policy_hash: None,
            signature: String::new(),
            public_key: String::new(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(
            json.contains("prev_bundle_hash"),
            "JSON should include prev_bundle_hash"
        );

        let deserialized: CrashJournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.prev_bundle_hash, Some(prev_hash));
    }

    // ========================================================================
    // Timestamp Tests
    // ========================================================================

    #[test]
    fn test_timestamp_is_recent() {
        // Verify that unix_timestamp_secs returns something reasonable
        let now = adapteros_core::time::unix_timestamp_secs();

        // Should be after Jan 1, 2020 (1577836800)
        assert!(now > 1577836800, "Timestamp should be after 2020");

        // Should be before year 2100 (4102444800)
        assert!(now < 4102444800, "Timestamp should be before 2100");
    }

    // ========================================================================
    // Filename Format Tests
    // ========================================================================

    #[test]
    fn test_filename_format() {
        let timestamp: u64 = 1700000000;
        let filename = format!("crash_{:016x}.json", timestamp);

        assert!(filename.starts_with("crash_"));
        assert!(filename.ends_with(".json"));
        assert_eq!(filename.len(), 6 + 16 + 5, "crash_ + 16 hex chars + .json");

        // Verify it's valid hex
        let hex_part = &filename[6..22];
        assert!(
            hex::decode(hex_part).is_ok(),
            "Filename should contain valid hex"
        );
    }

    #[test]
    fn test_filename_uniqueness_by_timestamp() {
        let ts1: u64 = 1700000000;
        let ts2: u64 = 1700000001;

        let fn1 = format!("crash_{:016x}.json", ts1);
        let fn2 = format!("crash_{:016x}.json", ts2);

        assert_ne!(
            fn1, fn2,
            "Different timestamps should produce different filenames"
        );
    }
}
