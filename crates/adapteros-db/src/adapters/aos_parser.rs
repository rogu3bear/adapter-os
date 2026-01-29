//! AOS file parsing utilities
//!
//! This module provides functions for parsing .aos adapter bundle files,
//! extracting metadata, computing hashes, and reading manifest information.

use adapteros_aos::single_file::{LoadOptions, SingleFileAdapterLoader};
use adapteros_aos::{compute_scope_hash, open_aos, BackendTag};
use adapteros_core::{AosError, B3Hash, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use tracing::{debug, warn};

const AOS_HASH_BUFFER_SIZE: usize = 64 * 1024;
const AOS_MAGIC: &[u8; 4] = b"AOS\0";
const AOS_HAS_INDEX_FLAG: u32 = 0x1;
const AOS_INDEX_ENTRY_SIZE: u64 = 80;
const AOS_HEADER_SIZE: usize = 64;

pub(crate) fn compute_aos_file_hash(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open .aos file {}: {}",
            path.display(),
            e
        ))
    })?;
    let mut buffer = vec![0u8; AOS_HASH_BUFFER_SIZE];
    let mut hasher = blake3::Hasher::new();

    loop {
        let n = file.read(&mut buffer).map_err(|e| {
            AosError::Io(format!(
                "Failed to read .aos file {}: {}",
                path.display(),
                e
            ))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

pub(crate) fn read_aos_segment_count(path: &Path) -> Result<Option<i64>> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open .aos file header {}: {}",
            path.display(),
            e
        ))
    })?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos file header {}: {}",
            path.display(),
            e
        ))
    })?;

    if !header.starts_with(AOS_MAGIC) {
        return Ok(None);
    }
    let flags = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if flags & AOS_HAS_INDEX_FLAG == 0 {
        return Ok(None);
    }

    let index_size = u64::from_le_bytes(header[16..24].try_into().unwrap());
    if index_size == 0 || index_size % AOS_INDEX_ENTRY_SIZE != 0 {
        return Ok(None);
    }

    let count = index_size / AOS_INDEX_ENTRY_SIZE;
    let count = i64::try_from(count).ok();
    Ok(count.filter(|value| *value > 0))
}

#[derive(Debug, Default)]
pub(crate) struct ParsedAosManifestMetadata {
    pub manifest_schema_version: Option<String>,
    pub base_model: Option<String>,
    pub category: Option<String>,
    pub tier: Option<String>,
    pub training_data_count: Option<i64>,
}

fn parse_manifest_count_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(num) => num.as_i64(),
        Value::String(raw) => raw.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub(crate) fn parse_aos_manifest_metadata(path: &Path) -> Result<ParsedAosManifestMetadata> {
    let bytes = std::fs::read(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos file {}: {}",
            path.display(),
            e
        ))
    })?;
    let view = open_aos(&bytes)?;
    let manifest: Value = serde_json::from_slice(view.manifest_bytes).map_err(|e| {
        AosError::InvalidManifest(format!(
            "Failed to parse .aos manifest JSON from {}: {}",
            path.display(),
            e
        ))
    })?;

    let manifest_schema_version = manifest
        .get("version")
        .or_else(|| manifest.get("schema_version"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let base_model = manifest
        .get("base_model")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let category = manifest
        .get("category")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let tier = manifest
        .get("tier")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let training_data_count = manifest
        .get("metadata")
        .and_then(|v| v.as_object())
        .and_then(|meta| {
            meta.get("training_data_count")
                .or_else(|| meta.get("dataset_examples"))
                .or_else(|| meta.get("dataset_count"))
                .and_then(parse_manifest_count_value)
        });

    Ok(ParsedAosManifestMetadata {
        manifest_schema_version,
        base_model,
        category,
        tier,
        training_data_count,
    })
}

pub(crate) fn read_aos_manifest_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open .aos file for manifest {}: {}",
            path.display(),
            e
        ))
    })?;
    let mut header = [0u8; AOS_HEADER_SIZE];
    file.read_exact(&mut header).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos manifest header {}: {}",
            path.display(),
            e
        ))
    })?;

    if !header.starts_with(AOS_MAGIC) {
        return Ok(None);
    }
    let flags = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if flags & AOS_HAS_INDEX_FLAG == 0 {
        return Ok(None);
    }

    let manifest_offset = u64::from_le_bytes(header[24..32].try_into().unwrap());
    let manifest_size = u64::from_le_bytes(header[32..40].try_into().unwrap());
    if manifest_size == 0 {
        return Ok(None);
    }

    let file_len = file.metadata().map_err(|e| {
        AosError::Io(format!(
            "Failed to stat .aos file for manifest {}: {}",
            path.display(),
            e
        ))
    })?;
    let manifest_end = match manifest_offset.checked_add(manifest_size) {
        Some(end) => end,
        None => return Ok(None),
    };
    if manifest_end > file_len.len() {
        return Ok(None);
    }

    file.seek(SeekFrom::Start(manifest_offset)).map_err(|e| {
        AosError::Io(format!(
            "Failed to seek to manifest in {}: {}",
            path.display(),
            e
        ))
    })?;
    let mut manifest_bytes = vec![0u8; manifest_size as usize];
    file.read_exact(&mut manifest_bytes).map_err(|e| {
        AosError::Io(format!(
            "Failed to read .aos manifest {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(Some(manifest_bytes))
}

#[derive(Debug, Clone)]
pub(crate) struct SingleFileAdapterMetadata {
    pub training_data_count: Option<i64>,
    pub lineage_version: Option<String>,
    pub signature_valid: Option<bool>,
}

fn single_file_metadata_options() -> LoadOptions {
    let production_mode = std::env::var("AOS_SERVER_PRODUCTION_MODE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if production_mode {
        LoadOptions::default()
    } else {
        LoadOptions {
            skip_verification: true,
            skip_signature_check: true,
        }
    }
}

pub(crate) async fn read_single_file_adapter_metadata(
    path: &Path,
) -> Result<Option<SingleFileAdapterMetadata>> {
    let options = single_file_metadata_options();
    let adapter = match SingleFileAdapterLoader::load_with_options(path, options).await {
        Ok(adapter) => adapter,
        Err(err) => {
            let message = err.to_string();
            if message.contains("Unknown file format") || message.contains("Unsupported legacy AOS")
            {
                debug!(
                    path = %path.display(),
                    "Skipping single-file adapter metadata for unsupported format"
                );
            } else {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "Failed to load single-file adapter metadata"
                );
            }
            return Ok(None);
        }
    };

    let training_data_count = i64::try_from(adapter.training_data.len()).ok();
    let lineage_version = adapter.lineage.version.trim().to_string();
    let lineage_version = if lineage_version.is_empty() {
        None
    } else {
        Some(lineage_version)
    };
    let signature_valid = if adapter.is_signed() {
        match adapter.verify_signature() {
            Ok(valid) => Some(valid),
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "Failed to verify adapter signature"
                );
                Some(false)
            }
        }
    } else {
        None
    };

    Ok(Some(SingleFileAdapterMetadata {
        training_data_count,
        lineage_version,
        signature_valid,
    }))
}

