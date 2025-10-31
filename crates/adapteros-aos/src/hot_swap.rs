//! Atomic hot-swap support for .aos files

use crate::metrics::SwapMetrics;
use crate::mmap_loader::{MmapAdapter, MmapAdapterLoader};
use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, instrument};

#[derive(Debug, Clone)]
pub struct SwapOperation {
    pub preload_path: PathBuf,
    pub slot: Option<String>,
}

impl SwapOperation {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            preload_path: path.as_ref().to_path_buf(),
            slot: None,
        }
    }
}

#[derive(Debug, Clone)]
struct AdapterSlot {
    adapter: Arc<MmapAdapter>,
    #[allow(dead_code)]
    loaded_at: Instant,
}

pub struct HotSwapManager {
    active: RwLock<HashMap<String, AdapterSlot>>,
    staged: RwLock<HashMap<String, AdapterSlot>>,
    rollback: RwLock<Option<HashMap<String, AdapterSlot>>>,
    loader: MmapAdapterLoader,
    metrics: Arc<SwapMetrics>,
}

impl HotSwapManager {
    pub fn new() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback: RwLock::new(None),
            loader: MmapAdapterLoader::new(),
            metrics: Arc::new(SwapMetrics::new()),
        }
    }

    pub fn without_verification() -> Self {
        Self {
            active: RwLock::new(HashMap::new()),
            staged: RwLock::new(HashMap::new()),
            rollback: RwLock::new(None),
            loader: MmapAdapterLoader::without_verification(),
            metrics: Arc::new(SwapMetrics::new()),
        }
    }

    #[instrument(skip(self), fields(slot = %slot, path = %path.as_ref().display()))]
    pub async fn preload<P: AsRef<Path>>(&self, slot: &str, path: P) -> Result<()> {
        debug!("Preloading adapter");

        let adapter = self.loader.load(path).await?;

        let slot_state = AdapterSlot {
            adapter: Arc::new(adapter),
            loaded_at: Instant::now(),
        };

        let mut staged = self.staged.write();
        staged.insert(slot.to_string(), slot_state);

        info!(slot, "Adapter preloaded");
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn swap(&self, slots: &[String]) -> Result<()> {
        let start = Instant::now();
        debug!("Beginning atomic swap");

        {
            let active = self.active.read();
            let mut rollback = self.rollback.write();
            *rollback = Some(active.clone());
        }

        let mut active = self.active.write();
        let mut staged = self.staged.write();

        for slot in slots {
            if let Some(slot_state) = staged.remove(slot) {
                active.insert(slot.clone(), slot_state);
                debug!(slot, "Swapped adapter");
            } else {
                drop(active);
                drop(staged);
                self.rollback_internal()?;
                return Err(AosError::Worker(format!(
                    "Staged adapter not found for slot: {}",
                    slot
                )));
            }
        }

        let duration = start.elapsed();
        self.metrics.record_swap(duration);
        info!(duration_ms = duration.as_millis(), "Swap completed");

        Ok(())
    }

    pub async fn swap_single<P: AsRef<Path>>(&self, slot: &str, path: P) -> Result<()> {
        self.preload(slot, path).await?;
        self.swap(&[slot.to_string()])
    }

    #[instrument(skip(self))]
    pub fn rollback(&self) -> Result<()> {
        debug!("Rolling back");
        self.rollback_internal()?;
        self.metrics.record_rollback();
        info!("Rollback completed");
        Ok(())
    }

    fn rollback_internal(&self) -> Result<()> {
        let mut active = self.active.write();
        let rollback = self.rollback.read();

        if let Some(ref saved_state) = *rollback {
            *active = saved_state.clone();
            Ok(())
        } else {
            Err(AosError::Worker("No rollback state available".to_string()))
        }
    }

    pub fn get_active(&self, slot: &str) -> Option<Arc<MmapAdapter>> {
        let active = self.active.read();
        active.get(slot).map(|s| Arc::clone(&s.adapter))
    }

    pub fn active_slots(&self) -> Vec<String> {
        let active = self.active.read();
        active.keys().cloned().collect()
    }

    pub fn staged_slots(&self) -> Vec<String> {
        let staged = self.staged.read();
        staged.keys().cloned().collect()
    }

    pub fn metrics(&self) -> Arc<SwapMetrics> {
        Arc::clone(&self.metrics)
    }
}

impl Default for HotSwapManager {
    fn default() -> Self {
        Self::new()
    }
}
