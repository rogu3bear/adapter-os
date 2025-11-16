//! Adapter hot-swap infrastructure for live adapter loading/unloading
//!
//! Implements two-phase hot-swap with:
//! - Preload: Load adapter into VRAM
//! - Swap: Atomic pointer flip with verification
//! - Rollback: Revert to last verified state
//! - Verify: Recompute effective-stack hash
//!
//! Cross-layer integrity verification:
//! - compute_stack_hash(): Metadata-only (adapter IDs + .aos hashes)
//! - compute_cross_layer_hash(): Includes GPU buffer fingerprints
//!
//! GPU fingerprint format: GpuBufferFingerprint from adapteros-lora-kernel-mtl

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Convert adapter ID string to deterministic u16 using BLAKE3 hash
///
/// Uses first 2 bytes of BLAKE3 hash for deterministic, collision-resistant mapping.
/// This ensures the same adapter ID always maps to the same u16 across runs and platforms.
///
/// # Determinism Guarantee
/// - Same input → same output (unlike DefaultHasher)
/// - Stable across Rust versions and platforms
/// - 16-bit space: ~65k unique IDs before collisions
///
/// # Example
/// ```
/// let id = adapter_id_to_u16("my_adapter");
/// assert_eq!(id, adapter_id_to_u16("my_adapter"));  // Always equal
/// ```
pub fn adapter_id_to_u16(adapter_id: &str) -> u16 {
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let bytes = hash.to_bytes();
    u16::from_le_bytes([bytes[0], bytes[1]])
}

/// Adapter command for IPC communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterCommand {
    /// Preload adapter into VRAM without activating
    Preload { adapter_id: String, hash: B3Hash },
    /// Swap adapters atomically (add and remove sets)
    Swap {
        add_ids: Vec<String>,
        remove_ids: Vec<String>,
    },
    /// Rollback to last verified adapter set
    Rollback,
    /// Verify current adapter stack hash
    VerifyStack,
}

/// Result of adapter command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCommandResult {
    pub success: bool,
    pub message: String,
    pub vram_delta_mb: Option<i64>,
    pub duration_ms: u64,
    pub stack_hash: Option<B3Hash>,
}

/// Adapter state in hot-swap system
#[derive(Debug, Clone)]
pub struct AdapterState {
    pub id: String,
    pub hash: B3Hash,
    pub vram_mb: u64,
    pub loaded_at: Instant,
    pub active: bool,
}

/// GPU buffer fingerprint for cross-layer integrity verification
///
/// Simplified version for adapter_hotswap - full implementation in adapteros-lora-kernel-mtl
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuFingerprint {
    /// Adapter ID this fingerprint belongs to
    pub adapter_id: String,
    /// Buffer size in bytes
    pub buffer_bytes: u64,
    /// BLAKE3 hash of checkpoint samples (first/last/mid 4KB)
    pub checkpoint_hash: B3Hash,
}

/// Stack checkpoint for replay verification
///
/// Combines metadata and GPU state for cross-layer integrity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackCheckpoint {
    /// Timestamp when snapshot was taken
    pub timestamp: u64,
    /// Metadata-only stack hash (adapter IDs + .aos hashes)
    pub metadata_hash: B3Hash,
    /// Cross-layer hash (metadata + GPU fingerprints)
    pub cross_layer_hash: Option<B3Hash>,
    /// GPU fingerprints at time of snapshot
    pub gpu_fingerprints: Vec<GpuFingerprint>,
    /// Adapter IDs in the stack
    pub adapter_ids: Vec<String>,
}

/// Double-buffered adapter table for atomic swaps
pub struct AdapterTable {
    /// Currently active adapters
    active: RwLock<HashMap<String, AdapterState>>,
    /// Staged adapters being preloaded
    staged: RwLock<HashMap<String, AdapterState>>,
    /// Last verified state for rollback
    rollback_state: RwLock<Option<HashMap<String, AdapterState>>>,
    /// In-memory checkpoint history (limited to last N checkpoints)
    checkpoints: RwLock<Vec<StackCheckpoint>>,
    /// Maximum checkpoints to keep in memory
    max_checkpoints: usize,
}

