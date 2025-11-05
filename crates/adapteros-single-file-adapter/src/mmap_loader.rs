// mmap-based loader for .aos files

use crate::format::{verify_format_version, AdapterManifest, AosSignature, SingleFileAdapter};
use crate::format::{AdapterWeights, LineageInfo, WeightGroup, WeightGroupType, WeightMetadata};
use crate::format::{CombinationStrategy, WeightGroupConfig};
use crate::loader::LoadOptions;
use crate::training::{TrainingConfig, TrainingExample};
use crate::weights::{WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, B3Hash, Result};
use memmap2::Mmap;
use parking_lot::Mutex;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use zip::read::ZipArchive;
use zip::CompressionMethod;

#[derive(Debug, Clone, Copy)]
pub enum WeightsKind {
    Positive,
    Negative,
    Combined,
}

#[derive(Debug, Clone)]
struct ZipEntryInfo {
    #[allow(dead_code)]
    name: &'static str,
    offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    compression: CompressionMethod,
}

impl ZipEntryInfo {
    fn is_stored(&self) -> bool {
        matches!(self.compression, CompressionMethod::Stored)
    }
}

type WeightCacheRefs<'a> = (
    &'a Option<ZipEntryInfo>,
    &'a Mutex<Option<Arc<Vec<u8>>>>,
    &'a str,
);

fn get_entry_info<R: std::io::Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &'static str,
) -> Option<ZipEntryInfo> {
    if let Ok(file) = zip.by_name(name) {
        Some(ZipEntryInfo {
            name,
            offset: file.data_start(),
            compressed_size: file.compressed_size(),
            uncompressed_size: file.size(),
            compression: file.compression(),
        })
    } else {
        None
    }
}

pub struct MmapAdapter {
    mmap: Mmap,
    #[allow(dead_code)]
    path: PathBuf,
    pub manifest: AdapterManifest,
    weights_pos: Option<ZipEntryInfo>,
    weights_neg: Option<ZipEntryInfo>,
    weights_comb: Option<ZipEntryInfo>,
    pos_cache: Mutex<Option<Arc<Vec<u8>>>>,
    neg_cache: Mutex<Option<Arc<Vec<u8>>>>,
    comb_cache: Mutex<Option<Arc<Vec<u8>>>>,
    sig_cache: Mutex<Option<AosSignature>>,
    file_len: usize,
}

impl MmapAdapter {
    pub fn from_path(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;
        let file_len = file
            .metadata()
            .map_err(|e| AosError::Io(format!("Failed to stat .aos file: {}", e)))?
            .len() as usize;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Io(format!("Failed to mmap .aos file: {}", e)))?;

        // Initialize zip reader over mmap
        let cursor = Cursor::new(&mmap[..]);
        let mut zip = ZipArchive::new(cursor)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;

        // Parse manifest immediately (borrow zip for manifest, then release)
        let manifest = {
            let mut manifest_file = zip.by_name("manifest.json").map_err(|_| {
                AosError::Training("Missing manifest.json in .aos file".to_string())
            })?;
            let mut manifest_data = Vec::new();
            manifest_file
                .read_to_end(&mut manifest_data)
                .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
            let m: AdapterManifest = serde_json::from_slice(&manifest_data)
                .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;
            verify_format_version(m.format_version)?;
            m
        };

        // Record weight entry offsets for lazy reads/zero-copy (sequential calls, no overlapping borrows)
        let weights_pos = get_entry_info(&mut zip, "weights_positive.safetensors");
        let weights_neg = get_entry_info(&mut zip, "weights_negative.safetensors");
        let weights_comb = get_entry_info(&mut zip, "weights_combined.safetensors");

