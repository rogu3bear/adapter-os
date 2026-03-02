//! Evidence tracker for model provenance, router decisions, and kernel audits

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::BTreeMap;
use std::sync::Arc;
use adapteros_id::{TypedId, IdPrefix};

/// Builder for creating evidence records
#[derive(Debug, Default)]
pub struct EvidenceRecordBuilder {
    model_id: Option<String>,
    model_path: Option<String>,
    model_hash: Option<B3Hash>,
    quantization_hash: Option<B3Hash>,
    active_loras: Option<Vec<String>>,
    router_scores_q15: Option<Vec<i16>>,
    kernel_checks: Option<Vec<KernelToleranceCheck>>,
    seed: Option<u64>,
    config: Option<Vec<u8>>,
}

/// Parameters for evidence record creation
#[derive(Debug)]
pub struct EvidenceRecordParams {
    pub model_id: String,
    pub model_path: String,
    pub model_hash: B3Hash,
    pub quantization_hash: Option<B3Hash>,
    pub active_loras: Vec<String>,
    pub router_scores_q15: Vec<i16>,
    pub kernel_checks: Vec<KernelToleranceCheck>,
    pub seed: u64,
    pub config: Vec<u8>,
}

impl EvidenceRecordBuilder {
    /// Create a new evidence record builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model ID (required)
    pub fn model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = Some(model_id.into());
        self
    }

    /// Set the model path (required)
    pub fn model_path(mut self, model_path: impl Into<String>) -> Self {
        self.model_path = Some(model_path.into());
        self
    }

    /// Set the model hash (required)
    pub fn model_hash(mut self, model_hash: B3Hash) -> Self {
        self.model_hash = Some(model_hash);
        self
    }

    /// Set the quantization hash (optional)
    pub fn quantization_hash(mut self, quantization_hash: Option<B3Hash>) -> Self {
        self.quantization_hash = quantization_hash;
        self
    }

    /// Set the active LoRAs (required)
    pub fn active_loras(mut self, active_loras: Vec<String>) -> Self {
        self.active_loras = Some(active_loras);
        self
    }

    /// Set the router scores (Q15 format, required)
    pub fn router_scores_q15(mut self, router_scores_q15: Vec<i16>) -> Self {
        self.router_scores_q15 = Some(router_scores_q15);
        self
    }

    /// Set the kernel checks (required)
    pub fn kernel_checks(mut self, kernel_checks: Vec<KernelToleranceCheck>) -> Self {
        self.kernel_checks = Some(kernel_checks);
        self
    }

    /// Set the deterministic seed (required)
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Set the configuration bytes (required)
    pub fn config(mut self, config: Vec<u8>) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the evidence record parameters
    pub fn build(self) -> Result<EvidenceRecordParams> {
        Ok(EvidenceRecordParams {
            model_id: self
                .model_id
                .ok_or_else(|| AosError::Policy("model_id is required".into()))?,
            model_path: self
                .model_path
                .ok_or_else(|| AosError::Policy("model_path is required".into()))?,
            model_hash: self
                .model_hash
                .ok_or_else(|| AosError::Policy("model_hash is required".into()))?,
            quantization_hash: self.quantization_hash,
            active_loras: self
                .active_loras
                .ok_or_else(|| AosError::Policy("active_loras is required".into()))?,
            router_scores_q15: self
                .router_scores_q15
                .ok_or_else(|| AosError::Policy("router_scores_q15 is required".into()))?,
            kernel_checks: self
                .kernel_checks
                .ok_or_else(|| AosError::Policy("kernel_checks is required".into()))?,
            seed: self
                .seed
                .ok_or_else(|| AosError::Policy("seed is required".into()))?,
            config: self
                .config
                .ok_or_else(|| AosError::Policy("config is required".into()))?,
        })
    }
}

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

/// Evidence output sink
///
/// Currently only Log sink is implemented. Database and File sinks are reserved
/// for future persistent evidence storage and offline export capabilities.
#[allow(dead_code)] // Database and File variants reserved for future implementation
enum EvidenceSink {
    /// Log to tracing span
    Log(tracing::Span),
    /// Store in database (reserved for persistent evidence storage)
    Database(adapteros_db::Db),
    /// Write to file (reserved for offline evidence export)
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
    ///
    /// # Arguments
    /// * `evidence` - The evidence record to store
    /// * `tenant_id` - Optional tenant ID for database storage (defaults to "default")
    pub async fn record(&self, evidence: EvidenceRecord) -> Result<()> {
        self.record_with_tenant(evidence, None).await
    }

