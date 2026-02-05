//! Discrepancy case database operations.
//!
//! This module provides database operations for tracking inference discrepancies
//! reported by users. It supports privacy-conscious storage where content digests
//! are always stored, but plaintext content is only stored when explicitly opted-in.
//!
//! # Privacy Model
//!
//! By default, only BLAKE3 hashes of questions/answers are stored. When
//! `store_content=true` is set, the actual text is stored for training purposes.
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_db::discrepancy_cases::{CreateDiscrepancyParams, DiscrepancyType};
//!
//! let params = CreateDiscrepancyParams::builder()
//!     .tenant_id("tenant-123")
//!     .inference_id("inf-abc")
//!     .discrepancy_type(DiscrepancyType::IncorrectAnswer)
//!     .user_question_hash_b3("abc123...")
//!     .model_answer_hash_b3("def456...")
//!     .build()?;
//!
//! let case_id = db.create_discrepancy_case(&params).await?;
//! ```

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============================================================================
// Type Definitions
// ============================================================================

/// Discrepancy type classification
///
/// Categorizes the nature of the model error for analysis and training prioritization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum DiscrepancyType {
    /// Model gave a factually wrong answer
    IncorrectAnswer,
    /// Model's answer was partially correct but missing important information
    IncompleteAnswer,
    /// Model fabricated information not present in context
    Hallucination,
    /// Output formatting was incorrect (e.g., wrong structure, encoding issues)
    FormattingError,
    /// Other type of discrepancy not covered by above categories
    Other,
}

impl DiscrepancyType {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IncorrectAnswer => "incorrect_answer",
            Self::IncompleteAnswer => "incomplete_answer",
            Self::Hallucination => "hallucination",
            Self::FormattingError => "formatting_error",
            Self::Other => "other",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "incorrect_answer" => Some(Self::IncorrectAnswer),
            "incomplete_answer" => Some(Self::IncompleteAnswer),
            "hallucination" => Some(Self::Hallucination),
            "formatting_error" => Some(Self::FormattingError),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

impl std::fmt::Display for DiscrepancyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Resolution status for discrepancy cases
///
/// Tracks the lifecycle of a discrepancy case from reporting to resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    /// Newly reported, awaiting review
    Open,
    /// Reviewed and confirmed to be a model error
    ConfirmedError,
    /// Reviewed and determined not to be an actual error
    NotAnError,
    /// Error was addressed in a subsequent training run
    FixedInTraining,
    /// Case deferred for future investigation
    Deferred,
}

impl ResolutionStatus {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ConfirmedError => "confirmed_error",
            Self::NotAnError => "not_an_error",
            Self::FixedInTraining => "fixed_in_training",
            Self::Deferred => "deferred",
        }
    }

    /// Parse from database string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "confirmed_error" => Some(Self::ConfirmedError),
            "not_an_error" => Some(Self::NotAnError),
            "fixed_in_training" => Some(Self::FixedInTraining),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }
}

impl std::fmt::Display for ResolutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Database Row Types
// ============================================================================