        Ok(Self {
            mmap,
            path: path.to_path_buf(),
            manifest,
            weights_pos,
            weights_neg,
            weights_comb,
            pos_cache: Mutex::new(None),
            neg_cache: Mutex::new(None),
            comb_cache: Mutex::new(None),
            sig_cache: Mutex::new(None),
            file_len,
        })
    }

    pub fn file_len(&self) -> usize {
        self.file_len
    }

    // Load signature on demand and verify against manifest
    pub fn verify_signature(&self) -> Result<bool> {
        // Try cached
        if let Some(sig) = self.sig_cache.lock().clone() {
            let manifest_hash = B3Hash::hash(&serde_json::to_vec(&self.manifest)?);
            sig.public_key
                .verify(&manifest_hash.to_bytes(), &sig.signature)?;
            return Ok(true);
        }

        // Load signature fresh
        let cursor = Cursor::new(&self.mmap[..]);
        let mut zip = ZipArchive::new(cursor).map_err(|e| {
            AosError::Io(format!("Failed to open ZIP archive for signature: {}", e))
        })?;
        let opt_sig = match zip.by_name("signature.sig") {
            Ok(mut f) => {
                let mut data = Vec::new();
                f.read_to_end(&mut data)
                    .map_err(|e| AosError::Io(format!("Failed to read signature.sig: {}", e)))?;
                let sig: AosSignature = serde_json::from_slice(&data)
                    .map_err(|e| AosError::Parse(format!("Invalid signature: {}", e)))?;
                Some(sig)
            }
            Err(_) => None,
        };
        // Cache it
        *self.sig_cache.lock() = opt_sig.clone();
        if let Some(sig) = opt_sig {
            let manifest_hash = B3Hash::hash(&serde_json::to_vec(&self.manifest)?);
            sig.public_key
                .verify(&manifest_hash.to_bytes(), &sig.signature)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn is_weights_stored(&self, kind: WeightsKind) -> bool {
        match kind {
            WeightsKind::Positive => self
                .weights_pos
                .as_ref()
                .map(|e| e.is_stored())
                .unwrap_or(false),
            WeightsKind::Negative => self
                .weights_neg
                .as_ref()
                .map(|e| e.is_stored())
                .unwrap_or(false),
            WeightsKind::Combined => self
                .weights_comb
                .as_ref()
                .map(|e| e.is_stored())
                .unwrap_or(false),
        }
    }

    pub fn get_weights_slice(&self, kind: WeightsKind) -> Result<Arc<Vec<u8>>> {
        let (info_opt, cache, name): WeightCacheRefs<'_> = match kind {
            WeightsKind::Positive => (
                &self.weights_pos,
                &self.pos_cache,
                "weights_positive.safetensors",
            ),
            WeightsKind::Negative => (
                &self.weights_neg,
                &self.neg_cache,
                "weights_negative.safetensors",
            ),
            WeightsKind::Combined => (
                &self.weights_comb,
                &self.comb_cache,
                "weights_combined.safetensors",
            ),
        };

        let info = info_opt
            .as_ref()
            .ok_or_else(|| AosError::Training(format!("Missing {} in .aos file", name)))?;

        if info.is_stored() {
            let start = info.offset as usize;
            let end = start + (info.compressed_size as usize);
            return Ok(Arc::new(self.mmap[start..end].to_vec()));
        }

        // Deflated: decompress lazily and cache
        {
            let guard = cache.lock();
            if let Some(ref v) = *guard {
                return Ok(Arc::clone(v));
            }
        }
        // Decompress and populate cache
        let mut buf = Vec::with_capacity(info.uncompressed_size as usize);
        let cursor = Cursor::new(&self.mmap[..]);
        let mut zip = ZipArchive::new(cursor)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
        let mut file = zip
            .by_name(name)
            .map_err(|_| AosError::Training(format!("Missing {} in .aos file", name)))?;
        file.read_to_end(&mut buf)
            .map_err(|e| AosError::Io(format!("Failed to read {}: {}", name, e)))?;
        let arc_buf = Arc::new(buf);
        {
            let mut guard = cache.lock();
            *guard = Some(Arc::clone(&arc_buf));
        }
        Ok(arc_buf)
    }

    pub fn to_standard_adapter(&self) -> Result<SingleFileAdapter> {
        // Config is needed for legacy weights parsing
        let config: TrainingConfig = {
            let cursor = Cursor::new(&self.mmap[..]);
            let mut zip = ZipArchive::new(cursor)
                .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
            let mut f = zip
                .by_name("config.toml")
                .map_err(|_| AosError::Training("Missing config.toml in .aos file".to_string()))?;
            let mut data = Vec::new();
            f.read_to_end(&mut data)
                .map_err(|e| AosError::Io(format!("Failed to read config.toml: {}", e)))?;
            let s = String::from_utf8(data)
                .map_err(|e| AosError::Parse(format!("Invalid UTF-8 in config.toml: {}", e)))?;
            toml::from_str(&s)
                .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?
        };

        // Determine if separated weights are present by presence of positive/negative files
        let has_separated = self.weights_pos.is_some() && self.weights_neg.is_some();
        let (weights, _weight_config) = if has_separated {
            // Load weight groups manifest metadata
            let info: WeightGroupsManifest = {
                let cursor = Cursor::new(&self.mmap[..]);
                let mut zip = ZipArchive::new(cursor)
                    .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
                let mut f = zip.by_name("weight_groups.json").map_err(|_| {
                    AosError::Training("Missing weight_groups.json in .aos file".to_string())
                })?;
                let mut data = Vec::new();
                f.read_to_end(&mut data).map_err(|e| {
                    AosError::Io(format!("Failed to read weight_groups.json: {}", e))
                })?;
                serde_json::from_slice(&data).map_err(|e| {
                    AosError::Parse(format!("Failed to parse weight_groups.json: {}", e))
                })?
            };

            let disk_meta_to_rt = |d: &WeightGroupDiskInfo, t: WeightGroupType| WeightMetadata {
                example_count: d.example_count,
                avg_loss: d.avg_loss,
                training_time_ms: d.training_time_ms,
                group_type: t,
                created_at: d.created_at.clone(),
            };

            let positive = crate::weights::deserialize_weight_group(
                self.get_weights_slice(WeightsKind::Positive)?.as_slice(),
                disk_meta_to_rt(&info.positive, WeightGroupType::Positive),
            )?;
            let negative = crate::weights::deserialize_weight_group(
                self.get_weights_slice(WeightsKind::Negative)?.as_slice(),
                disk_meta_to_rt(&info.negative, WeightGroupType::Negative),
            )?;
            let combined = match (self.weights_comb.as_ref(), info.combined.as_ref()) {
                (Some(_), Some(meta)) => Some(crate::weights::deserialize_weight_group(
                    self.get_weights_slice(WeightsKind::Combined)?.as_slice(),
                    disk_meta_to_rt(meta, WeightGroupType::Combined),
                )?),
                _ => None,
            };

            (
                AdapterWeights {
                    positive,
                    negative,
                    combined,
                },
                WeightGroupConfig {
                    use_separate_weights: info.use_separate_weights,
                    combination_strategy: info.combination_strategy.clone(),
                },
            )
        } else {
            // Legacy single weights format
            let cursor = Cursor::new(&self.mmap[..]);
            let mut zip = ZipArchive::new(cursor)
                .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
            let mut f = zip
                .by_name("weights.safetensors")
                .map_err(|_| AosError::Training("Missing weights file".to_string()))?;
            let mut data = Vec::new();
            f.read_to_end(&mut data)
                .map_err(|e| AosError::Io(format!("Failed to read weights.safetensors: {}", e)))?;
            deserialize_legacy_weights_mmap(&data, &config)?
        };

        // Load training data (JSONL)
        let training_data: Vec<TrainingExample> = {
            let cursor = Cursor::new(&self.mmap[..]);
            let mut zip = ZipArchive::new(cursor)
                .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
            let mut f = zip.by_name("training_data.jsonl").map_err(|_| {
                AosError::Training("Missing training_data.jsonl in .aos file".to_string())
            })?;
            let mut data = Vec::new();
            f.read_to_end(&mut data)
                .map_err(|e| AosError::Io(format!("Failed to read training_data.jsonl: {}", e)))?;
            let s = String::from_utf8(data)
                .map_err(|e| AosError::Parse(format!("Invalid UTF-8 in training data: {}", e)))?;
            let mut v = Vec::new();
            for (idx, line) in s.lines().enumerate() {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                let ex: TrainingExample = serde_json::from_str(t).map_err(|e| {
                    AosError::Parse(format!("Failed to parse training data line {}: {}", idx, e))
                })?;
                v.push(ex);
            }
            v
        };

        // Load lineage
        let lineage: LineageInfo = {
            let cursor = Cursor::new(&self.mmap[..]);
            let mut zip = ZipArchive::new(cursor)
                .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
            let mut f = zip
                .by_name("lineage.json")
                .map_err(|_| AosError::Training("Missing lineage.json in .aos file".to_string()))?;
            let mut data = Vec::new();
            f.read_to_end(&mut data)
                .map_err(|e| AosError::Io(format!("Failed to read lineage.json: {}", e)))?;
            serde_json::from_slice(&data)
                .map_err(|e| AosError::Parse(format!("Failed to parse lineage: {}", e)))?
        };

        // Load signature if present and cache
        let signature = {
            if let Some(sig) = self.sig_cache.lock().clone() {
                Some(sig)
            } else {
                let cursor = Cursor::new(&self.mmap[..]);
                let mut zip = ZipArchive::new(cursor)
                    .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
                let opt_sig = match zip.by_name("signature.sig") {
                    Ok(mut f) => {
                        let mut data = Vec::new();
                        f.read_to_end(&mut data).map_err(|e| {
                            AosError::Io(format!("Failed to read signature.sig: {}", e))
                        })?;
                        let sig: AosSignature = serde_json::from_slice(&data)
                            .map_err(|e| AosError::Parse(format!("Invalid signature: {}", e)))?;
                        Some(sig)
                    }
                    Err(_) => None,
                };
                *self.sig_cache.lock() = opt_sig.clone();
                opt_sig
            }
        };

        let adapter = SingleFileAdapter {
            manifest: self.manifest.clone(),
            weights,
            training_data,
            config,
            lineage,
            signature,
        };

        Ok(adapter)
    }
}

