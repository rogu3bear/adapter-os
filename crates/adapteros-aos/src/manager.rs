//! Unified manager API for .aos file operations

use crate::cache::{AdapterCache, CacheConfig};
use crate::hot_swap::HotSwapManager;
use crate::implementation::{AosLoader, LoadedAdapter, MoEConfigManifest};
use adapteros_core::{AosError, B3Hash, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, instrument, warn};

pub struct AosManager {
    loader: AosLoader,
    cache: Option<Arc<AdapterCache>>,
    hot_swap: Option<Arc<HotSwapManager>>,
}

impl AosManager {
    pub fn builder() -> AosManagerBuilder {
        AosManagerBuilder::default()
    }

    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub async fn load<P: AsRef<Path>>(&self, path: P) -> Result<Arc<LoadedAdapter>> {
        let path = path.as_ref();

        if let Some(ref cache) = self.cache {
            if let Some(adapter) = cache.get(path) {
                return Ok(adapter);
            }
        }

        let adapter = self.loader.load_from_path(path).await?;
        let adapter = Arc::new(adapter);

        if let Some(ref cache) = self.cache {
            cache.insert(path, Arc::clone(&adapter))?;
        }

        Ok(adapter)
    }

    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub async fn load_uncached<P: AsRef<Path>>(&self, path: P) -> Result<LoadedAdapter> {
        self.loader.load_from_path(path.as_ref()).await
    }

