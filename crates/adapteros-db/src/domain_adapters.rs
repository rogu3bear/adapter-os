use crate::Db;
use adapteros_api_types::{
    DomainAdapterExecutionResponse, DomainAdapterManifestResponse, DomainAdapterResponse,
    EpsilonStatsResponse, TestDomainAdapterResponse,
};
use adapteros_core::{AosError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Builder for creating domain adapter parameters
#[derive(Debug, Default)]
pub struct DomainAdapterCreateBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    domain_type: Option<String>,
    model: Option<String>,
    hash: Option<String>,
    input_format: Option<String>,
    output_format: Option<String>,
    config: Option<HashMap<String, serde_json::Value>>,
}

/// Parameters for domain adapter creation
#[derive(Debug)]
pub struct DomainAdapterCreateParams {
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
}

impl DomainAdapterCreateBuilder {
    /// Create a new domain adapter builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the version (required)
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the description (required)
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the domain type (required)
    pub fn domain_type(mut self, domain_type: impl Into<String>) -> Self {
        self.domain_type = Some(domain_type.into());
        self
    }

    /// Set the model (required)
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the hash (required)
    pub fn hash(mut self, hash: impl Into<String>) -> Self {
        self.hash = Some(hash.into());
        self
    }

    /// Set the input format (required)
    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(input_format.into());
        self
    }

    /// Set the output format (required)
    pub fn output_format(mut self, output_format: impl Into<String>) -> Self {
        self.output_format = Some(output_format.into());
        self
    }

    /// Set the config (optional, defaults to empty)
    pub fn config(mut self, config: HashMap<String, serde_json::Value>) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the domain adapter creation parameters
    pub fn build(self) -> Result<DomainAdapterCreateParams> {
        Ok(DomainAdapterCreateParams {
            name: self.name.ok_or_else(|| AosError::Validation("name is required".to_string()))?,
            version: self.version.ok_or_else(|| AosError::Validation("version is required".to_string()))?,
            description: self
                .description
                .ok_or_else(|| AosError::Validation("description is required".to_string()))?,
            domain_type: self
                .domain_type
                .ok_or_else(|| AosError::Validation("domain_type is required".to_string()))?,
            model: self.model.ok_or_else(|| AosError::Validation("model is required".to_string()))?,
            hash: self.hash.ok_or_else(|| AosError::Validation("hash is required".to_string()))?,
            input_format: self
                .input_format
                .ok_or_else(|| AosError::Validation("input_format is required".to_string()))?,
            output_format: self
                .output_format
                .ok_or_else(|| AosError::Validation("output_format is required".to_string()))?,
            config: self.config.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DomainAdapterRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: String, // JSON string
    pub status: String,
    pub epsilon_stats: Option<String>, // JSON string
    pub last_execution: Option<String>,
    pub execution_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DomainAdapterExecutionRecord {
    pub execution_id: String,
    pub adapter_id: String,
    pub input_hash: String,
    pub output_hash: String,
    pub epsilon: f64,
    pub execution_time_ms: i64,
    pub trace_events: String, // JSON string
    pub executed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DomainAdapterTestRecord {
    pub test_id: String,
    pub adapter_id: String,
    pub input_data: String,
    pub actual_output: String,
    pub expected_output: Option<String>,
    pub epsilon: Option<f64>,
    pub passed: bool,
    pub iterations: i32,
    pub execution_time_ms: i64,
    pub executed_at: String,
}

impl From<DomainAdapterRecord> for DomainAdapterResponse {
    fn from(record: DomainAdapterRecord) -> Self {
        let config: HashMap<String, serde_json::Value> =
            serde_json::from_str(&record.config).unwrap_or_default();
        let epsilon_stats: Option<EpsilonStatsResponse> = record
            .epsilon_stats
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok());

        DomainAdapterResponse {
            id: record.id,
            name: record.name,
            version: record.version,
            description: record.description,
            domain_type: record.domain_type,
            model: record.model,
            hash: record.hash,
            input_format: record.input_format,
            output_format: record.output_format,
            config,
            status: record.status,
            epsilon_stats,
            last_execution: record.last_execution,
            execution_count: record.execution_count as u64,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<DomainAdapterExecutionRecord> for DomainAdapterExecutionResponse {
    fn from(record: DomainAdapterExecutionRecord) -> Self {
        let trace_events: Vec<String> =
            serde_json::from_str(&record.trace_events).unwrap_or_default();

        DomainAdapterExecutionResponse {
            execution_id: record.execution_id,
            adapter_id: record.adapter_id,
            input_hash: record.input_hash,
            output_hash: record.output_hash,
            epsilon: record.epsilon,
            execution_time_ms: record.execution_time_ms as u64,
            trace_events,
            executed_at: record.executed_at,
        }
    }
}

impl From<DomainAdapterTestRecord> for TestDomainAdapterResponse {
    fn from(record: DomainAdapterTestRecord) -> Self {
        TestDomainAdapterResponse {
            test_id: record.test_id,
            adapter_id: record.adapter_id,
            input_data: record.input_data,
            actual_output: record.actual_output,
            expected_output: record.expected_output,
            epsilon: record.epsilon,
            passed: record.passed,
            iterations: record.iterations as u32,
            execution_time_ms: record.execution_time_ms as u64,
            executed_at: record.executed_at,
        }
    }
}

impl Db {
    /// Create a new domain adapter
    pub async fn create_domain_adapter(&self, params: DomainAdapterCreateParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let config_json = serde_json::to_string(&params.config)
            .map_err(|e| AosError::Validation(format!("Failed to serialize config: {}", e)))?;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO domain_adapters (
                id, name, version, description, domain_type, model, hash,
                input_format, output_format, config, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.name)
        .bind(&params.version)
        .bind(&params.description)
        .bind(&params.domain_type)
        .bind(&params.model)
        .bind(&params.hash)
        .bind(&params.input_format)
        .bind(&params.output_format)
        .bind(&config_json)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Get domain adapter by ID
    pub async fn get_domain_adapter(&self, id: &str) -> Result<Option<DomainAdapterResponse>> {
        let record = sqlx::query_as::<_, DomainAdapterRecord>(
            "SELECT id, name, version, description, domain_type, model, hash,
                    input_format, output_format, config, status, epsilon_stats,
                    last_execution, execution_count, created_at, updated_at
             FROM domain_adapters WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(record.map(Into::into))
    }

    /// List all domain adapters
    pub async fn list_domain_adapters(&self) -> Result<Vec<DomainAdapterResponse>> {
        let records = sqlx::query_as::<_, DomainAdapterRecord>(
            "SELECT id, name, version, description, domain_type, model, hash,
                    input_format, output_format, config, status, epsilon_stats,
                    last_execution, execution_count, created_at, updated_at
             FROM domain_adapters
             ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Update domain adapter status
    pub async fn update_domain_adapter_status(&self, id: &str, status: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query("UPDATE domain_adapters SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Update domain adapter epsilon stats
    pub async fn update_domain_adapter_epsilon_stats(
        &self,
        id: &str,
        epsilon_stats: &EpsilonStatsResponse,
    ) -> Result<()> {
        let epsilon_stats_json = serde_json::to_string(epsilon_stats)
            .map_err(|e| AosError::Validation(format!("Failed to serialize epsilon_stats: {}", e)))?;
        let now = Utc::now().to_rfc3339();

        sqlx::query("UPDATE domain_adapters SET epsilon_stats = ?, updated_at = ? WHERE id = ?")
            .bind(&epsilon_stats_json)
            .bind(&now)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Record domain adapter execution
    pub async fn record_domain_adapter_execution(
        &self,
        adapter_id: &str,
        input_hash: &str,
        output_hash: &str,
        epsilon: f64,
        execution_time_ms: u64,
        trace_events: &[String],
    ) -> Result<String> {
        let execution_id = Uuid::now_v7().to_string();
        let trace_events_json = serde_json::to_string(trace_events)
            .map_err(|e| AosError::Validation(format!("Failed to serialize trace_events: {}", e)))?;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO domain_adapter_executions (
                execution_id, adapter_id, input_hash, output_hash, epsilon,
                execution_time_ms, trace_events, executed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&execution_id)
        .bind(adapter_id)
        .bind(input_hash)
        .bind(output_hash)
        .bind(epsilon)
        .bind(execution_time_ms as i64)
        .bind(&trace_events_json)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // Update adapter's last execution and execution count
        sqlx::query(
            "UPDATE domain_adapters
             SET last_execution = ?, execution_count = execution_count + 1, updated_at = ?
             WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(adapter_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(execution_id)
    }

    /// Get domain adapter executions
    pub async fn get_domain_adapter_executions(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<DomainAdapterExecutionResponse>> {
        let records = sqlx::query_as::<_, DomainAdapterExecutionRecord>(
            "SELECT execution_id, adapter_id, input_hash, output_hash, epsilon,
                    execution_time_ms, trace_events, executed_at
             FROM domain_adapter_executions
             WHERE adapter_id = ?
             ORDER BY executed_at DESC
             LIMIT ?",
        )
        .bind(adapter_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Record domain adapter test
    pub async fn record_domain_adapter_test(
        &self,
        adapter_id: &str,
        input_data: &str,
        actual_output: &str,
        expected_output: Option<&str>,
        epsilon: Option<f64>,
        passed: bool,
        iterations: u32,
        execution_time_ms: u64,
    ) -> Result<String> {
        let test_id = Uuid::now_v7().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO domain_adapter_tests (
                test_id, adapter_id, input_data, actual_output, expected_output,
                epsilon, passed, iterations, execution_time_ms, executed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&test_id)
        .bind(adapter_id)
        .bind(input_data)
        .bind(actual_output)
        .bind(expected_output)
        .bind(epsilon)
        .bind(passed)
        .bind(iterations as i32)
        .bind(execution_time_ms as i64)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(test_id)
    }

    /// Get domain adapter tests
    pub async fn get_domain_adapter_tests(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<TestDomainAdapterResponse>> {
        let records = sqlx::query_as::<_, DomainAdapterTestRecord>(
            "SELECT test_id, adapter_id, input_data, actual_output, expected_output,
                    epsilon, passed, iterations, execution_time_ms, executed_at
             FROM domain_adapter_tests
             WHERE adapter_id = ?
             ORDER BY executed_at DESC
             LIMIT ?",
        )
        .bind(adapter_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Delete domain adapter (and associated data)
    pub async fn delete_domain_adapter(&self, id: &str) -> Result<()> {
        // Delete executions first (foreign key constraint)
        sqlx::query("DELETE FROM domain_adapter_executions WHERE adapter_id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Delete tests
        sqlx::query("DELETE FROM domain_adapter_tests WHERE adapter_id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Delete adapter
        sqlx::query("DELETE FROM domain_adapters WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get domain adapter manifest (same as adapter data but formatted for manifest)
    pub async fn get_domain_adapter_manifest(
        &self,
        id: &str,
    ) -> Result<Option<DomainAdapterManifestResponse>> {
        let record = sqlx::query_as::<_, DomainAdapterRecord>(
            "SELECT id, name, version, description, domain_type, model, hash,
                    input_format, output_format, config, status, epsilon_stats,
                    last_execution, execution_count, created_at, updated_at
             FROM domain_adapters WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(record.map(|r| {
            let config: HashMap<String, serde_json::Value> =
                serde_json::from_str(&r.config).unwrap_or_default();

            DomainAdapterManifestResponse {
                adapter_id: r.id,
                name: r.name,
                version: r.version,
                description: r.description,
                domain_type: r.domain_type,
                model: r.model,
                hash: r.hash,
                input_format: r.input_format,
                output_format: r.output_format,
                config,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        }))
    }
}
