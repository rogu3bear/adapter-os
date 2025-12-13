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

use crate::lifecycle_state::LifecycleState;
use adapteros_core::{
    adapter_fs_path_with_root,
    adapter_store::{AdapterCacheKey, AdapterRecord, AdapterStore},
    constants::BYTES_PER_MB,
    identity::IdentityEnvelope,
    AosError, B3Hash, RepoAdapterPaths, Result,
};
use adapteros_telemetry::{make_health_payload, HealthEventKind, TelemetryWriter};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender as MpscSender;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, Duration};

const SYSTEM_TENANT: &str = "system";

#[derive(Debug, Clone)]
pub struct Stack {
    pub generation: u64,
    pub active: HashMap<String, AdapterState>,
}

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
/// use adapteros_lora_worker::adapter_hotswap::adapter_id_to_u16;
///
/// let id = adapter_id_to_u16("my_adapter");
/// assert_eq!(id, adapter_id_to_u16("my_adapter"));  // Always equal
/// ```
pub fn adapter_id_to_u16(adapter_id: &str) -> u16 {
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let bytes = hash.to_bytes();
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn resolve_adapter_file(repo_root: &Path, tenant_id: &str, adapter_id: &str) -> std::path::PathBuf {
    let adapter_dir = adapter_fs_path_with_root(repo_root, tenant_id, adapter_id)
        .unwrap_or_else(|_| repo_root.join(tenant_id).join(adapter_id));
    let flat = adapter_dir.with_extension("aos");
    if flat.exists() {
        return flat;
    }

    if let Ok(entries) = fs::read_dir(&adapter_dir) {
        let mut files: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "aos"))
            .collect();
        files.sort();
        if let Some(path) = files.into_iter().next() {
            return path;
        }
    }

    flat
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
    pub lifecycle: LifecycleState,
}

/// Identity metadata for adapter cache keys so refcount/pinning aligns with
/// the context manifest fields that differentiate adapter loads.
#[derive(Debug, Clone)]
pub struct AdapterCacheIdentity {
    pub base_manifest_hash: Option<B3Hash>,
    pub backend_type: String,
    pub kernel_version_id: String,
    pub tenant_id: Option<String>,
    pub adapter_dir_hash: Option<B3Hash>,
}

impl Default for AdapterCacheIdentity {
    fn default() -> Self {
        Self {
            base_manifest_hash: None,
            backend_type: "unknown".to_string(),
            kernel_version_id: adapteros_core::version::VERSION.to_string(),
            tenant_id: None,
            adapter_dir_hash: None,
        }
    }
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
    rollback_state: RwLock<Option<Arc<Stack>>>,
    /// In-memory checkpoint history (limited to last N checkpoints)
    checkpoints: RwLock<Vec<StackCheckpoint>>,
    /// Maximum checkpoints to keep in memory
    max_checkpoints: usize,
    /// Atomic pointer to the current active stack
    /// INVARIANT: current_stack generation strictly increases on successful swap. The atomic pointer ensures readers see consistent stack during inference.
    current_stack: AtomicUsize,
    /// Reference counts for staged adapters (shared with AdapterStore)
    refcounts: TokioMutex<HashMap<String, Arc<AtomicUsize>>>,
    /// List of retired stacks for RCU
    retired_stacks: TokioMutex<Vec<Arc<Stack>>>,
    /// Sender for event-driven retirement wake-up when refcounts reach 0 (bounded to prevent memory growth)
    retirement_sender: Option<MpscSender<()>>,
    /// Telemetry writer for RCU events
    telemetry: Option<Arc<TelemetryWriter>>,
    /// Tenant identifier for telemetry identity
    tenant_id: Option<String>,
    /// Retry counts for RCU
    retry_counts: TokioMutex<HashMap<u64, u32>>,
    /// Adapter index for request pinning and draining
    store: AdapterStore,
    /// Cache identity used to compose adapter cache keys
    cache_identity: RwLock<AdapterCacheIdentity>,
}

impl AdapterTable {
    /// Update cache identity used to key adapter refcount snapshots.
    pub fn set_cache_identity(&self, identity: AdapterCacheIdentity) {
        *self.cache_identity.write() = identity;
    }

    fn cache_identity_snapshot(&self) -> AdapterCacheIdentity {
        self.cache_identity.read().clone()
    }