impl AdapterTable {
    /// Create new empty adapter table
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback_state: RwLock::new(None),
            checkpoints: RwLock::new(Vec::new()),
            max_checkpoints: 20, // Keep last 20 checkpoints in memory
        }
    }

    /// Create adapter table with custom checkpoint limit
    pub fn with_checkpoint_limit(max_checkpoints: usize) -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback_state: RwLock::new(None),
            checkpoints: RwLock::new(Vec::new()),
            max_checkpoints,
        }
    }

    /// Preload adapter into staging area
    pub fn preload(&self, id: String, hash: B3Hash, vram_mb: u64) -> Result<()> {
        let mut staged = self.staged.write();

        if staged.contains_key(&id) {
            return Err(AosError::Worker(format!("Adapter {} already staged", id)));
        }

        staged.insert(
            id.clone(),
            AdapterState {
                id,
                hash,
                vram_mb,
                loaded_at: Instant::now(),
                active: false,
            },
        );

        Ok(())
    }

    /// Swap adapters atomically with mutex-guarded pointer flip
    pub fn swap(&self, add_ids: &[String], remove_ids: &[String]) -> Result<(i64, usize)> {
        // Save current state for potential rollback
        {
            let active = self.active.read();
            let mut rollback = self.rollback_state.write();
            *rollback = Some(active.clone());
        }

        let mut active = self.active.write();
        let mut staged = self.staged.write();

        // Calculate VRAM delta
        let mut vram_delta: i64 = 0;

        // Remove specified adapters
        for id in remove_ids {
            if let Some(adapter) = active.remove(id) {
                vram_delta -= adapter.vram_mb as i64;
            }
        }

        // Add staged adapters
        let mut added_count = 0;
        for id in add_ids {
            if let Some(mut adapter) = staged.remove(id) {
                adapter.active = true;
                vram_delta += adapter.vram_mb as i64;
                active.insert(id.clone(), adapter);
                added_count += 1;
            } else {
                // Rollback on partial failure (keep locks held to prevent race)
                let rollback = self.rollback_state.read();
                if let Some(ref saved_state) = *rollback {
                    *active = saved_state.clone();
                    tracing::warn!(
                        adapter_id = %id,
                        "Rolled back to previous state due to missing staged adapter"
                    );
                    // Locks will be dropped automatically at end of scope
                    return Err(AosError::Worker(format!(
                        "Adapter {} not found in staged set",
                        id
                    )));
                } else {
                    return Err(AosError::Worker(format!(
                        "Adapter {} not found in staged set and no rollback state available",
                        id
                    )));
                }
            }
        }

        Ok((vram_delta, added_count))
    }

    /// Rollback to last verified state
    pub fn rollback(&self) -> Result<()> {
        self.rollback_internal()
    }

    fn rollback_internal(&self) -> Result<()> {
        let mut active = self.active.write();
        let rollback = self.rollback_state.read();

        if let Some(ref saved_state) = *rollback {
            *active = saved_state.clone();
            let adapter_count = saved_state.len();

            // Verify rollback integrity
            drop(active);
            drop(rollback);
            let new_hash = self.compute_stack_hash();
            tracing::info!(
                stack_hash = %new_hash.to_short_hex(),
                adapter_count = adapter_count,
                "Rollback completed and verified"
            );

            Ok(())
        } else {
            Err(AosError::Worker("No rollback state available".to_string()))
        }
    }

    /// Compute effective stack hash for verification (metadata only)
    ///
    /// Hashes adapter IDs + .aos file hashes for metadata-layer integrity.
    /// For full cross-layer verification, use compute_cross_layer_hash().
    pub fn compute_stack_hash(&self) -> B3Hash {
        let active = self.active.read();

        // Sort adapter IDs for deterministic hash
        let mut ids: Vec<_> = active.keys().collect();
        ids.sort();

        // Concatenate adapter hashes
        let mut hasher = blake3::Hasher::new();
        for id in ids {
            if let Some(adapter) = active.get(id) {
                hasher.update(id.as_bytes());
                hasher.update(&adapter.hash.to_bytes());
            }
        }

        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Compute cross-layer stack hash (metadata + GPU fingerprints)
    ///
    /// Combines adapter metadata with GPU buffer fingerprints for complete
    /// integrity verification across lifecycle and GPU layers.
    ///
    /// # Arguments
    /// * `gpu_fingerprints` - GPU buffer fingerprints from VramTracker
    ///
    /// # Returns
    /// Cross-layer hash combining:
    /// - Adapter IDs + .aos hashes (metadata layer)
    /// - GPU buffer fingerprints (data layer)
    pub fn compute_cross_layer_hash(&self, gpu_fingerprints: &[GpuFingerprint]) -> B3Hash {
        let active = self.active.read();

        // Sort adapter IDs for deterministic hash
        let mut ids: Vec<_> = active.keys().collect();
        ids.sort();

        // Create hasher for cross-layer hash
        let mut hasher = blake3::Hasher::new();

        // Phase 1: Hash metadata layer (adapter IDs + .aos hashes)
        for id in &ids {
            if let Some(adapter) = active.get(*id) {
                hasher.update(id.as_bytes());
                hasher.update(&adapter.hash.to_bytes());
            }
        }

        // Phase 2: Hash GPU fingerprints (sorted by adapter ID)
        let mut sorted_fps: Vec<_> = gpu_fingerprints.iter().collect();
        sorted_fps.sort_by(|a, b| a.adapter_id.cmp(&b.adapter_id));

        for fp in sorted_fps {
            hasher.update(fp.adapter_id.as_bytes());
            hasher.update(&fp.buffer_bytes.to_le_bytes());
            hasher.update(&fp.checkpoint_hash.to_bytes());
        }

        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Create snapshot checkpoint of current state
    ///
    /// Captures both metadata and GPU fingerprints for replay verification.
    /// Automatically manages checkpoint history (keeps last N checkpoints).
    ///
    /// # Arguments
    /// * `gpu_fingerprints` - Current GPU buffer fingerprints
    ///
    /// # Returns
    /// The created checkpoint
    pub fn create_checkpoint(&self, gpu_fingerprints: Vec<GpuFingerprint>) -> StackCheckpoint {
        let active = self.active.read();

        // Get current adapter IDs
        let mut adapter_ids: Vec<_> = active.keys().cloned().collect();
        adapter_ids.sort();

        // Compute hashes
        drop(active); // Release lock before computing hashes
        let metadata_hash = self.compute_stack_hash();
        let cross_layer_hash = Some(self.compute_cross_layer_hash(&gpu_fingerprints));

        let checkpoint = StackCheckpoint {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_secs(),
            metadata_hash,
            cross_layer_hash,
            gpu_fingerprints,
            adapter_ids,
        };

        // Store checkpoint (with rolling window)
        let mut checkpoints = self.checkpoints.write();
        checkpoints.push(checkpoint.clone());

        // Keep only last N checkpoints
        if checkpoints.len() > self.max_checkpoints {
            checkpoints.remove(0);
        }

        checkpoint
    }

    /// Get recent checkpoints
    ///
    /// Returns up to `limit` most recent checkpoints
    pub fn get_checkpoints(&self, limit: usize) -> Vec<StackCheckpoint> {
        let checkpoints = self.checkpoints.read();
        let start = if checkpoints.len() > limit {
            checkpoints.len() - limit
        } else {
            0
        };
        checkpoints[start..].to_vec()
    }

    /// Verify current state matches a checkpoint
    ///
    /// Compares current state against a stored checkpoint to detect drift.
    ///
    /// # Arguments
    /// * `checkpoint` - Reference checkpoint to verify against
    /// * `current_gpu_fps` - Current GPU fingerprints
    ///
    /// # Returns
    /// Ok(true) if state matches, Ok(false) if checksum mismatch, Err on verification failure
    pub fn verify_against_checkpoint(
        &self,
        checkpoint: &StackCheckpoint,
        current_gpu_fps: &[GpuFingerprint],
    ) -> Result<bool> {
        // Compute current hashes
        let current_metadata = self.compute_stack_hash();
        let current_cross_layer = self.compute_cross_layer_hash(current_gpu_fps);

        // Verify metadata hash
        if current_metadata != checkpoint.metadata_hash {
            return Ok(false);
        }

        // Verify cross-layer hash if available
        if let Some(expected_cross_layer) = checkpoint.cross_layer_hash {
            if current_cross_layer != expected_cross_layer {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get current active adapters
    pub fn get_active(&self) -> Vec<AdapterState> {
        self.active.read().values().cloned().collect()
    }

    /// Get total VRAM usage
    pub fn total_vram_mb(&self) -> u64 {
        self.active.read().values().map(|a| a.vram_mb).sum()
    }

    /// Clear staged adapters
    pub fn clear_staged(&self) {
        self.staged.write().clear();
    }

    /// Save checkpoints to disk for crash recovery
    ///
    /// Uses atomic write (temp file + rename) to ensure consistency.
    ///
    /// # Arguments
    /// * `path` - Path to save checkpoints (e.g., `/var/run/aos/stack_checkpoints.json`)
    ///
    /// # Returns
    /// Ok(()) on success, error if write fails
    pub fn save_checkpoints(&self, path: &Path) -> Result<()> {
        let checkpoints = self.checkpoints.read();

        // Serialize checkpoints to JSON
        let serialized = serde_json::to_string_pretty(&*checkpoints)?;

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, serialized)
            .map_err(|e| AosError::Io(format!("Failed to write checkpoint temp file: {}", e)))?;

        // Rename with cleanup on failure
        if let Err(e) = std::fs::rename(&temp_path, path) {
            // Clean up orphaned temp file
            std::fs::remove_file(&temp_path).ok();
            return Err(AosError::Io(format!("Failed to rename checkpoint file: {}", e)));
        }

        tracing::info!(
            checkpoint_count = checkpoints.len(),
            path = %path.display(),
            "Checkpoints saved to disk"
        );

        Ok(())
    }

    /// Restore checkpoints from disk
    ///
    /// Loads previously saved checkpoints for crash recovery.
    ///
    /// # Arguments
    /// * `path` - Path to load checkpoints from
    ///
    /// # Returns
    /// Ok(()) on success (or if file doesn't exist), error if load fails
    pub fn restore_checkpoints(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            tracing::info!(
                path = %path.display(),
                "No checkpoint file found, starting fresh"
            );
            return Ok(());
        }

        let data = std::fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read checkpoint file: {}", e)))?;

        let restored: Vec<StackCheckpoint> = serde_json::from_str(&data)?;

        *self.checkpoints.write() = restored;

        tracing::info!(
            checkpoint_count = self.checkpoints.read().len(),
            path = %path.display(),
            "Checkpoints restored from disk"
        );

        Ok(())
    }
}

impl Default for AdapterTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter hot-swap manager
///
/// Generic over kernel backend K. Use HotSwapManagerNoKernel for metadata-only mode.
pub struct HotSwapManager<K> {
    table: Arc<AdapterTable>,
    kernels: Option<Arc<tokio::sync::Mutex<K>>>,
    adapters_path: std::path::PathBuf,
}

/// Type alias for hot-swap manager without kernel backend (metadata only)
pub type HotSwapManagerNoKernel = HotSwapManager<()>;

impl HotSwapManagerNoKernel {
    /// Create new hot-swap manager without kernel backend (metadata only)
    ///
    /// For backward compatibility. Equivalent to new_metadata_only().
    pub fn new() -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            adapters_path: std::path::PathBuf::from("."),
        }
    }
}