/// Database row for discrepancy case
///
/// Represents a user-reported discrepancy between model output and expected truth.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DiscrepancyCase {
    /// Unique identifier (UUIDv7)
    pub id: String,
    /// Tenant ID for multi-tenant isolation
    pub tenant_id: String,
    /// Timestamp when case was created
    pub created_at: String,
    /// Reference to the inference trace
    pub inference_id: String,
    /// Optional reference to diagnostic run
    pub run_id: Option<String>,
    /// Optional reference to replay session
    pub replay_session_id: Option<String>,

    // Document reference
    /// Document ID if discrepancy relates to a specific document
    pub document_id: Option<String>,
    /// BLAKE3 hash of the document
    pub document_hash_b3: Option<String>,
    /// Page number within document
    pub page_number: Option<i64>,
    /// BLAKE3 hash of the specific chunk
    pub chunk_hash_b3: Option<String>,

    // Discrepancy details
    /// Type of discrepancy (stored as string in DB)
    pub discrepancy_type: String,
    /// Current resolution status (stored as string in DB)
    pub resolution_status: String,

    // Privacy-conscious content storage
    /// Whether plaintext content is stored (0=digests only, 1=store plaintext)
    pub store_content: bool,
    /// User's question (only stored if store_content=true)
    pub user_question: Option<String>,
    /// Model's answer (only stored if store_content=true)
    pub model_answer: Option<String>,
    /// Expected correct answer (only stored if store_content=true)
    pub ground_truth: Option<String>,
    /// BLAKE3 hash of user question (always stored)
    pub user_question_hash_b3: Option<String>,
    /// BLAKE3 hash of model answer (always stored)
    pub model_answer_hash_b3: Option<String>,

    // Metadata
    /// User/system that reported the discrepancy
    pub reported_by: Option<String>,
    /// Additional notes about the case
    pub notes: Option<String>,
}

impl DiscrepancyCase {
    /// Get the discrepancy type as an enum
    pub fn discrepancy_type_enum(&self) -> Option<DiscrepancyType> {
        DiscrepancyType::from_str(&self.discrepancy_type)
    }

    /// Get the resolution status as an enum
    pub fn resolution_status_enum(&self) -> Option<ResolutionStatus> {
        ResolutionStatus::from_str(&self.resolution_status)
    }
}

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for creating a discrepancy case
#[derive(Debug, Clone)]
pub struct CreateDiscrepancyParams {
    /// Optional pre-generated ID (UUIDv7 generated if not provided)
    pub id: Option<String>,
    /// Tenant ID (required)
    pub tenant_id: String,
    /// Inference trace ID (required)
    pub inference_id: String,
    /// Discrepancy type (required)
    pub discrepancy_type: DiscrepancyType,
    /// Optional diagnostic run ID
    pub run_id: Option<String>,
    /// Optional replay session ID
    pub replay_session_id: Option<String>,

    // Document reference
    pub document_id: Option<String>,
    pub document_hash_b3: Option<String>,
    pub page_number: Option<i64>,
    pub chunk_hash_b3: Option<String>,

    // Content storage
    /// Whether to store plaintext content (default: false)
    pub store_content: bool,
    /// User question (stored only if store_content=true)
    pub user_question: Option<String>,
    /// Model answer (stored only if store_content=true)
    pub model_answer: Option<String>,
    /// Ground truth answer (stored only if store_content=true)
    pub ground_truth: Option<String>,
    /// BLAKE3 hash of user question (always stored)
    pub user_question_hash_b3: Option<String>,
    /// BLAKE3 hash of model answer (always stored)
    pub model_answer_hash_b3: Option<String>,

    // Metadata
    pub reported_by: Option<String>,
    pub notes: Option<String>,
}

impl CreateDiscrepancyParams {
    /// Create a new builder for CreateDiscrepancyParams
    pub fn builder() -> CreateDiscrepancyParamsBuilder {
        CreateDiscrepancyParamsBuilder::default()
    }
}

/// Builder for CreateDiscrepancyParams with validation
#[derive(Debug, Default)]
pub struct CreateDiscrepancyParamsBuilder {
    id: Option<String>,
    tenant_id: Option<String>,
    inference_id: Option<String>,
    discrepancy_type: Option<DiscrepancyType>,
    run_id: Option<String>,
    replay_session_id: Option<String>,
    document_id: Option<String>,
    document_hash_b3: Option<String>,
    page_number: Option<i64>,
    chunk_hash_b3: Option<String>,
    store_content: bool,
    user_question: Option<String>,
    model_answer: Option<String>,
    ground_truth: Option<String>,
    user_question_hash_b3: Option<String>,
    model_answer_hash_b3: Option<String>,
    reported_by: Option<String>,
    notes: Option<String>,
}

