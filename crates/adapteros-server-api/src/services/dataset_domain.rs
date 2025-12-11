//! Dataset-domain service
//!
//! Canonical interface for dataset normalization, manifest generation, and
//! deterministic row streaming for training workers.
//!
//! Responsibilities:
//! - Normalize supported input dialects into canonical JSONL rows
//! - Generate and persist dataset manifests per dataset version
//! - Provide deterministic row streaming by dataset_version_id + split
//! - Keep training components from re-implementing schema/normalization

use crate::handlers::datasets::{ensure_dirs, resolve_dataset_root, DatasetPaths};
use crate::state::AppState;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::training_datasets::{TrainingDataset, TrainingDatasetVersion};
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey, StorageKind};
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tracing::info;
use uuid::Uuid;

const CANONICAL_FILENAME: &str = "canonical.jsonl";
const MANIFEST_FILENAME: &str = "manifest.json";

// Re-export shared dataset-domain types for callers.
pub use adapteros_api_types::{
    CanonicalRow, DatasetManifest, DatasetVersionDescriptor, NormalizationNotes, SamplingConfig,
    SplitStats,
};

/// Request for ingesting raw files into a dataset version.
#[derive(Debug, Clone)]
pub struct RawIngestRequest {
    pub tenant_id: String,
    pub dataset_id: String,
    pub version_label: Option<String>,
    pub created_by: Option<String>,
    pub files: Vec<RawFileDescriptor>,
}

/// Supported raw file descriptor.
#[derive(Debug, Clone)]
pub struct RawFileDescriptor {
    pub path: PathBuf,
    pub format: RawDialect,
    pub split: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RawDialect {
    CanonicalJsonl,
    LegacyJsonArray,
    Csv,
    PlainText,
}

impl RawDialect {
    fn as_tag(&self) -> &'static str {
        match self {
            RawDialect::CanonicalJsonl => "jsonl",
            RawDialect::LegacyJsonArray => "legacy_json_array",
            RawDialect::Csv => "csv",
            RawDialect::PlainText => "txt",
        }
    }
}

#[async_trait]
pub trait DatasetDomain {
    async fn ingest_raw_dataset(
        &self,
        request: RawIngestRequest,
    ) -> Result<DatasetVersionDescriptor>;
    async fn get_manifest(
        &self,
        dataset_version_id: &str,
        tenant_id: &str,
    ) -> Result<Option<DatasetManifest>>;
    async fn stream_rows(
        &self,
        dataset_version_id: &str,
        tenant_id: &str,
        sampling: SamplingConfig,
    ) -> Result<Vec<CanonicalRow>>;
}

pub struct DatasetDomainService {
    state: Arc<AppState>,
}

