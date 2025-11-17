//! Bundle management
//!
//! Implements NDJSON event bundling with Merkle tree signing for audit trails.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::signature::Keypair;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Bundle writer with automatic rotation and signing
pub struct BundleWriter {
    current_bundle: Option<BufWriter<File>>,
    current_bundle_path: Option<PathBuf>,
    event_count: usize,
    max_events: usize,
    max_bytes: u64,
    current_bytes: u64,
    output_dir: PathBuf,
    signer: Keypair,
    events_buffer: Vec<Vec<u8>>, // Store raw event bytes for Merkle tree
    last_merkle_root: Option<B3Hash>, // Track previous bundle for chaining
    event_seq_counter: AtomicU64,
}

impl BundleWriter {
    /// Create a new bundle writer
    pub fn new<P: AsRef<Path>>(output_dir: P, max_events: usize, max_bytes: u64) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&output_dir)?;

        // Generate signing keypair
        let signer = Keypair::generate();

        Ok(Self {
            current_bundle: None,
            current_bundle_path: None,
            event_count: 0,
            max_events,
            max_bytes,
            current_bytes: 0,
            output_dir,
            signer,
            events_buffer: Vec::new(),
            last_merkle_root: None,
            event_seq_counter: AtomicU64::new(0),
        })
    }

    /// Write an event to the current bundle
    pub fn write_event<T: Serialize>(&mut self, event: &T) -> Result<()> {
        // Serialize event to JSON
        let event_json = serde_json::to_vec(event)?;

        // Check if we need to rotate
        if self.should_rotate(&event_json)? {
            self.rotate_bundle()?;
        }

        // Create new bundle if needed
        if self.current_bundle.is_none() {
            self.create_new_bundle()?;
        }

        // Write event as NDJSON (newline-delimited JSON)
        if let Some(ref mut writer) = self.current_bundle {
            writer.write_all(&event_json)?;
            writer.write_all(b"\n")?;

            // Store for Merkle tree computation
            self.events_buffer.push(event_json.clone());
            self.event_count += 1;
            self.current_bytes += event_json.len() as u64 + 1; // +1 for newline
        }

        Ok(())
    }

    /// Check if bundle should be rotated
    fn should_rotate(&self, next_event: &[u8]) -> Result<bool> {
        if self.current_bundle.is_none() {
            return Ok(false);
        }

        // Rotate if we exceed event count
        if self.event_count >= self.max_events {
            return Ok(true);
        }

        // Rotate if we would exceed byte limit
        if self.current_bytes + next_event.len() as u64 + 1 > self.max_bytes {
            return Ok(true);
        }

        Ok(false)
    }

    /// Create a new bundle file
    fn create_new_bundle(&mut self) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_millis();

        let bundle_name = format!("bundle_{}.ndjson", timestamp);
        let bundle_path = self.output_dir.join(&bundle_name);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&bundle_path)
            .map_err(|e| AosError::Io(e.to_string()))?;

        self.current_bundle = Some(BufWriter::new(file));
        self.current_bundle_path = Some(bundle_path);
        self.event_count = 0;
        self.current_bytes = 0;
        self.events_buffer.clear();

        Ok(())
    }

    /// Rotate the current bundle (close, sign, start new)
    pub fn rotate_bundle(&mut self) -> Result<()> {
        if self.current_bundle.is_none() {
            return Ok(());
        }

        // Flush and close current bundle
        if let Some(mut writer) = self.current_bundle.take() {
            writer.flush().map_err(|e| AosError::Io(e.to_string()))?;
        }

        if let Some(bundle_path) = self.current_bundle_path.take() {
            // Compute Merkle root
            let merkle_root = self.compute_merkle_root()?;

            // Sign the Merkle root
            let signature = self.signer.sign(merkle_root.as_bytes());

            // Write signature file with chain link to previous bundle
            let sig_path = bundle_path.with_extension("ndjson.sig");
            let seq_no = self.event_seq_counter.fetch_add(1, Ordering::SeqCst);
            let sig_data = SignatureMetadata {
                merkle_root: merkle_root.to_string(),
                signature: hex::encode(signature.to_bytes()),
                public_key: hex::encode(self.signer.public_key().to_bytes()),
                event_count: self.event_count,
                sequence_no: seq_no,
                prev_bundle_hash: self.last_merkle_root,
                version: 1, // Default version
            };

            let sig_json =
                serde_json::to_string_pretty(&sig_data).map_err(AosError::Serialization)?;

            std::fs::write(&sig_path, sig_json).map_err(|e| AosError::Io(e.to_string()))?;

            // Update last_merkle_root for next bundle
            self.last_merkle_root = Some(merkle_root);
        }

        // Clear buffer
        self.events_buffer.clear();

        Ok(())
    }

    /// Compute Merkle root of all events in current bundle
    fn compute_merkle_root(&self) -> Result<B3Hash> {
        if self.events_buffer.is_empty() {
            return Ok(B3Hash::new([0u8; 32]));
        }

        // Sort events lexicographically for deterministic Merkle tree construction
        let mut sorted_events = self.events_buffer.clone();
        sorted_events.sort();

        // Build Merkle tree bottom-up
        let mut level: Vec<B3Hash> = sorted_events
            .iter()
            .map(|event| B3Hash::hash(event))
            .collect();

        // Build tree by pairing and hashing
        while level.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in level.chunks(2) {
                let hash = if chunk.len() == 2 {
                    // Pair: hash concatenation
                    let mut combined = chunk[0].as_bytes().to_vec();
                    combined.extend_from_slice(chunk[1].as_bytes());
                    B3Hash::hash(&combined)
                } else {
                    // Odd node: promote to next level
                    chunk[0]
                };
                next_level.push(hash);
            }

            level = next_level;
        }

        Ok(level[0])
    }

    /// Force rotation (for testing or shutdown)
    pub fn flush(&mut self) -> Result<()> {
        if self.event_count > 0 {
            self.rotate_bundle()?;
        }
        Ok(())
    }

    /// Get the public key for verification
    pub fn public_key(&self) -> String {
        hex::encode(self.signer.public_key().to_bytes())
    }
}

impl Drop for BundleWriter {
    fn drop(&mut self) {
        // Best effort rotation on drop
        let _ = self.flush();
    }
}

/// Signature metadata stored alongside bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureMetadata {
    pub merkle_root: String,
    pub signature: String,
    pub public_key: String,
    pub event_count: usize,
    pub sequence_no: u64,
    /// Previous bundle's Merkle root for chain verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_bundle_hash: Option<B3Hash>,
    pub version: u32, // default 1
}
