//! Telemetry with canonical JSON and BLAKE3 hashing

use crossbeam::channel::{unbounded, Receiver, Sender};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::Keypair;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;

pub mod bundle;
pub mod event;
pub mod replay;
pub mod report;

pub use bundle::BundleWriter;
pub use event::Event;
pub use replay::{
    find_divergence, format_divergence, load_replay_bundle, ReplayBundle, ReplayDivergence,
};
pub use report::generate_html_report;

/// Telemetry writer with background thread
pub struct TelemetryWriter {
    sender: Sender<TelemetryEvent>,
    _handle: thread::JoinHandle<()>,
}

impl Clone for TelemetryWriter {
    fn clone(&self) -> Self {
        // Clone the sender channel, but we can't clone the thread handle
        // Create a dummy handle that does nothing
        let sender = self.sender.clone();
        let handle = thread::spawn(|| {
            // Dummy thread that immediately exits
        });
        Self {
            sender,
            _handle: handle,
        }
    }
}

impl TelemetryWriter {
    /// Create a new telemetry writer
    pub fn new<P: AsRef<Path>>(output_dir: P, max_events: usize, max_bytes: usize) -> Result<Self> {
        let (sender, receiver) = unbounded();
        let output_dir = output_dir.as_ref().to_path_buf();

        let handle = thread::spawn(move || {
            if let Err(e) = run_writer(receiver, output_dir, max_events, max_bytes) {
                eprintln!("Telemetry writer error: {}", e);
            }
        });

        Ok(Self {
            sender,
            _handle: handle,
        })
    }

    /// Log an event
    pub fn log<T: Serialize>(&self, event_type: &str, payload: T) -> Result<()> {
        let event = TelemetryEvent {
            event_type: event_type.to_string(),
            payload: serde_json::to_value(payload)?,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_nanos() as u128,
        };

        self.sender
            .send(event)
            .map_err(|e| AosError::Telemetry(format!("Failed to send event: {}", e)))?;

        Ok(())
    }

    /// Log a security event (always logged at 100% sampling per Telemetry Ruleset #9)
    pub fn log_security_event(&self, event: SecurityEvent) -> Result<()> {
        self.log("security", event)
    }