impl DatasetDomainService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn dataset_paths(&self) -> DatasetPaths {
        DatasetPaths::new(resolve_dataset_root(&self.state))
    }

    fn resolve_version_dir(&self, dataset_id: &str, version_id: &str) -> PathBuf {
        self.dataset_paths()
            .dataset_dir(dataset_id)
            .join("versions")
            .join(version_id)
    }

    fn compute_row_id(row: &CanonicalRow) -> String {
        let meta = serde_json::to_vec(&row.metadata).unwrap_or_default();
        B3Hash::hash_multi(&[
            row.prompt.as_bytes(),
            row.response.as_bytes(),
            &meta,
            row.split.as_bytes(),
            &row.weight.to_be_bytes(),
        ])
        .to_hex()
    }

    fn canonicalize_object(
        value: &Map<String, Value>,
        default_split: &str,
        notes: &mut NormalizationNotes,
    ) -> Option<CanonicalRow> {
        let prompt = value
            .get("prompt")
            .or_else(|| value.get("input"))
            .or_else(|| value.get("question"))
            .or_else(|| value.get("text"))
            .and_then(|v| v.as_str())
            .map(str::to_string)?;

        let response = value
            .get("response")
            .or_else(|| value.get("output"))
            .or_else(|| value.get("answer"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if prompt.trim().is_empty() {
            notes
                .dropped_reasons
                .entry("empty_prompt".into())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            return None;
        }

        let weight = value
            .get("weight")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(1.0);

        let split = value
            .get("split")
            .and_then(|v| v.as_str())
            .unwrap_or(default_split)
            .to_string();

        let metadata = value
            .get("metadata")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut row = CanonicalRow {
            row_id: String::new(),
            split,
            prompt,
            response,
            weight,
            metadata,
        };
        row.row_id = Self::compute_row_id(&row);
        Some(row)
    }

    async fn normalize_file(
        desc: &RawFileDescriptor,
        writer: &mut BufWriter<fs::File>,
        notes: &mut NormalizationNotes,
        split_stats: &mut HashMap<String, SplitAccumulator>,
        dropped: &mut usize,
        kept: &mut usize,
    ) -> Result<()> {
        let format_tag = desc.format.as_tag().to_string();
        if !notes.dialects_seen.contains(&format_tag) {
            notes.dialects_seen.push(format_tag.clone());
        }

        match desc.format {
            RawDialect::CanonicalJsonl => {
                Self::normalize_jsonl(desc, writer, notes, split_stats, dropped, kept).await?
            }
            RawDialect::LegacyJsonArray => {
                Self::normalize_json_array(desc, writer, notes, split_stats, dropped, kept).await?
            }
            RawDialect::Csv => {
                Self::normalize_csv(desc, writer, notes, split_stats, dropped, kept).await?
            }
            RawDialect::PlainText => {
                Self::normalize_plain(desc, writer, notes, split_stats, dropped, kept).await?
            }
        }

        Ok(())
    }

    async fn normalize_jsonl(
        desc: &RawFileDescriptor,
        writer: &mut BufWriter<fs::File>,
        notes: &mut NormalizationNotes,
        split_stats: &mut HashMap<String, SplitAccumulator>,
        dropped: &mut usize,
        kept: &mut usize,
    ) -> Result<()> {
        let file = fs::File::open(&desc.path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    notes
                        .dropped_reasons
                        .entry(format!("parse_error:{}", e))
                        .and_modify(|c| *c += 1)
                        .or_insert(1);
                    *dropped += 1;
                    continue;
                }
            };

            let obj = match value.as_object() {
                Some(o) => o,
                None => {
                    notes
                        .dropped_reasons
                        .entry("non_object".into())
                        .and_modify(|c| *c += 1)
                        .or_insert(1);
                    *dropped += 1;
                    continue;
                }
            };

            if let Some(row) =
                Self::canonicalize_object(obj, desc.split.as_deref().unwrap_or("train"), notes)
            {
                Self::write_row(writer, &row).await?;
                Self::accumulate_split(split_stats, &row);
                *kept += 1;
            } else {
                *dropped += 1;
            }
        }
        Ok(())
    }

    async fn normalize_json_array(
        desc: &RawFileDescriptor,
        writer: &mut BufWriter<fs::File>,
        notes: &mut NormalizationNotes,
        split_stats: &mut HashMap<String, SplitAccumulator>,
        dropped: &mut usize,
        kept: &mut usize,
    ) -> Result<()> {
        let content = fs::read_to_string(&desc.path).await?;
        let values: Value = serde_json::from_str(&content).map_err(|e| {
            AosError::Validation(format!(
                "Failed to parse legacy JSON array {}: {}",
                desc.path.display(),
                e
            ))
        })?;
        let arr = values.as_array().ok_or_else(|| {
            AosError::Validation(format!(
                "Legacy JSON array expected array at {}",
                desc.path.display()
            ))
        })?;

        for value in arr {
            if let Some(obj) = value.as_object() {
                if let Some(row) =
                    Self::canonicalize_object(obj, desc.split.as_deref().unwrap_or("train"), notes)
                {
                    Self::write_row(writer, &row).await?;
                    Self::accumulate_split(split_stats, &row);
                    *kept += 1;
                } else {
                    *dropped += 1;
                }
            } else {
                notes
                    .dropped_reasons
                    .entry("non_object".into())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                *dropped += 1;
            }
        }

        Ok(())
    }

    async fn normalize_csv(
        desc: &RawFileDescriptor,
        writer: &mut BufWriter<fs::File>,
        notes: &mut NormalizationNotes,
        split_stats: &mut HashMap<String, SplitAccumulator>,
        dropped: &mut usize,
        kept: &mut usize,
    ) -> Result<()> {
        let content = fs::read_to_string(&desc.path).await?;
        for (idx, line) in content.lines().enumerate() {
            // Skip header if present; assume first line contains prompt if not header.
            if idx == 0 && line.contains("prompt") && line.contains("response") {
                continue;
            }
            let cols: Vec<&str> = line.split(',').collect();
            if cols.len() < 2 {
                notes
                    .dropped_reasons
                    .entry("csv_missing_columns".into())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                *dropped += 1;
                continue;
            }
            let prompt = cols[0].trim();
            let response = cols[1].trim();
            if prompt.is_empty() {
                notes
                    .dropped_reasons
                    .entry("empty_prompt".into())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                *dropped += 1;
                continue;
            }
            let weight = cols
                .get(2)
                .and_then(|c| c.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            let mut row = CanonicalRow {
                row_id: String::new(),
                split: desc.split.clone().unwrap_or_else(|| "train".into()),
                prompt: prompt.to_string(),
                response: response.to_string(),
                weight,
                metadata: Map::new(),
            };
            row.row_id = Self::compute_row_id(&row);
            Self::write_row(writer, &row).await?;
            Self::accumulate_split(split_stats, &row);
            *kept += 1;
        }
        Ok(())
    }

    async fn normalize_plain(
        desc: &RawFileDescriptor,
        writer: &mut BufWriter<fs::File>,
        notes: &mut NormalizationNotes,
        split_stats: &mut HashMap<String, SplitAccumulator>,
        dropped: &mut usize,
        kept: &mut usize,
    ) -> Result<()> {
        let content = fs::read_to_string(&desc.path).await?;
        for line in content.lines() {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let mut row = CanonicalRow {
                row_id: String::new(),
                split: desc.split.clone().unwrap_or_else(|| "train".into()),
                prompt: text.to_string(),
                response: String::new(),
                weight: 1.0,
                metadata: Map::new(),
            };
            if row.prompt.is_empty() {
                notes
                    .dropped_reasons
                    .entry("empty_prompt".into())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                *dropped += 1;
                continue;
            }
            row.row_id = Self::compute_row_id(&row);
            Self::write_row(writer, &row).await?;
            Self::accumulate_split(split_stats, &row);
            *kept += 1;
        }
        Ok(())
    }

    async fn write_row(writer: &mut BufWriter<fs::File>, row: &CanonicalRow) -> io::Result<()> {
        let line = serde_json::to_string(row).unwrap_or_else(|_| "{}".to_string());
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        Ok(())
    }

    fn accumulate_split(map: &mut HashMap<String, SplitAccumulator>, row: &CanonicalRow) {
        let entry = map
            .entry(row.split.clone())
            .or_insert_with(SplitAccumulator::default);
        entry.rows += 1;
        entry.prompt_chars += row.prompt.len() as u64;
        entry.response_chars += row.response.len() as u64;
    }

    fn finalize_manifest(
        dataset_id: &str,
        dataset_version_id: &str,
        hash_b3: &str,
        kept: usize,
        dropped: usize,
        notes: NormalizationNotes,
        split_stats: HashMap<String, SplitAccumulator>,
    ) -> DatasetManifest {
        let splits = split_stats
            .into_iter()
            .map(|(split, acc)| {
                let rows = acc.rows as f64;
                let stats = SplitStats {
                    rows: acc.rows as usize,
                    avg_prompt_chars: if rows > 0.0 {
                        acc.prompt_chars as f64 / rows
                    } else {
                        0.0
                    },
                    avg_response_chars: if rows > 0.0 {
                        acc.response_chars as f64 / rows
                    } else {
                        0.0
                    },
                };
                (split, stats)
            })
            .collect();

        DatasetManifest {
            dataset_id: dataset_id.to_string(),
            dataset_version_id: dataset_version_id.to_string(),
            hash_b3: hash_b3.to_string(),
            total_rows: kept,
            dropped_rows: dropped,
            splits,
            normalization: notes,
        }
    }

    async fn write_manifest(manifest_path: &Path, manifest: &DatasetManifest) -> Result<()> {
        let manifest_json = serde_json::to_vec_pretty(manifest)
            .map_err(|e| AosError::Internal(format!("Failed to serialize manifest: {}", e)))?;
        fs::write(manifest_path, manifest_json).await?;
        Ok(())
    }

    async fn load_version_for_tenant(
        &self,
        dataset_version_id: &str,
        tenant_id: &str,
    ) -> Result<TrainingDatasetVersion> {
        let version = self
            .state
            .db
            .get_training_dataset_version_for_tenant(dataset_version_id, tenant_id)
            .await?
            .ok_or_else(|| AosError::NotFound("Dataset version not found".into()))?;
        Ok(version)
    }
}

