//! Inference verdict database operations.
//!
//! Provides storage and retrieval for verdict evaluations on inference traces.
//! Verdicts track confidence levels from rule-based, human, or model evaluators.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Verdict confidence level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    High,
    Medium,
    Low,
    Paused,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::High => write!(f, "high"),
            Verdict::Medium => write!(f, "medium"),
            Verdict::Low => write!(f, "low"),
            Verdict::Paused => write!(f, "paused"),
        }
    }
}

impl std::str::FromStr for Verdict {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "high" => Ok(Verdict::High),
            "medium" => Ok(Verdict::Medium),
            "low" => Ok(Verdict::Low),
            "paused" => Ok(Verdict::Paused),
            _ => Err(AosError::Validation(format!("Invalid verdict: {}", s))),
        }
    }
}

/// Evaluator type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EvaluatorType {
    Rule,
    Human,
    Model,
}

impl std::fmt::Display for EvaluatorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluatorType::Rule => write!(f, "rule"),
            EvaluatorType::Human => write!(f, "human"),
            EvaluatorType::Model => write!(f, "model"),
        }
    }
}

impl std::str::FromStr for EvaluatorType {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "rule" => Ok(EvaluatorType::Rule),
            "human" => Ok(EvaluatorType::Human),
            "model" => Ok(EvaluatorType::Model),
            _ => Err(AosError::Validation(format!(
                "Invalid evaluator type: {}",
                s
            ))),
        }
    }
}

/// Database row for inference verdict
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InferenceVerdict {
    pub id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub created_at: String,
    pub verdict: String,
    pub confidence: f64,
    pub evaluator_type: String,
    pub evaluator_id: Option<String>,
    pub warnings_digest_b3: Option<String>,
    pub warnings_json: Option<String>,
    pub extraction_confidence_score: Option<f64>,
    pub trust_state: Option<String>,
}

impl InferenceVerdict {
    /// Parse the verdict string into a Verdict enum
    pub fn verdict_enum(&self) -> Result<Verdict> {
        self.verdict.parse()
    }

    /// Parse the evaluator_type string into an EvaluatorType enum
    pub fn evaluator_type_enum(&self) -> Result<EvaluatorType> {
        self.evaluator_type.parse()
    }

    /// Parse warnings_json into a structured list
    pub fn warnings(&self) -> Result<Option<Vec<String>>> {
        self.warnings_json
            .as_ref()
            .map(|json| {
                serde_json::from_str(json)
                    .map_err(|e| AosError::Validation(format!("Failed to parse warnings: {}", e)))
            })
            .transpose()
    }
}

/// Parameters for creating a verdict
#[derive(Debug, Clone)]
pub struct CreateVerdictParams {
    pub id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub verdict: Verdict,
    pub confidence: f64,
    pub evaluator_type: EvaluatorType,
    pub evaluator_id: Option<String>,
    pub warnings_digest_b3: Option<String>,
    pub warnings_json: Option<String>,
    pub extraction_confidence_score: Option<f64>,
    pub trust_state: Option<String>,
}

impl CreateVerdictParams {
    /// Create new verdict params with required fields
    pub fn new(
        tenant_id: impl Into<String>,
        inference_id: impl Into<String>,
        verdict: Verdict,
        confidence: f64,
        evaluator_type: EvaluatorType,
    ) -> Self {
        Self {
            id: crate::new_id(adapteros_id::IdPrefix::Dec),
            tenant_id: tenant_id.into(),
            inference_id: inference_id.into(),
            verdict,
            confidence,
            evaluator_type,
            evaluator_id: None,
            warnings_digest_b3: None,
            warnings_json: None,
            extraction_confidence_score: None,
            trust_state: None,
        }
    }

    /// Set evaluator ID (user ID for human, rule ID for rule, model ID for model)
    pub fn with_evaluator_id(mut self, evaluator_id: impl Into<String>) -> Self {
        self.evaluator_id = Some(evaluator_id.into());
        self
    }

    /// Set warnings digest (BLAKE3 hash)
    pub fn with_warnings_digest(mut self, digest: impl Into<String>) -> Self {
        self.warnings_digest_b3 = Some(digest.into());
        self
    }

    /// Set warnings JSON (detailed warnings list)
    pub fn with_warnings_json(mut self, json: impl Into<String>) -> Self {
        self.warnings_json = Some(json.into());
        self
    }

    /// Set extraction confidence score
    pub fn with_extraction_confidence(mut self, score: f64) -> Self {
        self.extraction_confidence_score = Some(score);
        self
    }