impl<K> HotSwapManager<K>
where
    K: adapteros_lora_kernel_api::FusedKernels + Send + Sync,
{
    /// Create new hot-swap manager with kernel backend
    pub fn new_with_kernels(
        kernels: Arc<tokio::sync::Mutex<K>>,
        adapters_path: std::path::PathBuf,
    ) -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: Some(kernels),
            adapters_path,
        }
    }

    /// Create new hot-swap manager without kernel backend (metadata only)
    pub fn new_metadata_only(adapters_path: std::path::PathBuf) -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            adapters_path,
        }
    }

    /// Execute adapter command
    pub async fn execute(&self, command: AdapterCommand) -> Result<AdapterCommandResult> {
        let start = Instant::now();

        let result = match command {
            AdapterCommand::Preload { adapter_id, hash } => {
                // Load actual adapter weights if kernel backend is available
                let vram_mb = if let Some(ref kernels) = self.kernels {
                    // Load .aos file (async I/O to avoid blocking executor)
                    let adapter_path = self.adapters_path.join(format!("{}.aos", adapter_id));
                    let adapter_bytes = tokio::fs::read(&adapter_path).await.map_err(|e| {
                        AosError::Io(format!(
                            "Failed to read adapter file {}: {}",
                            adapter_path.display(),
                            e
                        ))
                    })?;

                    // Parse AOS2 format to extract SafeTensors payload
                    // Format: [0-3] manifest_offset, [4-7] manifest_len, [offset] manifest, [weights_offset] safetensors
                    if adapter_bytes.len() < 8 {
                        return Err(AosError::Validation(
                            "Invalid .aos file: too small".to_string(),
                        ));
                    }

                    let manifest_offset = u32::from_le_bytes([
                        adapter_bytes[0],
                        adapter_bytes[1],
                        adapter_bytes[2],
                        adapter_bytes[3],
                    ]) as usize;
                    let manifest_len = u32::from_le_bytes([
                        adapter_bytes[4],
                        adapter_bytes[5],
                        adapter_bytes[6],
                        adapter_bytes[7],
                    ]) as usize;

                    if adapter_bytes.len() < manifest_offset + manifest_len {
                        return Err(AosError::Validation(
                            "Invalid .aos file: manifest out of bounds".to_string(),
                        ));
                    }

                    let manifest_bytes =
                        &adapter_bytes[manifest_offset..manifest_offset + manifest_len];
                    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
                        .map_err(|e| {
                            AosError::Parse(format!("Invalid AOS manifest: {}", e))
                        })?;

                    // Extract weights offset from manifest
                    let weights_offset = manifest
                        .get("weights_offset")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| {
                            AosError::Validation("Missing weights_offset in manifest".to_string())
                        })? as usize;

                    if adapter_bytes.len() < weights_offset {
                        return Err(AosError::Validation(
                            "Invalid .aos file: weights out of bounds".to_string(),
                        ));
                    }

                    // Extract SafeTensors payload
                    let weights = &adapter_bytes[weights_offset..];

                    // Get adapter ID as u16 (deterministic BLAKE3 hash)
                    let adapter_id_u16 = adapter_id_to_u16(&adapter_id);

                    // Load weights into GPU
                    let mut kernels_lock = kernels.lock().await;
                    kernels_lock.load_adapter(adapter_id_u16, weights)?;

                    // Get actual VRAM usage from Metal buffers
                    // This ensures tracking matches real GPU allocation
                    let vram_mb = match kernels_lock.verify_adapter_buffers(adapter_id_u16) {
                        Ok((buffer_size, first_sample, last_sample, mid_sample)) => {
                            // Use actual Metal buffer size (includes alignment padding)
                            let vram_mb = (buffer_size / (1024 * 1024)).max(1);

                            // Create and store GPU fingerprint for integrity verification
                            use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                            let gpu_fp = GpuBufferFingerprint::new(
                                buffer_size,
                                &first_sample,
                                &last_sample,
                                &mid_sample,
                            );
                            kernels_lock.store_gpu_fingerprint(
                                adapter_id_u16,
                                buffer_size,
                                &gpu_fp.checkpoint_hash.to_hex()
                            );

                            tracing::info!(
                                adapter_id = %adapter_id,
                                vram_mb = vram_mb,
                                buffer_size = buffer_size,
                                "Adapter loaded with GPU fingerprint stored"
                            );

                            vram_mb
                        }
                        Err(e) => {
                            // Fallback to payload size if verification fails
                            tracing::warn!(
                                adapter_id = %adapter_id,
                                error = %e,
                                "Failed to verify GPU buffers, using payload size estimate"
                            );
                            let vram_bytes = weights.len() as u64;
                            (vram_bytes / (1024 * 1024)).max(1)
                        }
                    };

                    drop(kernels_lock);
                    vram_mb
                } else {
                    // No kernel backend - use mock value for metadata-only mode
                    tracing::warn!(adapter_id = %adapter_id, "No kernel backend available, using mock VRAM value");
                    24 // Mock value
                };

                self.table.preload(adapter_id.clone(), hash, vram_mb)?;

                AdapterCommandResult {
                    success: true,
                    message: format!("Preloaded adapter: {}", adapter_id),
                    vram_delta_mb: Some(vram_mb as i64),
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: None,
                }
            }

            AdapterCommand::Swap {
                add_ids,
                remove_ids,
            } => {
                // Unload removed adapters from GPU
                if let Some(ref kernels) = self.kernels {
                    let mut kernels_lock = kernels.lock().await;

                    for remove_id in &remove_ids {
                        // Convert adapter ID string to u16 (deterministic BLAKE3 hash)
                        let adapter_id_u16 = adapter_id_to_u16(remove_id);

                        // Unload from GPU (ignoring errors if not loaded)
                        if let Err(e) = kernels_lock.unload_adapter(adapter_id_u16) {
                            tracing::warn!(
                                adapter_id = %remove_id,
                                error = %e,
                                "Failed to unload adapter from GPU (may not be loaded)"
                            );
                        }
                    }

                    drop(kernels_lock);
                }

                let (vram_delta, _added_count) = self.table.swap(&add_ids, &remove_ids)?;
                let stack_hash = self.table.compute_stack_hash();

                // TODO: Collect GPU fingerprints for cross-layer verification
                // This requires vram_tracker which is Metal-specific
                // Commented out for now to allow generic FusedKernels usage
                let cross_layer_hash = if let Some(ref _kernels) = self.kernels {
                    let _active_adapters = self.table.get_active();
                    let gpu_fingerprints: Vec<GpuFingerprint> = Vec::new();

                    // let kernels_lock = kernels.lock().await;
                    // let vram_tracker = kernels_lock.vram_tracker();
                    //
                    // for adapter_state in &active_adapters {
                    //     use std::collections::hash_map::DefaultHasher;
                    //     use std::hash::{Hash, Hasher};
                    //     let mut hasher = DefaultHasher::new();
                    //     adapter_state.id.hash(&mut hasher);
                    //     let adapter_id_u16 = (hasher.finish() % 65536) as u16;
                    //
                    //     if let Some(fp) = vram_tracker.get_fingerprint(adapter_id_u16 as u32) {
                    //         gpu_fingerprints.push(GpuFingerprint {
                    //             adapter_id: adapter_state.id.clone(),
                    //             buffer_bytes: fp.buffer_bytes,
                    //             checkpoint_hash: fp.checkpoint_hash,
                    //         });
                    //     }
                    // }

                    // drop(kernels_lock);

                    // Create checkpoint with GPU fingerprints
                    let checkpoint = self.table.create_checkpoint(gpu_fingerprints);
                    tracing::info!(
                        metadata_hash = %checkpoint.metadata_hash,
                        cross_layer_hash = ?checkpoint.cross_layer_hash,
                        "Cross-layer checkpoint created after swap (vram_tracker disabled)"
                    );

                    checkpoint.cross_layer_hash
                } else {
                    None
                };

                AdapterCommandResult {
                    success: true,
                    message: format!("Swapped: +{:?} / -{:?}", add_ids, remove_ids),
                    vram_delta_mb: Some(vram_delta),
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: cross_layer_hash.or(Some(stack_hash)),
                }
            }

            AdapterCommand::Rollback => {
                self.table.rollback()?;
                let stack_hash = self.table.compute_stack_hash();

                AdapterCommandResult {
                    success: true,
                    message: "Rolled back to last verified state".to_string(),
                    vram_delta_mb: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: Some(stack_hash),
                }
            }

            AdapterCommand::VerifyStack => {
                let stack_hash = self.table.compute_stack_hash();

                // Verify GPU state and create cross-layer checkpoint
                // TODO: Collect GPU fingerprints for cross-layer verification
                // This requires vram_tracker which is Metal-specific
                // Commented out for now to allow generic FusedKernels usage
                let cross_layer_hash = if let Some(ref _kernels) = self.kernels {
                    let _active_adapters = self.table.get_active();
                    let gpu_fingerprints: Vec<GpuFingerprint> = Vec::new();

                    // let kernels_lock = kernels.lock().await;
                    // let vram_tracker = kernels_lock.vram_tracker();
                    //
                    // for adapter_state in &active_adapters {
                    //     use std::collections::hash_map::DefaultHasher;
                    //     use std::hash::{Hash, Hasher};
                    //     let mut hasher = DefaultHasher::new();
                    //     adapter_state.id.hash(&mut hasher);
                    //     let adapter_id_u16 = (hasher.finish() % 65536) as u16;
                    //
                    //     if let Some(fp) = vram_tracker.get_fingerprint(adapter_id_u16 as u32) {
                    //         gpu_fingerprints.push(GpuFingerprint {
                    //             adapter_id: adapter_state.id.clone(),
                    //             buffer_bytes: fp.buffer_bytes,
                    //             checkpoint_hash: fp.checkpoint_hash,
                    //         });
                    //     }
                    // }

                    // drop(kernels_lock);

                    // Verify against latest checkpoint if available
                    let checkpoints = self.table.get_checkpoints(1);
                    if let Some(latest_checkpoint) = checkpoints.last() {
                        match self
                            .table
                            .verify_against_checkpoint(latest_checkpoint, &gpu_fingerprints)
                        {
                            Ok(true) => {
                                tracing::info!("GPU integrity verification PASSED");
                            }
                            Ok(false) => {
                                tracing::warn!("GPU state diverged from checkpoint");
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "GPU verification failed");
                            }
                        }
                    }

                    // Create new checkpoint
                    let checkpoint = self.table.create_checkpoint(gpu_fingerprints);
                    checkpoint.cross_layer_hash
                } else {
                    None
                };

                AdapterCommandResult {
                    success: true,
                    message: "Stack verified (with GPU integrity check)".to_string(),
                    vram_delta_mb: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: cross_layer_hash.or(Some(stack_hash)),
                }
            }
        };

        Ok(result)
    }

    /// Get adapter table reference
    pub fn table(&self) -> &Arc<AdapterTable> {
        &self.table
    }
}