#[async_trait]
impl DatasetDomain for DatasetDomainService {
    async fn ingest_raw_dataset(
        &self,
        request: RawIngestRequest,
    ) -> Result<DatasetVersionDescriptor> {
        if request.files.is_empty() {
            return Err(AosError::Validation(
                "At least one file is required for ingestion".into(),
            ));
        }

        // Ensure dataset exists and is tenant bound.
        let dataset: TrainingDataset = self
            .state
            .db
            .get_training_dataset(&request.dataset_id)
            .await?
            .ok_or_else(|| AosError::NotFound("Dataset not found".into()))?;
        if let Some(ds_tenant) = dataset.tenant_id {
            if ds_tenant != request.tenant_id {
                return Err(AosError::Authz(
                    "Dataset belongs to different tenant".into(),
                ));
            }
        }

        let version_id = Uuid::now_v7().to_string();
        let version_dir = self.resolve_version_dir(&request.dataset_id, &version_id);
        ensure_dirs([version_dir.as_path()])
            .await
            .map_err(|(_, json)| AosError::Io(json.0.error.clone()))?;

        let storage = FsByteStorage::new(
            resolve_dataset_root(&self.state),
            {
                let cfg = self.state.config.read().expect("Config lock poisoned");
                cfg.paths.adapters_root.clone()
            }
            .into(),
        );

        let canonical_key = StorageKey {
            tenant_id: Some(request.tenant_id.clone()),
            object_id: request.dataset_id.clone(),
            version_id: Some(version_id.clone()),
            file_name: CANONICAL_FILENAME.to_string(),
            kind: StorageKind::DatasetFile,
        };
        let canonical_path = storage.path_for(&canonical_key)?;
        let canonical_file = fs::File::create(&canonical_path).await?;
        let mut writer = BufWriter::new(canonical_file);

        let mut notes = NormalizationNotes::default();
        let mut split_stats: HashMap<String, SplitAccumulator> = HashMap::new();
        let mut dropped = 0usize;
        let mut kept = 0usize;

        for desc in &request.files {
            Self::normalize_file(
                desc,
                &mut writer,
                &mut notes,
                &mut split_stats,
                &mut dropped,
                &mut kept,
            )
            .await?;
        }

        writer.flush().await?;

        let hash = B3Hash::hash_file(&canonical_path)
            .map_err(|e| AosError::Internal(format!("Failed to hash dataset: {}", e)))?
            .to_hex();

        let manifest = Self::finalize_manifest(
            &request.dataset_id,
            &version_id,
            &hash,
            kept,
            dropped,
            notes,
            split_stats,
        );

        let manifest_key = StorageKey {
            tenant_id: Some(request.tenant_id.clone()),
            object_id: request.dataset_id.clone(),
            version_id: Some(version_id.clone()),
            file_name: MANIFEST_FILENAME.to_string(),
            kind: StorageKind::DatasetFile,
        };
        let manifest_path = storage.path_for(&manifest_key)?;
        Self::write_manifest(&manifest_path, &manifest).await?;

        // Persist dataset version
        self.state
            .db
            .create_training_dataset_version_with_id(
                &version_id,
                &request.dataset_id,
                Some(&request.tenant_id),
                request.version_label.as_deref(),
                canonical_path
                    .to_str()
                    .ok_or_else(|| AosError::Internal("Invalid canonical path".into()))?,
                &hash,
                manifest_path.to_str(),
                Some(&serde_json::to_string(&manifest).map_err(|e| {
                    AosError::Internal(format!("Failed to serialize manifest json: {}", e))
                })?),
                request.created_by.as_deref(),
            )
            .await?;

        info!(
            dataset_id = %request.dataset_id,
            dataset_version_id = %version_id,
            rows = kept,
            dropped = dropped,
            hash = %hash,
            "Normalized dataset version"
        );

        Ok(DatasetVersionDescriptor {
            dataset_id: request.dataset_id.clone(),
            dataset_version_id: version_id,
            storage_path: canonical_path.to_string_lossy().to_string(),
            hash_b3: hash,
            manifest,
        })
    }