    /// Set trust state
    pub fn with_trust_state(mut self, state: impl Into<String>) -> Self {
        self.trust_state = Some(state.into());
        self
    }
}

/// Create a new inference verdict
///
/// Uses INSERT OR REPLACE to handle the unique constraint on (inference_id, evaluator_type).
/// This means only the latest verdict per inference+evaluator is retained.
pub async fn create_verdict(pool: &SqlitePool, params: &CreateVerdictParams) -> Result<String> {
    // Validate confidence range
    if !(0.0..=1.0).contains(&params.confidence) {
        return Err(AosError::Validation(format!(
            "Confidence must be between 0.0 and 1.0, got {}",
            params.confidence
        )));
    }

    sqlx::query(
        r#"
        INSERT OR REPLACE INTO inference_verdicts (
            id, tenant_id, inference_id, verdict, confidence,
            evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
            extraction_confidence_score, trust_state
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&params.id)
    .bind(&params.tenant_id)
    .bind(&params.inference_id)
    .bind(params.verdict.to_string())
    .bind(params.confidence)
    .bind(params.evaluator_type.to_string())
    .bind(&params.evaluator_id)
    .bind(&params.warnings_digest_b3)
    .bind(&params.warnings_json)
    .bind(params.extraction_confidence_score)
    .bind(&params.trust_state)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("create_verdict: {}", e)))?;

    Ok(params.id.clone())
}

