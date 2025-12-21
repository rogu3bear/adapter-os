//! Unified manager API for .aos file operations

use crate::cache::{AdapterCache, CacheConfig};
use crate::hot_swap::HotSwapManager;
use crate::implementation::{AosLoader, LoadedAdapter};
use adapteros_core::{B3Hash, Result};
use std::path::Path;
use std::sync::Arc;
use tracing::instrument;

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
}

pub struct AosManagerBuilder {
    cache_config: Option<CacheConfig>,
    enable_hot_swap: bool,
    global_seed: Option<B3Hash>,
}

impl Default for AosManagerBuilder {
    fn default() -> Self {
        Self {
            cache_config: None,
            enable_hot_swap: false,
            global_seed: None,
        }
    }
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
            let seed = self.global_seed.clone();
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