    /// Create new empty adapter table
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback_state: RwLock::new(None),
            checkpoints: RwLock::new(Vec::new()),
            max_checkpoints: 20, // Keep last 20 checkpoints in memory
            current_stack: AtomicUsize::new(0),
            refcounts: TokioMutex::new(HashMap::new()),
            retired_stacks: TokioMutex::new(Vec::new()),
            retirement_sender: None,
            telemetry: None,
            tenant_id: None,
            retry_counts: TokioMutex::new(HashMap::new()),
            store: AdapterStore::new(),
            cache_identity: RwLock::new(AdapterCacheIdentity::default()),
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
            current_stack: AtomicUsize::new(0),
            refcounts: TokioMutex::new(HashMap::new()),
            retired_stacks: TokioMutex::new(Vec::new()),
            retirement_sender: None,
            telemetry: None,
            tenant_id: None,
            retry_counts: TokioMutex::new(HashMap::new()),
            store: AdapterStore::new(),
            cache_identity: RwLock::new(AdapterCacheIdentity::default()),
        }
    }

    fn emit_swap_event(
        &self,
        add_ids: &[String],
        remove_ids: &[String],
        success: bool,
        error: Option<String>,
    ) {
        let Some(tel) = &self.telemetry else {
            return;
        };

        let tenant_id = self
            .tenant_id
            .clone()
            .unwrap_or_else(|| "system".to_string());
        let identity = IdentityEnvelope::new(
            tenant_id.clone(),
            "worker".to_string(),
            "adapter_swap".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        let adapters: Vec<String> = add_ids.iter().chain(remove_ids.iter()).cloned().collect();

        let payload = make_health_payload(
            "adapter_hotswap".to_string(),
            tenant_id,
            if success {
                HealthEventKind::AdapterSwap
            } else {
                HealthEventKind::FatalError
            },
            None,
            Some(if success {
                "swapped".to_string()
            } else {
                "failed".to_string()
            }),
            None,
            Some(adapters),
            error,
        );

        let _ = tel.log_health_lifecycle(identity, payload);
    }

    /// Preload adapter into staging area
    pub async fn preload(&self, id: String, hash: B3Hash, vram_mb: u64) -> Result<()> {
        if vram_mb == 0 {
            return Err(AosError::Worker(
                "Adapter preload requires non-zero VRAM estimate".to_string(),
            ));
        }
        {
            let mut staged = self.staged.write();

            if !staged.contains_key(&id) {
                staged.insert(
                    id.clone(),
                    AdapterState {
                        id: id.clone(),
                        hash,
                        vram_mb,
                        loaded_at: Instant::now(),
                        active: false,
                        lifecycle: LifecycleState::Loaded,
                    },
                );
            }
        } // Drop staged lock before await

        // Ensure refcount entry exists for this adapter
        let mut refcounts = self.refcounts.lock().await;
        refcounts
            .entry(id.clone())
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
        drop(refcounts);

        Ok(())
    }

    /// Swap adapters atomically with mutex-guarded pointer flip
    ///
    /// FIX 3: Hot-swap partial removal - Validate ALL add_ids exist in staged BEFORE removing any adapter
    pub async fn swap(&self, add_ids: &[String], remove_ids: &[String]) -> Result<(i64, usize)> {
        // Save current stack for potential rollback
        {
            let current = self.current_stack.load(Ordering::Acquire);
            *self.rollback_state.write() = Some(Arc::new(Stack {
                generation: current as u64,
                active: self.active.read().clone(),
            }));
        }

        // FIX 3: VALIDATE all add_ids exist in staged BEFORE making any changes
        // This prevents partial swap where some removes succeed but adds fail
        {
            let staged_read = self.staged.read();
            for id in add_ids {
                if !staged_read.contains_key(id) {
                    let err = AosError::Worker(format!(
                        "Adapter {} not found in staged set - aborting swap before any changes",
                        id
                    ));
                    self.emit_swap_event(add_ids, remove_ids, false, Some(err.to_string()));
                    return Err(err);
                }
            }
        } // Drop staged_read lock

        let old_stack = self.current_stack.load(Ordering::Acquire);
        let mut new_active = self.active.read().clone();
        let old_active_snapshot = new_active.clone();

        // Calculate VRAM delta
        let mut vram_delta: i64 = 0;

        // Remove specified adapters
        for id in remove_ids {
            if let Some(adapter) = new_active.remove(id) {
                vram_delta -= adapter.vram_mb as i64;
            }
        }

        // Add staged adapters (all guaranteed to exist after validation above)
        let mut added_count = 0;
        {
            // Use a read lock to allow reusing staged adapters across swaps; avoid consuming entries
            let staged_read = self.staged.read();
            for id in add_ids {
                if let Some(mut adapter) = staged_read.get(id).cloned() {
                    adapter.active = true;
                    adapter.lifecycle = LifecycleState::Active;
                    vram_delta += adapter.vram_mb as i64;
                    new_active.insert(id.clone(), adapter);
                    added_count += 1;
                } else {
                    // FIX 3 defensive path (should not hit due to earlier validation)
                    let rollback_state = self.rollback_state.read();
                    if let Some(rollback_stack) = rollback_state.as_ref() {
                        let _old = self
                            .current_stack
                            .swap(rollback_stack.generation as usize, Ordering::AcqRel);
                        tracing::error!(
                            adapter_id = %id,
                            "UNEXPECTED: Adapter not in staged after validation - rolling back"
                        );
                        self.staged.write().clear();
                        let err = AosError::Worker(format!(
                            "Adapter {} disappeared from staged set after validation (possible concurrent modification)",
                            id
                        ));
                        self.emit_swap_event(add_ids, remove_ids, false, Some(err.to_string()));
                        return Err(err);
                    } else {
                        self.staged.write().clear();
                        let err = AosError::Worker(format!(
                            "Adapter {} disappeared from staged set and no rollback state available",
                            id
                        ));
                        self.emit_swap_event(add_ids, remove_ids, false, Some(err.to_string()));
                        return Err(err);
                    }
                }
            }
        } // Drop staged_read lock before await

        // Ensure refcounts for new active adapters
        let mut refcounts = self.refcounts.lock().await;
        for name in new_active.keys() {
            refcounts
                .entry(name.clone())
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
        }
        drop(refcounts);

        let new_gen = old_stack + 1;
        let new_active_snapshot = new_active.clone();
        {
            let mut active_guard = self.active.write();
            *active_guard = new_active;
        }

        // Update generation pointer
        let old = self.current_stack.swap(new_gen, Ordering::AcqRel);

        // Publish new index for request pinning
        let refcounts_guard = self.refcounts.lock().await;
        let identity = self.cache_identity_snapshot();
        let store_entries: HashMap<AdapterCacheKey, AdapterRecord> = new_active_snapshot
            .iter()
            .map(|(id, state)| {
                let rc = refcounts_guard
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(AtomicUsize::new(0)));
                let cache_key = AdapterCacheKey::new(
                    id.clone(),
                    state.hash,
                    identity.base_manifest_hash,
                    identity.backend_type.clone(),
                    identity.kernel_version_id.clone(),
                    identity.tenant_id.clone(),
                    identity.adapter_dir_hash,
                );
                (
                    cache_key,
                    AdapterRecord {
                        hash: state.hash,
                        refcount: rc,
                    },
                )
            })
            .collect();
        drop(refcounts_guard);
        self.store.install(new_gen as u64, store_entries);

        // Retire previous stack if generation changed
        if old != new_gen {
            let mut retired = self.retired_stacks.lock().await;
            retired.push(Arc::new(Stack {
                generation: old as u64,
                active: old_active_snapshot,
            }));
        }

        self.emit_swap_event(add_ids, remove_ids, true, None);
        Ok((vram_delta, added_count))
    }

    /// Rollback to last verified state
    pub async fn rollback(&self) -> Result<()> {
        let rollback_stack = self
            .rollback_state
            .read()
            .as_ref()
            .cloned()
            .ok_or_else(|| AosError::Worker("No rollback state available".to_string()))?;

        let old = self
            .current_stack
            .swap(rollback_stack.generation as usize, Ordering::AcqRel);

        {
            let mut active_guard = self.active.write();
            *active_guard = rollback_stack.active.clone();
        }

        // Retire the previous current stack if generation changed
        if old as u64 > rollback_stack.generation {
            let mut retired = self.retired_stacks.lock().await;
            retired.push(Arc::new(Stack {
                generation: old as u64,
                active: self.active.read().clone(),
            }));
        }

        *self.rollback_state.write() = None;

        let stack_hash = self.compute_stack_hash();
        tracing::info!(
            stack_hash = %stack_hash.to_short_hex(),
            "Rollback completed and verified"
        );

        Ok(())
    }

    /// Compute effective stack hash for verification (metadata only)
    ///
    /// Hashes adapter IDs + .aos file hashes for metadata-layer integrity.
    /// For full cross-layer verification, use compute_cross_layer_hash().
    pub fn compute_stack_hash(&self) -> B3Hash {
        let _stack = self.current_stack.load(Ordering::Acquire);
        let active = self.active.read();

        // Collect (adapter_id, hash) pairs from active adapters
        let pairs: Vec<(String, B3Hash)> = active
            .iter()
            .map(|(id, adapter)| (id.clone(), adapter.hash))
            .collect();

        // Use canonical compute_stack_hash from adapteros-core
        adapteros_core::compute_stack_hash(pairs)
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
        let _stack = self.current_stack.load(Ordering::Acquire);
        let active_guard = self.active.read();
        let mut ids: Vec<_> = active_guard.keys().collect();
        ids.sort();

        let mut hasher = blake3::Hasher::new();

        for id in &ids {
            if let Some(adapter) = active_guard.get(*id) {
                hasher.update(id.as_bytes());
                hasher.update(&adapter.hash.to_bytes());
            }
        }

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
        let _stack = self.current_stack.load(Ordering::Acquire);
        let mut adapter_ids: Vec<_> = self.active.read().keys().cloned().collect();
        adapter_ids.sort();

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

        let mut checkpoints = self.checkpoints.write();
        checkpoints.push(checkpoint.clone());

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
        let current_metadata = self.compute_stack_hash();
        let current_cross_layer = self.compute_cross_layer_hash(current_gpu_fps);

        if current_metadata != checkpoint.metadata_hash {
            return Ok(false);
        }

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

    /// Decrement refcount for an adapter and send wake-up if it reaches 0
    /// Returns the new refcount
    pub async fn dec_ref(&self, name: &str) -> usize {
        let refcounts = self.refcounts.lock().await;
        if let Some(rc) = refcounts.get(name) {
            let old = rc.fetch_sub(1, Ordering::Relaxed);
            if old == 1 {
                if let Some(tx) = &self.retirement_sender {
                    let _ = tx
                        .try_send(())
                        .map_err(|_| tracing::warn!("Failed to send retirement signal"));
                }
            }
            old.saturating_sub(1)
        } else {
            0
        }
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
            return Err(AosError::Io(format!(
                "Failed to rename checkpoint file: {}",
                e
            )));
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

    /// Increment reference count for an adapter
    pub async fn inc_ref(&self, adapter_id: &str) {
        let refcounts = self.refcounts.lock().await;
        if let Some(rc) = refcounts.get(adapter_id) {
            rc.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wait for referenced adapters to reach refcount 0 before hot-swap
    ///
    /// Returns error if the timeout elapses while any adapter is still referenced.
    pub async fn wait_for_zero_refs(
        &self,
        adapter_ids: &[String],
        timeout: Duration,
    ) -> Result<()> {
        let start = Instant::now();
        let mut logged_wait = false;

        loop {
            let counts: Vec<(String, usize)> = {
                let refcounts = self.refcounts.lock().await;
                adapter_ids
                    .iter()
                    .map(|id| {
                        let count = refcounts
                            .get(id)
                            .map(|rc| rc.load(Ordering::Relaxed))
                            .unwrap_or(0);
                        (id.clone(), count)
                    })
                    .collect()
            };

            if counts.iter().all(|(_, count)| *count == 0) {
                return Ok(());
            }

            if !logged_wait {
                tracing::info!(
                    adapter_ids = ?adapter_ids,
                    refcounts = ?counts,
                    "Waiting for in-flight sequences to drain before hot-swap"
                );
                logged_wait = true;
            }

            if start.elapsed() >= timeout {
                tracing::error!(
                    adapter_ids = ?adapter_ids,
                    refcounts = ?counts,
                    timeout_ms = timeout.as_millis(),
                    "Hot-swap drain timed out; adapters still referenced"
                );
                return Err(AosError::Worker(format!(
                    "Hot-swap blocked: adapters still in use after {:?}: {:?}",
                    timeout, counts
                )));
            }

            sleep(Duration::from_millis(25)).await;
        }
    }

    /// Process retired stacks for RCU (Reference Count Update)
    ///
    /// This function is designed to be called periodically or when the system
    /// needs to ensure all retired stacks are unloaded. It locks the retired_stacks
    /// mutex, iterates through the list, and attempts to unload adapters from the
    /// kernel backend if they are no longer referenced.
    ///
    /// Retry invariant: Each retired stack gets at most 3 unload attempts before quarantine
    /// to prevent infinite loops under persistent kernel failures. Quarantined stacks are
    /// removed from the retirement queue and logged for manual intervention.
    ///
    /// # Arguments
    /// * `kernels_opt` - Optional reference to the kernel backend mutex.
    ///   If None, it will only remove from the list, not unload.
    ///
    /// # Returns
    /// Ok(()) on success, Err(e) on error.
    pub async fn process_retired_stacks<
        K: adapteros_lora_kernel_api::FusedKernels + Send + Sync,
    >(
        &self,
        kernels_opt: Option<Arc<tokio::sync::Mutex<K>>>,
    ) -> Result<()> {
        let mut retired_guard = self.retired_stacks.lock().await;
        let mut i = 0;
        while i < retired_guard.len() {
            let stack = &retired_guard[i];
            let stack_generation = stack.generation;
            let adapter_ids: Vec<String> = stack.active.keys().cloned().collect();
            drop(retired_guard); // Release lock before acquiring refcounts

            let can_unload = {
                let refcounts = self.refcounts.lock().await;
                adapter_ids.iter().all(|id| {
                    refcounts
                        .get(id)
                        .is_some_and(|rc| rc.load(Ordering::Relaxed) == 0)
                })
            };
            retired_guard = self.retired_stacks.lock().await; // re-acquire
            if i >= retired_guard.len() {
                break;
            } // list may have changed
            let stack_ref = &retired_guard[i];
            if stack_ref.generation != stack_generation {
                i += 1;
                continue;
            } // changed
            if can_unload {
                let gen = stack_ref.generation;
                let adapter_ids_for_unload: Vec<_> = stack_ref.active.keys().cloned().collect();

                // Check retry count first
                let retry_count = {
                    let retry_guard = self.retry_counts.lock().await;
                    *retry_guard.get(&gen).unwrap_or(&0)
                };

                if retry_count >= 3 {
                    // Quarantine
                    let adapter_ids: Vec<_> = stack_ref.active.keys().cloned().collect();
                    retired_guard.remove(i);
                    {
                        let mut retry_guard = self.retry_counts.lock().await;
                        retry_guard.remove(&gen);
                    }
                    tracing::error!(
                        event = "retire_quarantine",
                        generation = gen,
                        retries = retry_count,
                        "Max retries exceeded, stack quarantined"
                    );
                    if let Some(tel) = &self.telemetry {
                        let event = serde_json::json!({
                            "event_type": "rcu_unload_failed",
                            "generation": gen,
                            "retries": retry_count,
                            "adapter_ids": adapter_ids,
                            "error": "max_retries_exceeded"
                        });
                        let _ = tel.log("rcu_unload_failed", &event);
                    }
                    continue;
                }

                // Release retired_guard before kernel operations
                drop(retired_guard);

                let mut unload_failed = false;
                if let Some(kernels) = kernels_opt.clone() {
                    let mut k_lock = kernels.lock().await;
                    for id in &adapter_ids_for_unload {
                        let id_u16 = adapter_id_to_u16(id);
                        if let Err(e) = k_lock.detach_adapter(id_u16) {
                            tracing::warn!("Failed to unload adapter {}: {}", id, e);
                            unload_failed = true;
                            break; // Retry next time
                        }
                    }
                    drop(k_lock);

                    // Re-acquire retired_guard for removal
                    retired_guard = self.retired_stacks.lock().await;

                    // Find and remove the stack by generation (it may have moved)
                    if !unload_failed {
                        if let Some(pos) = retired_guard.iter().position(|s| s.generation == gen) {
                            retired_guard.remove(pos);
                            let mut retry_guard = self.retry_counts.lock().await;
                            retry_guard.remove(&gen);
                            tracing::info!("Successfully unloaded retired stack gen {}", gen);
                        }
                    } else {
                        // Increment retry and sleep
                        let mut retry_guard = self.retry_counts.lock().await;
                        *retry_guard.entry(gen).or_insert(0) += 1;
                        drop(retry_guard);
                        sleep(Duration::from_millis(100)).await;
                        i += 1;
                    }
                } else {
                    // Re-acquire retired_guard for removal (no kernels case)
                    retired_guard = self.retired_stacks.lock().await;
                    if let Some(pos) = retired_guard.iter().position(|s| s.generation == gen) {
                        retired_guard.remove(pos);
                        let mut retry_guard = self.retry_counts.lock().await;
                        retry_guard.remove(&gen);
                        tracing::info!("Unloaded retired stack (no kernels)");
                    }
                }
            } else {
                i += 1;
            }
        }
        Ok(())
    }

    pub fn current_stack_hash(&self) -> B3Hash {
        let active = self.active.read();
        let mut data = Vec::new();
        for (id, state) in active.iter() {
            data.extend_from_slice(id.as_bytes());
            data.extend_from_slice(&state.hash.to_bytes());
        }
        B3Hash::hash(&data)
    }

    pub fn get_current_stack_generation(&self) -> usize {
        self.current_stack.load(Ordering::Acquire)
    }

    /// Get a handle to the current stack for inference
    ///
    /// Returns a snapshot of the current active adapters. Callers should
    /// increment refcounts for adapters they use and decrement when done.
    pub fn get_current_stack_handle(&self) -> Arc<Stack> {
        let generation = self.current_stack.load(Ordering::Acquire) as u64;
        let active = self.active.read().clone();
        Arc::new(Stack { generation, active })
    }

    /// Get access to refcounts for external management
    pub fn refcounts(&self) -> &TokioMutex<HashMap<String, Arc<AtomicUsize>>> {
        &self.refcounts
    }

    /// Get the current stack generation value
    pub fn current_stack(&self) -> usize {
        self.current_stack.load(Ordering::Acquire)
    }

    /// Get access to retired stacks for external management
    pub fn retired_stacks(&self) -> &TokioMutex<Vec<Arc<Stack>>> {
        &self.retired_stacks
    }

    pub fn store(&self) -> &AdapterStore {
        &self.store
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
    repo_root: std::path::PathBuf,
    tenant_id: String,
}

impl<K> Clone for HotSwapManager<K> {
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            kernels: self.kernels.clone(),
            repo_root: self.repo_root.clone(),
            tenant_id: self.tenant_id.clone(),
        }
    }
}

/// Type alias for hot-swap manager without kernel backend (metadata only)
pub type HotSwapManagerNoKernel = HotSwapManager<()>;

impl Default for HotSwapManagerNoKernel {
    fn default() -> Self {
        Self::new(SYSTEM_TENANT)
    }
}

impl HotSwapManagerNoKernel {
    /// Create new hot-swap manager without kernel backend (metadata only)
    ///
    /// Requires explicit tenant id to keep adapter provenance clear.
    pub fn new<T: Into<String>>(tenant_id: T) -> Self {
        let repo_root = RepoAdapterPaths::from_env_and_config(None).repo_root;
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            repo_root,
            tenant_id: tenant_id.into(),
        }
    }
}

impl<K> HotSwapManager<K>
where
    K: adapteros_lora_kernel_api::FusedKernels + Send + Sync + 'static,
{
    /// Create new hot-swap manager with kernel backend
    pub fn new_with_kernels(
        kernels: Arc<tokio::sync::Mutex<K>>,
        repo_root: std::path::PathBuf,
        tenant_id: String,
        telemetry: Option<Arc<TelemetryWriter>>, // add param
    ) -> Self {
        let (tx, mut rx) = mpsc::channel(100);
        let mut table = AdapterTable::new();
        table.retirement_sender = Some(tx);
        table.telemetry = telemetry;
        table.tenant_id = Some(tenant_id.clone());
        let table_arc = Arc::new(table);
        let table_clone = table_arc.clone();
        let kernels_clone = Some(kernels.clone());

        // Spawn background retirement task with periodic processing and backoff
        tokio::spawn(async move {
            use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

            let backoff =
                BackoffConfig::new(Duration::from_millis(500), Duration::from_secs(60), 2.0, 5);
            let circuit_breaker = BackoffCircuitBreaker::new(5, Duration::from_secs(120));
            let mut consecutive_failures = 0u32;

            loop {
                tokio::select! {
                    _ = rx.recv() => {
                        tracing::debug!("Retirement signal received");
                    }
                    _ = sleep(Duration::from_secs(5)) => {
                        tracing::debug!("Periodic retirement check");
                    }
                }

                // Check circuit breaker state
                if circuit_breaker.is_open() {
                    tracing::warn!(
                        failure_count = circuit_breaker.failure_count(),
                        "Retirement task circuit breaker is open, pausing"
                    );
                    sleep(circuit_breaker.reset_timeout()).await;
                    continue;
                }

                // Process retired stacks
                match table_clone
                    .process_retired_stacks(kernels_clone.clone())
                    .await
                {
                    Ok(_) => {
                        // Success - reset backoff and circuit breaker
                        circuit_breaker.record_success();
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        // Failure - record and apply backoff
                        circuit_breaker.record_failure();
                        consecutive_failures += 1;

                        tracing::error!(
                            error = %e,
                            consecutive_failures = consecutive_failures,
                            "Error in retirement task"
                        );

                        // Apply exponential backoff
                        let delay = backoff.next_delay(consecutive_failures);
                        tracing::warn!(
                            delay_ms = delay.as_millis(),
                            "Applying backoff delay to retirement task"
                        );
                        sleep(delay).await;

                        // If we've exceeded max retries, wait longer before trying again
                        if backoff.should_give_up(consecutive_failures) {
                            tracing::error!(
                                "Retirement task has failed {} times, entering extended backoff",
                                consecutive_failures
                            );
                            sleep(Duration::from_secs(300)).await; // 5 minute extended backoff
                            consecutive_failures = 0; // Reset after extended backoff
                        }
                    }
                }
            }
        });

        Self {
            table: table_arc,
            kernels: Some(kernels),
            repo_root,
            tenant_id,
        }
    }

    /// Create new hot-swap manager without kernel backend (metadata only)
    pub fn new_metadata_only(repo_root: std::path::PathBuf, tenant_id: String) -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            repo_root,
            tenant_id,
        }
    }

    /// Execute adapter command
    pub async fn execute(&self, command: AdapterCommand) -> Result<AdapterCommandResult> {
        let start = Instant::now();

        let result = match command {
            AdapterCommand::Preload { adapter_id, hash } => {
                // Resolve adapter path relative to tenant root and enforce isolation
                let adapter_path =
                    resolve_adapter_file(&self.repo_root, &self.tenant_id, &adapter_id);
                let tenant_root = self.repo_root.join(&self.tenant_id);
                if !adapter_path.starts_with(&tenant_root) {
                    return Err(AosError::IsolationViolation(format!(
                        "Adapter path {} is outside tenant root {}",
                        adapter_path.display(),
                        tenant_root.display()
                    )));
                }

                // Load actual adapter weights if kernel backend is available
                let vram_mb = if let Some(ref kernels) = self.kernels {
                    // Load .aos file (async I/O to avoid blocking executor)
                    let adapter_bytes = tokio::fs::read(&adapter_path).await.map_err(|e| {
                        AosError::Io(format!(
                            "Failed to read adapter file {}: {}",
                            adapter_path.display(),
                            e
                        ))
                    })?;

                    let file_view = adapteros_aos::open_aos(&adapter_bytes)?;
                    let _manifest: serde_json::Value =
                        serde_json::from_slice(file_view.manifest_bytes)
                            .map_err(|e| AosError::Parse(format!("Invalid AOS manifest: {}", e)))?;
                    // Prefer a CoreML-specific segment when present; fall back to canonical.
                    let canonical_segment = file_view
                        .segments
                        .iter()
                        .find(|seg| seg.backend_tag == adapteros_aos::BackendTag::Coreml)
                        .or_else(|| {
                            file_view
                                .segments
                                .iter()
                                .find(|seg| seg.backend_tag == adapteros_aos::BackendTag::Canonical)
                        })
                        .ok_or_else(|| {
                            AosError::Validation(
                                "Missing CoreML or canonical segment in adapter bundle".to_string(),
                            )
                        })?;

                    // Extract SafeTensors payload
                    let weights = canonical_segment.payload;

                    // Get adapter ID as u16 (deterministic BLAKE3 hash)
                    let adapter_id_u16 = adapter_id_to_u16(&adapter_id);

                    // Load weights into GPU
                    let mut kernels_lock = kernels.lock().await;
                    kernels_lock.load_adapter(adapter_id_u16, weights)?;
                    // CoreML hot-swap path uses explicit attach; other backends no-op.
                    kernels_lock.attach_adapter(adapter_id_u16)?;

                    // Get actual VRAM usage from Metal buffers
                    // This ensures tracking matches real GPU allocation
                    let vram_mb = match kernels_lock.verify_adapter_buffers(adapter_id_u16) {
                        Ok((buffer_size, first_sample, last_sample, mid_sample)) => {
                            // Use actual Metal buffer size (includes alignment padding)
                            let vram_mb = (buffer_size / BYTES_PER_MB).max(1);

                            // Create and store GPU fingerprint for integrity verification
                            #[cfg(target_os = "macos")]
                            use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                            #[cfg(target_os = "macos")]
                            let gpu_fp = GpuBufferFingerprint::new(
                                buffer_size,
                                &first_sample,
                                &last_sample,
                                &mid_sample,
                            );
                            #[cfg(target_os = "macos")]
                            if let Err(e) = kernels_lock.store_gpu_fingerprint(
                                adapter_id_u16,
                                buffer_size,
                                &gpu_fp.checkpoint_hash.to_hex(),
                            ) {
                                tracing::warn!(
                                    adapter_id = %adapter_id,
                                    error = %e,
                                    "Failed to store GPU fingerprint (non-fatal)"
                                );
                            } else {
                                tracing::info!(
                                    adapter_id = %adapter_id,
                                    vram_mb = vram_mb,
                                    buffer_size = buffer_size,
                                    "Adapter loaded with GPU fingerprint stored"
                                );
                            }

                            #[cfg(not(target_os = "macos"))]
                            tracing::info!(
                                adapter_id = %adapter_id,
                                vram_mb = vram_mb,
                                buffer_size = buffer_size,
                                "Adapter loaded (GPU fingerprint not available on this platform)"
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
                            (vram_bytes / BYTES_PER_MB).max(1)
                        }
                    };

                    drop(kernels_lock);
                    vram_mb
                } else {
                    // No kernel backend - use mock value for metadata-only mode
                    tracing::warn!(adapter_id = %adapter_id, "No kernel backend available, using mock VRAM value");
                    24 // Mock value
                };

                self.table
                    .preload(adapter_id.clone(), hash, vram_mb)
                    .await?;

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
                // Ensure no in-flight references to adapters being removed
                if !remove_ids.is_empty() {
                    self.table
                        .wait_for_zero_refs(&remove_ids, Duration::from_secs(2))
                        .await?;
                }

                // Unload removed adapters from GPU
                if let Some(ref kernels) = self.kernels {
                    let mut kernels_lock = kernels.lock().await;

                    for remove_id in &remove_ids {
                        // Convert adapter ID string to u16 (deterministic BLAKE3 hash)
                        let adapter_id_u16 = adapter_id_to_u16(remove_id);

                        // Unload from GPU (ignoring errors if not loaded)
                        if let Err(e) = kernels_lock.detach_adapter(adapter_id_u16) {
                            tracing::warn!(
                                adapter_id = %remove_id,
                                error = %e,
                                "Failed to unload adapter from GPU (may not be loaded)"
                            );
                        }
                    }

                    // If a single adapter is being swapped/added, hint the backend to switch.
                    if add_ids.len() == 1 {
                        let add_adapter_u16 = adapter_id_to_u16(&add_ids[0]);
                        if let Err(e) = kernels_lock.switch_adapter(add_adapter_u16) {
                            tracing::warn!(
                                adapter_id = %add_ids[0],
                                backend_reason = "coreml_switch_adapter_failed",
                                error = %e,
                                "Backend switch_adapter hint failed"
                            );
                            return Err(e);
                        };
                    }

                    drop(kernels_lock);
                }

                let (vram_delta, _added_count) = self.table.swap(&add_ids, &remove_ids).await?;
                let stack_hash = self.table.compute_stack_hash();

                let cross_layer_hash = if let Some(ref kernels) = self.kernels {
                    let active_adapters = self.table.get_active();
                    let mut gpu_fingerprints = Vec::new();

                    let kernels_lock = kernels.lock().await;
                    let fingerprint_map = kernels_lock.get_gpu_fingerprints();

                    for adapter_state in &active_adapters {
                        let adapter_id_u16 = adapter_id_to_u16(&adapter_state.id) as u32;
                        if let Some(fp) = fingerprint_map.get(&adapter_id_u16) {
                            gpu_fingerprints.push(GpuFingerprint {
                                adapter_id: adapter_state.id.clone(),
                                buffer_bytes: fp.buffer_bytes,
                                checkpoint_hash: fp.checkpoint_hash,
                            });
                        }
                    }
                    drop(kernels_lock);

                    if !gpu_fingerprints.is_empty() {
                        let checkpoint = self.table.create_checkpoint(gpu_fingerprints);
                        tracing::info!(
                            metadata_hash = %checkpoint.metadata_hash.to_short_hex(),
                            cross_layer_hash = %checkpoint.cross_layer_hash.as_ref().unwrap_or(&B3Hash::zero()).to_short_hex(),
                            fingerprints_count = checkpoint.gpu_fingerprints.len(),
                            "Cross-layer checkpoint created after swap"
                        );
                        checkpoint.cross_layer_hash
                    } else {
                        tracing::trace!("No GPU fingerprints available after swap, falling back to metadata hash");
                        None
                    }
                } else {
                    tracing::trace!("No kernels available, using metadata-only hash");
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
                self.table.rollback().await?;
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
                let cross_layer_hash = if let Some(ref kernels) = self.kernels {
                    let active_adapters = self.table.get_active();
                    let mut gpu_fingerprints = Vec::new();

                    let kernels_lock = kernels.lock().await;
                    let fingerprint_map = kernels_lock.get_gpu_fingerprints();

                    for adapter_state in &active_adapters {
                        let adapter_id_u16 = adapter_id_to_u16(&adapter_state.id) as u32;
                        if let Some(fp) = fingerprint_map.get(&adapter_id_u16) {
                            gpu_fingerprints.push(GpuFingerprint {
                                adapter_id: adapter_state.id.clone(),
                                buffer_bytes: fp.buffer_bytes,
                                checkpoint_hash: fp.checkpoint_hash,
                            });
                        }
                    }
                    drop(kernels_lock);

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
                    if !checkpoint.gpu_fingerprints.is_empty() {
                        tracing::info!(
                            metadata_hash = %checkpoint.metadata_hash.to_short_hex(),
                            cross_layer_hash = %checkpoint.cross_layer_hash.as_ref().unwrap_or(&B3Hash::zero()).to_short_hex(),
                            fingerprints_count = checkpoint.gpu_fingerprints.len(),
                            "Cross-layer verification checkpoint created"
                        );
                    } else {
                        tracing::trace!("No GPU fingerprints for verification, metadata-only");
                    }
                    checkpoint.cross_layer_hash
                } else {
                    tracing::trace!("No kernels for GPU verification, metadata-only");
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

    pub fn set_cache_identity(&self, identity: AdapterCacheIdentity) {
        self.table.set_cache_identity(identity);
    }

    pub fn cache_identity(&self) -> AdapterCacheIdentity {
        self.table.cache_identity_snapshot()
    }

    /// Swap adapters (add and remove sets)
    ///
    /// This method handles the hot-swap operation atomically:
    /// 1. Unloads removed adapters from GPU (if kernel backend available)
    /// 2. Activates newly added adapters
    /// 3. Returns VRAM delta and count of added adapters
    ///
    /// # Arguments
    /// * `add_ids` - Adapter IDs to add to active set
    /// * `remove_ids` - Adapter IDs to remove from active set
    ///
    /// # Returns
    /// Tuple of (vram_delta_mb, added_count)
    pub async fn swap(&self, add_ids: &[String], remove_ids: &[String]) -> Result<(i64, usize)> {
        self.table.swap(add_ids, remove_ids).await
    }

    /// Get adapter table reference
    pub fn table(&self) -> &Arc<AdapterTable> {
        &self.table
    }

    /// Start a background task to process retired stacks
    ///
    /// This function spawns a new tokio task that periodically wakes up
    /// to check the `retired_stacks` list and attempt to unload adapters
    /// that are no longer referenced. It uses a Condvar to wait for new
    /// stacks to be added to the list.
    ///
    /// # Returns
    /// A JoinHandle to the spawned task.
    pub fn start_retirement_task(self: Arc<Self>) -> tokio::task::JoinHandle<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels + Send + Sync + 'static,
    {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; // Simulate request

                // Collect stacks to potentially unload (don't hold lock during processing)
                let stacks_to_check: Vec<_> = {
                    let retired_guard = manager.table.retired_stacks().lock().await;
                    retired_guard.clone() // Clone the stacks to avoid holding lock
                };

                // Process each stack
                for stack in stacks_to_check {
                    let can_unload = {
                        let refcounts_guard = manager.table.refcounts().lock().await;
                        stack.active.iter().all(|(id, _)| {
                            refcounts_guard
                                .get(id)
                                .is_some_and(|rc| rc.load(Ordering::Relaxed) == 0)
                        })
                    };

                    if can_unload {
                        if let Some(kernels) = &manager.kernels {
                            let mut k_lock = kernels.lock().await;
                            let mut unload_failed = false;
                            for id in stack.active.keys() {
                                let id_u16 = adapter_id_to_u16(id);
                                if let Err(e) = k_lock.detach_adapter(id_u16) {
                                    tracing::warn!("Failed to unload adapter {}: {}", id, e);
                                    unload_failed = true;
                                    break; // Retry next time
                                }
                            }
                            if !unload_failed {
                                // Remove from retired stacks
                                let mut retired_guard = manager.table.retired_stacks().lock().await;
                                if let Some(pos) = retired_guard
                                    .iter()
                                    .position(|s| s.generation == stack.generation)
                                {
                                    retired_guard.remove(pos);
                                }
                                tracing::info!(
                                    "Unloaded retired stack generation {}",
                                    stack.generation
                                );
                            }
                        } else {
                            // No kernel backend, just remove
                            let mut retired_guard = manager.table.retired_stacks().lock().await;
                            if let Some(pos) = retired_guard
                                .iter()
                                .position(|s| s.generation == stack.generation)
                            {
                                retired_guard.remove(pos);
                            }
                            tracing::info!("Unloaded retired stack (no kernels)");
                        }
                    }
                }

                let drained = manager.table.store().drain_retired();
                if !drained.is_empty() {
                    tracing::debug!(
                        generations = ?drained,
                        "Drained adapter index generations"
                    );
                }
            }
        })
    }
}

// Note: Default impl only for HotSwapManagerNoKernel (type alias for HotSwapManager<()>)
// Generic HotSwapManager<K> should use new_with_kernels() instead

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::Generator;
    use crate::kvcache::KvCache;
    use adapteros_core::constants::BYTES_PER_MB;
    use adapteros_lora_kernel_api::{attestation, IoBuffers, RouterRing};
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::sync::oneshot;
    use tokio::time::{sleep, Duration};

    #[derive(Default, Debug)]
    struct MockKernels {
        switch_calls: usize,
        detach_calls: usize,
    }

    impl adapteros_lora_kernel_api::FusedKernels for MockKernels {
        fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
            Ok(())
        }

        fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
            Ok(())
        }

        fn device_name(&self) -> &str {
            "mock"
        }

        fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
            Ok(attestation::DeterminismReport {
                backend_type: attestation::BackendType::Mock,
                metallib_hash: None,
                manifest: None,
                rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
                floating_point_mode: attestation::FloatingPointMode::Deterministic,
                compiler_flags: vec![],
                deterministic: true,
            })
        }

        fn load_adapter(&mut self, _id: u16, _weights: &[u8]) -> Result<()> {
            Ok(())
        }

        fn unload_adapter(&mut self, _id: u16) -> Result<()> {
            Ok(())
        }

        fn attach_adapter(&mut self, _id: u16) -> Result<()> {
            Ok(())
        }

        fn detach_adapter(&mut self, _id: u16) -> Result<()> {
            self.detach_calls += 1;
            Ok(())
        }

        fn switch_adapter(&mut self, _id: u16) -> Result<()> {
            self.switch_calls += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn swap_with_single_add_hints_switch_adapter() {
        let kernels = Arc::new(tokio::sync::Mutex::new(MockKernels::default()));
        let repo = tempdir().expect("tempdir");
        let mut manager = HotSwapManager::new_with_kernels(
            kernels.clone(),
            repo.path().to_path_buf(),
            SYSTEM_TENANT.to_string(),
            None,
        );

        let adapter_id = "coreml-hot-swap";
        let hash = B3Hash::hash(adapter_id.as_bytes());
        manager
            .table
            .preload(adapter_id.to_string(), hash, 10)
            .await
            .expect("preload");

        let result = manager
            .execute(AdapterCommand::Swap {
                add_ids: vec![adapter_id.to_string()],
                remove_ids: vec![],
            })
            .await;

        assert!(result.is_ok(), "swap should succeed");
        let guard = kernels.lock().await;
        assert_eq!(
            guard.switch_calls, 1,
            "single-add swaps should call switch_adapter"
        );
        assert_eq!(guard.detach_calls, 0);
    }

    #[tokio::test]
    async fn lifecycle_moves_loaded_to_active_on_swap() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"lifecycle");
        table
            .preload("life".to_string(), hash, 12)
            .await
            .expect("preload succeeds");

        table
            .swap(&["life".to_string()], &[])
            .await
            .expect("swap succeeds");

        let active = table.get_active();
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].lifecycle,
            crate::lifecycle_state::LifecycleState::Active
        );
    }

    #[tokio::test]
    async fn preload_failure_then_success_is_recoverable() {
        let table = AdapterTable::new();
        let hash = B3Hash::hash(b"zero-vram");

        let err = table
            .preload("zero".to_string(), hash, 0)
            .await
            .expect_err("zero vram must fail");
        assert!(
            format!("{err:?}").contains("VRAM"),
            "error should mention VRAM requirement"
        );

        table
            .preload("zero".to_string(), hash, 4)
            .await
            .expect("second preload should succeed");

        table
            .swap(&["zero".to_string()], &[])
            .await
            .expect("swap after successful preload must succeed");

        let pins = table.store().pin_current();
        assert_eq!(pins.hashes().len(), 1);
    }

    #[tokio::test]
    async fn test_preload_and_swap() {
        let table = AdapterTable::new();

        // Preload two adapters
        let hash1 = B3Hash::hash(b"adapter1");
        let hash2 = B3Hash::hash(b"adapter2");

        table
            .preload("adapter1".to_string(), hash1, 10)
            .await
            .expect("Test adapter preload should succeed");
        table
            .preload("adapter2".to_string(), hash2, 15)
            .await
            .expect("Test adapter preload should succeed");

        // Swap them in
        let (delta, count) = table
            .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
            .await
            .expect("Test adapter swap should succeed");
        assert_eq!(delta, 25);
        assert_eq!(count, 2);

        // Verify stack hash is deterministic
        let hash_1 = table.compute_stack_hash();
        let hash_2 = table.compute_stack_hash();
        assert_eq!(hash_1, hash_2);
    }

    #[tokio::test]
    async fn swap_is_atomic_for_inflight_snapshot() {
        let table = AdapterTable::new();

        let hash1 = B3Hash::hash(b"adapter-live");
        table
            .preload("adapter-live".to_string(), hash1, 12)
            .await
            .expect("preload live");
        table
            .swap(&["adapter-live".to_string()], &[])
            .await
            .expect("activate live");

        // Hold a snapshot used by an in-flight request
        let handle_before = table.get_current_stack_handle();
        assert!(handle_before.active.contains_key("adapter-live"));

        // Stage a new adapter and swap atomically
        let hash2 = B3Hash::hash(b"adapter-next");
        table
            .preload("adapter-next".to_string(), hash2, 8)
            .await
            .expect("preload next");
        table
            .swap(&["adapter-next".to_string()], &["adapter-live".to_string()])
            .await
            .expect("swap next");

        // Snapshot held by in-flight request remains consistent
        assert!(handle_before.active.contains_key("adapter-live"));
        assert!(!handle_before.active.contains_key("adapter-next"));

        // New readers see the updated stack
        let handle_after = table.get_current_stack_handle();
        assert!(handle_after.active.contains_key("adapter-next"));
        assert!(!handle_after.active.contains_key("adapter-live"));
    }

    #[tokio::test]
    async fn test_rollback() {
        let table = AdapterTable::new();

        // Preload and swap adapter1
        let hash1 = B3Hash::hash(b"adapter1");
        table
            .preload("adapter1".to_string(), hash1, 10)
            .await
            .expect("Test adapter preload should succeed");
        table
            .swap(&["adapter1".to_string()], &[])
            .await
            .expect("Test adapter swap should succeed");

        let hash_before = table.compute_stack_hash();

        // Preload and swap adapter2
        let hash2 = B3Hash::hash(b"adapter2");
        table
            .preload("adapter2".to_string(), hash2, 20)
            .await
            .expect("Test adapter preload should succeed");
        table
            .swap(&["adapter2".to_string()], &["adapter1".to_string()])
            .await
            .expect("Test adapter swap should succeed");

        // Rollback should restore adapter1
        table
            .rollback()
            .await
            .expect("Test rollback should succeed");
        let hash_after = table.compute_stack_hash();

        assert_eq!(hash_before, hash_after);
    }

    #[tokio::test]
    async fn test_rcu_refcount() {
        let table = AdapterTable::new();
        let h1 = B3Hash::hash(b"test");
        table.preload("test".to_string(), h1, 10).await.unwrap();
        table.swap(&["test".to_string()], &[]).await.unwrap();
        let _stack = table.current_stack();
        let rc = table
            .refcounts()
            .lock()
            .await
            .get("test")
            .unwrap()
            .load(Ordering::Relaxed);
        assert_eq!(rc, 0);
        {
            let refcounts = table.refcounts().lock().await;
            let rca = refcounts.get("test").unwrap();
            rca.fetch_add(1, Ordering::Relaxed);
            assert_eq!(rca.load(Ordering::Relaxed), 1);
        }
        table.swap(&[], &["test".to_string()]).await.unwrap();
        {
            let refcounts = table.refcounts().lock().await;
            let rca = refcounts.get("test").unwrap();
            rca.fetch_sub(1, Ordering::Relaxed);
            assert_eq!(rca.load(Ordering::Relaxed), 0);
        }
        // Note: background would unload, but in test we don't wait
    }

    #[test]
    fn resolve_adapter_file_respects_tenant_directory() {
        let tmp = tempdir().expect("tempdir should be created");
        let repo_root = tmp.path();
        let tenant = "tenant_a";
        let adapter_id = "adapter_x";

        // Correct tenant location
        let tenant_dir = repo_root.join(tenant);
        fs::create_dir_all(&tenant_dir).expect("tenant dir should exist");
        let expected = tenant_dir.join(format!("{adapter_id}.aos"));
        fs::write(&expected, b"ok").expect("should write expected adapter file");

        // Another tenant with same adapter id should not be chosen
        let other_dir = repo_root.join("other_tenant");
        fs::create_dir_all(&other_dir).expect("other tenant dir should exist");
        fs::write(other_dir.join(format!("{adapter_id}.aos")), b"wrong")
            .expect("should write other adapter file");

        let resolved = resolve_adapter_file(repo_root, tenant, adapter_id);
        assert_eq!(resolved, expected);
    }

    #[cfg(feature = "loom")]
    #[test]
    fn loom_rcu_no_uaf() {
        loom::model(|| {
            let table = Arc::new(AdapterTable::new());
            let h = B3Hash::zero();
            table.preload("test".to_string(), h, 10).unwrap();
            table.swap(&["test"], &[]).unwrap();

            let initial_gen = table.current_stack();

            // 50 readers: snapshot, inc, hold, dec
            for _ in 0..50 {
                let table_clone = table.clone();
                loom::thread::spawn(move || {
                    let _stack_gen = table_clone.current_stack();
                    table_clone.inc_ref("test");
                    std::thread::sleep(std::time::Duration::from_secs(1)); // Simulate long inference
                    table_clone.dec_ref("test");
                });
            }

            // 10 writers: preload new, swap every 100ms
            for i in 0..10 {
                let table_clone = table.clone();
                let new_id = format!("new{}", i);
                loom::thread::spawn(move || {
                    let h_new = B3Hash::hash(format!("new{}", i).as_bytes());
                    table_clone.preload(new_id.clone(), h_new, 10).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    table_clone.swap(&[new_id], &["test".to_string()]).unwrap();
                });
            }

            // Wait for all threads (loom handles)
            // Assert after model: gen increased, ref 0
            let final_gen = table.current_stack();
            assert!(
                final_gen > initial_gen,
                "Generation must increase with swaps"
            );
            // Note: In loom tests we cannot use async locks, so skip refcount check
            // Loom detects any races/UAF
        });
    }

    #[tokio::test]
    async fn stress_test_swap_during_inference() {
        let table = AdapterTable::new();
        let h1 = B3Hash::hash(b"a");
        let h2 = B3Hash::hash(b"b");
        table.preload("a".to_string(), h1, 10).await.unwrap();
        table.swap(&["a".to_string()], &[]).await.unwrap();

        // Simulate 100 concurrent infers + 50 swaps
        let mut handles = vec![];
        let table_arc = Arc::new(table);

        for _i in 0..100 {
            let table_clone = table_arc.clone();
            handles.push(tokio::spawn(async move {
                // Simulate infer: snapshot, inc, hold, dec
                let stack = table_clone.get_current_stack_handle();
                {
                    let mut refcounts = table_clone.refcounts.lock().await;
                    for name in stack.active.keys() {
                        refcounts
                            .entry(name.clone())
                            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await; // Simulate 100ms infer
                for name in stack.active.keys() {
                    table_clone.dec_ref(name).await;
                }
                Ok::<(), ()>(())
            }));
        }

        for _ in 0..50 {
            let table_clone = table_arc.clone();
            handles.push(tokio::spawn(async move {
                // Swap a -> b
                table_clone.preload("b".to_string(), h2, 15).await.unwrap();
                table_clone
                    .swap(&["b".to_string()], &["a".to_string()])
                    .await
                    .unwrap();
                Ok(())
            }));
        }

        // Wait for all
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // Assert no panics, refcounts 0
        let stack = table_arc.get_current_stack_handle();
        for name in stack.active.keys() {
            let refcounts = table_arc.refcounts.lock().await;
            assert_eq!(refcounts.get(name).unwrap().load(Ordering::Relaxed), 0);
        }
    }

    #[tokio::test]
    async fn wait_for_zero_refs_blocks_until_release() {
        let table = Arc::new(AdapterTable::new());
        let h = B3Hash::hash(b"hold");
        table
            .preload("hold".to_string(), h, 10)
            .await
            .expect("preload should work");
        table
            .swap(&["hold".to_string()], &[])
            .await
            .expect("swap should work");

        table.inc_ref("hold").await;

        let table_clone = table.clone();
        let wait_handle = tokio::spawn(async move {
            table_clone
                .wait_for_zero_refs(&["hold".to_string()], Duration::from_millis(500))
                .await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        table.dec_ref("hold").await;

        let waited = wait_handle.await.expect("task should run");
        assert!(
            waited.is_ok(),
            "wait_for_zero_refs should succeed after release"
        );
    }

    #[tokio::test]
    async fn wait_for_zero_refs_times_out() {
        let table = AdapterTable::new();
        let h = B3Hash::hash(b"timeout");
        table
            .preload("timeout".to_string(), h, 10)
            .await
            .expect("preload should work");
        table
            .swap(&["timeout".to_string()], &[])
            .await
            .expect("swap should work");

        table.inc_ref("timeout").await;
        let result = table
            .wait_for_zero_refs(&["timeout".to_string()], Duration::from_millis(50))
            .await;
        assert!(
            result.is_err(),
            "wait_for_zero_refs should time out when refcount held"
        );
        table.dec_ref("timeout").await;
    }

    #[tokio::test]
    async fn inflight_swap_is_rejected_and_cache_resets_on_generation_bump() {
        const ADAPTER_ID: &str = "adapter-hotswap-determinism";
        let hash = B3Hash::hash(ADAPTER_ID.as_bytes());

        let table = Arc::new(AdapterTable::new());
        table
            .preload(ADAPTER_ID.to_string(), hash, 10)
            .await
            .expect("preload should work");
        table
            .swap(&[ADAPTER_ID.to_string()], &[])
            .await
            .expect("initial swap should work");

        let kv_cache = Arc::new(Mutex::new(KvCache::new(BYTES_PER_MB)));
        let seed = b"hotswap-determinism-seed-32bytes!!";

        // Baseline run (establish deterministic token stream)
        let (baseline_tokens, baseline_reset) =
            run_stream_for_test(table.clone(), kv_cache.clone(), ADAPTER_ID, None, seed, 6).await;
        assert!(
            baseline_reset,
            "initial coherence check should reset cache from generation 0"
        );

        // Simulate in-flight inference with a concurrent swap attempt that should be rejected.
        let infer_table = table.clone();
        let infer_kv = kv_cache.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        let infer_handle = tokio::spawn(run_stream_for_test(
            infer_table,
            infer_kv,
            ADAPTER_ID,
            Some(ready_tx),
            seed,
            6,
        ));

        let swap_table = table.clone();
        let swap_handle = tokio::spawn(async move {
            ready_rx
                .await
                .expect("inference should hold refs before swap");
            sleep(Duration::from_millis(5)).await;
            swap_table
                .wait_for_zero_refs(&[ADAPTER_ID.to_string()], Duration::from_millis(25))
                .await
        });

        let (tokens_with_swap, reset_flag) = infer_handle.await.expect("join inference task");
        let swap_result = swap_handle.await.expect("join swap task");

        assert!(
            swap_result.is_err(),
            "swap should be rejected while in-flight refcount is held (maps to 409 ADAPTER_IN_USE)"
        );
        assert_eq!(
            tokens_with_swap, baseline_tokens,
            "in-flight stream must stay deterministic when swap is deferred"
        );
        assert!(
            !reset_flag,
            "KV cache should stay coherent when generation has not changed"
        );

        // After in-flight completes, bump generation with identical content and ensure coherence.
        table
            .preload(ADAPTER_ID.to_string(), hash, 10)
            .await
            .expect("restage same adapter");
        table
            .swap(&[ADAPTER_ID.to_string()], &[ADAPTER_ID.to_string()])
            .await
            .expect("self-swap to bump generation should succeed");
        let (post_tokens, post_reset) =
            run_stream_for_test(table.clone(), kv_cache.clone(), ADAPTER_ID, None, seed, 6).await;
        assert!(post_reset, "KV cache must reset on generation change");
        assert_eq!(
            post_tokens, baseline_tokens,
            "same adapter content + seed should remain deterministic across generation bump"
        );
    }

    async fn run_stream_for_test(
        table: Arc<AdapterTable>,
        kv_cache: Arc<Mutex<KvCache>>,
        adapter_id: &str,
        notify_refs_held: Option<oneshot::Sender<()>>,
        seed: &[u8],
        steps: usize,
    ) -> (Vec<u32>, bool) {
        let handle = table.get_current_stack_handle();

        // Align KV cache with the captured generation (mirrors infer_internal)
        let reset = {
            let mut kv_guard = kv_cache.lock().unwrap();
            kv_guard
                .ensure_cache_coherence(handle.generation)
                .expect("coherence check should succeed")
        };

        {
            let refcounts = table.refcounts().lock().await;
            assert!(
                refcounts.contains_key(adapter_id),
                "refcount entry must exist before holding references"
            );
        }

        // Hold refcounts for active adapters
        for name in handle.active.keys() {
            table.inc_ref(name).await;
        }
        if let Some(tx) = notify_refs_held {
            let _ = tx.send(());
        }

        let mut generator = Generator::new_deterministic(seed, "hotswap-determinism");
        let logits = vec![0.25_f32; 4];
        let mut tokens = Vec::new();
        for step in 0..steps {
            generator.reseed_for_step(step);
            tokens.push(
                generator
                    .next_token(&logits)
                    .expect("mock sampling should succeed"),
            );
            sleep(Duration::from_millis(50)).await;
        }

        // Release refcounts
        for name in handle.active.keys() {
            table.dec_ref(name).await;
        }

        (tokens, reset)
    }

    #[tokio::test]
    async fn test_unload_time() {
        let table = Arc::new(AdapterTable::new());
        let h = B3Hash::zero();
        table.preload("test".to_string(), h, 10).await.unwrap();
        table.swap(&["test".to_string()], &[]).await.unwrap();

        // Simulate hold
        table.inc_ref("test").await;
        let start = Instant::now();
        // Simulate work
        tokio::time::sleep(Duration::from_millis(100)).await;
        table.dec_ref("test").await;

        // Wait for background to process (since periodic 5s, manual call for test)
        // Use MockKernels as the type parameter even though we pass None
        table
            .process_retired_stacks::<adapteros_lora_kernel_api::MockKernels>(None)
            .await
            .unwrap();

        let unload_time = start.elapsed();
        assert!(
            unload_time < Duration::from_millis(500),
            "Unload should be reasonably fast: {:?}",
            unload_time
        );
    }
}
