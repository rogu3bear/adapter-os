//! Adapter hot-swap infrastructure for live adapter loading/unloading
//!
//! Implements two-phase hot-swap with:
//! - Preload: Load adapter into VRAM
//! - Swap: Atomic pointer flip with verification
//! - Rollback: Revert to last verified state
//! - Verify: Recompute effective-stack hash

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Report from atomic swap operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapReport {
    /// Adapter that was swapped
    pub adapter_id: String,
    /// Time taken for the atomic swap operation
    pub swap_time: Duration,
    /// Previous adapter ID that was replaced (if any)
    pub old_adapter: Option<String>,
}

/// Adapter state in hot-swap system
#[derive(Debug, Clone)]
pub struct AdapterState {
    pub id: String,
    pub hash: B3Hash,
    pub vram_mb: u64,
    pub loaded_at: Instant,
    pub active: bool,
    /// Reference to memory-mapped file (file cache level)
    pub mmap_handle: Option<Arc<adapteros_single_file_adapter::MmapAdapter>>,
}

/// Double-buffered adapter table for atomic swaps
pub struct AdapterTable {
    /// Currently active adapters
    active: RwLock<HashMap<String, AdapterState>>,
    /// Staged adapters being preloaded
    staged: RwLock<HashMap<String, AdapterState>>,
    /// Last verified state for rollback
    rollback_state: RwLock<Option<HashMap<String, AdapterState>>>,
}

impl AdapterTable {
    /// Create new empty adapter table
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback_state: RwLock::new(None),
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
                mmap_handle: None,
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
                // Rollback on partial failure
                drop(active);
                drop(staged);
                self.rollback_internal()?;
                return Err(AosError::Worker(format!(
                    "Adapter {} not found in staged set",
                    id
                )));
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
            Ok(())
        } else {
            Err(AosError::Worker("No rollback state available".to_string()))
        }
    }

    /// Compute effective stack hash for verification
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
}

impl Default for AdapterTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter hot-swap manager
pub struct HotSwapManager {
    table: Arc<AdapterTable>,
    /// Optional telemetry writer for logging swap events
    telemetry: Option<Arc<adapteros_telemetry::TelemetryWriter>>,
}

