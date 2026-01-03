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

use crate::adapter_integrity::{
    AdapterIntegrityError, AdapterIntegrityMode, AdapterIntegrityReason, AdapterIntegrityVerifier,
};
use crate::galaxy_loader::{AdapterBacking, GalaxyLoader};
use crate::lifecycle_state::LifecycleState;
use adapteros_core::{
    adapter_fs_path_with_root,
    adapter_store::{AdapterCacheKey, AdapterRecord, AdapterStore},
    constants::BYTES_PER_MB,
    identity::IdentityEnvelope,
    AosError, B3Hash, RepoAdapterPaths, Result,
};
use adapteros_telemetry::{
    make_health_payload, CriticalComponentMetrics, HealthEventKind, TelemetryWriter,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender as MpscSender;
use tokio::sync::watch;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, Duration};

const SYSTEM_TENANT: &str = "system";
type AdapterIntegrityResult<T> = std::result::Result<T, AdapterIntegrityError>;

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
        #[serde(default)]
        expected_stack_hash: Option<B3Hash>,
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
    #[serde(default)]
    pub memory_state: Option<MemoryState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

/// Adapter state in hot-swap system
#[derive(Debug, Clone)]
pub struct AdapterState {
    pub id: String,
    pub hash: B3Hash,
    pub expected_hash: B3Hash,
    pub vram_mb: u64,
    pub loaded_at: Instant,
    pub active: bool,
    pub lifecycle: LifecycleState,
    /// Optional backing for zero-copy galaxy mmaps. Keeps the mmap alive while
    /// the adapter is referenced.
    pub backing: Option<AdapterBacking>,
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

#[derive(Debug, Clone)]
struct AdapterEvictionCandidate {
    adapter_id: String,
    loaded_at: Instant,
    vram_bytes: u64,
}

#[derive(Debug, Default)]
struct AdapterEvictionBlockCounts {
    pinned: usize,
    active: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryState {
    pub total_vram_mb: u64,
    pub active_adapters: Vec<MemoryStateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStateEntry {
    pub id: String,
    pub vram_mb: u64,
    pub active: bool,
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
    /// Optional cache budget for adapter residency (bytes)
    adapter_cache_budget_bytes: RwLock<Option<u64>>,
    /// Optional metrics for adapter cache tracking
    metrics: RwLock<Option<Arc<CriticalComponentMetrics>>>,
}

impl AdapterTable {
    /// Update cache identity used to key adapter refcount snapshots.
    pub fn set_cache_identity(&self, identity: AdapterCacheIdentity) {
        *self.cache_identity.write() = identity;
    }

    fn cache_identity_snapshot(&self) -> AdapterCacheIdentity {
        self.cache_identity.read().clone()
    }

    pub fn set_adapter_cache_budget_bytes(&self, budget_bytes: Option<u64>) {
        *self.adapter_cache_budget_bytes.write() = budget_bytes;
    }

    pub fn adapter_cache_budget_bytes(&self) -> Option<u64> {
        *self.adapter_cache_budget_bytes.read()
    }

    pub fn set_metrics(&self, metrics: Arc<CriticalComponentMetrics>) {
        *self.metrics.write() = Some(metrics);
    }

    fn metrics(&self) -> Option<Arc<CriticalComponentMetrics>> {
        self.metrics.read().clone()
    }

    async fn adapter_cache_bytes(&self) -> u64 {
        let mut bytes_by_id: HashMap<String, u64> = HashMap::new();

        {
            let active = self.active.read();
            for (id, state) in active.iter() {
                let bytes = state.vram_mb.saturating_mul(BYTES_PER_MB);
                bytes_by_id
                    .entry(id.clone())
                    .and_modify(|existing| *existing = (*existing).max(bytes))
                    .or_insert(bytes);
            }
        }

        {
            let staged = self.staged.read();
            for (id, state) in staged.iter() {
                let bytes = state.vram_mb.saturating_mul(BYTES_PER_MB);
                bytes_by_id
                    .entry(id.clone())
                    .and_modify(|existing| *existing = (*existing).max(bytes))
                    .or_insert(bytes);
            }
        }

        let retired_guard = self.retired_stacks.lock().await;
        for stack in retired_guard.iter() {
            for (id, state) in stack.active.iter() {
                let bytes = state.vram_mb.saturating_mul(BYTES_PER_MB);
                bytes_by_id
                    .entry(id.clone())
                    .and_modify(|existing| *existing = (*existing).max(bytes))
                    .or_insert(bytes);
            }
        }

        bytes_by_id.values().sum()
    }

    async fn record_adapter_cache_bytes(&self) {
        if let Some(metrics) = self.metrics() {
            let bytes = self.adapter_cache_bytes().await;
            metrics.set_adapter_cache_bytes(bytes);
        }
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
            adapter_cache_budget_bytes: RwLock::new(None),
            metrics: RwLock::new(None),
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
            adapter_cache_budget_bytes: RwLock::new(None),
            metrics: RwLock::new(None),
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
        self.preload_with_backing(id, hash, hash, vram_mb, None)
            .await
    }

    /// Preload adapter into staging area, optionally tracking mmap backing.
    pub async fn preload_with_backing(
        &self,
        id: String,
        expected_hash: B3Hash,
        hash: B3Hash,
        vram_mb: u64,
        backing: Option<AdapterBacking>,
    ) -> Result<()> {
        if vram_mb == 0 {
            return Err(AosError::Worker(
                format!(
                    "Adapter preload failed: VRAM estimate is zero for adapter '{}'. This indicates the adapter weights could not be measured. Check that the .aos file contains valid SafeTensors data.",
                    id
                )
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
                        expected_hash,
                        vram_mb,
                        loaded_at: Instant::now(),
                        active: false,
                        lifecycle: LifecycleState::Loaded,
                        backing: backing.clone(),
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

        self.record_adapter_cache_bytes().await;

        Ok(())
    }

    pub fn staged_expected_hash(&self, adapter_id: &str) -> Option<B3Hash> {
        self.staged
            .read()
            .get(adapter_id)
            .map(|state| state.expected_hash)
    }

    pub fn update_staged_hash(&self, adapter_id: &str, hash: B3Hash) {
        if let Some(state) = self.staged.write().get_mut(adapter_id) {
            state.hash = hash;
        }
    }

    /// Swap adapters atomically with mutex-guarded pointer flip
    ///
    /// FIX 3: Hot-swap partial removal - Validate ALL add_ids exist in staged BEFORE removing any adapter
    pub async fn swap(&self, add_ids: &[String], remove_ids: &[String]) -> Result<(i64, usize)> {
        let current_active_count = self.active.read().len();
        let staged_count = self.staged.read().len();

        tracing::info!(
            target: "inference.cache",
            add_count = add_ids.len(),
            remove_count = remove_ids.len(),
            current_active = current_active_count,
            staged_available = staged_count,
            add_ids = ?add_ids,
            remove_ids = ?remove_ids,
            "Swap decision: preparing adapter hot-swap"
        );

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
                        "Hot-swap aborted: adapter '{}' was not preloaded into staging. You must preload all adapters before swapping them in. Use the Preload command first with adapter ID and hash.",
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
                            "Critical hot-swap error: adapter '{}' was removed from staging between validation and swap (concurrent modification detected). This indicates a race condition. Rolling back to previous state.",
                            id
                        ));
                        self.emit_swap_event(add_ids, remove_ids, false, Some(err.to_string()));
                        return Err(err);
                    } else {
                        self.staged.write().clear();
                        let err = AosError::Worker(format!(
                            "Fatal hot-swap error: adapter '{}' is missing from staging and no rollback state exists. System state may be inconsistent. Manual intervention required.",
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
        let previous_gen = {
            let mut active_guard = self.active.write();
            *active_guard = new_active;
            self.current_stack.swap(new_gen, Ordering::AcqRel)
        };

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
                let mut cache_key = AdapterCacheKey::new(
                    id.clone(),
                    state.hash,
                    identity.base_manifest_hash,
                    identity.backend_type.clone(),
                    identity.kernel_version_id.clone(),
                    identity.tenant_id.clone(),
                    identity.adapter_dir_hash,
                );
                if let Some(backing) = &state.backing {
                    cache_key = cache_key.with_galaxy(backing.galaxy_id());
                }
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
        if previous_gen != new_gen {
            let mut retired = self.retired_stacks.lock().await;
            let retired_adapter_ids: Vec<String> = old_active_snapshot.keys().cloned().collect();
            retired.push(Arc::new(Stack {
                generation: previous_gen as u64,
                active: old_active_snapshot,
            }));

            tracing::warn!(
                target: "inference.cache",
                retired_generation = previous_gen,
                new_generation = new_gen,
                retired_adapter_count = retired_adapter_ids.len(),
                retired_adapters = ?retired_adapter_ids,
                pending_retirement_count = retired.len(),
                "LRU eviction: stack retired pending cleanup"
            );
        }

        tracing::info!(
            target: "inference.cache",
            vram_delta_mb = vram_delta,
            added_count = added_count,
            new_generation = new_gen,
            total_active = new_active_snapshot.len(),
            "Swap decision: adapter hot-swap completed successfully"
        );

        self.emit_swap_event(add_ids, remove_ids, true, None);
        self.record_adapter_cache_bytes().await;
        Ok((vram_delta, added_count))
    }

    /// Rollback to last verified state
    pub async fn rollback(&self) -> Result<()> {
        let rollback_stack = self
            .rollback_state
            .read()
            .as_ref()
            .cloned()
            .ok_or_else(|| AosError::Worker(
                "Rollback failed: no previous adapter state saved. Cannot revert to earlier configuration. This typically happens when attempting rollback before any successful swap.".to_string()
            ))?;

        let (old_generation, old_active_snapshot) = {
            let mut active_guard = self.active.write();
            let snapshot = active_guard.clone();
            *active_guard = rollback_stack.active.clone();

            let previous = self
                .current_stack
                .swap(rollback_stack.generation as usize, Ordering::AcqRel);
            (previous, snapshot)
        };

        // Retire the previous current stack if generation changed
        if old_generation as u64 > rollback_stack.generation {
            let mut retired = self.retired_stacks.lock().await;
            retired.push(Arc::new(Stack {
                generation: old_generation as u64,
                active: old_active_snapshot,
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
    /// * `path` - Path to save checkpoints (e.g., `./var/run/aos/stack_checkpoints.json`)
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
                    "Hot-swap blocked: {} adapter(s) still have active inference requests after {:?} timeout. Adapters in use: {}. Wait for requests to complete or increase timeout.",
                    counts.iter().filter(|(_, c)| *c > 0).count(),
                    timeout,
                    counts.iter()
                        .filter(|(_, c)| *c > 0)
                        .map(|(id, c)| format!("'{}' ({} refs)", id, c))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }

            sleep(Duration::from_millis(25)).await;
        }
    }

    /// Force cleanup of retired adapters with zero refcount
    ///
    /// Workstream 7: This method aggressively cleans up retired stacks where all
    /// adapters have zero refcount. Used during memory pressure to free VRAM.
    ///
    /// Returns the number of stacks cleaned up.
    pub async fn force_cleanup_retired<K: adapteros_lora_kernel_api::FusedKernels + Send + Sync>(
        &self,
        kernels_opt: Option<Arc<tokio::sync::Mutex<K>>>,
    ) -> Result<usize> {
        let mut cleaned_count = 0;
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
            }

            let stack_ref = &retired_guard[i];
            if stack_ref.generation != stack_generation {
                i += 1;
                continue;
            }

            if can_unload {
                let gen = stack_ref.generation;
                let adapter_ids_for_unload: Vec<_> = stack_ref.active.keys().cloned().collect();

                // Release retired_guard before kernel operations
                drop(retired_guard);

                if let Some(kernels) = kernels_opt.clone() {
                    let mut k_lock = kernels.lock().await;
                    let mut unload_failed = false;
                    for id in &adapter_ids_for_unload {
                        let id_u16 = adapter_id_to_u16(id);
                        if let Err(e) = k_lock.detach_adapter(id_u16) {
                            tracing::warn!("Force cleanup failed to unload adapter {}: {}", id, e);
                            unload_failed = true;
                            break;
                        }
                    }
                    drop(k_lock);

                    // Re-acquire retired_guard for removal
                    retired_guard = self.retired_stacks.lock().await;

                    if !unload_failed {
                        if let Some(pos) = retired_guard.iter().position(|s| s.generation == gen) {
                            retired_guard.remove(pos);
                            let mut retry_guard = self.retry_counts.lock().await;
                            retry_guard.remove(&gen);
                            tracing::info!("Force cleanup: unloaded retired stack gen {}", gen);
                            cleaned_count += 1;
                        }
                    } else {
                        i += 1;
                    }
                } else {
                    // Re-acquire retired_guard for removal (no kernels case)
                    retired_guard = self.retired_stacks.lock().await;
                    if let Some(pos) = retired_guard.iter().position(|s| s.generation == gen) {
                        retired_guard.remove(pos);
                        let mut retry_guard = self.retry_counts.lock().await;
                        retry_guard.remove(&gen);
                        tracing::info!("Force cleanup: unloaded retired stack (no kernels)");
                        cleaned_count += 1;
                    }
                }
            } else {
                i += 1;
            }
        }

        if cleaned_count > 0 {
            drop(retired_guard);
            self.record_adapter_cache_bytes().await;
        }

        Ok(cleaned_count)
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
        let mut evicted = false;
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
                    evicted = true;
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
                            evicted = true;
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
                        evicted = true;
                        let mut retry_guard = self.retry_counts.lock().await;
                        retry_guard.remove(&gen);
                        tracing::info!("Unloaded retired stack (no kernels)");
                    }
                }
            } else {
                i += 1;
            }
        }
        if evicted {
            drop(retired_guard);
            self.record_adapter_cache_bytes().await;
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
        let active_guard = self.active.read();
        let generation = self.current_stack.load(Ordering::Acquire) as u64;
        let active = active_guard.clone();
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
    integrity: Arc<AdapterIntegrityVerifier>,
    memory_monitor: Option<Arc<crate::memory::UmaPressureMonitor>>,
    galaxy_loader: GalaxyLoader,
}

impl<K> Clone for HotSwapManager<K> {
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            kernels: self.kernels.clone(),
            repo_root: self.repo_root.clone(),
            tenant_id: self.tenant_id.clone(),
            integrity: self.integrity.clone(),
            memory_monitor: self.memory_monitor.clone(),
            galaxy_loader: self.galaxy_loader.clone(),
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
        let tenant_id = tenant_id.into();
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            repo_root,
            tenant_id: tenant_id.clone(),
            integrity: Arc::new(AdapterIntegrityVerifier::disabled(tenant_id)),
            memory_monitor: None,
            galaxy_loader: GalaxyLoader::new(),
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
        integrity: Arc<AdapterIntegrityVerifier>,
        telemetry: Option<Arc<TelemetryWriter>>,
        memory_monitor: Option<Arc<crate::memory::UmaPressureMonitor>>,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel(100);
        let mut table = AdapterTable::new();
        table.retirement_sender = Some(tx);
        table.telemetry = telemetry;
        table.tenant_id = Some(tenant_id.clone());
        let table_arc = Arc::new(table);
        let table_clone = table_arc.clone();
        let kernels_clone = Some(kernels.clone());
        let memory_monitor_for_task = memory_monitor.clone();

        // Spawn background retirement task with periodic processing and backoff
        // Workstream 7: Enhanced to include memory-pressure-based cleanup
        tokio::spawn(async move {
            use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

            let backoff =
                BackoffConfig::new(Duration::from_millis(500), Duration::from_secs(60), 2.0, 5);
            let circuit_breaker = BackoffCircuitBreaker::new(5, Duration::from_secs(120));
            let mut consecutive_failures = 0u32;

            // Periodic cleanup every 30 seconds
            let mut cleanup_interval = tokio::time::interval(Duration::from_secs(30));
            cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = rx.recv() => {
                        tracing::debug!("Retirement signal received");
                    }
                    _ = sleep(Duration::from_secs(5)) => {
                        tracing::debug!("Periodic retirement check");
                    }
                    _ = cleanup_interval.tick() => {
                        // Workstream 7: Check memory pressure and force cleanup if needed
                        if let Some(ref mem_monitor) = memory_monitor_for_task {
                            let pressure = mem_monitor.get_current_pressure();
                            if pressure >= crate::memory::MemoryPressureLevel::High {
                                tracing::warn!(
                                    pressure = %pressure,
                                    "Memory pressure detected, forcing retired adapter cleanup"
                                );
                                match table_clone.force_cleanup_retired(kernels_clone.clone()).await {
                                    Ok(cleaned) => {
                                        if cleaned > 0 {
                                            tracing::info!(
                                                cleaned_stacks = cleaned,
                                                "Force cleanup completed due to memory pressure"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            error = %e,
                                            "Force cleanup failed during memory pressure"
                                        );
                                    }
                                }
                            }
                        }
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
            integrity,
            memory_monitor,
            galaxy_loader: GalaxyLoader::new(),
        }
    }

    fn memory_state(&self) -> MemoryState {
        let active = self.table.get_active();
        MemoryState {
            total_vram_mb: self.table.total_vram_mb(),
            active_adapters: active
                .into_iter()
                .map(|state| MemoryStateEntry {
                    id: state.id,
                    vram_mb: state.vram_mb,
                    active: state.active,
                })
                .collect(),
        }
    }

    pub fn set_adapter_cache_budget_bytes(&self, budget_bytes: Option<u64>) {
        self.table.set_adapter_cache_budget_bytes(budget_bytes);
    }

    pub fn set_adapter_cache_metrics(&self, metrics: Arc<CriticalComponentMetrics>) {
        self.table.set_metrics(metrics);
    }

    async fn enforce_adapter_cache_budget(
        &self,
        needed_bytes: u64,
        protected_id: Option<&str>,
    ) -> Result<()> {
        let Some(budget_bytes) = self.table.adapter_cache_budget_bytes() else {
            return Ok(());
        };

        if budget_bytes == 0 {
            return Err(AosError::Validation(
                "Adapter cache budget bytes must be greater than zero".to_string(),
            ));
        }

        let current_bytes = self.table.adapter_cache_bytes().await;
        if current_bytes + needed_bytes <= budget_bytes {
            self.table.record_adapter_cache_bytes().await;
            return Ok(());
        }

        let target_bytes = current_bytes + needed_bytes - budget_bytes;
        let mut protected_ids = HashSet::new();
        if let Some(id) = protected_id {
            protected_ids.insert(id.to_string());
        }

        let (candidates, blocked) = self.collect_eviction_candidates(&protected_ids).await;
        let freed_bytes = self
            .evict_candidates_for_budget(target_bytes, candidates)
            .await?;

        let after_bytes = self.table.adapter_cache_bytes().await;
        if after_bytes + needed_bytes > budget_bytes {
            if let Some(metrics) = self.table.metrics() {
                metrics.record_adapter_cache_budget_exceeded();
            }
            let needed_mb = needed_bytes.div_ceil(BYTES_PER_MB);
            let freed_mb = freed_bytes / BYTES_PER_MB;
            let max_mb = budget_bytes / BYTES_PER_MB;
            return Err(AosError::ResourceExhaustion(format!(
                "Adapter cache budget exceeded: needed {needed_mb} MB, freed {freed_mb} MB (pinned={}, active={}), max {max_mb} MB",
                blocked.pinned, blocked.active
            )));
        }

        self.table.record_adapter_cache_bytes().await;
        Ok(())
    }

    async fn collect_eviction_candidates(
        &self,
        protected_ids: &HashSet<String>,
    ) -> (Vec<AdapterEvictionCandidate>, AdapterEvictionBlockCounts) {
        let active_ids: HashSet<String> = self.table.active.read().keys().cloned().collect();
        let refcounts_snapshot: HashMap<String, usize> = {
            let refcounts = self.table.refcounts.lock().await;
            refcounts
                .iter()
                .map(|(id, rc)| (id.clone(), rc.load(Ordering::Relaxed)))
                .collect()
        };

        let mut blocked = AdapterEvictionBlockCounts::default();
        let mut candidates: HashMap<String, AdapterEvictionCandidate> = HashMap::new();

        let mut consider = |adapter_id: &str, state: &AdapterState| {
            if protected_ids.contains(adapter_id) {
                return;
            }
            if active_ids.contains(adapter_id) {
                blocked.active += 1;
                return;
            }
            if refcounts_snapshot.get(adapter_id).copied().unwrap_or(0) > 0 {
                blocked.pinned += 1;
                return;
            }

            let vram_bytes = state.vram_mb.saturating_mul(BYTES_PER_MB);
            if vram_bytes == 0 {
                return;
            }

            if let Some(existing) = candidates.get_mut(adapter_id) {
                if state.loaded_at < existing.loaded_at {
                    existing.loaded_at = state.loaded_at;
                }
                existing.vram_bytes = existing.vram_bytes.max(vram_bytes);
            } else {
                candidates.insert(
                    adapter_id.to_string(),
                    AdapterEvictionCandidate {
                        adapter_id: adapter_id.to_string(),
                        loaded_at: state.loaded_at,
                        vram_bytes,
                    },
                );
            }
        };

        {
            let staged = self.table.staged.read();
            for (id, state) in staged.iter() {
                consider(id, state);
            }
        }

        let retired_guard = self.table.retired_stacks.lock().await;
        for stack in retired_guard.iter() {
            for (id, state) in stack.active.iter() {
                consider(id, state);
            }
        }

        let mut list: Vec<AdapterEvictionCandidate> = candidates.into_values().collect();
        list.sort_by(|a, b| {
            a.loaded_at
                .cmp(&b.loaded_at)
                .then_with(|| a.adapter_id.cmp(&b.adapter_id))
        });

        (list, blocked)
    }

    async fn evict_candidates_for_budget(
        &self,
        target_bytes: u64,
        candidates: Vec<AdapterEvictionCandidate>,
    ) -> Result<u64> {
        if target_bytes == 0 {
            return Ok(0);
        }

        let mut freed_bytes = 0u64;
        for candidate in candidates {
            if freed_bytes >= target_bytes {
                break;
            }

            match self
                .evict_adapter_by_id(&candidate.adapter_id, "budget")
                .await
            {
                Ok(bytes) => {
                    freed_bytes += bytes;
                }
                Err(e) => {
                    tracing::warn!(
                        adapter_id = %candidate.adapter_id,
                        error = %e,
                        "Failed to evict adapter during budget enforcement"
                    );
                }
            }
        }

        Ok(freed_bytes)
    }

    async fn evict_adapter_by_id(&self, adapter_id: &str, reason: &str) -> Result<u64> {
        if self.table.active.read().contains_key(adapter_id) {
            return Ok(0);
        }

        let refcount = {
            let refcounts = self.table.refcounts.lock().await;
            refcounts
                .get(adapter_id)
                .map(|rc| rc.load(Ordering::Relaxed))
                .unwrap_or(0)
        };
        if refcount > 0 {
            return Ok(0);
        }

        let mut vram_bytes = 0u64;
        {
            let staged = self.table.staged.read();
            if let Some(state) = staged.get(adapter_id) {
                vram_bytes = vram_bytes.max(state.vram_mb.saturating_mul(BYTES_PER_MB));
            }
        }
        {
            let retired_guard = self.table.retired_stacks.lock().await;
            for stack in retired_guard.iter() {
                if let Some(state) = stack.active.get(adapter_id) {
                    vram_bytes = vram_bytes.max(state.vram_mb.saturating_mul(BYTES_PER_MB));
                }
            }
        }

        if vram_bytes == 0 {
            return Ok(0);
        }

        if let Some(kernels) = &self.kernels {
            let mut k_lock = kernels.lock().await;
            k_lock.detach_adapter(adapter_id_to_u16(adapter_id))?;
        }

        {
            let mut staged = self.table.staged.write();
            staged.remove(adapter_id);
        }

        let removed_generations = self.remove_adapter_from_retired(adapter_id).await;
        if !removed_generations.is_empty() {
            let mut retry_guard = self.table.retry_counts.lock().await;
            for gen in removed_generations {
                retry_guard.remove(&gen);
            }
        }

        let still_present = {
            if self.table.active.read().contains_key(adapter_id)
                || self.table.staged.read().contains_key(adapter_id)
            {
                true
            } else {
                let retired_guard = self.table.retired_stacks.lock().await;
                retired_guard
                    .iter()
                    .any(|stack| stack.active.contains_key(adapter_id))
            }
        };

        if !still_present {
            let mut refcounts = self.table.refcounts.lock().await;
            refcounts.remove(adapter_id);
            drop(refcounts);
        }

        if let Some(metrics) = self.table.metrics() {
            metrics.record_adapter_eviction(adapter_id, reason);
        }

        self.table.record_adapter_cache_bytes().await;
        Ok(vram_bytes)
    }

    async fn remove_adapter_from_retired(&self, adapter_id: &str) -> Vec<u64> {
        let mut removed_generations = Vec::new();
        let mut retired_guard = self.table.retired_stacks.lock().await;
        let mut updated: Vec<Arc<Stack>> = Vec::with_capacity(retired_guard.len());

        for stack in retired_guard.iter() {
            if stack.active.contains_key(adapter_id) {
                let mut active = stack.active.clone();
                active.remove(adapter_id);
                if active.is_empty() {
                    removed_generations.push(stack.generation);
                } else {
                    updated.push(Arc::new(Stack {
                        generation: stack.generation,
                        active,
                    }));
                }
            } else {
                updated.push(Arc::clone(stack));
            }
        }

        *retired_guard = updated;
        removed_generations
    }

    fn should_reject_integrity(mode: AdapterIntegrityMode, err: &AdapterIntegrityError) -> bool {
        matches!(
            err.reason,
            AdapterIntegrityReason::MissingAdapter | AdapterIntegrityReason::ManifestParseFailed
        ) || mode.is_enforce()
    }

    fn rejection_result(
        &self,
        start: Instant,
        err: &AdapterIntegrityError,
        stack_hash: Option<B3Hash>,
    ) -> AdapterCommandResult {
        AdapterCommandResult {
            success: false,
            message: err.message.clone(),
            vram_delta_mb: None,
            duration_ms: start.elapsed().as_millis() as u64,
            stack_hash,
            memory_state: Some(self.memory_state()),
            reject_reason: Some(err.reason.as_str().to_string()),
        }
    }

    fn emit_adapter_verify(
        &self,
        adapter_id: &str,
        stack_hash: Option<B3Hash>,
        result: &str,
        reason: Option<AdapterIntegrityReason>,
        duration: Duration,
    ) {
        let Some(tel) = &self.table.telemetry else {
            return;
        };

        let payload = json!({
            "adapter_id": adapter_id,
            "stack_hash": stack_hash.map(|hash| hash.to_hex()),
            "result": result,
            "reason": reason.map(|value| value.as_str()),
            "duration_ms": duration.as_millis() as u64,
        });

        let _ = tel.log("adapter.verify", payload);
    }

    fn emit_adapter_reject(&self, reason: AdapterIntegrityReason, adapter_id: Option<&str>) {
        let Some(tel) = &self.table.telemetry else {
            return;
        };

        let mut payload = json!({
            "tenant_id": self.tenant_id.clone(),
            "reason": reason.as_str(),
        });
        if let Some(id) = adapter_id {
            payload["adapter_id"] = json!(id);
        }

        let _ = tel.log("adapter.reject", payload);
    }

    fn missing_adapter_error(&self, adapter_id: &str, message: String) -> AdapterIntegrityError {
        AdapterIntegrityError {
            adapter_id: adapter_id.to_string(),
            reason: AdapterIntegrityReason::MissingAdapter,
            message,
            expected: None,
            actual: None,
        }
    }

    async fn verify_add_ids_for_swap(
        &self,
        add_ids: &[String],
        stack_hash_hint: Option<B3Hash>,
    ) -> AdapterIntegrityResult<()> {
        let mode = self.integrity.mode();
        if add_ids.is_empty() {
            return Ok(());
        }

        for adapter_id in add_ids {
            let expected_hash = self.table.staged_expected_hash(adapter_id).ok_or_else(|| {
                self.missing_adapter_error(
                    adapter_id,
                    format!(
                        "Hot-swap aborted: adapter '{}' was not preloaded into staging",
                        adapter_id
                    ),
                )
            })?;

            let loader = self.galaxy_loader.clone();
            let adapter_for_loader = adapter_id.clone();
            let path_for_loader =
                resolve_adapter_file(&self.repo_root, &self.tenant_id, adapter_id);
            let path_for_loader_for_load = path_for_loader.clone();
            let load_result = tokio::task::spawn_blocking(move || {
                loader.load_adapter(&adapter_for_loader, &path_for_loader_for_load)
            })
            .await
            .map_err(|e| AdapterIntegrityError {
                adapter_id: adapter_id.clone(),
                reason: AdapterIntegrityReason::ManifestParseFailed,
                message: format!("Adapter verification task failed: {e}"),
                expected: None,
                actual: None,
            })?;

            let outcome = match load_result {
                Ok(outcome) => outcome,
                Err(err) => {
                    let integrity_err = if matches!(err, AosError::NotFound(_)) {
                        self.missing_adapter_error(
                            adapter_id,
                            format!(
                                "Adapter file not found for '{}' at '{}'",
                                adapter_id,
                                path_for_loader.display()
                            ),
                        )
                    } else {
                        AdapterIntegrityError {
                            adapter_id: adapter_id.clone(),
                            reason: AdapterIntegrityReason::ManifestParseFailed,
                            message: format!("Adapter verification load failed: {err}"),
                            expected: None,
                            actual: None,
                        }
                    };

                    self.emit_adapter_verify(
                        adapter_id,
                        stack_hash_hint,
                        "reject",
                        Some(integrity_err.reason),
                        Duration::from_millis(0),
                    );
                    return Err(integrity_err);
                }
            };

            let verify_start = Instant::now();
            match self
                .integrity
                .verify_outcome(adapter_id, expected_hash, &outcome)
                .await
            {
                Ok(verification) => {
                    self.table
                        .update_staged_hash(adapter_id, verification.weights_hash);
                    self.emit_adapter_verify(
                        adapter_id,
                        stack_hash_hint,
                        "ok",
                        None,
                        verify_start.elapsed(),
                    );
                }
                Err(err) => {
                    if let Some(actual) = err.actual {
                        self.table.update_staged_hash(adapter_id, actual);
                    }
                    let should_reject = Self::should_reject_integrity(mode, &err);
                    let result = if should_reject { "reject" } else { "warn" };
                    self.emit_adapter_verify(
                        adapter_id,
                        stack_hash_hint,
                        result,
                        Some(err.reason),
                        verify_start.elapsed(),
                    );
                    if should_reject {
                        return Err(err);
                    }
                }
            }
        }

        Ok(())
    }

    fn staged_adds_for_swap(&self, add_ids: &[String]) -> (Vec<String>, HashSet<String>) {
        if add_ids.is_empty() {
            return (Vec::new(), HashSet::new());
        }

        let active_ids: HashSet<String> = self.table.active.read().keys().cloned().collect();
        let staged_guard = self.table.staged.read();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut staged_adds = Vec::new();

        for id in add_ids {
            if !seen.insert(id.as_str()) {
                continue;
            }
            if staged_guard.contains_key(id) && !active_ids.contains(id) {
                staged_adds.push(id.clone());
            }
        }

        (staged_adds, active_ids)
    }

    async fn cleanup_staged_on_failure(&self, add_ids: &[String], reason: &str) {
        let (staged_adds, active_ids) = self.staged_adds_for_swap(add_ids);
        if staged_adds.is_empty() {
            return;
        }

        tracing::warn!(
            reason = reason,
            staged_adapter_ids = ?staged_adds,
            "Cleaning staged adapters after swap failure"
        );

        if let Some(kernels) = &self.kernels {
            let mut kernels_lock = kernels.lock().await;
            for adapter_id in &staged_adds {
                let adapter_id_u16 = adapter_id_to_u16(adapter_id);
                if let Err(e) = kernels_lock.detach_adapter(adapter_id_u16) {
                    tracing::warn!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "Failed to unload staged adapter after swap failure"
                    );
                }
            }
        }

        {
            let mut staged_guard = self.table.staged.write();
            for adapter_id in &staged_adds {
                if !active_ids.contains(adapter_id) {
                    staged_guard.remove(adapter_id);
                }
            }
        }

        let mut refcounts = self.table.refcounts.lock().await;
        for adapter_id in &staged_adds {
            if !active_ids.contains(adapter_id) {
                refcounts.remove(adapter_id);
            }
        }
        drop(refcounts);

        self.table.record_adapter_cache_bytes().await;
    }

    async fn attach_staged_for_swap(&self, add_ids: &[String]) -> Result<()> {
        let (staged_adds, _) = self.staged_adds_for_swap(add_ids);
        if staged_adds.is_empty() {
            return Ok(());
        }

        let Some(kernels) = &self.kernels else {
            return Ok(());
        };

        let mut kernels_lock = kernels.lock().await;
        for adapter_id in staged_adds {
            let adapter_id_u16 = adapter_id_to_u16(&adapter_id);
            kernels_lock.attach_adapter(adapter_id_u16)?;
        }

        Ok(())
    }

    async fn detach_removed_after_swap(&self, remove_ids: &[String]) {
        if remove_ids.is_empty() {
            return;
        }

        let active_ids: HashSet<String> = self.table.active.read().keys().cloned().collect();
        let mut seen = HashSet::new();
        let to_detach: Vec<String> = remove_ids
            .iter()
            .filter(|id| seen.insert((*id).clone()) && !active_ids.contains(id.as_str()))
            .cloned()
            .collect();

        if to_detach.is_empty() {
            return;
        }

        let Some(kernels) = &self.kernels else {
            return;
        };

        let mut kernels_lock = kernels.lock().await;
        for adapter_id in to_detach {
            let adapter_id_u16 = adapter_id_to_u16(&adapter_id);
            if let Err(e) = kernels_lock.detach_adapter(adapter_id_u16) {
                tracing::warn!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to unload adapter after swap"
                );
            }
        }
    }

    async fn hint_switch_adapter_after_swap(&self, add_ids: &[String]) {
        if add_ids.len() != 1 {
            return;
        }

        let adapter_id = &add_ids[0];
        let should_switch = {
            let active_guard = self.table.active.read();
            active_guard.len() == 1 && active_guard.contains_key(adapter_id)
        };
        if !should_switch {
            return;
        }

        let Some(kernels) = &self.kernels else {
            return;
        };

        let mut kernels_lock = kernels.lock().await;
        if let Err(e) = kernels_lock.switch_adapter(adapter_id_to_u16(adapter_id)) {
            tracing::warn!(
                adapter_id = %adapter_id,
                backend_reason = "switch_adapter_failed",
                error = %e,
                "Backend switch_adapter hint failed"
            );
        }
    }

    fn compute_stack_hash_with(
        &self,
        add_ids: &[String],
        remove_ids: &[String],
    ) -> AdapterIntegrityResult<B3Hash> {
        let active = self.table.active.read();
        let staged = self.table.staged.read();

        let mut entries: HashMap<String, B3Hash> = active
            .iter()
            .map(|(id, state)| (id.clone(), state.hash))
            .collect();

        for id in remove_ids {
            entries.remove(id);
        }

        for id in add_ids {
            let Some(state) = staged.get(id) else {
                return Err(self.missing_adapter_error(
                    id,
                    format!(
                        "Hot-swap aborted: adapter '{}' was not preloaded into staging",
                        id
                    ),
                ));
            };
            entries.insert(id.clone(), state.hash);
        }

        Ok(adapteros_core::compute_stack_hash(entries))
    }

    async fn check_swap_integrity(
        &self,
        add_ids: &[String],
        remove_ids: &[String],
        expected_stack_hash: Option<B3Hash>,
    ) -> AdapterIntegrityResult<()> {
        if add_ids
            .iter()
            .any(|id| self.table.staged_expected_hash(id).is_none())
        {
            let missing = add_ids
                .iter()
                .find(|id| self.table.staged_expected_hash(id).is_none())
                .map(|id| id.as_str())
                .unwrap_or("unknown");
            let err = self.missing_adapter_error(
                missing,
                format!(
                    "Hot-swap aborted: adapter '{}' was not preloaded into staging",
                    missing
                ),
            );
            self.emit_adapter_verify(
                missing,
                expected_stack_hash,
                "reject",
                Some(err.reason),
                Duration::from_millis(0),
            );
            return Err(err);
        }

        let mode = self.integrity.mode();
        if !mode.is_off() || expected_stack_hash.is_some() {
            self.verify_add_ids_for_swap(add_ids, expected_stack_hash)
                .await?;

            if let Some(expected_hash) = expected_stack_hash {
                let check_start = Instant::now();
                let predicted = self.compute_stack_hash_with(add_ids, remove_ids)?;
                if predicted != expected_hash {
                    let err = AdapterIntegrityError {
                        adapter_id: "stack".to_string(),
                        reason: AdapterIntegrityReason::StackHashMismatch,
                        message: format!(
                            "Adapter stack hash mismatch: expected {}, got {}",
                            expected_hash.to_hex(),
                            predicted.to_hex()
                        ),
                        expected: Some(expected_hash),
                        actual: Some(predicted),
                    };
                    self.emit_adapter_verify(
                        "stack",
                        Some(predicted),
                        "reject",
                        Some(err.reason),
                        check_start.elapsed(),
                    );
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    /// Create new hot-swap manager without kernel backend (metadata only)
    pub fn new_metadata_only(repo_root: std::path::PathBuf, tenant_id: String) -> Self {
        let integrity_tenant_id = tenant_id.clone();
        Self {
            table: Arc::new(AdapterTable::new()),
            kernels: None,
            repo_root,
            tenant_id,
            integrity: Arc::new(AdapterIntegrityVerifier::disabled(integrity_tenant_id)),
            memory_monitor: None,
            galaxy_loader: GalaxyLoader::new(),
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
                        "Tenant isolation violation: adapter path '{}' is outside tenant root '{}'. This is a security violation - adapters must reside within their tenant's directory.",
                        adapter_path.display(),
                        tenant_root.display()
                    )));
                }

                let integrity_mode = self.integrity.mode();
                let mut loaded_adapter_u16: Option<u16> = None;
                let requires_load = self.kernels.is_some() || !integrity_mode.is_off();
                let mut load_outcome = None;

                if requires_load {
                    let loader = self.galaxy_loader.clone();
                    let adapter_for_loader = adapter_id.clone();
                    let path_for_loader = adapter_path.clone();
                    let load_result = tokio::task::spawn_blocking(move || {
                        loader.load_adapter(&adapter_for_loader, &path_for_loader)
                    })
                    .await
                    .map_err(|e| {
                        AosError::Worker(format!(
                            "Adapter preload failed: galaxy loader join error: {e}"
                        ))
                    })?;

                    let outcome = match load_result {
                        Ok(outcome) => outcome,
                        Err(err) => {
                            if matches!(err, AosError::NotFound(_)) {
                                let integrity_err = AdapterIntegrityError {
                                    adapter_id: adapter_id.clone(),
                                    reason: AdapterIntegrityReason::MissingAdapter,
                                    message: format!(
                                        "Adapter file not found for '{}' at '{}'",
                                        adapter_id,
                                        adapter_path.display()
                                    ),
                                    expected: Some(hash),
                                    actual: None,
                                };
                                self.emit_adapter_verify(
                                    &adapter_id,
                                    None,
                                    "reject",
                                    Some(integrity_err.reason),
                                    Duration::from_millis(0),
                                );
                                self.emit_adapter_reject(integrity_err.reason, Some(&adapter_id));
                                return Ok(self.rejection_result(start, &integrity_err, None));
                            }
                            return Err(err);
                        }
                    };
                    load_outcome = Some(outcome);
                }

                let mut observed_hash = hash;

                if !integrity_mode.is_off() {
                    let Some(outcome) = load_outcome.as_ref() else {
                        return Err(AosError::Worker(
                            "Adapter preload failed: integrity verification requires adapter load outcome"
                                .to_string(),
                        ));
                    };
                    let verify_start = Instant::now();
                    match self
                        .integrity
                        .verify_outcome(&adapter_id, hash, outcome)
                        .await
                    {
                        Ok(verification) => {
                            observed_hash = verification.weights_hash;
                            self.emit_adapter_verify(
                                &adapter_id,
                                None,
                                "ok",
                                None,
                                verify_start.elapsed(),
                            );
                        }
                        Err(err) => {
                            observed_hash = err.actual.unwrap_or(hash);
                            let should_reject = Self::should_reject_integrity(integrity_mode, &err);
                            let result = if should_reject { "reject" } else { "warn" };
                            self.emit_adapter_verify(
                                &adapter_id,
                                None,
                                result,
                                Some(err.reason),
                                verify_start.elapsed(),
                            );
                            if should_reject {
                                self.emit_adapter_reject(err.reason, Some(&adapter_id));
                                return Ok(self.rejection_result(start, &err, None));
                            }
                        }
                    }
                } else if let Some(outcome) = load_outcome.as_ref() {
                    let manifest_bytes = outcome.backing.slice(&outcome.view.manifest_range);
                    if let Err(e) = serde_json::from_slice::<serde_json::Value>(manifest_bytes) {
                        let integrity_err = AdapterIntegrityError {
                            adapter_id: adapter_id.clone(),
                            reason: AdapterIntegrityReason::ManifestParseFailed,
                            message: format!(
                                "Adapter manifest parse failed for '{}': {e}",
                                adapter_path.display()
                            ),
                            expected: None,
                            actual: None,
                        };
                        self.emit_adapter_verify(
                            &adapter_id,
                            None,
                            "reject",
                            Some(integrity_err.reason),
                            Duration::from_millis(0),
                        );
                        self.emit_adapter_reject(integrity_err.reason, Some(&adapter_id));
                        return Ok(self.rejection_result(start, &integrity_err, None));
                    }
                }

                // Load actual adapter weights if kernel backend is available
                let (vram_mb, backing) = if let Some(ref kernels) = self.kernels {
                    let load_outcome = load_outcome.ok_or_else(|| {
                        AosError::Worker(
                            "Adapter preload failed: missing adapter load outcome".to_string(),
                        )
                    })?;

                    let weights = load_outcome.payload();

                    // Workstream 6: VRAM validation before preload
                    // Estimate VRAM requirement from payload size
                    let estimated_vram_mb = (weights.len() as u64 / BYTES_PER_MB).max(1);
                    let already_loaded = if self.table.active.read().contains_key(&adapter_id)
                        || self.table.staged.read().contains_key(&adapter_id)
                    {
                        true
                    } else {
                        let retired_guard = self.table.retired_stacks.lock().await;
                        retired_guard
                            .iter()
                            .any(|stack| stack.active.contains_key(&adapter_id))
                    };
                    let budget_needed = if already_loaded {
                        0
                    } else {
                        estimated_vram_mb.saturating_mul(BYTES_PER_MB)
                    };
                    self.enforce_adapter_cache_budget(budget_needed, Some(&adapter_id))
                        .await?;

                    // Check available VRAM and memory pressure
                    if let Some(ref monitor) = self.memory_monitor {
                        let available_mb = monitor.get_available_mb();
                        let pressure = monitor.get_current_pressure();

                        // Check if we have enough available memory
                        if estimated_vram_mb > available_mb {
                            return Err(AosError::MemoryPressure(format!(
                                "Adapter preload blocked: insufficient VRAM for adapter '{}'. Requires ~{}MB but only {}MB available. Unload other adapters or reduce active adapter count.",
                                adapter_id, estimated_vram_mb, available_mb
                            )));
                        }

                        // Check memory pressure level - reject if critical
                        if pressure == crate::memory::MemoryPressureLevel::Critical {
                            return Err(AosError::MemoryPressure(format!(
                                "Adapter preload blocked: critical memory pressure detected. Cannot load adapter '{}' (requires ~{}MB). System is under severe memory stress. Wait for memory to free up or terminate other processes.",
                                adapter_id, estimated_vram_mb
                            )));
                        }
                    }

                    // Get adapter ID as u16 (deterministic BLAKE3 hash)
                    let adapter_id_u16 = adapter_id_to_u16(&adapter_id);

                    // Load weights into GPU
                    let mut kernels_lock = kernels.lock().await;
                    if let Err(e) = kernels_lock.load_adapter(adapter_id_u16, weights) {
                        if let Err(detach_err) = kernels_lock.detach_adapter(adapter_id_u16) {
                            tracing::warn!(
                                adapter_id = %adapter_id,
                                error = %detach_err,
                                "Failed to unload adapter after load error"
                            );
                        }
                        return Err(e);
                    }
                    loaded_adapter_u16 = Some(adapter_id_u16);
                    // CoreML hot-swap path uses explicit attach; other backends no-op.
                    if let Err(e) = kernels_lock.attach_adapter(adapter_id_u16) {
                        if let Err(detach_err) = kernels_lock.detach_adapter(adapter_id_u16) {
                            tracing::warn!(
                                adapter_id = %adapter_id,
                                error = %detach_err,
                                "Failed to unload adapter after attach error"
                            );
                        }
                        return Err(e);
                    }

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
                    (vram_mb, Some(load_outcome.backing))
                } else {
                    // No kernel backend - use mock value for metadata-only mode
                    tracing::warn!(adapter_id = %adapter_id, "No kernel backend available, using mock VRAM value");
                    (24, None) // Mock value
                };

                if let Err(e) = self
                    .table
                    .preload_with_backing(adapter_id.clone(), hash, observed_hash, vram_mb, backing)
                    .await
                {
                    if let Some(adapter_id_u16) = loaded_adapter_u16 {
                        if let Some(ref kernels) = self.kernels {
                            let mut kernels_lock = kernels.lock().await;
                            if let Err(detach_err) = kernels_lock.detach_adapter(adapter_id_u16) {
                                tracing::warn!(
                                    adapter_id = %adapter_id,
                                    error = %detach_err,
                                    "Failed to unload adapter after preload failure"
                                );
                            }
                        }
                    }
                    self.table.staged.write().remove(&adapter_id);
                    let mut refcounts = self.table.refcounts.lock().await;
                    refcounts.remove(&adapter_id);
                    drop(refcounts);
                    self.table.record_adapter_cache_bytes().await;
                    return Err(e);
                }

                if let Err(e) = self
                    .enforce_adapter_cache_budget(0, Some(&adapter_id))
                    .await
                {
                    let _ = self.evict_adapter_by_id(&adapter_id, "budget").await;
                    return Err(e);
                }

                AdapterCommandResult {
                    success: true,
                    message: format!("Preloaded adapter: {}", adapter_id),
                    vram_delta_mb: Some(vram_mb as i64),
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: None,
                    memory_state: Some(self.memory_state()),
                    reject_reason: None,
                }
            }

            AdapterCommand::Swap {
                add_ids,
                remove_ids,
                expected_stack_hash,
            } => {
                if let Err(err) = self
                    .check_swap_integrity(&add_ids, &remove_ids, expected_stack_hash)
                    .await
                {
                    self.cleanup_staged_on_failure(&add_ids, "swap_integrity")
                        .await;
                    self.emit_adapter_reject(err.reason, Some(err.adapter_id.as_str()));
                    return Ok(self.rejection_result(start, &err, err.actual));
                }

                // Ensure no in-flight references to adapters being removed
                if !remove_ids.is_empty() {
                    if let Err(e) = self
                        .table
                        .wait_for_zero_refs(&remove_ids, Duration::from_secs(2))
                        .await
                    {
                        self.cleanup_staged_on_failure(&add_ids, "swap_drain_timeout")
                            .await;
                        return Err(e);
                    }
                }

                if let Err(e) = self.attach_staged_for_swap(&add_ids).await {
                    self.cleanup_staged_on_failure(&add_ids, "swap_attach_failed")
                        .await;
                    return Err(e);
                }

                let (vram_delta, _added_count) = match self.table.swap(&add_ids, &remove_ids).await
                {
                    Ok(result) => result,
                    Err(e) => {
                        self.cleanup_staged_on_failure(&add_ids, "swap_table_failed")
                            .await;
                        return Err(e);
                    }
                };

                self.detach_removed_after_swap(&remove_ids).await;
                self.hint_switch_adapter_after_swap(&add_ids).await;
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
                    memory_state: Some(self.memory_state()),
                    reject_reason: None,
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
                    memory_state: Some(self.memory_state()),
                    reject_reason: None,
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
                    memory_state: Some(self.memory_state()),
                    reject_reason: None,
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
        self.swap_with_expected_hash(add_ids, remove_ids, None)
            .await
    }

    pub async fn swap_with_expected_hash(
        &self,
        add_ids: &[String],
        remove_ids: &[String],
        expected_stack_hash: Option<B3Hash>,
    ) -> Result<(i64, usize)> {
        if let Err(err) = self
            .check_swap_integrity(add_ids, remove_ids, expected_stack_hash)
            .await
        {
            self.cleanup_staged_on_failure(add_ids, "swap_integrity")
                .await;
            self.emit_adapter_reject(err.reason, Some(err.adapter_id.as_str()));
            return Err(AosError::Worker(format!(
                "Adapter integrity reject ({}): {}",
                err.reason.as_str(),
                err.message
            )));
        }

        if !remove_ids.is_empty() {
            if let Err(e) = self
                .table
                .wait_for_zero_refs(remove_ids, Duration::from_secs(2))
                .await
            {
                self.cleanup_staged_on_failure(add_ids, "swap_drain_timeout")
                    .await;
                return Err(e);
            }
        }

        if let Err(e) = self.attach_staged_for_swap(add_ids).await {
            self.cleanup_staged_on_failure(add_ids, "swap_attach_failed")
                .await;
            return Err(e);
        }

        let (vram_delta, added_count) = match self.table.swap(add_ids, remove_ids).await {
            Ok(result) => result,
            Err(e) => {
                self.cleanup_staged_on_failure(add_ids, "swap_table_failed")
                    .await;
                return Err(e);
            }
        };

        self.detach_removed_after_swap(remove_ids).await;
        self.hint_switch_adapter_after_swap(add_ids).await;

        Ok((vram_delta, added_count))
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
    pub fn start_retirement_task(
        self: Arc<Self>,
        mut shutdown_rx: watch::Receiver<()>,
    ) -> tokio::task::JoinHandle<()>
    where
        K: adapteros_lora_kernel_api::FusedKernels + Send + Sync + 'static,
    {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Retirement task received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}
                }

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
                                drop(retired_guard);
                                manager.table.record_adapter_cache_bytes().await;
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
                            drop(retired_guard);
                            manager.table.record_adapter_cache_bytes().await;
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
    use adapteros_aos::writer::{parse_segments, INDEX_ENTRY_SIZE};
    use adapteros_aos::{AosWriter, BackendTag};
    use adapteros_core::constants::BYTES_PER_MB;
    use adapteros_lora_kernel_api::{attestation, IoBuffers, RouterRing};
    use adapteros_manifest::{AdapterScope, AdapterTier};
    use serde::Serialize;
    use std::collections::HashMap;
    use std::path::Path;
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
                determinism_level: attestation::DeterminismLevel::BitExact,
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

    #[derive(Debug)]
    struct FailingAttachKernels {
        fail_id: u16,
        attach_calls: usize,
        fail_on_call: usize,
        detach_calls: usize,
    }

    impl FailingAttachKernels {
        fn new(fail_id: u16) -> Self {
            Self {
                fail_id,
                attach_calls: 0,
                fail_on_call: 2,
                detach_calls: 0,
            }
        }
    }

    impl adapteros_lora_kernel_api::FusedKernels for FailingAttachKernels {
        fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
            Ok(())
        }

        fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
            Ok(())
        }

        fn device_name(&self) -> &str {
            "failing-attach"
        }

        fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
            Ok(attestation::DeterminismReport {
                backend_type: attestation::BackendType::Mock,
                metallib_hash: None,
                manifest: None,
                rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
                floating_point_mode: attestation::FloatingPointMode::Deterministic,
                determinism_level: attestation::DeterminismLevel::BitExact,
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

        fn attach_adapter(&mut self, id: u16) -> Result<()> {
            if id == self.fail_id {
                self.attach_calls += 1;
                if self.attach_calls >= self.fail_on_call {
                    return Err(AosError::Kernel("attach failed".to_string()));
                }
            }
            Ok(())
        }

        fn detach_adapter(&mut self, _id: u16) -> Result<()> {
            self.detach_calls += 1;
            Ok(())
        }
    }

    #[derive(Serialize)]
    struct TestManifest<'a> {
        adapter_id: &'a str,
        base_model: &'a str,
        tier: &'a str,
        scope: &'a str,
        metadata: TestManifestMetadata<'a>,
    }

    #[derive(Serialize)]
    struct TestManifestMetadata<'a> {
        scope_path: &'a str,
    }

    fn make_payload(seed: &str, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut block = B3Hash::hash(seed.as_bytes()).to_bytes().to_vec();
        while out.len() < len {
            out.extend_from_slice(&block);
            block = B3Hash::hash(&block).to_bytes().to_vec();
        }
        out.truncate(len);
        out
    }

    fn write_test_adapter(
        path: &Path,
        adapter_id: &str,
        base_model: &str,
        tier: &str,
        scope: &str,
        scope_path: &str,
        weights: &[u8],
    ) -> B3Hash {
        let manifest = TestManifest {
            adapter_id,
            base_model,
            tier,
            scope,
            metadata: TestManifestMetadata { scope_path },
        };

        let mut writer = AosWriter::new();
        writer
            .add_segment(BackendTag::Canonical, Some(scope_path.to_string()), weights)
            .expect("add canonical segment");
        writer
            .write_archive(path, &manifest)
            .expect("write test adapter");

        B3Hash::hash(weights)
    }

    fn tamper_adapter_payload(path: &Path) -> B3Hash {
        let mut bytes = fs::read(path).expect("read adapter");
        let header = AosWriter::parse_header_bytes(&bytes).expect("parse header");
        let segments = parse_segments(&bytes, &header).expect("parse segments");
        let segment = segments.first().expect("segment");
        let offset = segment.offset;
        let len = segment.len;

        let payload = &mut bytes[offset..offset + len];
        payload[0] ^= 0xFF;
        let new_hash = B3Hash::hash(payload);

        let entry_offset =
            header.index_offset as usize + (segment.segment_id as usize * INDEX_ENTRY_SIZE);
        bytes[entry_offset + 40..entry_offset + 72].copy_from_slice(new_hash.as_bytes());
        fs::write(path, &bytes).expect("write tampered adapter");
        new_hash
    }

    #[tokio::test]
    async fn swap_with_single_add_hints_switch_adapter() {
        let kernels = Arc::new(tokio::sync::Mutex::new(MockKernels::default()));
        let repo = tempdir().expect("tempdir");
        let integrity = Arc::new(AdapterIntegrityVerifier::disabled(
            SYSTEM_TENANT.to_string(),
        ));
        let mut manager = HotSwapManager::new_with_kernels(
            kernels.clone(),
            repo.path().to_path_buf(),
            SYSTEM_TENANT.to_string(),
            integrity,
            None,
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
                expected_stack_hash: None,
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
    async fn swap_rejects_tampered_adapter_and_keeps_active() {
        let repo = tempdir().expect("tempdir");
        let tenant = "tenant_integrity";
        let base_model = "base-model";
        let scope_path = "tenant/scope";
        let tenant_dir = repo.path().join(tenant);
        fs::create_dir_all(&tenant_dir).expect("tenant dir");

        let adapter_a = "adapter_a";
        let adapter_b = "adapter_b";

        let weights_a = make_payload(adapter_a, 256);
        let hash_a = write_test_adapter(
            &tenant_dir.join(format!("{adapter_a}.aos")),
            adapter_a,
            base_model,
            "persistent",
            "tenant",
            scope_path,
            &weights_a,
        );

        let weights_b = make_payload(adapter_b, 256);
        let hash_b = write_test_adapter(
            &tenant_dir.join(format!("{adapter_b}.aos")),
            adapter_b,
            base_model,
            "persistent",
            "tenant",
            scope_path,
            &weights_b,
        );

        let mut expected_metadata = HashMap::new();
        expected_metadata.insert(
            adapter_a.to_string(),
            crate::adapter_integrity::ExpectedAdapterMetadata {
                tier: Some(AdapterTier::Persistent),
                scope: Some(AdapterScope::Tenant),
            },
        );
        expected_metadata.insert(
            adapter_b.to_string(),
            crate::adapter_integrity::ExpectedAdapterMetadata {
                tier: Some(AdapterTier::Persistent),
                scope: Some(AdapterScope::Tenant),
            },
        );

        let integrity = Arc::new(AdapterIntegrityVerifier::new(
            tenant.to_string(),
            base_model.to_string(),
            expected_metadata,
        ));
        let kernels = Arc::new(tokio::sync::Mutex::new(MockKernels::default()));
        let manager = HotSwapManager::new_with_kernels(
            kernels,
            repo.path().to_path_buf(),
            tenant.to_string(),
            integrity,
            None,
            None,
        );

        manager
            .execute(AdapterCommand::Preload {
                adapter_id: adapter_a.to_string(),
                hash: hash_a,
            })
            .await
            .expect("preload adapter_a");
        manager
            .execute(AdapterCommand::Swap {
                add_ids: vec![adapter_a.to_string()],
                remove_ids: vec![],
                expected_stack_hash: None,
            })
            .await
            .expect("swap adapter_a");

        manager
            .execute(AdapterCommand::Preload {
                adapter_id: adapter_b.to_string(),
                hash: hash_b,
            })
            .await
            .expect("preload adapter_b");

        sleep(Duration::from_millis(1100)).await;
        tamper_adapter_payload(&tenant_dir.join(format!("{adapter_b}.aos")));

        let expected_stack_hash =
            adapteros_core::compute_stack_hash(vec![(adapter_b.to_string(), hash_b)]);
        let result = manager
            .execute(AdapterCommand::Swap {
                add_ids: vec![adapter_b.to_string()],
                remove_ids: vec![adapter_a.to_string()],
                expected_stack_hash: Some(expected_stack_hash),
            })
            .await
            .expect("swap attempt returns result");

        assert!(!result.success, "tampered swap should be rejected");
        let reason = result.reject_reason.as_deref().unwrap_or("missing");
        assert!(
            reason == "hash_mismatch" || reason == "stack_hash_mismatch",
            "unexpected rejection reason: {reason}"
        );

        let active = manager.table.active.read();
        assert!(active.contains_key(adapter_a));
        assert!(!active.contains_key(adapter_b));
        drop(active);

        let staged = manager.table.staged.read();
        assert!(!staged.contains_key(adapter_b));
        drop(staged);

        let refcounts = manager.table.refcounts.lock().await;
        assert!(!refcounts.contains_key(adapter_b));
    }

    #[tokio::test]
    async fn cleanup_runs_when_attach_fails() {
        let repo = tempdir().expect("tempdir");
        let tenant = "tenant_cleanup";
        let base_model = "base-model";
        let scope_path = "tenant/scope";
        let tenant_dir = repo.path().join(tenant);
        fs::create_dir_all(&tenant_dir).expect("tenant dir");

        let adapter_a = "adapter_a";
        let adapter_b = "adapter_b";

        let weights_a = make_payload(adapter_a, 128);
        let hash_a = write_test_adapter(
            &tenant_dir.join(format!("{adapter_a}.aos")),
            adapter_a,
            base_model,
            "persistent",
            "tenant",
            scope_path,
            &weights_a,
        );

        let weights_b = make_payload(adapter_b, 128);
        let hash_b = write_test_adapter(
            &tenant_dir.join(format!("{adapter_b}.aos")),
            adapter_b,
            base_model,
            "persistent",
            "tenant",
            scope_path,
            &weights_b,
        );

        let fail_id = adapter_id_to_u16(adapter_b);
        let kernels = Arc::new(tokio::sync::Mutex::new(FailingAttachKernels::new(fail_id)));
        let integrity = Arc::new(AdapterIntegrityVerifier::disabled(tenant.to_string()));
        let manager = HotSwapManager::new_with_kernels(
            kernels.clone(),
            repo.path().to_path_buf(),
            tenant.to_string(),
            integrity,
            None,
            None,
        );

        manager
            .execute(AdapterCommand::Preload {
                adapter_id: adapter_a.to_string(),
                hash: hash_a,
            })
            .await
            .expect("preload adapter_a");
        manager
            .execute(AdapterCommand::Swap {
                add_ids: vec![adapter_a.to_string()],
                remove_ids: vec![],
                expected_stack_hash: None,
            })
            .await
            .expect("swap adapter_a");

        manager
            .execute(AdapterCommand::Preload {
                adapter_id: adapter_b.to_string(),
                hash: hash_b,
            })
            .await
            .expect("preload adapter_b");

        manager
            .execute(AdapterCommand::Swap {
                add_ids: vec![adapter_b.to_string()],
                remove_ids: vec![adapter_a.to_string()],
                expected_stack_hash: None,
            })
            .await
            .expect_err("attach failure should abort swap");

        let active = manager.table.active.read();
        assert!(active.contains_key(adapter_a));
        assert!(!active.contains_key(adapter_b));
        drop(active);

        let staged = manager.table.staged.read();
        assert!(!staged.contains_key(adapter_b));
        drop(staged);

        let refcounts = manager.table.refcounts.lock().await;
        assert!(!refcounts.contains_key(adapter_b));
        drop(refcounts);

        let kernels_guard = kernels.lock().await;
        assert!(
            kernels_guard.detach_calls > 0,
            "cleanup should detach staged"
        );
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
            loom::future::block_on(async {
                table.preload("test".to_string(), h, 10).await.unwrap();
                table
                    .swap(&["test".to_string()], &[] as &[String])
                    .await
                    .unwrap();
            });

            let initial_gen = table.current_stack();

            // 50 readers: snapshot, inc, hold, dec
            for _ in 0..50 {
                let table_clone = table.clone();
                loom::thread::spawn(move || {
                    loom::future::block_on(async move {
                        let _stack_gen = table_clone.current_stack();
                        table_clone.inc_ref("test").await;
                        std::thread::sleep(std::time::Duration::from_secs(1)); // Simulate long inference
                        table_clone.dec_ref("test").await;
                    });
                });
            }

            // 10 writers: preload new, swap every 100ms
            for i in 0..10 {
                let table_clone = table.clone();
                let new_id = format!("new{}", i);
                loom::thread::spawn(move || {
                    loom::future::block_on(async move {
                        let h_new = B3Hash::hash(format!("new{}", i).as_bytes());
                        table_clone
                            .preload(new_id.clone(), h_new, 10)
                            .await
                            .unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        table_clone
                            .swap(&[new_id], &["test".to_string()])
                            .await
                            .unwrap();
                    });
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