impl CreateDiscrepancyParamsBuilder {
    /// Set the case ID (optional, UUIDv7 generated if not provided)
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the inference trace ID (required)
    pub fn inference_id(mut self, inference_id: impl Into<String>) -> Self {
        self.inference_id = Some(inference_id.into());
        self
    }

    /// Set the discrepancy type (required)
    pub fn discrepancy_type(mut self, discrepancy_type: DiscrepancyType) -> Self {
        self.discrepancy_type = Some(discrepancy_type);
        self
    }

    /// Set the diagnostic run ID (optional)
    pub fn run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Set the replay session ID (optional)
    pub fn replay_session_id(mut self, replay_session_id: impl Into<String>) -> Self {
        self.replay_session_id = Some(replay_session_id.into());
        self
    }

    /// Set the document ID (optional)
    pub fn document_id(mut self, document_id: impl Into<String>) -> Self {
        self.document_id = Some(document_id.into());
        self
    }

    /// Set the document BLAKE3 hash (optional)
    pub fn document_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.document_hash_b3 = Some(hash.into());
        self
    }

    /// Set the page number (optional)
    pub fn page_number(mut self, page: i64) -> Self {
        self.page_number = Some(page);
        self
    }

    /// Set the chunk BLAKE3 hash (optional)
    pub fn chunk_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.chunk_hash_b3 = Some(hash.into());
        self
    }

    /// Enable storing plaintext content
    pub fn store_content(mut self, store: bool) -> Self {
        self.store_content = store;
        self
    }

    /// Set the user question (stored only if store_content=true)
    pub fn user_question(mut self, question: impl Into<String>) -> Self {
        self.user_question = Some(question.into());
        self
    }

    /// Set the model answer (stored only if store_content=true)
    pub fn model_answer(mut self, answer: impl Into<String>) -> Self {
        self.model_answer = Some(answer.into());
        self
    }

    /// Set the ground truth answer (stored only if store_content=true)
    pub fn ground_truth(mut self, truth: impl Into<String>) -> Self {
        self.ground_truth = Some(truth.into());
        self
    }

    /// Set the user question BLAKE3 hash (always stored)
    pub fn user_question_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.user_question_hash_b3 = Some(hash.into());
        self
    }

    /// Set the model answer BLAKE3 hash (always stored)
    pub fn model_answer_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.model_answer_hash_b3 = Some(hash.into());
        self
    }

    /// Set the reporter (optional)
    pub fn reported_by(mut self, reporter: impl Into<String>) -> Self {
        self.reported_by = Some(reporter.into());
        self
    }

    /// Set additional notes (optional)
    pub fn notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Build the CreateDiscrepancyParams, validating required fields
    pub fn build(self) -> Result<CreateDiscrepancyParams> {
        let tenant_id = self.tenant_id.ok_or_else(|| {
            AosError::Validation("tenant_id is required for CreateDiscrepancyParams".into())
        })?;

        let inference_id = self.inference_id.ok_or_else(|| {
            AosError::Validation("inference_id is required for CreateDiscrepancyParams".into())
        })?;

        let discrepancy_type = self.discrepancy_type.ok_or_else(|| {
            AosError::Validation("discrepancy_type is required for CreateDiscrepancyParams".into())
        })?;

        Ok(CreateDiscrepancyParams {
            id: self.id,
            tenant_id,
            inference_id,
            discrepancy_type,
            run_id: self.run_id,
            replay_session_id: self.replay_session_id,
            document_id: self.document_id,
            document_hash_b3: self.document_hash_b3,
            page_number: self.page_number,
            chunk_hash_b3: self.chunk_hash_b3,
            store_content: self.store_content,
            user_question: self.user_question,
            model_answer: self.model_answer,
            ground_truth: self.ground_truth,
            user_question_hash_b3: self.user_question_hash_b3,
            model_answer_hash_b3: self.model_answer_hash_b3,
            reported_by: self.reported_by,
            notes: self.notes,
        })
    }
}

