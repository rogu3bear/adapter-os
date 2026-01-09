//! Training pipeline state machine with phase receipts and resume guards.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_worker::training::trainer::TrainingResult;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs;
use tracing::{info, warn};

use super::job::{DataLineageMode, DatasetVersionSelection, TrainingConfig};

const PIPELINE_SCHEMA_VERSION: u32 = 1;
const PIPELINE_RECEIPT_VERSION: u32 = 1;
const EMPTY_HASH: &str = "";
const OUTPUT_DATASET_HASH: &str = "dataset_content_hash";
const OUTPUT_PREPROCESS_HASH: &str = "preprocess_hash";
const OUTPUT_SPLIT_HASH: &str = "split_hash";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePhase {
    DatasetBuild,
    Preprocess,
    Split,
    TrainingLoop,
    ValidationEarlyStopping,
    Packaging,
    Complete,
}

impl PipelinePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelinePhase::DatasetBuild => "dataset_build",
            PipelinePhase::Preprocess => "preprocess",
            PipelinePhase::Split => "train_validation_split",
            PipelinePhase::TrainingLoop => "training_loop",
            PipelinePhase::ValidationEarlyStopping => "validation_early_stopping",
            PipelinePhase::Packaging => "packaging",
            PipelinePhase::Complete => "complete",
        }
    }

    fn next(&self) -> Option<Self> {
        match self {
            PipelinePhase::DatasetBuild => Some(PipelinePhase::Preprocess),
            PipelinePhase::Preprocess => Some(PipelinePhase::Split),
            PipelinePhase::Split => Some(PipelinePhase::TrainingLoop),
            PipelinePhase::TrainingLoop => Some(PipelinePhase::ValidationEarlyStopping),
            PipelinePhase::ValidationEarlyStopping => Some(PipelinePhase::Packaging),
            PipelinePhase::Packaging => Some(PipelinePhase::Complete),
            PipelinePhase::Complete => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pending,
    InProgress,
    Completed,
    Skipped,
}

impl PhaseStatus {
    fn as_str(&self) -> &'static str {
        match self {
            PhaseStatus::Pending => "pending",
            PhaseStatus::InProgress => "in_progress",
            PhaseStatus::Completed => "completed",
            PhaseStatus::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseReceipt {
    pub phase: PipelinePhase,
    pub status: PhaseStatus,
    pub started_at: String,
    pub completed_at: String,
    #[serde(default)]
    pub started_at_unix_ms: u64,
    #[serde(default)]
    pub completed_at_unix_ms: u64,
    #[serde(default)]
    pub phase_id: String,
    #[serde(default)]
    pub inputs: HashMap<String, String>,
    #[serde(default)]
    pub outputs: HashMap<String, String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStatusV1 {
    pub phase: PipelinePhase,
    pub status: PhaseStatus,
    pub phase_id: String,
    pub inputs_hash: String,
    pub outputs_hash: String,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReceiptV1 {
    pub pipeline_id: String,
    pub contract_version: u32,
    #[serde(default)]
    pub training_contract_version: Option<String>,
    pub dataset_id: String,
    pub dataset_content_hash: String,
    pub preprocess_id: Option<String>,
    pub preprocess_hash: Option<String>,
    pub split_hash: String,
    pub training_config_hash: String,
    pub base_model_hash: String,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub phase_statuses: Vec<PhaseStatusV1>,
}

impl PipelineReceiptV1 {
    pub fn new(
        dataset_id: Option<&str>,
        training_contract_version: Option<&str>,
        started_at_unix_ms: u64,
    ) -> Self {
        Self {
            pipeline_id: String::new(),
            contract_version: PIPELINE_RECEIPT_VERSION,
            training_contract_version: training_contract_version.map(|value| value.to_string()),
            dataset_id: dataset_id.unwrap_or("").to_string(),
            dataset_content_hash: String::new(),
            preprocess_id: None,
            preprocess_hash: None,
            split_hash: String::new(),
            training_config_hash: String::new(),
            base_model_hash: String::new(),
            started_at_unix_ms,
            finished_at_unix_ms: None,
            phase_statuses: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineConfigSnapshot {
    pub training_config: TrainingConfig,
    pub dataset_id: Option<String>,
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    pub data_spec_hash: Option<String>,
    pub synthetic_mode: bool,
    pub data_lineage_mode: DataLineageMode,
    pub base_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub schema_version: u32,
    pub job_id: String,
    pub created_at: String,
    pub config_fingerprint: String,
    pub config_snapshot: PipelineConfigSnapshot,
    #[serde(default)]
    pub pipeline_id: Option<String>,
    pub current_phase: PipelinePhase,
    pub current_status: PhaseStatus,
    pub current_started_at: Option<String>,
    #[serde(default)]
    pub current_started_at_unix_ms: Option<u64>,
    pub receipts: Vec<PhaseReceipt>,
}

#[derive(Debug, Clone)]
pub struct PipelinePaths {
    pub root_dir: PathBuf,
    pub state_path: PathBuf,
    pub receipts_dir: PathBuf,
    pub receipt_path: PathBuf,
    pub training_result_path: PathBuf,
}

impl PipelinePaths {
    pub fn for_job(storage_root: Option<&Path>, job_id: &str) -> Self {
        let root_dir = storage_root
            .map(|root| root.join("pipelines"))
            .unwrap_or_else(|| PathBuf::from("var/training_pipeline"));
        let job_root = root_dir.join(job_id);
        let state_path = job_root.join("pipeline_state.json");
        let receipts_dir = job_root.join("receipts");
        let receipt_path = job_root.join("pipeline_receipt.json");
        let training_result_path = job_root.join("training_result.json");
        Self {
            root_dir,
            state_path,
            receipts_dir,
            receipt_path,
            training_result_path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineEventContext {
    job_id: String,
    pipeline_id: Option<String>,
}

impl PipelineEventContext {
    pub fn emit_phase_progress(&self, phase: PipelinePhase, progress_pct: f32, metadata: Option<Value>) {
        let pipeline_id = self.pipeline_id.as_deref().unwrap_or("");
        tracing::event!(
            tracing::Level::INFO,
            name = "training_pipeline_phase_progress",
            event_type = "phase_progress",
            job_id = %self.job_id,
            pipeline_id = %pipeline_id,
            phase = phase.as_str(),
            progress_pct = progress_pct,
            metadata = ?metadata,
            "Training pipeline phase progress"
        );
    }

    pub fn emit_phase_error(&self, phase: PipelinePhase, error: &str) {
        let pipeline_id = self.pipeline_id.as_deref().unwrap_or("");
        tracing::event!(
            tracing::Level::ERROR,
            name = "training_pipeline_phase_error",
            event_type = "phase_error",
            job_id = %self.job_id,
            pipeline_id = %pipeline_id,
            phase = phase.as_str(),
            error = %error,
            "Training pipeline phase error"
        );
    }
}

pub struct TrainingPipeline {
    state: PipelineState,
    paths: PipelinePaths,
    receipt: PipelineReceiptV1,
}

impl TrainingPipeline {
    pub async fn load_or_init(
        job_id: &str,
        config_snapshot: PipelineConfigSnapshot,
        storage_root: Option<&Path>,
    ) -> Result<Self> {
        let paths = PipelinePaths::for_job(storage_root, job_id);
        let (mut state, mut receipt) = if fs::metadata(&paths.state_path).await.is_ok() {
            let contents = fs::read_to_string(&paths.state_path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to read pipeline state {}: {}",
                    paths.state_path.display(),
                    e
                ))
            })?;
            let state: PipelineState = serde_json::from_str(&contents).map_err(|e| {
                AosError::Training(format!(
                    "Failed to parse pipeline state {}: {}",
                    paths.state_path.display(),
                    e
                ))
            })?;

            if state.schema_version != PIPELINE_SCHEMA_VERSION {
                return Err(AosError::Validation(format!(
                    "Pipeline schema version mismatch: expected {}, got {}",
                    PIPELINE_SCHEMA_VERSION, state.schema_version
                )));
            }

            let fingerprint = compute_config_fingerprint(&config_snapshot)?;
            if state.config_fingerprint != fingerprint || state.config_snapshot != config_snapshot {
                warn!(
                    job_id = %state.job_id,
                    "Training pipeline config snapshot changed on resume; preserving original snapshot"
                );
            }

            let receipt = load_pipeline_receipt(&paths, &state).await?;
            (state, receipt)
        } else {
            let created_at = chrono::Utc::now().to_rfc3339();
            // Extract values before moving config_snapshot
            let dataset_id_for_receipt = config_snapshot.dataset_id.clone();
            let contract_version_for_receipt = config_snapshot.training_config.training_contract_version.clone();
            let state = PipelineState {
                schema_version: PIPELINE_SCHEMA_VERSION,
                job_id: job_id.to_string(),
                created_at: created_at.clone(),
                config_fingerprint: compute_config_fingerprint(&config_snapshot)?,
                config_snapshot,
                pipeline_id: None,
                current_phase: PipelinePhase::DatasetBuild,
                current_status: PhaseStatus::Pending,
                current_started_at: None,
                current_started_at_unix_ms: None,
                receipts: Vec::new(),
            };
            let receipt = PipelineReceiptV1::new(
                dataset_id_for_receipt.as_deref(),
                Some(contract_version_for_receipt.as_str()),
                parse_rfc3339_to_unix_ms(&created_at).unwrap_or_else(now_unix_ms),
            );
            (state, receipt)
        };

        if receipt.contract_version != PIPELINE_RECEIPT_VERSION {
            return Err(AosError::Validation(format!(
                "Pipeline receipt version mismatch: expected {}, got {}",
                PIPELINE_RECEIPT_VERSION, receipt.contract_version
            )));
        }

        let expected_contract_version = state.config_snapshot.training_config.training_contract_version.clone();
        match receipt.training_contract_version.as_ref() {
            Some(version) if version != &expected_contract_version => {
                return Err(AosError::Validation(format!(
                    "Pipeline receipt training contract version mismatch: expected {}, got {}",
                    expected_contract_version, version
                )));
            }
            Some(_) => {}
            None => {
                receipt.training_contract_version = Some(expected_contract_version);
            }
        }

        if state.pipeline_id.is_none() && !receipt.pipeline_id.is_empty() {
            state.pipeline_id = Some(receipt.pipeline_id.clone());
        }
        if receipt.pipeline_id.is_empty() {
            if let Some(pipeline_id) = state.pipeline_id.clone() {
                receipt.pipeline_id = pipeline_id;
            }
        }

        let pipeline = Self {
            state,
            paths,
            receipt,
        };
        pipeline.persist_state().await?;
        pipeline.persist_pipeline_receipt().await?;
        Ok(pipeline)
    }

    pub fn current_phase(&self) -> PipelinePhase {
        self.state.current_phase
    }

    pub fn is_complete(&self) -> bool {
        self.state.current_phase == PipelinePhase::Complete
    }

    pub fn receipt(&self, phase: PipelinePhase) -> Option<&PhaseReceipt> {
        self.state
            .receipts
            .iter()
            .rev()
            .find(|receipt| receipt.phase == phase)
    }

    pub fn pipeline_id(&self) -> Option<&str> {
        self.state.pipeline_id.as_deref()
    }

    pub fn receipt_v1(&self) -> &PipelineReceiptV1 {
        &self.receipt
    }

    pub fn event_context(&self) -> PipelineEventContext {
        PipelineEventContext {
            job_id: self.state.job_id.clone(),
            pipeline_id: self.state.pipeline_id.clone(),
        }
    }

    pub async fn seed_receipt(
        &mut self,
        training_config_hash: &str,
        base_model_hash: &str,
        dataset_id: Option<&str>,
        training_contract_version: &str,
    ) -> Result<()> {
        let mut changed = false;
        if self.receipt.training_config_hash.is_empty() && !training_config_hash.is_empty() {
            self.receipt.training_config_hash = training_config_hash.to_string();
            changed = true;
        }
        if self.receipt.base_model_hash.is_empty() && !base_model_hash.is_empty() {
            self.receipt.base_model_hash = base_model_hash.to_string();
            changed = true;
        }
        if self.receipt.dataset_id.is_empty() {
            if let Some(dataset_id) = dataset_id {
                if !dataset_id.is_empty() {
                    self.receipt.dataset_id = dataset_id.to_string();
                    changed = true;
                }
            }
        }
        if self
            .receipt
            .training_contract_version
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            if !training_contract_version.is_empty() {
                self.receipt.training_contract_version =
                    Some(training_contract_version.to_string());
                changed = true;
            }
        }
        if self.receipt.pipeline_id.is_empty()
            && !self.receipt.dataset_content_hash.is_empty()
            && !self.receipt.training_config_hash.is_empty()
            && !self.receipt.base_model_hash.is_empty()
        {
            let pipeline_id = compute_pipeline_id(
                &self.receipt.dataset_content_hash,
                &self.receipt.training_config_hash,
                &self.receipt.base_model_hash,
            );
            self.receipt.pipeline_id = pipeline_id.clone();
            self.state.pipeline_id = Some(pipeline_id);
            changed = true;
        }
        if changed {
            self.persist_pipeline_receipt().await?;
            if self.state.pipeline_id.is_some() {
                self.persist_state().await?;
            }
        }
        Ok(())
    }

    pub fn assert_resume_compatible(
        &self,
        dataset_content_hash: &str,
        split_hash: &str,
        base_model_hash: &str,
        training_config_hash: &str,
        training_contract_version: &str,
        force_resume: bool,
    ) -> Result<()> {
        if self.receipt.contract_version != PIPELINE_RECEIPT_VERSION {
            return Err(AosError::Validation(format!(
                "Pipeline receipt contract version mismatch: expected {}, got {}",
                PIPELINE_RECEIPT_VERSION, self.receipt.contract_version
            )));
        }

        let mut mismatches = Vec::new();
        if !self.receipt.dataset_content_hash.is_empty()
            && self.receipt.dataset_content_hash != dataset_content_hash
        {
            mismatches.push(format!(
                "dataset_content_hash: receipt={} current={}",
                self.receipt.dataset_content_hash, dataset_content_hash
            ));
        }
        if !self.receipt.split_hash.is_empty() && self.receipt.split_hash != split_hash {
            mismatches.push(format!(
                "split_hash: receipt={} current={}",
                self.receipt.split_hash, split_hash
            ));
        }
        if !self.receipt.base_model_hash.is_empty() && self.receipt.base_model_hash != base_model_hash
        {
            mismatches.push(format!(
                "base_model_hash: receipt={} current={}",
                self.receipt.base_model_hash, base_model_hash
            ));
        }
        if !self.receipt.training_config_hash.is_empty()
            && self.receipt.training_config_hash != training_config_hash
        {
            mismatches.push(format!(
                "training_config_hash: receipt={} current={}",
                self.receipt.training_config_hash, training_config_hash
            ));
        }
        if let Some(ref version) = self.receipt.training_contract_version {
            if version != training_contract_version {
                mismatches.push(format!(
                    "training_contract_version: receipt={} current={}",
                    version, training_contract_version
                ));
            }
        }

        if mismatches.is_empty() {
            return Ok(());
        }

        if force_resume {
            tracing::event!(
                tracing::Level::ERROR,
                name = "training_pipeline_resume_forced",
                event_type = "phase_error",
                job_id = %self.state.job_id,
                pipeline_id = %self.state.pipeline_id.clone().unwrap_or_default(),
                mismatches = ?mismatches,
                "Force resume enabled; proceeding despite pipeline receipt mismatches"
            );
            Ok(())
        } else {
            Err(AosError::Validation(format!(
                "Cannot resume pipeline: receipt mismatch [{}]. Use --force-resume to override (may produce incorrect results).",
                mismatches.join(", ")
            )))
        }
    }

    pub async fn enter_phase(&mut self, phase: PipelinePhase) -> Result<()> {
        self.ensure_phase(phase)?;

        match self.state.current_status {
            PhaseStatus::Pending => {
                self.state.current_status = PhaseStatus::InProgress;
                self.state.current_started_at = Some(chrono::Utc::now().to_rfc3339());
                self.state.current_started_at_unix_ms = Some(now_unix_ms());
                self.persist_state().await?;
                self.emit_phase_start(phase, false);
            }
            PhaseStatus::InProgress => {
                self.emit_phase_start(phase, true);
            }
            PhaseStatus::Completed | PhaseStatus::Skipped => {
                return Err(AosError::Validation(format!(
                    "Phase {} already completed",
                    phase.as_str()
                )));
            }
        }

        Ok(())
    }

    pub async fn complete_phase(
        &mut self,
        phase: PipelinePhase,
        status: PhaseStatus,
        inputs: HashMap<String, String>,
        outputs: HashMap<String, String>,
        metadata: Value,
    ) -> Result<()> {
        self.ensure_phase(phase)?;

        if !matches!(status, PhaseStatus::Completed | PhaseStatus::Skipped) {
            return Err(AosError::Validation(format!(
                "Invalid phase completion status: {}",
                status.as_str()
            )));
        }
        if self.state.current_status != PhaseStatus::InProgress {
            return Err(AosError::Validation(format!(
                "Phase {} is not in progress",
                phase.as_str()
            )));
        }

        let started_at = self
            .state
            .current_started_at
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let started_at_unix_ms = self
            .state
            .current_started_at_unix_ms
            .unwrap_or_else(now_unix_ms);
        let completed_at = chrono::Utc::now().to_rfc3339();
        let completed_at_unix_ms = now_unix_ms();
        self.maybe_seed_pipeline_id(phase, &outputs)?;
        let inputs_hash = hash_kv_map(&inputs);
        let outputs_hash = hash_kv_map(&outputs);
        let phase_id = compute_phase_id(
            self.state
                .pipeline_id
                .as_deref()
                .unwrap_or(EMPTY_HASH),
            phase,
            &inputs_hash,
            &outputs_hash,
        );
        let receipt = PhaseReceipt {
            phase,
            status,
            started_at,
            completed_at,
            started_at_unix_ms,
            completed_at_unix_ms,
            phase_id: phase_id.clone(),
            inputs,
            outputs,
            metadata,
        };
        self.state.receipts.push(receipt.clone());
        self.persist_receipt(&receipt).await?;
        self.update_pipeline_receipt(phase, &receipt, &inputs_hash, &outputs_hash)?;
        self.persist_pipeline_receipt().await?;

        self.advance_phase();
        self.persist_state().await?;

        self.emit_phase_end(phase, status, &phase_id);

        Ok(())
    }

    fn emit_phase_start(&self, phase: PipelinePhase, resumed: bool) {
        let pipeline_id = self.state.pipeline_id.as_deref().unwrap_or("");
        tracing::event!(
            tracing::Level::INFO,
            name = "training_pipeline_phase_start",
            event_type = "phase_start",
            job_id = %self.state.job_id,
            pipeline_id = %pipeline_id,
            phase = phase.as_str(),
            status = %self.state.current_status.as_str(),
            resumed = resumed,
            "Training pipeline phase start"
        );
    }

    fn emit_phase_end(&self, phase: PipelinePhase, status: PhaseStatus, phase_id: &str) {
        let pipeline_id = self.state.pipeline_id.as_deref().unwrap_or("");
        tracing::event!(
            tracing::Level::INFO,
            name = "training_pipeline_phase_end",
            event_type = "phase_end",
            job_id = %self.state.job_id,
            pipeline_id = %pipeline_id,
            phase = phase.as_str(),
            phase_id = %phase_id,
            status = %status.as_str(),
            "Training pipeline phase end"
        );
    }

    fn maybe_seed_pipeline_id(
        &mut self,
        phase: PipelinePhase,
        outputs: &HashMap<String, String>,
    ) -> Result<()> {
        if self.state.pipeline_id.is_some() {
            return Ok(());
        }
        if phase != PipelinePhase::DatasetBuild {
            return Ok(());
        }
        let Some(dataset_hash) = outputs.get(OUTPUT_DATASET_HASH) else {
            return Ok(());
        };
        if self.receipt.training_config_hash.is_empty() || self.receipt.base_model_hash.is_empty() {
            warn!(
                job_id = %self.state.job_id,
                "Pipeline ID seed missing training_config_hash or base_model_hash"
            );
            return Ok(());
        }
        let pipeline_id =
            compute_pipeline_id(dataset_hash, &self.receipt.training_config_hash, &self.receipt.base_model_hash);
        self.state.pipeline_id = Some(pipeline_id.clone());
        self.receipt.pipeline_id = pipeline_id;
        Ok(())
    }

    fn update_pipeline_receipt(
        &mut self,
        phase: PipelinePhase,
        receipt: &PhaseReceipt,
        inputs_hash: &str,
        outputs_hash: &str,
    ) -> Result<()> {
        let phase_status = PhaseStatusV1 {
            phase,
            status: receipt.status,
            phase_id: receipt.phase_id.clone(),
            inputs_hash: inputs_hash.to_string(),
            outputs_hash: outputs_hash.to_string(),
            started_at_unix_ms: receipt.started_at_unix_ms,
            finished_at_unix_ms: Some(receipt.completed_at_unix_ms),
        };
        self.receipt
            .phase_statuses
            .retain(|entry| entry.phase != phase);
        self.receipt.phase_statuses.push(phase_status);

        match phase {
            PipelinePhase::DatasetBuild => {
                if let Some(dataset_hash) = receipt.outputs.get(OUTPUT_DATASET_HASH) {
                    self.receipt.dataset_content_hash = dataset_hash.clone();
                }
                if self.receipt.dataset_id.is_empty() {
                    if let Some(dataset_id) = receipt.inputs.get("dataset_id") {
                        if !dataset_id.is_empty() {
                            self.receipt.dataset_id = dataset_id.clone();
                        }
                    }
                }
            }
            PipelinePhase::Preprocess => {
                if receipt.status == PhaseStatus::Completed {
                    self.receipt.preprocess_id = Some(receipt.phase_id.clone());
                    if let Some(preprocess_hash) = receipt.outputs.get(OUTPUT_PREPROCESS_HASH) {
                        self.receipt.preprocess_hash = Some(preprocess_hash.clone());
                    }
                } else {
                    self.receipt.preprocess_id = None;
                    self.receipt.preprocess_hash = None;
                }
            }
            PipelinePhase::Split => {
                if let Some(split_hash) = receipt.outputs.get(OUTPUT_SPLIT_HASH) {
                    self.receipt.split_hash = split_hash.clone();
                }
            }
            PipelinePhase::Packaging => {
                if self.receipt.finished_at_unix_ms.is_none() {
                    self.receipt.finished_at_unix_ms = Some(receipt.completed_at_unix_ms);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn ensure_phase(&self, phase: PipelinePhase) -> Result<()> {
        if self.state.current_phase != phase {
            return Err(AosError::Validation(format!(
                "Invalid pipeline transition: expected {}, got {}",
                self.state.current_phase.as_str(),
                phase.as_str()
            )));
        }
        Ok(())
    }

    fn advance_phase(&mut self) {
        self.state.current_phase = self.state.current_phase.next().unwrap_or(PipelinePhase::Complete);
        self.state.current_status = PhaseStatus::Pending;
        self.state.current_started_at = None;
        self.state.current_started_at_unix_ms = None;
    }

    async fn persist_state(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.state).map_err(|e| {
            AosError::Training(format!("Failed to serialize pipeline state: {}", e))
        })?;
        write_atomic(&self.paths.state_path, &json).await?;
        info!(
            path = %self.paths.state_path.display(),
            phase = %self.state.current_phase.as_str(),
            status = %self.state.current_status.as_str(),
            pipeline_id = ?self.state.pipeline_id,
            "Training pipeline state persisted"
        );
        Ok(())
    }

    async fn persist_pipeline_receipt(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.receipt).map_err(|e| {
            AosError::Training(format!("Failed to serialize pipeline receipt: {}", e))
        })?;
        write_atomic(&self.paths.receipt_path, &json).await?;
        Ok(())
    }

    async fn persist_receipt(&self, receipt: &PhaseReceipt) -> Result<()> {
        let json = serde_json::to_string_pretty(receipt).map_err(|e| {
            AosError::Training(format!("Failed to serialize phase receipt: {}", e))
        })?;
        let path = self
            .paths
            .receipts_dir
            .join(format!("{}.json", receipt.phase.as_str()));
        if let Err(e) = write_atomic(&path, &json).await {
            warn!(path = %path.display(), error = %e, "Failed to persist phase receipt");
        }
        Ok(())
    }

    pub async fn persist_training_result(&self, training_result: &TrainingResult) -> Result<String> {
        let bytes = serde_json::to_vec(training_result).map_err(|e| {
            AosError::Training(format!("Failed to serialize training result: {}", e))
        })?;
        let hash = B3Hash::hash(&bytes).to_hex().to_string();
        write_atomic_bytes(&self.paths.training_result_path, &bytes).await?;
        Ok(hash)
    }

    pub async fn load_training_result(&self) -> Result<Option<TrainingResult>> {
        if fs::metadata(&self.paths.training_result_path).await.is_err() {
            return Ok(None);
        }
        let bytes = fs::read(&self.paths.training_result_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read training result {}: {}",
                self.paths.training_result_path.display(),
                e
            ))
        })?;
        let training_result: TrainingResult = serde_json::from_slice(&bytes).map_err(|e| {
            AosError::Training(format!(
                "Failed to parse training result {}: {}",
                self.paths.training_result_path.display(),
                e
            ))
        })?;
        Ok(Some(training_result))
    }
}

async fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create pipeline directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, contents).await.map_err(|e| {
        AosError::Io(format!(
            "Failed to write pipeline temp file {}: {}",
            tmp_path.display(),
            e
        ))
    })?;

    if let Err(e) = fs::rename(&tmp_path, path).await {
        let _ = fs::remove_file(&tmp_path).await;
        return Err(AosError::Io(format!(
            "Failed to rename pipeline state file {}: {}",
            path.display(),
            e
        )));
    }

    Ok(())
}

async fn write_atomic_bytes(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create pipeline directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, contents).await.map_err(|e| {
        AosError::Io(format!(
            "Failed to write pipeline temp file {}: {}",
            tmp_path.display(),
            e
        ))
    })?;

    if let Err(e) = fs::rename(&tmp_path, path).await {
        let _ = fs::remove_file(&tmp_path).await;
        return Err(AosError::Io(format!(
            "Failed to rename pipeline state file {}: {}",
            path.display(),
            e
        )));
    }

    Ok(())
}

fn compute_config_fingerprint(snapshot: &PipelineConfigSnapshot) -> Result<String> {
    let bytes = serde_json::to_vec(snapshot).map_err(|e| {
        AosError::Training(format!("Failed to serialize pipeline config snapshot: {}", e))
    })?;
    Ok(B3Hash::hash(&bytes).to_hex().to_string())
}

async fn load_pipeline_receipt(
    paths: &PipelinePaths,
    state: &PipelineState,
) -> Result<PipelineReceiptV1> {
    if fs::metadata(&paths.receipt_path).await.is_ok() {
        let contents = fs::read_to_string(&paths.receipt_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read pipeline receipt {}: {}",
                paths.receipt_path.display(),
                e
            ))
        })?;
        let receipt: PipelineReceiptV1 = serde_json::from_str(&contents).map_err(|e| {
            AosError::Training(format!(
                "Failed to parse pipeline receipt {}: {}",
                paths.receipt_path.display(),
                e
            ))
        })?;
        Ok(receipt)
    } else {
        Ok(PipelineReceiptV1::new(
            state.config_snapshot.dataset_id.as_deref(),
            Some(state.config_snapshot.training_config.training_contract_version.as_str()),
            parse_rfc3339_to_unix_ms(&state.created_at).unwrap_or_else(now_unix_ms),
        ))
    }
}

fn hash_kv_map(map: &HashMap<String, String>) -> String {
    let mut ordered = BTreeMap::new();
    for (key, value) in map {
        ordered.insert(key, value);
    }
    match serde_json::to_vec(&ordered) {
        Ok(bytes) => B3Hash::hash(&bytes).to_hex().to_string(),
        Err(err) => {
            warn!(error = %err, "Failed to serialize phase inputs/outputs for hashing");
            String::new()
        }
    }
}

fn compute_phase_id(pipeline_id: &str, phase: PipelinePhase, inputs_hash: &str, outputs_hash: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"training_pipeline_phase_v1");
    hasher.update(pipeline_id.as_bytes());
    hasher.update(phase.as_str().as_bytes());
    hasher.update(inputs_hash.as_bytes());
    hasher.update(outputs_hash.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn compute_pipeline_id(dataset_hash: &str, training_config_hash: &str, base_model_hash: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"training_pipeline_v1");
    hasher.update(dataset_hash.as_bytes());
    hasher.update(training_config_hash.as_bytes());
    hasher.update(base_model_hash.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn now_unix_ms() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}

fn parse_rfc3339_to_unix_ms(value: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|ts| ts.timestamp_millis() as u64)
}