    /// Record evidence with explicit tenant ID
    pub async fn record_with_tenant(&self, evidence: EvidenceRecord, tenant_id: Option<&str>) -> Result<()> {
        // Add evidence to in-memory store
        {
            let mut ev = self.evidence.write().map_err(|_| {
                AosError::Internal("Failed to acquire write lock on evidence tracker".to_string())
            })?;
            ev.push(evidence.clone());
        } // Lock released here

        // Write to sink (lock no longer held)
        match &self.sink {
            EvidenceSink::Log(_) => {
                tracing::info!(
                    evidence = ?serde_json::to_value(&evidence)?,
                    "Evidence recorded"
                );
            }
            EvidenceSink::Database(db) => {
                let id = TypedId::new(IdPrefix::Evt).to_string();
                let tenant_id = tenant_id.unwrap_or("default");

                let active_loras_json =
                    serde_json::to_string(&evidence.active_loras).map_err(|e| {
                        AosError::Parse(format!("Failed to serialize active_loras: {}", e))
                    })?;
                let router_scores_json = serde_json::to_string(&evidence.router_scores_q15)
                    .map_err(|e| {
                        AosError::Parse(format!("Failed to serialize router_scores: {}", e))
                    })?;
                let kernel_tolerance_json = serde_json::to_string(&evidence.kernel_tolerance)
                    .map_err(|e| {
                        AosError::Parse(format!("Failed to serialize kernel_tolerance: {}", e))
                    })?;
                let metadata_json = serde_json::to_string(&evidence.metadata)
                    .map_err(|e| AosError::Parse(format!("Failed to serialize metadata: {}", e)))?;

                sqlx::query(
                    "INSERT INTO policy_evidence (
                        id, tenant_id, timestamp, model_id, model_path, model_hash,
                        model_load_timestamp, quantization_hash, active_loras_json,
                        router_scores_q15_json, kernel_tolerance_json, seed_hash, metadata_json
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
///
/// Use [`EvidenceRecordBuilder`] to construct evidence record parameters:
/// ```no_run
/// use adapteros_policy::evidence_tracker::{
///     create_evidence_record_from_params, EvidenceRecordBuilder, KernelToleranceCheck,
/// };
/// use adapteros_core::B3Hash;
///
/// let model_hash = B3Hash::hash(b"model");
/// let quant_hash = Some(B3Hash::hash(b"quant"));
/// let kernel_check = KernelToleranceCheck {
///     kernel_name: "attention".to_string(),
///     max_error: 0.0001,
///     mean_error: 0.0,
///     passed: true,
///     input_checksum: B3Hash::hash(b"in"),
///     output_checksum: B3Hash::hash(b"out"),
/// };
/// let config_bytes = vec![0; 16];
///
/// let params = EvidenceRecordBuilder::new()
///     .model_id("model-123")
///     .model_path("/path/to/model")
///     .model_hash(model_hash)
///     .quantization_hash(quant_hash)
///     .active_loras(vec!["lora1".to_string(), "lora2".to_string()])
///     .router_scores_q15(vec![16384, 8192])
///     .kernel_checks(vec![kernel_check])
///     .seed(42)
///     .config(config_bytes)
///     .build()
///     .expect("builder validation");
/// let record = create_evidence_record_from_params(params);
/// assert_eq!(record.router_scores_q15.len(), 2);
/// ```
pub fn create_evidence_record_from_params(params: EvidenceRecordParams) -> EvidenceRecord {
    let seed_hash = {
        let mut bytes = params.seed.to_le_bytes().to_vec();
        bytes.extend_from_slice(&params.config);
        B3Hash::hash(&bytes)
    };

    EvidenceRecord {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        model_provenance: ModelProvenance {
            model_id: params.model_id,
            model_path: params.model_path,
            model_hash: params.model_hash,
            load_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        },
        quantization_hash: params.quantization_hash,
        active_loras: params.active_loras,
        router_scores_q15: params.router_scores_q15,
        kernel_tolerance: params.kernel_checks,
        seed_hash,
        metadata: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kernel() -> KernelToleranceCheck {
        KernelToleranceCheck {
            kernel_name: "attention".to_string(),
            max_error: 0.0001,
            mean_error: 0.0,
            passed: true,
            input_checksum: B3Hash::hash(b"in"),
            output_checksum: B3Hash::hash(b"out"),
        }
    }

    #[test]
    fn builder_populates_all_fields() {
        let params = EvidenceRecordBuilder::new()
            .model_id("model-123")
            .model_path("/path/model")
            .model_hash(B3Hash::hash(b"model"))
            .quantization_hash(Some(B3Hash::hash(b"quant")))
            .active_loras(vec!["lora-a".into(), "lora-b".into()])
            .router_scores_q15(vec![1, 2, 3])
            .kernel_checks(vec![sample_kernel()])
            .seed(7)
            .config(vec![1, 2, 3])
            .build()
            .expect("builder fills required fields");

        assert_eq!(params.model_id, "model-123");
        assert_eq!(params.active_loras.len(), 2);
        assert_eq!(params.router_scores_q15, vec![1, 2, 3]);
        assert!(params.quantization_hash.is_some());
    }

    #[test]
    fn create_record_hashes_seed_and_config() {
        let config = vec![10, 20, 30];
        let seed = 42u64;
        let params = EvidenceRecordBuilder::new()
            .model_id("model")
            .model_path("/model/path")
            .model_hash(B3Hash::hash(b"model"))
            .quantization_hash(None)
            .active_loras(vec![])
            .router_scores_q15(vec![])
            .kernel_checks(vec![sample_kernel()])
            .seed(seed)
            .config(config.clone())
            .build()
            .expect("builder fills required fields");

        let record = create_evidence_record_from_params(params);
        let mut expected = seed.to_le_bytes().to_vec();
        expected.extend_from_slice(&config);
        assert_eq!(record.seed_hash.to_hex(), B3Hash::hash(&expected).to_hex());
    }

    #[test]
    fn builder_requires_model_id() {
        let err = EvidenceRecordBuilder::new()
            .model_path("/path")
            .model_hash(B3Hash::hash(b"model"))
            .active_loras(vec![])
            .router_scores_q15(vec![])
            .kernel_checks(vec![sample_kernel()])
            .seed(1)
            .config(vec![])
            .build()
            .expect_err("missing model_id should error");

        assert!(
            err.to_string().contains("model_id is required"),
            "unexpected error: {}",
            err
        );
    }
}