/// Parameters for updating a discrepancy case resolution
#[derive(Debug, Clone)]
pub struct UpdateResolutionParams {
    /// Case ID to update
    pub case_id: String,
    /// New resolution status
    pub resolution_status: ResolutionStatus,
    /// Optional notes about the resolution
    pub notes: Option<String>,
}

/// Filter options for listing discrepancy cases
#[derive(Debug, Clone, Default)]
pub struct DiscrepancyCaseFilter {
    /// Filter by resolution status
    pub status: Option<ResolutionStatus>,
    /// Filter by discrepancy type
    pub discrepancy_type: Option<DiscrepancyType>,
    /// Filter by document hash
    pub document_hash_b3: Option<String>,
    /// Filter by inference ID
    pub inference_id: Option<String>,
    /// Limit number of results
    pub limit: Option<i64>,
    /// Offset for pagination
    pub offset: Option<i64>,
}

/// Export record for confirmed errors (used for training)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmedErrorExport {
    /// Case ID for reference
    pub case_id: String,
    /// User question (if stored)
    pub user_question: Option<String>,
    /// Model's incorrect answer (if stored)
    pub model_answer: Option<String>,
    /// Correct answer (if stored)
    pub ground_truth: Option<String>,
    /// Hash of user question
    pub user_question_hash_b3: Option<String>,
    /// Hash of model answer
    pub model_answer_hash_b3: Option<String>,
    /// Type of error
    pub discrepancy_type: String,
    /// Document reference for context retrieval
    pub document_hash_b3: Option<String>,
    /// Page number for context
    pub page_number: Option<i64>,
    /// Chunk hash for context
    pub chunk_hash_b3: Option<String>,
}

// ============================================================================
// Database Column Constants
// ============================================================================

/// Discrepancy case table columns for SELECT queries
pub const DISCREPANCY_CASE_COLUMNS: &str = "id, tenant_id, created_at, inference_id, run_id, \
    replay_session_id, document_id, document_hash_b3, page_number, chunk_hash_b3, \
    discrepancy_type, resolution_status, store_content, user_question, model_answer, \
    ground_truth, user_question_hash_b3, model_answer_hash_b3, reported_by, notes";

// ============================================================================
// Database Operations
// ============================================================================

impl Db {
    /// Create a new discrepancy case
    ///
    /// Returns the ID of the newly created case.
    pub async fn create_discrepancy_case(&self, params: &CreateDiscrepancyParams) -> Result<String> {
        let id = params
            .id
            .clone()
            .unwrap_or_else(|| Uuid::now_v7().to_string());

        // Apply privacy rule: only store content if explicitly opted-in
        let user_question = if params.store_content {
            params.user_question.as_deref()
        } else {
            None
        };
        let model_answer = if params.store_content {
            params.model_answer.as_deref()
        } else {
            None
        };
        let ground_truth = if params.store_content {
            params.ground_truth.as_deref()
        } else {
            None
        };

        sqlx::query(
            "INSERT INTO discrepancy_cases (
                id, tenant_id, inference_id, run_id, replay_session_id,
                document_id, document_hash_b3, page_number, chunk_hash_b3,
                discrepancy_type, resolution_status, store_content,
                user_question, model_answer, ground_truth,
                user_question_hash_b3, model_answer_hash_b3,
                reported_by, notes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.inference_id)
        .bind(&params.run_id)
        .bind(&params.replay_session_id)
        .bind(&params.document_id)
        .bind(&params.document_hash_b3)
        .bind(params.page_number)
        .bind(&params.chunk_hash_b3)
        .bind(params.discrepancy_type.as_str())
        .bind(ResolutionStatus::Open.as_str())
        .bind(params.store_content)
        .bind(user_question)
        .bind(model_answer)
        .bind(ground_truth)
        .bind(&params.user_question_hash_b3)
        .bind(&params.model_answer_hash_b3)
        .bind(&params.reported_by)
        .bind(&params.notes)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create discrepancy case: {}", e)))?;

        Ok(id)
    }