/// Minimal .aos metadata for adapter registration.
///
/// This struct captures a subset of manifest/file metadata that should be
/// persisted alongside adapter registration records.
#[derive(Debug, Clone, Default)]
pub struct AosRegistrationMetadata {
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub base_model_id: Option<String>,
    pub category: Option<String>,
    pub tier: Option<String>,
    pub manifest_metadata: Option<HashMap<String, String>>,
}

impl AosRegistrationMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn aos_file_path(mut self, path: impl Into<String>) -> Self {
        self.aos_file_path = Some(path.into());
        self
    }

    pub fn aos_file_hash(mut self, hash: impl Into<String>) -> Self {
        self.aos_file_hash = Some(hash.into());
        self
    }

    pub fn manifest_schema_version(mut self, version: impl Into<String>) -> Self {
        self.manifest_schema_version = Some(version.into());
        self
    }

    pub fn content_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.content_hash_b3 = Some(hash.into());
        self
    }

    pub fn base_model_id(mut self, base_model_id: impl Into<String>) -> Self {
        self.base_model_id = Some(base_model_id.into());
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn tier(mut self, tier: impl Into<String>) -> Self {
        self.tier = Some(tier.into());
        self
    }

    pub fn manifest_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.manifest_metadata = Some(metadata);
        self
    }
}

pub(crate) fn load_aos_registration_metadata(path: &Path) -> Option<AosRegistrationMetadata> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "Failed to read .aos file for manifest metadata"
            );
            return None;
        }
    };

    let view = match open_aos(&bytes) {
        Ok(view) => view,
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "Failed to parse .aos file for manifest metadata"
            );
            return None;
        }
    };

    let manifest_value: Value = match serde_json::from_slice(view.manifest_bytes) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "Failed to decode .aos manifest JSON"
            );
            return None;
        }
    };

    let manifest_schema_version = manifest_value
        .get("version")
        .or_else(|| manifest_value.get("schema_version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_model_id = manifest_value
        .get("base_model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let category = manifest_value
        .get("category")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let tier = manifest_value
        .get("tier")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let manifest_metadata = manifest_value
        .get("metadata")
        .and_then(|meta| meta.as_object())
        .map(|meta| {
            meta.iter()
                .filter_map(|(key, value)| value.as_str().map(|val| (key.clone(), val.to_string())))
                .collect::<HashMap<String, String>>()
        })
        .filter(|meta| !meta.is_empty());

    let scope_path = manifest_metadata
        .as_ref()
        .and_then(|meta| meta.get("scope_path"))
        .map(|value| value.as_str());

    let canonical_segment = scope_path
        .and_then(|path| {
            let scope_hash = compute_scope_hash(path);
            view.segments.iter().find(|seg| {
                seg.backend_tag == BackendTag::Canonical && seg.scope_hash == scope_hash
            })
        })
        .or_else(|| {
            view.segments
                .iter()
                .find(|seg| seg.backend_tag == BackendTag::Canonical)
        })
        .or_else(|| view.segments.first());

    let content_hash_b3 = canonical_segment.map(|seg| {
        B3Hash::hash_multi(&[view.manifest_bytes, seg.payload])
            .to_hex()
            .to_string()
    });

    let mut metadata = AosRegistrationMetadata::new();
    metadata.manifest_schema_version = manifest_schema_version;
    metadata.base_model_id = base_model_id;
    metadata.category = category;
    metadata.tier = tier;
    metadata.content_hash_b3 = content_hash_b3;
    metadata.manifest_metadata = manifest_metadata;

    Some(metadata)
}