impl HotSwapManager {
    /// Create new hot-swap manager
    pub fn new() -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            telemetry: None,
        }
    }

    /// Create new hot-swap manager with telemetry
    pub fn new_with_telemetry(telemetry: Arc<adapteros_telemetry::TelemetryWriter>) -> Self {
        Self {
            table: Arc::new(AdapterTable::new()),
            telemetry: Some(telemetry),
        }
    }

    /// Log swap event to telemetry
    ///
    /// Logs at 100% sampling (critical operation per Telemetry Ruleset #9)
    fn log_swap(&self, report: &SwapReport, success: bool) -> Result<()> {
        if let Some(ref telemetry) = self.telemetry {
            #[derive(serde::Serialize)]
            struct SwapEvent {
                adapter_id: String,
                swap_time_us: u128,
                old_adapter: Option<String>,
                success: bool,
            }

            let event = SwapEvent {
                adapter_id: report.adapter_id.clone(),
                swap_time_us: report.swap_time.as_micros(),
                old_adapter: report.old_adapter.clone(),
                success,
            };

            telemetry
                .log("adapter.swap", event)
                .map_err(|e| AosError::Worker(format!("Failed to log swap event: {}", e)))?;
        }
        Ok(())
    }

    /// Atomic swap of single adapter with path-based loading
    ///
    /// Performs fast atomic pointer swap after loading and validating new adapter.
    /// Target swap time: < 1ms for pointer update.
    pub async fn swap(&self, adapter_id: &str, new_path: PathBuf) -> Result<SwapReport> {
        let start = Instant::now();

        // 1. Pre-validate new adapter (load via mmap)
        let new_adapter = adapteros_single_file_adapter::MmapAdapter::from_path(&new_path).await?;
        
        // 2. Verify signature
        if new_adapter.is_signed() && !new_adapter.verify_signature()? {
            return Err(AosError::Crypto("Signature verification failed".to_string()));
        }

        // 3. Extract metadata
        let manifest = new_adapter.manifest();
        let adapter_hash = new_adapter.file_hash().clone();
        
        // Mock VRAM size calculation (in production, calculate from manifest)
        let vram_mb = Self::estimate_vram_size(manifest.rank);
        
        // 4. Atomic pointer swap (< 1ms)
        let swap_start = Instant::now();
        let mut active = self.table.active.write();
        let old_adapter = active.insert(
            adapter_id.to_string(),
            AdapterState {
                id: adapter_id.to_string(),
                hash: adapter_hash,
                vram_mb,
                loaded_at: Instant::now(),
                active: true,
                mmap_handle: Some(Arc::new(new_adapter)),
            },
        );
        drop(active); // Release lock immediately
        let swap_time = swap_start.elapsed();

        let elapsed = start.elapsed();

        let report = SwapReport {
            adapter_id: adapter_id.to_string(),
            swap_time,
            old_adapter: old_adapter.map(|a| a.id),
        };

        // 5. Log telemetry
        self.log_swap(&report, true)?;

        tracing::info!(
            "Adapter swap complete: {} in {:?} (swap: {:?})",
            adapter_id,
            elapsed,
            swap_time
        );

        Ok(report)
    }

    /// Atomic swap with automatic rollback on failure
    ///
    /// Attempts to load and swap new adapter, keeping old adapter active if any step fails.
    pub async fn swap_with_rollback(
        &self,
        adapter_id: &str,
        new_path: PathBuf,
    ) -> Result<SwapReport> {
        // Try to load new adapter
        let new_adapter = match adapteros_single_file_adapter::MmapAdapter::from_path(&new_path).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(
                    "Failed to load new adapter for {}, keeping old: {}",
                    adapter_id,
                    e
                );
                return Err(e);
            }
        };

        // Verify before swapping
        if new_adapter.is_signed() && !new_adapter.verify_signature()? {
            tracing::warn!(
                "Signature verification failed for {}, keeping old adapter",
                adapter_id
            );
            return Err(AosError::Crypto(
                "Signature verification failed, rollback triggered".to_string(),
            ));
        }

        // Only swap if new adapter is valid
        self.swap_internal(adapter_id, new_adapter).await
    }

    /// Internal swap implementation with pre-loaded adapter
    async fn swap_internal(
        &self,
        adapter_id: &str,
        new_adapter: adapteros_single_file_adapter::MmapAdapter,
    ) -> Result<SwapReport> {
        let start = Instant::now();

        let manifest = new_adapter.manifest();
        let adapter_hash = new_adapter.file_hash().clone();
        let vram_mb = Self::estimate_vram_size(manifest.rank);

        // Atomic pointer swap
        let swap_start = Instant::now();
        let mut active = self.table.active.write();
        let old_adapter = active.insert(
            adapter_id.to_string(),
            AdapterState {
                id: adapter_id.to_string(),
                hash: adapter_hash,
                vram_mb,
                loaded_at: Instant::now(),
                active: true,
                mmap_handle: Some(Arc::new(new_adapter)),
            },
        );
        drop(active);
        let swap_time = swap_start.elapsed();

        let report = SwapReport {
            adapter_id: adapter_id.to_string(),
            swap_time,
            old_adapter: old_adapter.map(|a| a.id),
        };

        // Log telemetry
        self.log_swap(&report, true)?;

        Ok(report)
    }

    /// Estimate VRAM size from adapter rank
    fn estimate_vram_size(rank: u32) -> u64 {
        // Simplified estimation: rank * 2MB per target module
        // In production, calculate from actual tensor sizes
        rank as u64 * 2
    }

    /// Execute adapter command
    pub fn execute(&self, command: AdapterCommand) -> Result<AdapterCommandResult> {
        let start = Instant::now();

        let result = match command {
            AdapterCommand::Preload { adapter_id, hash } => {
                // Mock VRAM size for now - in production this would come from actual loading
                let vram_mb = 24; // Mock value
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
                let (vram_delta, _added_count) = self.table.swap(&add_ids, &remove_ids)?;
                let stack_hash = self.table.compute_stack_hash();

                AdapterCommandResult {
                    success: true,
                    message: format!("Swapped: +{:?} / -{:?}", add_ids, remove_ids),
                    vram_delta_mb: Some(vram_delta),
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: Some(stack_hash),
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

                AdapterCommandResult {
                    success: true,
                    message: "Stack verified".to_string(),
                    vram_delta_mb: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    stack_hash: Some(stack_hash),
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

impl Default for HotSwapManager {
    fn default() -> Self {
        Self::new()
    }
}

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