    /// Get a single discrepancy case by ID
    pub async fn get_discrepancy_case(
        &self,
        case_id: &str,
        tenant_id: &str,
    ) -> Result<Option<DiscrepancyCase>> {
        let query = format!(
            "SELECT {} FROM discrepancy_cases WHERE id = ? AND tenant_id = ?",
            DISCREPANCY_CASE_COLUMNS
        );

        let case = sqlx::query_as::<_, DiscrepancyCase>(&query)
            .bind(case_id)
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get discrepancy case: {}", e)))?;

        Ok(case)
    }

    /// List discrepancy cases for a tenant with optional filtering
    pub async fn list_discrepancy_cases(
        &self,
        tenant_id: &str,
        filter: &DiscrepancyCaseFilter,
    ) -> Result<Vec<DiscrepancyCase>> {
        let mut conditions = vec!["tenant_id = ?".to_string()];
        let mut bind_values: Vec<String> = vec![tenant_id.to_string()];

        if let Some(ref status) = filter.status {
            conditions.push("resolution_status = ?".to_string());
            bind_values.push(status.as_str().to_string());
        }

        if let Some(ref dtype) = filter.discrepancy_type {
            conditions.push("discrepancy_type = ?".to_string());
            bind_values.push(dtype.as_str().to_string());
        }

        if let Some(ref doc_hash) = filter.document_hash_b3 {
            conditions.push("document_hash_b3 = ?".to_string());
            bind_values.push(doc_hash.clone());
        }

        if let Some(ref inf_id) = filter.inference_id {
            conditions.push("inference_id = ?".to_string());
            bind_values.push(inf_id.clone());
        }

        let where_clause = conditions.join(" AND ");
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        let query = format!(
            "SELECT {} FROM discrepancy_cases WHERE {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            DISCREPANCY_CASE_COLUMNS, where_clause
        );

        // Build dynamic query with bindings
        let mut q = sqlx::query_as::<_, DiscrepancyCase>(&query);
        for val in &bind_values {
            q = q.bind(val);
        }
        q = q.bind(limit).bind(offset);

        let cases = q.fetch_all(self.pool()).await.map_err(|e| {
            AosError::Database(format!("Failed to list discrepancy cases: {}", e))
        })?;

        Ok(cases)
    }