/// Get a verdict by inference ID with tenant isolation
///
/// Returns the most recent verdict for the inference if it exists.
pub async fn get_verdict_by_inference(
    pool: &SqlitePool,
    tenant_id: &str,
    inference_id: &str,
) -> Result<Option<InferenceVerdict>> {
    let row = sqlx::query_as::<_, InferenceVerdict>(
        r#"
        SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
               evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
               extraction_confidence_score, trust_state
        FROM inference_verdicts
        WHERE tenant_id = ? AND inference_id = ?
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(inference_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_verdict_by_inference: {}", e)))?;

    Ok(row)
}

/// Get verdicts by inference ID and evaluator type with tenant isolation
///
/// Returns the verdict for a specific evaluator type, respecting the unique index.
pub async fn get_verdict_by_inference_and_evaluator(
    pool: &SqlitePool,
    tenant_id: &str,
    inference_id: &str,
    evaluator_type: EvaluatorType,
) -> Result<Option<InferenceVerdict>> {
    let row = sqlx::query_as::<_, InferenceVerdict>(
        r#"
        SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
               evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
               extraction_confidence_score, trust_state
        FROM inference_verdicts
        WHERE tenant_id = ? AND inference_id = ? AND evaluator_type = ?
        "#,
    )
    .bind(tenant_id)
    .bind(inference_id)
    .bind(evaluator_type.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_verdict_by_inference_and_evaluator: {}", e)))?;

    Ok(row)
}

/// Get the latest verdict for an inference (respecting unique index on inference_id + evaluator_type)
///
/// Returns the most recent verdict across all evaluator types.
pub async fn get_latest_verdict(
    pool: &SqlitePool,
    tenant_id: &str,
    inference_id: &str,
) -> Result<Option<InferenceVerdict>> {
    let row = sqlx::query_as::<_, InferenceVerdict>(
        r#"
        SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
               evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
               extraction_confidence_score, trust_state
        FROM inference_verdicts
        WHERE tenant_id = ? AND inference_id = ?
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(inference_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_latest_verdict: {}", e)))?;

    Ok(row)
}

/// List all verdicts for an inference (one per evaluator type due to unique index)
pub async fn list_verdicts_by_inference(
    pool: &SqlitePool,
    tenant_id: &str,
    inference_id: &str,
) -> Result<Vec<InferenceVerdict>> {
    let rows = sqlx::query_as::<_, InferenceVerdict>(
        r#"
        SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
               evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
               extraction_confidence_score, trust_state
        FROM inference_verdicts
        WHERE tenant_id = ? AND inference_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant_id)
    .bind(inference_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("list_verdicts_by_inference: {}", e)))?;

    Ok(rows)
}

/// List verdicts for a tenant with optional filtering and pagination
pub async fn list_verdicts_by_tenant(
    pool: &SqlitePool,
    tenant_id: &str,
    verdict_filter: Option<Verdict>,
    evaluator_type_filter: Option<EvaluatorType>,
    limit: u32,
    offset: u32,
) -> Result<Vec<InferenceVerdict>> {
    let limit = limit.min(1000) as i64;
    let offset = offset as i64;

    let rows = match (verdict_filter, evaluator_type_filter) {
        (Some(verdict), Some(evaluator)) => {
            sqlx::query_as::<_, InferenceVerdict>(
                r#"
                SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
                       evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
                       extraction_confidence_score, trust_state
                FROM inference_verdicts
                WHERE tenant_id = ? AND verdict = ? AND evaluator_type = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(verdict.to_string())
            .bind(evaluator.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        }
        (Some(verdict), None) => {
            sqlx::query_as::<_, InferenceVerdict>(
                r#"
                SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
                       evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
                       extraction_confidence_score, trust_state
                FROM inference_verdicts
                WHERE tenant_id = ? AND verdict = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(verdict.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        }
        (None, Some(evaluator)) => {
            sqlx::query_as::<_, InferenceVerdict>(
                r#"
                SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
                       evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
                       extraction_confidence_score, trust_state
                FROM inference_verdicts
                WHERE tenant_id = ? AND evaluator_type = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(evaluator.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        }
        (None, None) => {
            sqlx::query_as::<_, InferenceVerdict>(
                r#"
                SELECT id, tenant_id, inference_id, created_at, verdict, confidence,
                       evaluator_type, evaluator_id, warnings_digest_b3, warnings_json,
                       extraction_confidence_score, trust_state
                FROM inference_verdicts
                WHERE tenant_id = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
        }
    }
    .map_err(|e| AosError::Database(format!("list_verdicts_by_tenant: {}", e)))?;

    Ok(rows)
}

/// Count verdicts for a tenant with optional filtering
pub async fn count_verdicts_by_tenant(
    pool: &SqlitePool,
    tenant_id: &str,
    verdict_filter: Option<Verdict>,
    evaluator_type_filter: Option<EvaluatorType>,
) -> Result<i64> {
    let (count,): (i64,) = match (verdict_filter, evaluator_type_filter) {
        (Some(verdict), Some(evaluator)) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM inference_verdicts
                WHERE tenant_id = ? AND verdict = ? AND evaluator_type = ?
                "#,
            )
            .bind(tenant_id)
            .bind(verdict.to_string())
            .bind(evaluator.to_string())
            .fetch_one(pool)
            .await
        }
        (Some(verdict), None) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM inference_verdicts
                WHERE tenant_id = ? AND verdict = ?
                "#,
            )
            .bind(tenant_id)
            .bind(verdict.to_string())
            .fetch_one(pool)
            .await
        }
        (None, Some(evaluator)) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM inference_verdicts
                WHERE tenant_id = ? AND evaluator_type = ?
                "#,
            )
            .bind(tenant_id)
            .bind(evaluator.to_string())
            .fetch_one(pool)
            .await
        }
        (None, None) => {
            sqlx::query_as(
                r#"
                SELECT COUNT(*) FROM inference_verdicts
                WHERE tenant_id = ?
                "#,
            )
            .bind(tenant_id)
            .fetch_one(pool)
            .await
        }
    }
    .map_err(|e| AosError::Database(format!("count_verdicts_by_tenant: {}", e)))?;

    Ok(count)
}

/// Delete a verdict by ID with tenant isolation
pub async fn delete_verdict(pool: &SqlitePool, tenant_id: &str, verdict_id: &str) -> Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM inference_verdicts
        WHERE tenant_id = ? AND id = ?
        "#,
    )
    .bind(tenant_id)
    .bind(verdict_id)
    .execute(pool)
    .await
    .map_err(|e| AosError::Database(format!("delete_verdict: {}", e)))?;

    Ok(result.rows_affected() > 0)
}

/// Summary statistics for verdicts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictSummary {
    pub total: i64,
    pub high: i64,
    pub medium: i64,
    pub low: i64,
    pub paused: i64,
    pub by_rule: i64,
    pub by_human: i64,
    pub by_model: i64,
    pub avg_confidence: f64,
}

/// Get summary statistics for verdicts by tenant
pub async fn get_verdict_summary(pool: &SqlitePool, tenant_id: &str) -> Result<VerdictSummary> {
    let total = count_verdicts_by_tenant(pool, tenant_id, None, None).await?;

    let high = count_verdicts_by_tenant(pool, tenant_id, Some(Verdict::High), None).await?;
    let medium = count_verdicts_by_tenant(pool, tenant_id, Some(Verdict::Medium), None).await?;
    let low = count_verdicts_by_tenant(pool, tenant_id, Some(Verdict::Low), None).await?;
    let paused = count_verdicts_by_tenant(pool, tenant_id, Some(Verdict::Paused), None).await?;

    let by_rule =
        count_verdicts_by_tenant(pool, tenant_id, None, Some(EvaluatorType::Rule)).await?;
    let by_human =
        count_verdicts_by_tenant(pool, tenant_id, None, Some(EvaluatorType::Human)).await?;
    let by_model =
        count_verdicts_by_tenant(pool, tenant_id, None, Some(EvaluatorType::Model)).await?;

    // Get average confidence
    let (avg_confidence,): (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(AVG(confidence), 0.0) FROM inference_verdicts
        WHERE tenant_id = ?
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AosError::Database(format!("get_verdict_summary avg: {}", e)))?;

    Ok(VerdictSummary {
        total,
        high,
        medium,
        low,
        paused,
        by_rule,
        by_human,
        by_model,
        avg_confidence,
    })
}

// ============================================================================
// Db impl methods for Db struct integration
// ============================================================================

impl Db {
    /// Create a new inference verdict
    pub async fn create_inference_verdict(&self, params: &CreateVerdictParams) -> Result<String> {
        create_verdict(self.pool(), params).await
    }

    /// Get verdict by inference ID
    pub async fn get_inference_verdict_by_inference(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Result<Option<InferenceVerdict>> {
        get_verdict_by_inference(self.pool(), tenant_id, inference_id).await
    }

    /// Get verdict by inference ID and evaluator type
    pub async fn get_inference_verdict_by_evaluator(
        &self,
        tenant_id: &str,
        inference_id: &str,
        evaluator_type: EvaluatorType,
    ) -> Result<Option<InferenceVerdict>> {
        get_verdict_by_inference_and_evaluator(self.pool(), tenant_id, inference_id, evaluator_type)
            .await
    }

    /// Get latest verdict for an inference
    pub async fn get_latest_inference_verdict(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Result<Option<InferenceVerdict>> {
        get_latest_verdict(self.pool(), tenant_id, inference_id).await
    }

    /// List all verdicts for an inference
    pub async fn list_inference_verdicts_by_inference(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Result<Vec<InferenceVerdict>> {
        list_verdicts_by_inference(self.pool(), tenant_id, inference_id).await
    }

    /// List verdicts by tenant with filtering
    pub async fn list_inference_verdicts_by_tenant(
        &self,
        tenant_id: &str,
        verdict_filter: Option<Verdict>,
        evaluator_type_filter: Option<EvaluatorType>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<InferenceVerdict>> {
        list_verdicts_by_tenant(
            self.pool(),
            tenant_id,
            verdict_filter,
            evaluator_type_filter,
            limit,
            offset,
        )
        .await
    }

    /// Get verdict summary statistics
    pub async fn get_inference_verdict_summary(&self, tenant_id: &str) -> Result<VerdictSummary> {
        get_verdict_summary(self.pool(), tenant_id).await
    }

    /// Delete an inference verdict
    pub async fn delete_inference_verdict(
        &self,
        tenant_id: &str,
        verdict_id: &str,
    ) -> Result<bool> {
        delete_verdict(self.pool(), tenant_id, verdict_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verdict_display() {
        assert_eq!(Verdict::High.to_string(), "high");
        assert_eq!(Verdict::Medium.to_string(), "medium");
        assert_eq!(Verdict::Low.to_string(), "low");
        assert_eq!(Verdict::Paused.to_string(), "paused");
    }

    #[test]
    fn test_verdict_parse() {
        assert_eq!("high".parse::<Verdict>().unwrap(), Verdict::High);
        assert_eq!("medium".parse::<Verdict>().unwrap(), Verdict::Medium);
        assert_eq!("low".parse::<Verdict>().unwrap(), Verdict::Low);
        assert_eq!("paused".parse::<Verdict>().unwrap(), Verdict::Paused);
        assert!("invalid".parse::<Verdict>().is_err());
    }

    #[test]
    fn test_evaluator_type_display() {
        assert_eq!(EvaluatorType::Rule.to_string(), "rule");
        assert_eq!(EvaluatorType::Human.to_string(), "human");
        assert_eq!(EvaluatorType::Model.to_string(), "model");
    }

    #[test]
    fn test_evaluator_type_parse() {
        assert_eq!(
            "rule".parse::<EvaluatorType>().unwrap(),
            EvaluatorType::Rule
        );
        assert_eq!(
            "human".parse::<EvaluatorType>().unwrap(),
            EvaluatorType::Human
        );
        assert_eq!(
            "model".parse::<EvaluatorType>().unwrap(),
            EvaluatorType::Model
        );
        assert!("invalid".parse::<EvaluatorType>().is_err());
    }

    #[test]
    fn test_create_verdict_params_builder() {
        let params = CreateVerdictParams::new(
            "tenant-1",
            "inference-1",
            Verdict::High,
            0.95,
            EvaluatorType::Rule,
        )
        .with_evaluator_id("rule-001")
        .with_warnings_json(r#"["warning1"]"#)
        .with_extraction_confidence(0.87)
        .with_trust_state("trusted");

        assert_eq!(params.tenant_id, "tenant-1");
        assert_eq!(params.inference_id, "inference-1");
        assert_eq!(params.verdict, Verdict::High);
        assert_eq!(params.confidence, 0.95);
        assert_eq!(params.evaluator_type, EvaluatorType::Rule);
        assert_eq!(params.evaluator_id, Some("rule-001".to_string()));
        assert_eq!(params.warnings_json, Some(r#"["warning1"]"#.to_string()));
        assert_eq!(params.extraction_confidence_score, Some(0.87));
        assert_eq!(params.trust_state, Some("trusted".to_string()));
    }

    #[tokio::test]
    async fn test_create_and_get_verdict() {
        // Create in-memory test database
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        // Create the table
        sqlx::query(
            r#"
            CREATE TABLE inference_verdicts (
                id TEXT PRIMARY KEY NOT NULL,
                tenant_id TEXT NOT NULL,
                inference_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                verdict TEXT NOT NULL CHECK (verdict IN ('high', 'medium', 'low', 'paused')),
                confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
                evaluator_type TEXT NOT NULL CHECK (evaluator_type IN ('rule', 'human', 'model')),
                evaluator_id TEXT,
                warnings_digest_b3 TEXT,
                warnings_json TEXT,
                extraction_confidence_score REAL,
                trust_state TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        // Create unique index
        sqlx::query(
            r#"
            CREATE UNIQUE INDEX idx_inference_verdicts_unique_latest
            ON inference_verdicts(inference_id, evaluator_type)
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create index");

        // Create a verdict
        let params = CreateVerdictParams::new(
            "tenant-1",
            "inference-1",
            Verdict::High,
            0.95,
            EvaluatorType::Rule,
        )
        .with_evaluator_id("rule-001");

        let id = create_verdict(&pool, &params)
            .await
            .expect("create verdict");

        // Get the verdict
        let verdict = get_verdict_by_inference(&pool, "tenant-1", "inference-1")
            .await
            .expect("get verdict")
            .expect("verdict should exist");

        assert_eq!(verdict.id, id);
        assert_eq!(verdict.tenant_id, "tenant-1");
        assert_eq!(verdict.inference_id, "inference-1");
        assert_eq!(verdict.verdict, "high");
        assert_eq!(verdict.confidence, 0.95);
        assert_eq!(verdict.evaluator_type, "rule");
        assert_eq!(verdict.evaluator_id, Some("rule-001".to_string()));

        // Verify tenant isolation
        let other_tenant = get_verdict_by_inference(&pool, "tenant-2", "inference-1")
            .await
            .expect("get verdict");
        assert!(
            other_tenant.is_none(),
            "tenant-2 should not see tenant-1's verdict"
        );
    }

    #[tokio::test]
    async fn test_unique_index_replacement() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        sqlx::query(
            r#"
            CREATE TABLE inference_verdicts (
                id TEXT PRIMARY KEY NOT NULL,
                tenant_id TEXT NOT NULL,
                inference_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                verdict TEXT NOT NULL,
                confidence REAL NOT NULL,
                evaluator_type TEXT NOT NULL,
                evaluator_id TEXT,
                warnings_digest_b3 TEXT,
                warnings_json TEXT,
                extraction_confidence_score REAL,
                trust_state TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create table");

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX idx_inference_verdicts_unique_latest
            ON inference_verdicts(inference_id, evaluator_type)
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create index");

        // Create initial verdict
        let params1 = CreateVerdictParams::new(
            "tenant-1",
            "inference-1",
            Verdict::Medium,
            0.75,
            EvaluatorType::Rule,
        );
        create_verdict(&pool, &params1)
            .await
            .expect("create first verdict");

        // Create replacement verdict (same inference_id + evaluator_type)
        let params2 = CreateVerdictParams::new(
            "tenant-1",
            "inference-1",
            Verdict::High,
            0.95,
            EvaluatorType::Rule,
        );
        create_verdict(&pool, &params2)
            .await
            .expect("create replacement verdict");

        // Should only have one verdict
        let verdicts = list_verdicts_by_inference(&pool, "tenant-1", "inference-1")
            .await
            .expect("list verdicts");
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].verdict, "high");
        assert_eq!(verdicts[0].confidence, 0.95);
    }
}