    async fn get_manifest(
        &self,
        dataset_version_id: &str,
        tenant_id: &str,
    ) -> Result<Option<DatasetManifest>> {
        let version = self
            .load_version_for_tenant(dataset_version_id, tenant_id)
            .await?;

        if let Some(manifest_json) = self
            .state
            .db
            .get_dataset_version_manifest(dataset_version_id)
            .await?
        {
            let manifest: DatasetManifest = serde_json::from_str(&manifest_json)
                .map_err(|e| AosError::Internal(format!("Failed to parse manifest json: {}", e)))?;
            return Ok(Some(manifest));
        }

        if let Some(path) = version.manifest_path {
            let content = fs::read_to_string(&path).await.map_err(|e| {
                AosError::Internal(format!("Failed to read manifest {}: {}", path, e))
            })?;
            let manifest: DatasetManifest = serde_json::from_str(&content)
                .map_err(|e| AosError::Internal(format!("Failed to parse manifest json: {}", e)))?;
            return Ok(Some(manifest));
        }

        Ok(None)
    }

    async fn stream_rows(
        &self,
        dataset_version_id: &str,
        tenant_id: &str,
        sampling: SamplingConfig,
    ) -> Result<Vec<CanonicalRow>> {
        let version = self
            .load_version_for_tenant(dataset_version_id, tenant_id)
            .await?;

        let path = Path::new(&version.storage_path);
        let file = fs::File::open(path).await.map_err(|e| {
            AosError::NotFound(format!(
                "Canonical dataset file not found {}: {}",
                path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);
        let mut rows: Vec<CanonicalRow> = Vec::new();
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let row: CanonicalRow = serde_json::from_str(trimmed).map_err(|e| {
                AosError::Validation(format!("Failed to parse canonical row: {}", e))
            })?;
            if let Some(ref target_split) = sampling.split {
                if &row.split != target_split {
                    continue;
                }
            }
            rows.push(row);
        }

        if let Some(seed) = sampling.shuffle_seed {
            rows.sort_by_key(|row| {
                blake3::hash(format!("{}:{}", seed, row.row_id).as_bytes())
                    .to_hex()
                    .to_string()
            });
        }

        Ok(rows)
    }
}

#[derive(Default, Clone)]
struct SplitAccumulator {
    rows: u64,
    prompt_chars: u64,
    response_chars: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mk_row(prompt: &str, response: &str) -> CanonicalRow {
        let mut row = CanonicalRow {
            row_id: String::new(),
            split: "train".into(),
            prompt: prompt.into(),
            response: response.into(),
            weight: 1.0,
            metadata: Map::new(),
        };
        row.row_id = DatasetDomainService::compute_row_id(&row);
        row
    }

    #[tokio::test]
    async fn canonicalizes_jsonl_and_builds_manifest() {
        let tmp = tempdir().unwrap();
        let jsonl_path = tmp.path().join("data.jsonl");
        let content = r#"
{"prompt":"p1","response":"r1"}
{"prompt":"p2","response":"r2","split":"eval"}
{"input":"legacy","output":"resp"}
"#;
        fs::write(&jsonl_path, content).await.unwrap();

        let mut notes = NormalizationNotes::default();
        let mut split_stats = HashMap::new();
        let mut dropped = 0usize;
        let mut kept = 0usize;
        let dest_path = tmp.path().join("canonical.jsonl");
        let file = fs::File::create(&dest_path).await.unwrap();
        let mut writer = BufWriter::new(file);

        let desc = RawFileDescriptor {
            path: jsonl_path,
            format: RawDialect::CanonicalJsonl,
            split: None,
        };
        DatasetDomainService::normalize_file(
            &desc,
            &mut writer,
            &mut notes,
            &mut split_stats,
            &mut dropped,
            &mut kept,
        )
        .await
        .unwrap();
        writer.flush().await.unwrap();

        let data = fs::read_to_string(&dest_path).await.unwrap();
        let lines: Vec<&str> = data.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(kept, 3);
        assert_eq!(dropped, 0);
        assert!(notes.dialects_seen.contains(&"jsonl".to_string()));

        let manifest = DatasetDomainService::finalize_manifest(
            "ds",
            "v1",
            "hash",
            kept,
            dropped,
            notes.clone(),
            split_stats.clone(),
        );
        assert_eq!(manifest.total_rows, 3);
        assert_eq!(manifest.dropped_rows, 0);
        assert!(manifest.splits.contains_key("train"));
    }

    #[tokio::test]
    async fn deterministic_shuffle_by_seed() {
        let rows = vec![mk_row("p1", "r1"), mk_row("p2", "r2"), mk_row("p3", "r3")];

        let mut shuffled_a = rows.clone();
        shuffled_a.sort_by_key(|row| {
            blake3::hash(format!("seed:{}", row.row_id).as_bytes())
                .to_hex()
                .to_string()
        });

        let mut shuffled_b = rows.clone();
        shuffled_b.sort_by_key(|row| {
            blake3::hash(format!("seed:{}", row.row_id).as_bytes())
                .to_hex()
                .to_string()
        });

        assert_eq!(shuffled_a, shuffled_b);
    }
}