    /// Update the resolution status of a discrepancy case
    pub async fn update_discrepancy_resolution(
        &self,
        tenant_id: &str,
        params: &UpdateResolutionParams,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE discrepancy_cases
             SET resolution_status = ?, notes = COALESCE(?, notes)
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(params.resolution_status.as_str())
        .bind(&params.notes)
        .bind(&params.case_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update discrepancy resolution: {}", e))
        })?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "Discrepancy case {} not found for tenant {}",
                params.case_id, tenant_id
            )));
        }

        Ok(())
    }

    /// Export confirmed errors for training dataset generation
    ///
    /// Returns all cases with status `ConfirmedError` for the tenant,
    /// formatted for use in training data generation.
    pub async fn export_confirmed_errors(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ConfirmedErrorExport>> {
        let query = format!(
            "SELECT {} FROM discrepancy_cases
             WHERE tenant_id = ? AND resolution_status = ?
             ORDER BY created_at ASC",
            DISCREPANCY_CASE_COLUMNS
        );

        let cases = sqlx::query_as::<_, DiscrepancyCase>(&query)
            .bind(tenant_id)
            .bind(ResolutionStatus::ConfirmedError.as_str())
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to export confirmed errors: {}", e)))?;

        let exports: Vec<ConfirmedErrorExport> = cases
            .into_iter()
            .map(|case| ConfirmedErrorExport {
                case_id: case.id,
                user_question: case.user_question,
                model_answer: case.model_answer,
                ground_truth: case.ground_truth,
                user_question_hash_b3: case.user_question_hash_b3,
                model_answer_hash_b3: case.model_answer_hash_b3,
                discrepancy_type: case.discrepancy_type,
                document_hash_b3: case.document_hash_b3,
                page_number: case.page_number,
                chunk_hash_b3: case.chunk_hash_b3,
            })
            .collect();

        Ok(exports)
    }

    /// Mark a discrepancy case as fixed in training
    ///
    /// Convenience method to update status to `FixedInTraining`.
    pub async fn mark_discrepancy_fixed_in_training(
        &self,
        tenant_id: &str,
        case_id: &str,
        training_job_id: Option<&str>,
    ) -> Result<()> {
        let notes = training_job_id.map(|id| format!("Fixed in training job: {}", id));

        self.update_discrepancy_resolution(
            tenant_id,
            &UpdateResolutionParams {
                case_id: case_id.to_string(),
                resolution_status: ResolutionStatus::FixedInTraining,
                notes,
            },
        )
        .await
    }

    /// Count discrepancy cases by status for a tenant
    pub async fn count_discrepancy_cases_by_status(
        &self,
        tenant_id: &str,
    ) -> Result<std::collections::HashMap<String, i64>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT resolution_status, COUNT(*) as count
             FROM discrepancy_cases
             WHERE tenant_id = ?
             GROUP BY resolution_status",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to count discrepancy cases: {}", e))
        })?;

        let mut counts = std::collections::HashMap::new();
        for (status, count) in rows {
            counts.insert(status, count);
        }

        Ok(counts)
    }

    /// Delete a discrepancy case
    pub async fn delete_discrepancy_case(&self, case_id: &str, tenant_id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM discrepancy_cases WHERE id = ? AND tenant_id = ?")
            .bind(case_id)
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete discrepancy case: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "Discrepancy case {} not found for tenant {}",
                case_id, tenant_id
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discrepancy_type_round_trip() {
        let types = [
            DiscrepancyType::IncorrectAnswer,
            DiscrepancyType::IncompleteAnswer,
            DiscrepancyType::Hallucination,
            DiscrepancyType::FormattingError,
            DiscrepancyType::Other,
        ];

        for dtype in types {
            let s = dtype.as_str();
            let parsed = DiscrepancyType::from_str(s);
            assert_eq!(parsed, Some(dtype), "Round-trip failed for {:?}", dtype);
        }
    }

    #[test]
    fn test_resolution_status_round_trip() {
        let statuses = [
            ResolutionStatus::Open,
            ResolutionStatus::ConfirmedError,
            ResolutionStatus::NotAnError,
            ResolutionStatus::FixedInTraining,
            ResolutionStatus::Deferred,
        ];

        for status in statuses {
            let s = status.as_str();
            let parsed = ResolutionStatus::from_str(s);
            assert_eq!(parsed, Some(status), "Round-trip failed for {:?}", status);
        }
    }

    #[test]
    fn test_builder_validation() {
        // Missing tenant_id should fail
        let result = CreateDiscrepancyParams::builder()
            .inference_id("inf-123")
            .discrepancy_type(DiscrepancyType::IncorrectAnswer)
            .build();
        assert!(result.is_err());

        // Missing inference_id should fail
        let result = CreateDiscrepancyParams::builder()
            .tenant_id("tenant-123")
            .discrepancy_type(DiscrepancyType::IncorrectAnswer)
            .build();
        assert!(result.is_err());

        // Missing discrepancy_type should fail
        let result = CreateDiscrepancyParams::builder()
            .tenant_id("tenant-123")
            .inference_id("inf-123")
            .build();
        assert!(result.is_err());

        // All required fields present should succeed
        let result = CreateDiscrepancyParams::builder()
            .tenant_id("tenant-123")
            .inference_id("inf-123")
            .discrepancy_type(DiscrepancyType::IncorrectAnswer)
            .build();
        assert!(result.is_ok());
    }
}
