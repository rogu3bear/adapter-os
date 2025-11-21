//! Crash Journal - Panic hook with signed, chained bundles
//!
//! Implements custom panic hook that captures crash information and writes
//! signed, chained bundles for audit and forensic analysis.
//!
//! Per Telemetry Ruleset #9: All security events (including crashes) logged at 100% sampling.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{Keypair, ProviderAttestation};
use serde::{Deserialize, Serialize};
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use ::tracing::{error, info};

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
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AosError::Io(format!("System time error: {}", e)))?
            .as_secs();

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