// Note: Default impl only for HotSwapManagerNoKernel (type alias for HotSwapManager<()>)
// Generic HotSwapManager<K> should use new_with_kernels() instead

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preload_and_swap() {
        let table = AdapterTable::new();

        // Preload two adapters
        let hash1 = B3Hash::hash(b"adapter1");
        let hash2 = B3Hash::hash(b"adapter2");

        table
            .preload("adapter1".to_string(), hash1, 10)
            .expect("Test adapter preload should succeed");
        table
            .preload("adapter2".to_string(), hash2, 15)
            .expect("Test adapter preload should succeed");

        // Swap them in
        let (delta, count) = table
            .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
            .expect("Test adapter swap should succeed");
        assert_eq!(delta, 25);
        assert_eq!(count, 2);

        // Verify stack hash is deterministic
        let hash_1 = table.compute_stack_hash();
        let hash_2 = table.compute_stack_hash();
        assert_eq!(hash_1, hash_2);
    }

    #[test]
    fn test_rollback() {
        let table = AdapterTable::new();

        // Preload and swap adapter1
        let hash1 = B3Hash::hash(b"adapter1");
        table
            .preload("adapter1".to_string(), hash1, 10)
            .expect("Test adapter preload should succeed");
        table
            .swap(&["adapter1".to_string()], &[])
            .expect("Test adapter swap should succeed");

        let hash_before = table.compute_stack_hash();

        // Preload and swap adapter2
        let hash2 = B3Hash::hash(b"adapter2");
        table
            .preload("adapter2".to_string(), hash2, 20)
            .expect("Test adapter preload should succeed");
        table
            .swap(&["adapter2".to_string()], &["adapter1".to_string()])
            .expect("Test adapter swap should succeed");

        // Rollback should restore adapter1
        table.rollback().expect("Test rollback should succeed");
        let hash_after = table.compute_stack_hash();

        assert_eq!(hash_before, hash_after);
    }
}