// Local legacy weights deserialization (copy of logic in loader.rs)
fn deserialize_legacy_weights_mmap(
    bytes: &[u8],
    config: &TrainingConfig,
) -> Result<(AdapterWeights, WeightGroupConfig)> {
    let rank = config.rank;
    let hidden_dim = config.hidden_dim;
    let expected_floats = rank * hidden_dim * 2;
    let expected_bytes = expected_floats * std::mem::size_of::<f32>();

    if bytes.len() < expected_bytes {
        return Err(AosError::Training(format!(
            "Legacy weights payload too small: expected at least {} bytes, found {}",
            expected_bytes,
            bytes.len()
        )));
    }

    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();

    if floats.len() < expected_floats {
        return Err(AosError::Training(
            "Legacy weights payload truncated".to_string(),
        ));
    }

    let (a_slice, b_slice) = floats.split_at(rank * hidden_dim);

    let mut lora_a = Vec::with_capacity(rank);
    for r in 0..rank {
        let start = r * hidden_dim;
        let end = start + hidden_dim;
        lora_a.push(a_slice[start..end].to_vec());
    }

    let mut lora_b = Vec::with_capacity(hidden_dim);
    for h in 0..hidden_dim {
        let start = h * rank;
        let end = start + rank;
        lora_b.push(b_slice[start..end].to_vec());
    }

    let created_at = chrono::Utc::now().to_rfc3339();

    let positive = WeightGroup {
        lora_a: lora_a.clone(),
        lora_b: lora_b.clone(),
        metadata: WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Positive,
            created_at: created_at.clone(),
        },
    };

    let negative = WeightGroup {
        lora_a: vec![vec![0.0; hidden_dim]; rank],
        lora_b: vec![vec![0.0; rank]; hidden_dim],
        metadata: WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Negative,
            created_at: created_at.clone(),
        },
    };

    let combined = WeightGroup {
        lora_a,
        lora_b,
        metadata: WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Combined,
            created_at,
        },
    };

    let config = WeightGroupConfig {
        use_separate_weights: false,
        combination_strategy: CombinationStrategy::Difference,
    };

    Ok((
        AdapterWeights {
            positive,
            negative,
            combined: Some(combined),
        },
        config,
    ))
}