    /// Log a policy violation (always logged at 100% sampling)
    pub fn log_policy_violation(
        &self,
        policy: &str,
        violation_type: &str,
        details: &str,
    ) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::PolicyViolation {
            policy: policy.to_string(),
            violation_type: violation_type.to_string(),
            details: details.to_string(),
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log an egress attempt (always logged at 100% sampling)
    pub fn log_egress_attempt(&self, destination: &str, blocked: bool) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::EgressAttempt {
            destination: destination.to_string(),
            blocked,
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log an isolation violation (always logged at 100% sampling)
    pub fn log_isolation_violation(&self, tenant_id: &str, details: &str) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::IsolationViolation {
            tenant_id: tenant_id.to_string(),
            details: details.to_string(),
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log adapter swap operation (Tier 6)
    pub fn log_adapter_swap(&self, event: AdapterSwapEvent) -> Result<()> {
        self.log("adapter_swap", event)
    }

    /// Log adapter preload operation (Tier 6)
    pub fn log_adapter_preload(&self, event: AdapterPreloadEvent) -> Result<()> {
        self.log("adapter_preload", event)
    }

    /// Log stack verification (Tier 6)
    pub fn log_stack_verification(&self, event: StackVerificationEvent) -> Result<()> {
        self.log("stack_verification", event)
    }

    /// Log node sync operation (Tier 6)
    pub fn log_node_sync(&self, event: NodeSyncEvent) -> Result<()> {
        self.log("node_sync", event)
    }

    /// Log a kernel noise event
    pub fn log_kernel_noise(&self, event: crate::event::KernelNoiseEvent) -> Result<()> {
        self.log("kernel.noise", event)
    }

    /// Log a kernel step event
    pub fn log_kernel_step(&self, event: crate::event::KernelStepEvent) -> Result<()> {
        self.log("kernel.step", event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: u128,
}

impl TelemetryEvent {
    /// Compute canonical hash
    fn compute_hash(&self) -> Result<B3Hash> {
        let canonical_bytes = serde_jcs::to_vec(self)
            .map_err(|e| AosError::Telemetry(format!("Failed to canonicalize: {}", e)))?;
        Ok(B3Hash::hash(&canonical_bytes))
    }
}

fn run_writer(
    receiver: Receiver<TelemetryEvent>,
    output_dir: PathBuf,
    max_events: usize,
    max_bytes: usize,
) -> Result<()> {
    std::fs::create_dir_all(&output_dir)?;

    let mut bundle_idx = 0;
    let mut event_count = 0;
    let mut byte_count = 0;
    let mut event_hashes = Vec::new();

    let bundle_path = output_dir.join(format!("bundle_{:06}.ndjson", bundle_idx));
    let mut writer = BufWriter::new(File::create(&bundle_path)?);

    for event in receiver {
        // Compute event hash
        let event_hash = event.compute_hash()?;
        event_hashes.push(event_hash);

        // Write as NDJSON
        let line = serde_json::to_string(&event)? + "\n";
        let line_bytes = line.as_bytes();
        writer.write_all(line_bytes)?;

        event_count += 1;
        byte_count += line_bytes.len();

        // Check rotation conditions
        if event_count >= max_events || byte_count >= max_bytes {
            writer.flush()?;
            drop(writer);

            // TODO: Sign bundle with Merkle root
            finalize_bundle(&bundle_path, &event_hashes)?;

            // Start new bundle
            bundle_idx += 1;
            event_count = 0;
            byte_count = 0;
            event_hashes.clear();

            let bundle_path = output_dir.join(format!("bundle_{:06}.ndjson", bundle_idx));
            writer = BufWriter::new(File::create(&bundle_path)?);
        }
    }

    // Flush final bundle
    writer.flush()?;
    Ok(())
}

fn finalize_bundle(path: &Path, event_hashes: &[B3Hash]) -> Result<()> {
    // Compute Merkle root (simplified but deterministic)
    let merkle_root = if event_hashes.is_empty() {
        B3Hash::hash(b"empty")
    } else {
        let mut combined = Vec::new();
        for hash in event_hashes {
            combined.extend_from_slice(hash.as_bytes());
        }
        B3Hash::hash(&combined)
    };

    // Sign bundle with Merkle root (per Artifacts Ruleset #13)
    let signature = sign_bundle_merkle_root(&merkle_root)?;

    // Write metadata file
    let meta_path = path.with_extension("meta.json");
    let metadata = BundleMetadata {
        event_count: event_hashes.len(),
        merkle_root,
        signature: Some(hex::encode(&signature)),
    };

    let meta_file = File::create(meta_path)?;
    serde_json::to_writer_pretty(meta_file, &metadata)?;

    Ok(())
}

/// Sign bundle Merkle root with Ed25519 keypair
/// Per Artifacts Ruleset #13: Sign with Ed25519, store signature in bundle metadata
fn sign_bundle_merkle_root(merkle_root: &B3Hash) -> Result<Vec<u8>> {
    // In production, this would use a key from Secure Enclave
    // For now, generate an ephemeral keypair for development
    // TODO: Integrate with Secure Enclave for production
    let keypair = Keypair::generate();

    // Sign the Merkle root bytes
    let signature = keypair.sign(merkle_root.as_bytes());

    Ok(signature.to_bytes().to_vec())
}

/// Verify bundle signature (for audit/validation)
pub fn verify_bundle_signature(
    merkle_root: &B3Hash,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool> {
    use adapteros_crypto::{PublicKey, Signature};

    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid public key hex: {}", e)))?;

    // Convert bytes to proper types
    let mut sig_array = [0u8; 64];
    if signature_bytes.len() != 64 {
        return Err(AosError::Crypto("Invalid signature length".to_string()));
    }
    sig_array.copy_from_slice(&signature_bytes);
    let signature = Signature::from_bytes(&sig_array)?;

    let mut pk_array = [0u8; 32];
    if public_key_bytes.len() != 32 {
        return Err(AosError::Crypto("Invalid public key length".to_string()));
    }
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key = PublicKey::from_bytes(&pk_array)?;

    // Verify signature
    public_key.verify(merkle_root.as_bytes(), &signature)?;
    Ok(true)
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleMetadata {
    event_count: usize,
    merkle_root: B3Hash,
    signature: Option<String>, // Ed25519 signature in hex format
}

/// Security events (always logged at 100% sampling per Telemetry Ruleset #9)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SecurityEvent {
    PolicyViolation {
        policy: String,
        violation_type: String,
        details: String,
        timestamp: String,
    },
    EgressAttempt {
        destination: String,
        blocked: bool,
        timestamp: String,
    },
    IsolationViolation {
        tenant_id: String,
        details: String,
        timestamp: String,
    },
}

// ============================================================
// Tier 6: Adapters & Clusters Telemetry Events
// ============================================================

/// Adapter swap event (hot-swap operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSwapEvent {
    pub tenant: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub vram_mb: i64,
    pub latency_ms: u64,
    pub result: String, // "ok", "failed", "rollback"
    pub stack_hash: Option<String>,
}

/// Adapter preload event (phase 1 of hot-swap)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPreloadEvent {
    pub adapter_id: String,
    pub vram_mb: u64,
    pub latency_ms: u64,
    pub result: String, // "ok", "failed"
}

/// Stack verification event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackVerificationEvent {
    pub plan_id: String,
    pub stack_hash: String,
    pub adapters: Vec<String>,
    pub result: String, // "ok", "mismatch", "error"
}

/// Node sync event (replication operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSyncEvent {
    pub session_id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub bytes: u64,
    pub artifacts: usize,
    pub duration_ms: u64,
    pub result: String, // "success", "failed", "partial"
    pub verified: bool,
}