    #[instrument(skip(self), fields(slot = %slot, path = %path.as_ref().display()))]
    pub async fn hot_swap<P: AsRef<Path>>(&self, slot: &str, path: P) -> Result<()> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.swap_single(slot, path).await
        } else {
            Err(adapteros_core::AosError::Config(
                "Hot-swap not enabled".to_string(),
            ))
        }
    }

    pub async fn preload<P: AsRef<Path>>(&self, slot: &str, path: P) -> Result<()> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.preload(slot, path).await
        } else {
            Err(adapteros_core::AosError::Config(
                "Hot-swap not enabled".to_string(),
            ))
        }
    }

    pub fn commit_swap(&self, slots: &[String]) -> Result<()> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.swap(slots)
        } else {
            Err(adapteros_core::AosError::Config(
                "Hot-swap not enabled".to_string(),
            ))
        }
    }

    pub fn rollback(&self) -> Result<()> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.rollback()
        } else {
            Err(adapteros_core::AosError::Config(
                "Hot-swap not enabled".to_string(),
            ))
        }
    }

    pub fn cache(&self) -> Option<Arc<AdapterCache>> {
        self.cache.as_ref().map(Arc::clone)
    }

    pub fn hot_swap_manager(&self) -> Option<Arc<HotSwapManager>> {
        self.hot_swap.as_ref().map(Arc::clone)
    }

    pub fn evict<P: AsRef<Path>>(&self, path: P) -> Option<Arc<LoadedAdapter>> {
        if let Some(ref cache) = self.cache {
            cache.remove(path)
        } else {
            None
        }
    }

    pub fn clear_cache(&self) {
        if let Some(ref cache) = self.cache {
            cache.clear();
        }
    }

    // ==================== MoE Support Methods ====================

    /// Load an MoE adapter with validation
    ///
    /// This validates that the adapter has MoE configuration before loading.
    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub async fn load_moe<P: AsRef<Path>>(&self, path: P) -> Result<Arc<LoadedAdapter>> {
        let adapter = self.load(path.as_ref()).await?;

        if !adapter.is_moe_adapter() {
            return Err(AosError::Validation(format!(
                "Adapter '{}' is not configured for MoE models",
                adapter.adapter_id()
            )));
        }

        Ok(adapter)
    }

    /// Load an MoE adapter and validate it matches the expected configuration
    ///
    /// This ensures the adapter's MoE config is compatible with the target model.
    #[instrument(skip(self, expected_config), fields(path = %path.as_ref().display()))]
    pub async fn load_moe_validated<P: AsRef<Path>>(
        &self,
        path: P,
        expected_config: &MoEConfigManifest,
    ) -> Result<Arc<LoadedAdapter>> {
        let adapter = self.load(path.as_ref()).await?;

        let moe_config = adapter.moe_config().ok_or_else(|| {
            AosError::Validation(format!(
                "Adapter '{}' is not configured for MoE models",
                adapter.adapter_id()
            ))
        })?;

        // Validate expert counts match
        if moe_config.num_experts != expected_config.num_experts {
            return Err(AosError::Validation(format!(
                "MoE expert count mismatch: adapter has {} experts, expected {}",
                moe_config.num_experts, expected_config.num_experts
            )));
        }

        if moe_config.num_experts_per_token != expected_config.num_experts_per_token {
            return Err(AosError::Validation(format!(
                "MoE experts-per-token mismatch: adapter has {}, expected {}",
                moe_config.num_experts_per_token, expected_config.num_experts_per_token
            )));
        }

        debug!(
            adapter_id = adapter.adapter_id(),
            num_experts = moe_config.num_experts,
            "MoE adapter validated successfully"
        );

        Ok(adapter)
    }

    /// Scan a directory for MoE-compatible .aos files
    ///
    /// Returns paths to all .aos files that have MoE configuration.
    pub async fn discover_moe_adapters<P: AsRef<Path>>(
        &self,
        directory: P,
    ) -> Result<Vec<(PathBuf, MoEConfigManifest)>> {
        let dir = directory.as_ref();
        if !dir.is_dir() {
            return Err(AosError::Io(format!(
                "Path is not a directory: {}",
                dir.display()
            )));
        }

        let mut moe_adapters = Vec::new();

        let entries = std::fs::read_dir(dir).map_err(|e| {
            AosError::Io(format!("Failed to read directory {}: {}", dir.display(), e))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "aos") {
                match self.loader.load_from_path(&path).await {
                    Ok(adapter) => {
                        if let Some(moe_config) = adapter.moe_config() {
                            debug!(
                                path = %path.display(),
                                num_experts = moe_config.num_experts,
                                "Found MoE adapter"
                            );
                            moe_adapters.push((path, moe_config.clone()));
                        }
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "Failed to load adapter");
                    }
                }
            }
        }

        Ok(moe_adapters)
    }

    /// Find MoE adapters compatible with a specific configuration
    ///
    /// Scans a directory and returns only adapters matching the expected MoE config.
    pub async fn find_compatible_moe_adapters<P: AsRef<Path>>(
        &self,
        directory: P,
        expected_config: &MoEConfigManifest,
    ) -> Result<Vec<PathBuf>> {
        let all_moe = self.discover_moe_adapters(directory).await?;

        let compatible: Vec<PathBuf> = all_moe
            .into_iter()
            .filter(|(_, config)| {
                config.num_experts == expected_config.num_experts
                    && config.num_experts_per_token == expected_config.num_experts_per_token
            })
            .map(|(path, _)| path)
            .collect();

        Ok(compatible)
    }

    /// Hot-swap an MoE adapter with validation
    #[instrument(skip(self), fields(slot = %slot, path = %path.as_ref().display()))]
    pub async fn hot_swap_moe<P: AsRef<Path>>(&self, slot: &str, path: P) -> Result<()> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.preload_moe(slot, path).await?;
            hot_swap.swap_moe(&[slot.to_string()])
        } else {
            Err(AosError::Config("Hot-swap not enabled".to_string()))
        }
    }

    /// Get all active MoE adapters from hot-swap manager
    pub fn get_active_moe_adapters(&self) -> Vec<(String, Arc<LoadedAdapter>)> {
        if let Some(ref hot_swap) = self.hot_swap {
            hot_swap.get_active_moe_adapters()
        } else {
            Vec::new()
        }
    }

    /// Get MoE memory usage from cache
    pub fn moe_cache_size_bytes(&self) -> u64 {
        if let Some(ref cache) = self.cache {
            cache.moe_size_bytes()
        } else {
            0
        }
    }

    /// Get count of cached MoE adapters
    pub fn moe_cache_count(&self) -> usize {
        if let Some(ref cache) = self.cache {
            cache.moe_count()
        } else {
            0
        }
    }

    /// Evict all MoE adapters from cache to free memory
    pub fn evict_moe_adapters(&self) -> u64 {
        if let Some(ref cache) = self.cache {
            cache.evict_all_moe_adapters()
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct AosManagerBuilder {
    cache_config: Option<CacheConfig>,
    enable_hot_swap: bool,
    global_seed: Option<B3Hash>,
}

impl AosManagerBuilder {
    pub fn with_cache(mut self, max_size_bytes: u64) -> Self {
        self.cache_config = Some(CacheConfig {
            max_size_bytes,
            max_count: 0,
        });
        self
    }

    pub fn with_cache_config(mut self, config: CacheConfig) -> Self {
        self.cache_config = Some(config);
        self
    }

    pub fn with_hot_swap(mut self) -> Self {
        self.enable_hot_swap = true;
        self
    }

    pub fn with_seed(mut self, seed: B3Hash) -> Self {
        self.global_seed = Some(seed);
        self
    }

    /// Deprecated: AosLoader always validates format. Kept for API compatibility.
    #[deprecated(note = "AosLoader always validates format. This method has no effect.")]
    pub fn without_verification(self) -> Self {
        // AosLoader always validates the AOS format. This method is kept for
        // API compatibility but has no effect.
        self
    }

    pub fn build(self) -> Result<AosManager> {
        let loader = if let Some(seed) = self.global_seed {
            AosLoader::with_seed(&seed)?
        } else {
            AosLoader::new()?
        };

        let cache = self
            .cache_config
            .map(|config| Arc::new(AdapterCache::new(config)));

        let hot_swap = if self.enable_hot_swap {
            let seed = self.global_seed;
            let manager = if let Some(seed) = seed {
                HotSwapManager::with_seed(&seed)?
            } else {
                HotSwapManager::new()?
            };
            Some(Arc::new(manager))
        } else {
            None
        };

        Ok(AosManager {
            loader,
            cache,
            hot_swap,
        })
    }
}
