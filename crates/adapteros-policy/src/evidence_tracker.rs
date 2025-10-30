//! Evidence tracker for model provenance, router decisions, and kernel audits

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

/// Evidence record for deterministic audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    /// Timestamp (nanoseconds since epoch)
    pub timestamp: u128,
    /// Model load provenance
    pub model_provenance: ModelProvenance,
    /// Quantization manifest hash (if int4)
    pub quantization_hash: Option<B3Hash>,
    /// Active LoRA adapters
    pub active_loras: Vec<String>,
    /// Router scores (Q15 format)
    pub router_scores_q15: Vec<i16>,
    /// Kernel tolerance check results
    pub kernel_tolerance: Vec<KernelToleranceCheck>,
    /// Deterministic seed/config hash
    pub seed_hash: B3Hash,
    /// Custom metadata
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Model provenance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvenance {
    pub model_id: String,
    pub model_path: String,
    pub model_hash: B3Hash,
    pub load_timestamp: u128,
}

/// Kernel tolerance check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelToleranceCheck {
    pub kernel_name: String,
    pub max_error: f32,
    pub mean_error: f32,
    pub passed: bool,
    pub input_checksum: B3Hash,
    pub output_checksum: B3Hash,
}

/// Evidence tracker for append-only evidence logging
pub struct EvidenceTracker {
    /// Append-only evidence log
    evidence: Arc<std::sync::RwLock<Vec<EvidenceRecord>>>,
    /// Output sink (structured log or DB)
    sink: EvidenceSink,
}

enum EvidenceSink {
    Log(tracing::Span),
    Database(adapteros_db::Db),
    File(std::path::PathBuf),
}

impl EvidenceTracker {
    /// Create a new evidence tracker with log sink
    pub fn new_log() -> Self {
        Self {
            evidence: Arc::new(std::sync::RwLock::new(Vec::new())),
            sink: EvidenceSink::Log(tracing::Span::current()),
        }
    }

    /// Record evidence (append-only)
    pub async fn record(&self, evidence: EvidenceRecord) -> Result<()> {
        let mut ev = self.evidence.write()
            .map_err(|_| AosError::Internal("Failed to acquire write lock on evidence tracker".to_string()))?;
        ev.push(evidence.clone());

        // Write to sink
        match &self.sink {
            EvidenceSink::Log(_) => {
                tracing::info!(
                    evidence = ?serde_json::to_value(&evidence)?,
                    "Evidence recorded"
                );
            }
            EvidenceSink::Database(db) => {
                let id = Uuid::now_v7().to_string();
                let tenant_id = "default"; // TODO: Get from context when available
                
                let active_loras_json = serde_json::to_string(&evidence.active_loras)
                    .map_err(|e| AosError::Parse(format!("Failed to serialize active_loras: {}", e)))?;
                let router_scores_json = serde_json::to_string(&evidence.router_scores_q15)
                    .map_err(|e| AosError::Parse(format!("Failed to serialize router_scores: {}", e)))?;
                let kernel_tolerance_json = serde_json::to_string(&evidence.kernel_tolerance)
                    .map_err(|e| AosError::Parse(format!("Failed to serialize kernel_tolerance: {}", e)))?;
                let metadata_json = serde_json::to_string(&evidence.metadata)
                    .map_err(|e| AosError::Parse(format!("Failed to serialize metadata: {}", e)))?;
                
                sqlx::query(
                    "INSERT INTO policy_evidence (
                        id, tenant_id, timestamp, model_id, model_path, model_hash,
                        model_load_timestamp, quantization_hash, active_loras_json,
                        router_scores_q15_json, kernel_tolerance_json, seed_hash, metadata_json
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(&id)
                .bind(tenant_id)
                .bind(evidence.timestamp as i64)
                .bind(&evidence.model_provenance.model_id)
                .bind(&evidence.model_provenance.model_path)
                .bind(evidence.model_provenance.model_hash.to_string())
                .bind(evidence.model_provenance.load_timestamp as i64)
                .bind(evidence.quantization_hash.map(|h| h.to_string()))
                .bind(&active_loras_json)
                .bind(&router_scores_json)
                .bind(&kernel_tolerance_json)
                .bind(evidence.seed_hash.to_string())
                .bind(&metadata_json)
                .execute(db.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to insert evidence: {}", e)))?;
            }
            EvidenceSink::File(path) => {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .map_err(|e| AosError::Io(format!("Failed to open evidence file: {}", e)))?;
                let json = serde_json::to_string(&evidence)?;
                writeln!(f, "{}", json)
                    .map_err(|e| AosError::Io(format!("Failed to write evidence: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Get all evidence records
    pub fn get_all(&self) -> Vec<EvidenceRecord> {
        match self.evidence.read() {
            Ok(guard) => guard.iter().cloned().collect(),
            Err(_) => Vec::new(), // Poisoned lock - return empty
        }
    }

    /// Get evidence records for a time range
    pub fn get_range(&self, start: u128, end: u128) -> Vec<EvidenceRecord> {
        match self.evidence.read() {
            Ok(guard) => guard
                .iter()
                .filter(|e| e.timestamp >= start && e.timestamp <= end)
                .cloned()
                .collect(),
            Err(_) => Vec::new(), // Poisoned lock - return empty
        }
    }
}

/// Helper to create evidence record from runtime state
pub fn create_evidence_record(
    model_id: &str,
    model_path: &str,
    model_hash: B3Hash,
    quantization_hash: Option<B3Hash>,
    active_loras: Vec<String>,
    router_scores_q15: Vec<i16>,
    kernel_checks: Vec<KernelToleranceCheck>,
    seed: u64,
    config: &[u8],
) -> EvidenceRecord {
    let seed_hash = {
        let mut bytes = seed.to_le_bytes().to_vec();
        bytes.extend_from_slice(config);
        B3Hash::hash(&bytes)
    };

    EvidenceRecord {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        model_provenance: ModelProvenance {
            model_id: model_id.to_string(),
            model_path: model_path.to_string(),
            model_hash,
            load_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        },
        quantization_hash,
        active_loras,
        router_scores_q15,
        kernel_tolerance: kernel_checks,
        seed_hash,
        metadata: BTreeMap::new(),
    }
}