pub struct MmapAdapterLoader {
    inner: Mutex<CacheInner>,
}

struct CacheInner {
    cache: lru::LruCache<PathBuf, Arc<MmapAdapter>>,
    current_bytes: usize,
    max_bytes: usize,
}

impl MmapAdapterLoader {
    pub fn with_capacity_bytes(cap_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                cache: lru::LruCache::unbounded(),
                current_bytes: 0,
                max_bytes: cap_bytes,
            }),
        }
    }

    pub fn global() -> &'static Self {
        static INSTANCE: OnceLock<MmapAdapterLoader> = OnceLock::new();
        INSTANCE.get_or_init(|| Self::with_capacity_bytes(1024 * 1024 * 1024)) // 1 GiB default
    }

    pub fn load(&self, path: &Path, options: &LoadOptions) -> Result<Arc<MmapAdapter>> {
        // Check file size before loading to prevent OOM attacks
        if let Ok(metadata) = std::fs::metadata(path) {
            let max_size_bytes = 500 * 1024 * 1024; // 500MB limit
            if metadata.len() > max_size_bytes {
                return Err(AosError::PolicyViolation(format!(
                    "Adapter file size {} bytes exceeds maximum {} bytes",
                    metadata.len(),
                    max_size_bytes
                )));
            }
        }

        // Fast path: check cache
        {
            let mut guard = self.inner.lock();
            if let Some(entry) = guard.cache.get(path) {
                return Ok(entry.clone());
            }
        }

        // Miss: load mmap adapter without holding the lock
        let adapter = Arc::new(MmapAdapter::from_path(path)?);

        // Verify signature before inserting into cache if requested
        if !options.skip_signature_check {
            // If signature exists and verification fails, error out
            let has_sig = {
                let cursor = Cursor::new(&adapter.mmap[..]);
                let mut zip = ZipArchive::new(cursor)
                    .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;
                let result = zip.by_name("signature.sig").is_ok();
                drop(zip);
                result
            };
            if has_sig && !adapter.verify_signature()? {
                return Err(AosError::Training(
                    "Signature verification failed".to_string(),
                ));
            }
        }

        // Insert and evict by total bytes
        let mut guard = self.inner.lock();
        let size = adapter.file_len();
        guard.current_bytes = guard.current_bytes.saturating_add(size);
        guard.cache.put(path.to_path_buf(), adapter.clone());
        while guard.current_bytes > guard.max_bytes {
            if let Some((_k, v)) = guard.cache.pop_lru() {
                guard.current_bytes = guard.current_bytes.saturating_sub(v.file_len());
            } else {
                break;
            }
        }
        Ok(adapter)
    }

    #[cfg(test)]
    pub fn cache_len(&self) -> usize {
        self.inner.lock().cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{SingleFileAdapter, WeightGroup};
    use crate::packager::{PackageOptions, SingleFileAdapterPackager};
    use tempfile::TempDir;

    fn create_test_adapter() -> SingleFileAdapter {
        let positive = WeightGroup {
            lora_a: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            lora_b: vec![vec![0.5, 0.6], vec![0.7, 0.8]],
            metadata: WeightMetadata {
                example_count: 10,
                avg_loss: 0.5,
                training_time_ms: 1000,
                group_type: WeightGroupType::Positive,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };
        let negative = WeightGroup {
            lora_a: vec![vec![0.0, 0.1], vec![0.2, 0.3]],
            lora_b: vec![vec![0.4, 0.5], vec![0.6, 0.7]],
            metadata: WeightMetadata {
                example_count: 5,
                avg_loss: 0.4,
                training_time_ms: 800,
                group_type: WeightGroupType::Negative,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };
        let weights = AdapterWeights {
            positive,
            negative,
            combined: None,
        };
        let training_data: Vec<TrainingExample> = vec![];
        let config = TrainingConfig::default();
        let lineage = LineageInfo {
            adapter_id: "test_adapter".to_string(),
            version: "1.0.0".to_string(),
            parent_version: None,
            parent_hash: None,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        SingleFileAdapter::create(
            "test_adapter".to_string(),
            weights,
            training_data,
            config,
            lineage,
        )
        .unwrap()
    }

    #[tokio::test]
    #[ignore = "FIXME: This test hangs indefinitely - investigate ZIP parsing performance issue"]
    async fn test_mmap_vs_standard_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("mmap_manifest_test.aos");

        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        let loaded_std = crate::loader::SingleFileAdapterLoader::load(&aos_path)
            .await
            .unwrap();

        // Add timeout safeguard to prevent hanging - spawn blocking operations
        let load_options = LoadOptions {
            skip_verification: true,
            skip_signature_check: true,
            use_mmap: true,
        };
        let aos_path_clone = aos_path.clone();

        let mmap_loaded = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                MmapAdapterLoader::global().load(&aos_path_clone, &load_options)
            }),
        )
        .await
        .expect("MmapAdapterLoader::load should complete within 10 seconds")
        .expect("Spawn blocking failed")
        .unwrap();

        let converted = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || mmap_loaded.to_standard_adapter()),
        )
        .await
        .expect("to_standard_adapter should complete within 10 seconds")
        .expect("Spawn blocking failed")
        .unwrap();

        assert_eq!(
            loaded_std.manifest.adapter_id,
            converted.manifest.adapter_id
        );
        assert_eq!(
            loaded_std.manifest.format_version,
            converted.manifest.format_version
        );
    }

    #[tokio::test]
    async fn test_zero_copy_weights_when_stored() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("mmap_zero_copy_test.aos");

        let adapter = create_test_adapter();
        let options = PackageOptions {
            compression: crate::format::CompressionLevel::Store,
            ..Default::default()
        };
        SingleFileAdapterPackager::save_with_options(&adapter, &aos_path, options)
            .await
            .unwrap();

        let mmap_loaded = MmapAdapterLoader::global()
            .load(
                &aos_path,
                &LoadOptions {
                    skip_verification: true,
                    skip_signature_check: true,
                    use_mmap: true,
                },
            )
            .unwrap();
        assert!(mmap_loaded.is_weights_stored(WeightsKind::Positive));
        let slice = mmap_loaded
            .get_weights_slice(WeightsKind::Positive)
            .unwrap();
        assert!(!slice.is_empty());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let temp_dir = TempDir::new().unwrap();
        let aos1 = temp_dir.path().join("a1.aos");
        let aos2 = temp_dir.path().join("a2.aos");

        let adapter1 = create_test_adapter();
        let adapter2 = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter1, &aos1)
            .await
            .unwrap();
        SingleFileAdapterPackager::save(&adapter2, &aos2)
            .await
            .unwrap();

        let loader = MmapAdapterLoader::with_capacity_bytes(1); // tiny cache to force eviction
        let _a1 = loader
            .load(
                &aos1,
                &LoadOptions {
                    skip_verification: true,
                    skip_signature_check: true,
                    use_mmap: true,
                },
            )
            .unwrap();
        let _a2 = loader
            .load(
                &aos2,
                &LoadOptions {
                    skip_verification: true,
                    skip_signature_check: true,
                    use_mmap: true,
                },
            )
            .unwrap();

        // With tiny capacity, at most 1 entry should remain
        assert!(loader.cache_len() <= 1);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        use std::sync::Arc as StdArc;
        use tokio::task::JoinSet;

        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("mmap_concurrent.aos");

        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        let loader = StdArc::new(MmapAdapterLoader::with_capacity_bytes(1024 * 1024 * 100));
        let mut set = JoinSet::new();
        for _ in 0..8 {
            let l = loader.clone();
            let p = aos_path.clone();
            set.spawn(async move {
                let a = l
                    .load(
                        &p,
                        &LoadOptions {
                            skip_verification: true,
                            skip_signature_check: true,
                            use_mmap: true,
                        },
                    )
                    .unwrap();
                let _slice = a.get_weights_slice(WeightsKind::Positive).unwrap();
            });
        }
        while let Some(res) = set.join_next().await {
            res.unwrap();
        }
    }
}
