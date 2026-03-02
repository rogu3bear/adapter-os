use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_types::training::TrainingBackendKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryTrainingPolicy {
    pub repo_id: String,
    pub tenant_id: String,
    pub preferred_backends: Vec<TrainingBackendKind>,
    pub allowed_dataset_types: Vec<String>,
    pub trust_states: Vec<String>,
    pub coreml_allowed: bool,
    pub coreml_required: bool,
    pub pinned_dataset_version_ids: Vec<String>,
}

impl Default for RepositoryTrainingPolicy {
    fn default() -> Self {
        Self {
            repo_id: String::new(),
            tenant_id: String::new(),
            preferred_backends: vec![
                TrainingBackendKind::CoreML,
                TrainingBackendKind::Mlx,
                TrainingBackendKind::Metal,
                TrainingBackendKind::Cpu,
            ],
            allowed_dataset_types: vec!["train".to_string()],
            trust_states: vec!["allowed".to_string(), "allowed_with_warning".to_string()],
            coreml_allowed: true,
            coreml_required: false,
            pinned_dataset_version_ids: Vec::new(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct RepositoryTrainingPolicyRow {
    repo_id: String,
    tenant_id: String,
    preferred_backends_json: String,
    allowed_dataset_types_json: String,
    trust_states_json: String,
    coreml_allowed: i64,
    coreml_required: i64,
    pinned_dataset_version_ids_json: Option<String>,
}

impl From<RepositoryTrainingPolicyRow> for RepositoryTrainingPolicy {
    fn from(row: RepositoryTrainingPolicyRow) -> Self {
        let preferred_backends: Vec<TrainingBackendKind> =
            serde_json::from_str(&row.preferred_backends_json).unwrap_or_default();
        let allowed_dataset_types: Vec<String> =
            serde_json::from_str(&row.allowed_dataset_types_json).unwrap_or_default();
        let trust_states: Vec<String> =
            serde_json::from_str(&row.trust_states_json).unwrap_or_default();
        let pinned_dataset_version_ids: Vec<String> = row
            .pinned_dataset_version_ids_json
            .as_ref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();

        Self {
            repo_id: row.repo_id,
            tenant_id: row.tenant_id,
            preferred_backends,
            allowed_dataset_types,
            trust_states,
            coreml_allowed: row.coreml_allowed != 0,
            coreml_required: row.coreml_required != 0,
            pinned_dataset_version_ids,
        }
    }
}

impl Db {
    /// Fetch a repository training policy (tenant + repo scoped).
    pub async fn get_repository_training_policy(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<RepositoryTrainingPolicy>> {
        let row = sqlx::query_as::<_, RepositoryTrainingPolicyRow>(
            r#"
            SELECT repo_id, tenant_id, preferred_backends_json, allowed_dataset_types_json,
                   trust_states_json, coreml_allowed, coreml_required, pinned_dataset_version_ids_json
            FROM repository_training_policies
            WHERE tenant_id = ? AND repo_id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(repo_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(row.map(RepositoryTrainingPolicy::from))
    }

    /// Upsert a repository training policy for self-hosting orchestration.
    pub async fn upsert_repository_training_policy(
        &self,
        policy: &RepositoryTrainingPolicy,
    ) -> Result<()> {
        let preferred_backends_json = serde_json::to_string(&policy.preferred_backends)
            .map_err(|e| AosError::Validation(format!("invalid preferred_backends: {}", e)))?;
        let allowed_dataset_types_json = serde_json::to_string(&policy.allowed_dataset_types)
            .map_err(|e| AosError::Validation(format!("invalid allowed_dataset_types: {}", e)))?;
        let trust_states_json = serde_json::to_string(&policy.trust_states)
            .map_err(|e| AosError::Validation(format!("invalid trust_states: {}", e)))?;
        let pinned_dataset_version_ids_json = if policy.pinned_dataset_version_ids.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&policy.pinned_dataset_version_ids)
                    .map_err(|e| AosError::Validation(format!("invalid pinned versions: {}", e)))?,
            )
        };

        sqlx::query(
            r#"
            INSERT INTO repository_training_policies (
                repo_id, tenant_id, preferred_backends_json, allowed_dataset_types_json,
                trust_states_json, coreml_allowed, coreml_required, pinned_dataset_version_ids_json,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
            ON CONFLICT(repo_id, tenant_id) DO UPDATE SET
                preferred_backends_json = excluded.preferred_backends_json,
                allowed_dataset_types_json = excluded.allowed_dataset_types_json,
                trust_states_json = excluded.trust_states_json,
                coreml_allowed = excluded.coreml_allowed,
                coreml_required = excluded.coreml_required,
                pinned_dataset_version_ids_json = excluded.pinned_dataset_version_ids_json,
                updated_at = datetime('now')
            "#,
        )
        .bind(&policy.repo_id)
        .bind(&policy.tenant_id)
        .bind(&preferred_backends_json)
        .bind(&allowed_dataset_types_json)
        .bind(&trust_states_json)
        .bind(policy.coreml_allowed)
        .bind(policy.coreml_required)
        .bind(pinned_dataset_version_ids_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }
}
