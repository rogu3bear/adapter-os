//! Adapter repository + version persistence
//! Implements the core entity model backed by migrations/0175_adapter_repositories_and_versions.sql

use adapteros_core::{AosError, Result};
use adapteros_types::coreml::CoreMLMode;
use adapteros_types::repository::RepoTier;
use semver::Version;
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use sqlx::{Executor, Row, Sqlite, Transaction};
use std::collections::HashSet;
use std::str::FromStr;
use tracing::warn;
use uuid::Uuid;

use crate::Db;

fn validate_branch_classification(value: &str) -> Result<()> {
    match value {
        "protected" | "high" | "sandbox" => Ok(()),
        other => Err(AosError::Validation(format!(
            "invalid branch_classification: {}",
            other
        ))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterRepository {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub default_branch: String,
    pub archived: i64,
    pub created_by: Option<String>,
    pub created_at: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRepositoryPolicy {
    pub repo_id: String,
    pub tenant_id: String,
    pub preferred_backends_json: Option<String>,
    pub coreml_allowed: bool,
    pub coreml_required: bool,
    pub autopromote_coreml: bool,
    pub coreml_mode: CoreMLMode,
    pub repo_tier: RepoTier,
    pub auto_rollback_on_trust_regress: bool,
    pub created_at: String,
}

#[derive(sqlx::FromRow)]
struct AdapterRepositoryPolicyRow {
    repo_id: String,
    tenant_id: String,
    preferred_backends_json: Option<String>,
    coreml_allowed: i64,
    coreml_required: i64,
    autopromote_coreml: i64,
    coreml_mode: String,
    repo_tier: String,
    auto_rollback_on_trust_regress: i64,
    created_at: String,
}

impl TryFrom<AdapterRepositoryPolicyRow> for AdapterRepositoryPolicy {
    type Error = AosError;

    fn try_from(row: AdapterRepositoryPolicyRow) -> std::result::Result<Self, Self::Error> {
        let coreml_mode =
            CoreMLMode::from_str(&row.coreml_mode).unwrap_or(CoreMLMode::CoremlPreferred);
        let repo_tier = RepoTier::from_str(&row.repo_tier).unwrap_or(RepoTier::Normal);

        Ok(Self {
            repo_id: row.repo_id,
            tenant_id: row.tenant_id,
            preferred_backends_json: row.preferred_backends_json,
            coreml_allowed: row.coreml_allowed != 0,
            coreml_required: row.coreml_required != 0,
            autopromote_coreml: row.autopromote_coreml != 0,
            coreml_mode,
            repo_tier,
            auto_rollback_on_trust_regress: row.auto_rollback_on_trust_regress != 0,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterVersion {
    pub id: String,
    pub repo_id: String,
    pub tenant_id: String,
    pub version: String,
    pub branch: String,
    pub branch_classification: String,
    pub aos_path: Option<String>,
    pub aos_hash: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub parent_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
    pub training_backend: Option<String>,
    pub coreml_used: bool,
    pub coreml_device_type: Option<String>,
    #[sqlx(default)]
    pub adapter_trust_state: String,
    pub release_state: String,
    pub metrics_snapshot_id: Option<String>,
    pub evaluation_summary: Option<String>,
    pub created_at: String,
    // Publish + Attach Mode fields (v1)
    /// Attach mode: 'free' or 'requires_dataset'
    #[sqlx(default)]
    pub attach_mode: String,
    /// Required dataset version ID when attach_mode = 'requires_dataset'
    pub required_scope_dataset_version_id: Option<String>,
    /// Archive flag: true = hidden from normal use
    #[sqlx(default)]
    pub is_archived: bool,
    /// Timestamp when adapter was published (NULL = unpublished)
    pub published_at: Option<String>,
    /// Short description for published adapter
    pub short_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterVersionRuntimeState {
    pub version_id: String,
    pub runtime_state: String,
    pub updated_at: String,
    pub worker_id: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterVersionTag {
    pub id: String,
    pub version_id: String,
    pub repo_id: String,
    pub tenant_id: String,
    pub tag_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterVersionHistory {
    pub id: String,
    pub repo_id: String,
    pub tenant_id: String,
    pub version_id: String,
    pub branch: String,
    pub old_state: Option<String>,
    pub new_state: String,
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub train_job_id: Option<String>,
    pub created_at: String,
}

pub struct CreateRepositoryParams<'a> {
    pub tenant_id: &'a str,
    pub name: &'a str,
    pub base_model_id: Option<&'a str>,
    pub default_branch: Option<&'a str>,
    pub created_by: Option<&'a str>,
    pub description: Option<&'a str>,
}

pub struct CreateDraftVersionParams<'a> {
    pub repo_id: &'a str,
    pub tenant_id: &'a str,
    pub branch: &'a str,
    pub branch_classification: &'a str,
    pub parent_version_id: Option<&'a str>,
    pub code_commit_sha: Option<&'a str>,
    pub data_spec_hash: Option<&'a str>,
    pub training_backend: Option<&'a str>,
    pub dataset_version_ids: Option<&'a [String]>,
    pub actor: Option<&'a str>,
    pub reason: Option<&'a str>,
}

pub struct CreateVersionParams<'a> {
    pub repo_id: &'a str,
    pub tenant_id: &'a str,
    pub version: &'a str,
    pub branch: &'a str,
    pub branch_classification: &'a str,
    pub aos_path: Option<&'a str>,
    pub aos_hash: Option<&'a str>,
    pub manifest_schema_version: Option<&'a str>,
    pub parent_version_id: Option<&'a str>,
    pub code_commit_sha: Option<&'a str>,
    pub data_spec_hash: Option<&'a str>,
    pub training_backend: Option<&'a str>,
    pub coreml_used: Option<bool>,
    pub coreml_device_type: Option<&'a str>,
    pub dataset_version_ids: Option<&'a [String]>,
    pub release_state: &'a str,
    pub metrics_snapshot_id: Option<&'a str>,
    pub evaluation_summary: Option<&'a str>,
    /// Allow creating versions even when the repository is archived.
    /// Defaults to false to enforce archival freeze.
    pub allow_archived: bool,
    pub actor: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub train_job_id: Option<&'a str>,
}

pub struct UpsertAdapterRepositoryPolicyParams<'a> {
    pub repo_id: &'a str,
    pub tenant_id: &'a str,
    pub preferred_backends_json: Option<&'a str>,
    pub coreml_allowed: Option<bool>,
    pub coreml_required: Option<bool>,
    pub autopromote_coreml: Option<bool>,
    pub coreml_mode: Option<CoreMLMode>,
    pub repo_tier: Option<RepoTier>,
    pub auto_rollback_on_trust_regress: Option<bool>,
}

pub struct UpsertRuntimeStateParams<'a> {
    pub version_id: &'a str,
    pub runtime_state: &'a str,
    pub worker_id: Option<&'a str>,
    pub last_error: Option<&'a str>,
}

pub struct VersionHistoryEntry<'a> {
    pub repo_id: &'a str,
    pub tenant_id: &'a str,
    pub branch: &'a str,
    pub version_id: &'a str,
    pub old_state: Option<&'a str>,
    pub new_state: &'a str,
    pub actor: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub train_job_id: Option<&'a str>,
}

fn normalize_release_state(state: &str) -> String {
    state.trim().to_ascii_lowercase()
}

fn validate_release_state(state: &str) -> Result<()> {
    match normalize_release_state(state).as_str() {
        "draft" | "training" | "ready" | "active" | "deprecated" | "retired" | "failed" => Ok(()),
        other => Err(AosError::Validation(format!(
            "invalid release_state '{}'",
            other
        ))),
    }
}

fn validate_release_transition(old_state: Option<&str>, new_state: &str) -> Result<()> {
    let new_state = normalize_release_state(new_state);
    validate_release_state(&new_state)?;

    if let Some(old_state_raw) = old_state {
        let old_state = normalize_release_state(old_state_raw);
        validate_release_state(&old_state)?;

        if old_state == new_state {
            return Ok(());
        }

        let allowed = matches!(
            (old_state.as_str(), new_state.as_str()),
            ("draft", "training")
                | ("training", "ready")
                | ("training", "failed")
                | ("ready", "active")
                | ("active", "deprecated")
                | ("deprecated", "active")
                | ("deprecated", "retired")
                | ("active", "ready")
                | (_, "failed")
        );

        if !allowed {
            return Err(AosError::Validation(format!(
                "illegal release_state transition {} -> {}",
                old_state, new_state
            )));
        }
    }

    Ok(())
}

fn normalize_adapter_trust_state(state: &str) -> String {
    state.trim().to_ascii_lowercase()
}

/// Aggregate dataset trust states into adapter trust (blocked > warn > unknown > allowed).
fn map_dataset_trust_to_adapter_trust(states: &[String]) -> String {
    if states.is_empty() {
        return "unknown".to_string();
    }

    let mut has_warn = false;
    let mut has_unknown = false;

    for state in states {
        let normalized = normalize_adapter_trust_state(state);
        match normalized.as_str() {
            "blocked" => return "blocked".to_string(),
            "allowed_with_warning" | "needs_approval" => has_warn = true,
            "unknown" => has_unknown = true,
            "allowed" => {}
            other => {
                // Preserve any unexpected states as warn to avoid over-trust.
                warn!(state = %other, "unknown dataset trust_state; defaulting adapter trust to warn");
                has_warn = true;
            }
        }
    }

    if has_warn {
        "warn".to_string()
    } else if has_unknown {
        "unknown".to_string()
    } else {
        "allowed".to_string()
    }
}

fn is_serveable_version(version: &AdapterVersion) -> bool {
    let release = normalize_release_state(&version.release_state);
    if release != "active" && release != "ready" {
        return false;
    }

    let trust = version.adapter_trust_state.to_ascii_lowercase();
    !matches!(
        trust.as_str(),
        "blocked" | "blocked_regressed" | "needs_approval" | "unknown"
    )
}

#[cfg(test)]
mod trust_tests {
    use super::map_dataset_trust_to_adapter_trust;

    #[test]
    fn adapter_trust_blocks_on_any_blocked() {
        let trust = map_dataset_trust_to_adapter_trust(&[
            "allowed".into(),
            "blocked".into(),
            "allowed_with_warning".into(),
        ]);
        assert_eq!(trust, "blocked");
    }

    #[test]
    fn adapter_trust_warns_on_warning_or_needs_approval() {
        let trust = map_dataset_trust_to_adapter_trust(&[
            "allowed".into(),
            "needs_approval".into(),
            "unknown".into(),
        ]);
        assert_eq!(trust, "warn");
    }

    #[test]
    fn adapter_trust_unknown_when_only_unknown_or_empty() {
        assert_eq!(
            map_dataset_trust_to_adapter_trust(&["unknown".into(), "allowed".into()]),
            "unknown"
        );
        assert_eq!(map_dataset_trust_to_adapter_trust(&[]), "unknown");
    }

    #[test]
    fn adapter_trust_allowed_when_all_allowed() {
        let trust = map_dataset_trust_to_adapter_trust(&["allowed".into(), "allowed".into()]);
        assert_eq!(trust, "allowed");
    }
}

impl Db {
    async fn ensure_no_version_cycle(
        &self,
        parent_version_id: &str,
        repo_id: &str,
        tenant_id: &str,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let mut current: Option<String> = Some(parent_version_id.to_string());

        while let Some(id) = current {
            if !seen.insert(id.clone()) {
                return Err(AosError::Validation(
                    "parent_version_id introduces a version cycle".to_string(),
                ));
            }

            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT parent_version_id FROM adapter_versions WHERE id = ? AND repo_id = ? AND tenant_id = ?",
            )
            .bind(&id)
            .bind(repo_id)
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            current = row.and_then(|(parent,)| parent);
        }

        Ok(())
    }

    async fn insert_version_history(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        entry: VersionHistoryEntry<'_>,
    ) -> Result<()> {
        let new_state = normalize_release_state(entry.new_state);
        let old_state = entry.old_state.map(normalize_release_state);

        validate_release_transition(old_state.as_deref(), &new_state)?;

        let id = Uuid::now_v7().to_string();

        tx.execute(
            sqlx::query(
                r#"
                INSERT INTO adapter_version_history (
                    id, repo_id, tenant_id, version_id, branch,
                    old_state, new_state, actor, reason, train_job_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(entry.repo_id)
            .bind(entry.tenant_id)
            .bind(entry.version_id)
            .bind(entry.branch)
            .bind(old_state.as_deref())
            .bind(&new_state)
            .bind(entry.actor)
            .bind(entry.reason)
            .bind(entry.train_job_id),
        )
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    async fn derive_adapter_trust_state_from_dataset_versions(
        &self,
        dataset_version_ids: &[String],
    ) -> Result<String> {
        if dataset_version_ids.is_empty() {
            return Ok("unknown".to_string());
        }

        let mut trust_states = Vec::with_capacity(dataset_version_ids.len());
        for ds_ver in dataset_version_ids {
            match self.get_effective_trust_state(ds_ver).await? {
                Some(state) => trust_states.push(state),
                None => {
                    return Err(AosError::Validation(format!(
                        "dataset version {} not found",
                        ds_ver
                    )))
                }
            }
        }

        Ok(map_dataset_trust_to_adapter_trust(&trust_states))
    }

    /// Derive adapter trust state from dataset versions using an existing transaction.
    ///
    /// This variant avoids acquiring new pool connections, preventing pool exhaustion
    /// when called within an outer transaction.
    async fn derive_adapter_trust_state_from_dataset_versions_with_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        dataset_version_ids: &[String],
    ) -> Result<String> {
        if dataset_version_ids.is_empty() {
            return Ok("unknown".to_string());
        }

        let mut trust_states = Vec::with_capacity(dataset_version_ids.len());
        for ds_ver in dataset_version_ids {
            match self.get_effective_trust_state_with_tx(tx, ds_ver).await? {
                Some(state) => trust_states.push(state),
                None => {
                    return Err(AosError::Validation(format!(
                        "dataset version {} not found",
                        ds_ver
                    )))
                }
            }
        }

        Ok(map_dataset_trust_to_adapter_trust(&trust_states))
    }

    async fn set_adapter_trust_state(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        version_id: &str,
        trust_state: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE adapter_versions SET adapter_trust_state = ? WHERE id = ?")
            .bind(trust_state)
            .bind(version_id)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    async fn recompute_adapter_trust_for_version(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        version_id: &str,
    ) -> Result<String> {
        let dataset_version_ids = sqlx::query_scalar::<Sqlite, String>(
            r#"
            SELECT dataset_version_id
            FROM adapter_version_dataset_versions
            WHERE adapter_version_id = ?
            ORDER BY dataset_version_id
            "#,
        )
        .bind(version_id)
        .fetch_all(tx.as_mut())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let trust_state = self
            .derive_adapter_trust_state_from_dataset_versions(&dataset_version_ids)
            .await?;
        self.set_adapter_trust_state(tx, version_id, &trust_state)
            .await?;
        Ok(trust_state)
    }

    /// Propagate dataset trust changes to linked adapter versions.
    pub async fn propagate_dataset_trust_change(
        &self,
        dataset_version_id: &str,
        previous_effective: Option<&str>,
        new_effective: &str,
    ) -> Result<Vec<String>> {
        let prev_norm = previous_effective.map(normalize_adapter_trust_state);
        let new_norm = normalize_adapter_trust_state(new_effective);
        let blocked_transition = new_norm == "blocked" && prev_norm.as_deref() != Some("blocked");

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let linked_versions: Vec<(String, String, String, String)> = sqlx::query_as(
            r#"
            SELECT avdv.adapter_version_id, av.repo_id, av.tenant_id, av.branch
            FROM adapter_version_dataset_versions avdv
            JOIN adapter_versions av ON avdv.adapter_version_id = av.id
            WHERE avdv.dataset_version_id = ?
            "#,
        )
        .bind(dataset_version_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let mut regressed_versions: Vec<(String, String, String, String)> = Vec::new();
        for (version_id, repo_id, tenant_id, branch) in linked_versions.iter() {
            if blocked_transition {
                self.set_adapter_trust_state(&mut tx, version_id, "blocked_regressed")
                    .await?;
                regressed_versions.push((
                    version_id.clone(),
                    repo_id.clone(),
                    tenant_id.clone(),
                    branch.clone(),
                ));
            } else {
                self.recompute_adapter_trust_for_version(&mut tx, version_id)
                    .await?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        for (version_id, repo_id, tenant_id, branch) in regressed_versions.iter() {
            if let Some(policy) = self
                .get_adapter_repository_policy(tenant_id, repo_id)
                .await?
            {
                if policy.auto_rollback_on_trust_regress {
                    let active_version: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM adapter_versions WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active' LIMIT 1",
                    )
                    .bind(repo_id)
                    .bind(tenant_id)
                    .bind(branch)
                    .fetch_optional(self.pool())
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?;

                    if active_version.as_deref() == Some(version_id.as_str()) {
                        let rollback_target: Option<String> = sqlx::query_scalar(
                            r#"
                            SELECT id
                            FROM adapter_versions
                            WHERE repo_id = ?
                              AND tenant_id = ?
                              AND branch = ?
                              AND id != ?
                              AND adapter_trust_state NOT IN ('blocked', 'blocked_regressed')
                              AND release_state IN ('ready', 'active', 'deprecated')
                            ORDER BY created_at DESC
                            LIMIT 1
                            "#,
                        )
                        .bind(repo_id)
                        .bind(tenant_id)
                        .bind(branch)
                        .bind(version_id)
                        .fetch_optional(self.pool())
                        .await
                        .map_err(|e| AosError::Database(e.to_string()))?;

                        if let Some(target) = rollback_target {
                            let reason = Some("auto rollback on dataset trust regression");
                            self.rollback_adapter_branch(
                                tenant_id, repo_id, branch, &target, None, reason,
                            )
                            .await?;
                        }
                    }
                }
            }
        }

        Ok(linked_versions
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect())
    }

    pub async fn create_adapter_repository(
        &self,
        params: CreateRepositoryParams<'_>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let default_branch = params.default_branch.unwrap_or("main");

        sqlx::query(
            r#"
            INSERT INTO adapter_repositories (
                id, tenant_id, name, base_model_id, default_branch, archived, created_by, description
            ) VALUES (?, ?, ?, ?, ?, 0, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(params.tenant_id)
        .bind(params.name)
        .bind(params.base_model_id)
        .bind(default_branch)
        .bind(params.created_by)
        .bind(params.description)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    pub async fn get_adapter_repository(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<AdapterRepository>> {
        let repo = sqlx::query_as::<_, AdapterRepository>(
            r#"
            SELECT id, tenant_id, name, base_model_id, default_branch, archived,
                   created_by, created_at, description
            FROM adapter_repositories
            WHERE id = ? AND tenant_id = ?
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(repo)
    }

    pub async fn list_adapter_repositories(
        &self,
        tenant_id: &str,
        base_model_id: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Vec<AdapterRepository>> {
        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, tenant_id, name, base_model_id, default_branch, archived,
                   created_by, created_at, description
            FROM adapter_repositories
            WHERE tenant_id = 
            "#,
        );

        qb.push_bind(tenant_id);

        if let Some(model_id) = base_model_id {
            qb.push(" AND base_model_id = ").push_bind(model_id);
        }

        if let Some(archived_flag) = archived {
            qb.push(" AND archived = ")
                .push_bind(if archived_flag { 1 } else { 0 });
        }

        qb.push(" ORDER BY name");

        let repos = qb
            .build_query_as::<AdapterRepository>()
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(repos)
    }

    pub async fn get_adapter_repository_policy(
        &self,
        tenant_id: &str,
        repo_id: &str,
    ) -> Result<Option<AdapterRepositoryPolicy>> {
        let policy = sqlx::query_as::<_, AdapterRepositoryPolicyRow>(
            r#"
            SELECT repo_id, tenant_id, preferred_backends_json, coreml_allowed, coreml_required,
                   autopromote_coreml, coreml_mode, repo_tier, auto_rollback_on_trust_regress,
                   created_at
            FROM adapter_repository_policies
            WHERE repo_id = ? AND tenant_id = ?
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .map(AdapterRepositoryPolicy::try_from)
        .transpose()?;

        Ok(policy)
    }

    pub async fn upsert_adapter_repository_policy(
        &self,
        params: UpsertAdapterRepositoryPolicyParams<'_>,
    ) -> Result<()> {
        let coreml_mode = params.coreml_mode.map(|mode| mode.as_str().to_string());
        let repo_tier = params.repo_tier.map(|tier| tier.as_str().to_string());

        sqlx::query(
            r#"
            INSERT INTO adapter_repository_policies (
                repo_id, tenant_id, preferred_backends_json, coreml_allowed, coreml_required, autopromote_coreml,
                coreml_mode, repo_tier, auto_rollback_on_trust_regress
            )
            VALUES (
                ?, ?, ?, COALESCE(?, 1), COALESCE(?, 0), COALESCE(?, 0),
                COALESCE(?, 'coreml_preferred'), COALESCE(?, 'normal'), COALESCE(?, 0)
            )
            ON CONFLICT(repo_id) DO UPDATE SET
                preferred_backends_json = COALESCE(excluded.preferred_backends_json, adapter_repository_policies.preferred_backends_json),
                coreml_allowed = COALESCE(excluded.coreml_allowed, adapter_repository_policies.coreml_allowed),
                coreml_required = COALESCE(excluded.coreml_required, adapter_repository_policies.coreml_required),
                autopromote_coreml = COALESCE(excluded.autopromote_coreml, adapter_repository_policies.autopromote_coreml),
                coreml_mode = COALESCE(excluded.coreml_mode, adapter_repository_policies.coreml_mode),
                repo_tier = COALESCE(excluded.repo_tier, adapter_repository_policies.repo_tier),
                auto_rollback_on_trust_regress = COALESCE(excluded.auto_rollback_on_trust_regress, adapter_repository_policies.auto_rollback_on_trust_regress)
            "#,
        )
        .bind(params.repo_id)
        .bind(params.tenant_id)
        .bind(params.preferred_backends_json)
        .bind(params.coreml_allowed)
        .bind(params.coreml_required)
        .bind(params.autopromote_coreml)
        .bind(coreml_mode)
        .bind(repo_tier)
        .bind(params.auto_rollback_on_trust_regress)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn create_adapter_version(&self, params: CreateVersionParams<'_>) -> Result<String> {
        // Ensure repository exists and is not archived
        let archived: Option<i64> =
            sqlx::query("SELECT archived FROM adapter_repositories WHERE id = ? AND tenant_id = ?")
                .bind(params.repo_id)
                .bind(params.tenant_id)
                .map(|row: sqlx::sqlite::SqliteRow| row.get::<i64, _>("archived"))
                .fetch_optional(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

        match archived {
            None => {
                return Err(AosError::NotFound(format!(
                    "adapter repository {}",
                    params.repo_id
                )))
            }
            Some(1) if !params.allow_archived => {
                return Err(AosError::Validation(
                    "repository is archived; new versions are frozen".to_string(),
                ))
            }
            _ => {}
        }

        // System-owned repositories must record a concrete code snapshot for traceability
        if params.tenant_id == "system"
            && params
                .code_commit_sha
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(AosError::Validation(
                "code_commit_sha is required for system repositories".to_string(),
            ));
        }

        validate_branch_classification(params.branch_classification)?;

        validate_release_transition(None, params.release_state)?;

        let id = Uuid::now_v7().to_string();
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        if normalize_release_state(params.release_state) == "active" {
            let existing: Option<String> = sqlx::query_scalar::<Sqlite, String>(
                r#"
                SELECT id FROM adapter_versions
                WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active' AND id != ?
                LIMIT 1
                "#,
            )
            .bind(params.repo_id)
            .bind(params.tenant_id)
            .bind(params.branch)
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if let Some(existing_id) = existing {
                return Err(AosError::Validation(format!(
                    "branch {} already has active version {}",
                    params.branch, existing_id
                )));
            }
        }

        let adapter_trust_state = if let Some(dataset_ids) = params.dataset_version_ids {
            self.derive_adapter_trust_state_from_dataset_versions_with_tx(&mut tx, dataset_ids)
                .await?
        } else {
            "unknown".to_string()
        };

        sqlx::query(
            r#"
            INSERT INTO adapter_versions (
                id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                manifest_schema_version, parent_version_id, code_commit_sha,
                data_spec_hash, training_backend, coreml_used, coreml_device_type,
                adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(params.repo_id)
        .bind(params.tenant_id)
        .bind(params.version)
        .bind(params.branch)
        .bind(params.branch_classification)
        .bind(params.aos_path)
        .bind(params.aos_hash)
        .bind(params.manifest_schema_version)
        .bind(params.parent_version_id)
        .bind(params.code_commit_sha)
        .bind(params.data_spec_hash)
        .bind(params.training_backend)
        .bind(params.coreml_used.unwrap_or(false))
        .bind(params.coreml_device_type)
        .bind(&adapter_trust_state)
        .bind(params.release_state)
        .bind(params.metrics_snapshot_id)
        .bind(params.evaluation_summary)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(dataset_ids) = params.dataset_version_ids {
            self.upsert_adapter_version_dataset_versions_with_tx(
                &mut tx,
                params.tenant_id,
                &id,
                dataset_ids,
            )
            .await?;
        }

        self.insert_version_history(
            &mut tx,
            VersionHistoryEntry {
                repo_id: params.repo_id,
                tenant_id: params.tenant_id,
                branch: params.branch,
                version_id: &id,
                old_state: None,
                new_state: params.release_state,
                actor: params.actor,
                reason: params.reason,
                train_job_id: params.train_job_id,
            },
        )
        .await?;

        sqlx::query(
            r#"
            INSERT INTO adapter_version_runtime_state (version_id, runtime_state, updated_at, worker_id, last_error)
            VALUES (?, 'unloaded', datetime('now'), NULL, NULL)
            ON CONFLICT(version_id) DO NOTHING
            "#,
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Create a draft adapter version (no artifact yet).
    pub async fn create_adapter_draft_version(
        &self,
        params: CreateDraftVersionParams<'_>,
    ) -> Result<String> {
        if let Some(ids) = params.dataset_version_ids {
            if ids.is_empty() {
                return Err(AosError::Validation(
                    "dataset_version_ids cannot be empty for adapter versions".to_string(),
                ));
            }
            if params.data_spec_hash.is_none() {
                return Err(AosError::Validation(
                    "data_spec_hash required when linking dataset_version_ids".to_string(),
                ));
            }
        }

        // Ensure repository exists and is not archived
        let repo_row: Option<(i64, String)> = sqlx::query_as(
            "SELECT archived, default_branch FROM adapter_repositories WHERE id = ? AND tenant_id = ?",
        )
        .bind(params.repo_id)
        .bind(params.tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let default_branch = match repo_row {
            None => {
                return Err(AosError::NotFound(format!(
                    "adapter repository {}",
                    params.repo_id
                )))
            }
            Some((archived, branch)) => {
                if archived == 1 {
                    return Err(AosError::Validation(
                        "repository is archived; drafts are frozen".to_string(),
                    ));
                }
                branch
            }
        };

        validate_branch_classification(params.branch_classification)?;

        if let Some(parent) = params.parent_version_id {
            let parent_repo: Option<(String,)> = sqlx::query_as(
                "SELECT repo_id FROM adapter_versions WHERE id = ? AND tenant_id = ?",
            )
            .bind(parent)
            .bind(params.tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if parent_repo
                .as_ref()
                .map(|(r,)| r != params.repo_id)
                .unwrap_or(false)
            {
                return Err(AosError::Validation(
                    "parent_version_id must belong to the same repository".to_string(),
                ));
            }

            self.ensure_no_version_cycle(parent, params.repo_id, params.tenant_id)
                .await?;
        }

        let id = Uuid::now_v7().to_string();
        // Draft versions get a deterministic placeholder version string to satisfy uniqueness.
        let version_label = format!("draft-{}", &id[..8]);
        let branch = if params.branch.is_empty() {
            default_branch
        } else {
            params.branch.to_string()
        };

        validate_release_transition(None, "draft")?;

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let adapter_trust_state = if let Some(dataset_ids) = params.dataset_version_ids {
            self.derive_adapter_trust_state_from_dataset_versions_with_tx(&mut tx, dataset_ids)
                .await?
        } else {
            "unknown".to_string()
        };

        sqlx::query(
            r#"
            INSERT INTO adapter_versions (
                id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                manifest_schema_version, parent_version_id, code_commit_sha,
                data_spec_hash, training_backend, coreml_used, coreml_device_type,
                adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary
            )
            VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, NULL, ?, ?, ?, ?, 0, NULL, ?, 'draft', NULL, NULL)
            "#,
        )
        .bind(&id)
        .bind(params.repo_id)
        .bind(params.tenant_id)
        .bind(&version_label)
        .bind(&branch)
        .bind(params.branch_classification)
        .bind(params.parent_version_id)
        .bind(params.code_commit_sha)
        .bind(params.data_spec_hash)
        .bind(params.training_backend)
        .bind(&adapter_trust_state)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(dataset_ids) = params.dataset_version_ids {
            self.upsert_adapter_version_dataset_versions_with_tx(
                &mut tx,
                params.tenant_id,
                &id,
                dataset_ids,
            )
            .await?;
        }

        self.insert_version_history(
            &mut tx,
            VersionHistoryEntry {
                repo_id: params.repo_id,
                tenant_id: params.tenant_id,
                branch: &branch,
                version_id: &id,
                old_state: None,
                new_state: "draft",
                actor: params.actor,
                reason: params.reason,
                train_job_id: None,
            },
        )
        .await?;

        sqlx::query(
            r#"
            INSERT INTO adapter_version_runtime_state (version_id, runtime_state, updated_at, worker_id, last_error)
            VALUES (?, 'unloaded', datetime('now'), NULL, NULL)
            ON CONFLICT(version_id) DO NOTHING
            "#,
        )
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    pub async fn get_adapter_version(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<AdapterVersion>> {
        let version = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ? AND tenant_id = ?
            "#,
        )
        .bind(version_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(version)
    }

    pub async fn list_adapter_versions_for_repo(
        &self,
        tenant_id: &str,
        repo_id: &str,
        branch: Option<&str>,
        release_states: Option<&[&str]>,
    ) -> Result<Vec<AdapterVersion>> {
        let mut qb = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE tenant_id =
            "#,
        );

        qb.push_bind(tenant_id)
            .push(" AND repo_id = ")
            .push_bind(repo_id);

        if let Some(branch) = branch {
            qb.push(" AND branch = ").push_bind(branch);
        }

        if let Some(states) = release_states {
            if !states.is_empty() {
                qb.push(" AND release_state IN (");
                let mut separated = qb.separated(", ");
                for state in states {
                    separated.push_bind(*state);
                }
                qb.push(")");
            }
        }

        qb.push(" ORDER BY created_at DESC");

        let versions: Vec<AdapterVersion> = qb
            .build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let mut versions = versions;
        versions.sort_by(|a, b| compare_versions_desc(a, b));

        Ok(versions)
    }

    pub async fn upsert_adapter_version_runtime_state(
        &self,
        params: UpsertRuntimeStateParams<'_>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO adapter_version_runtime_state (version_id, runtime_state, updated_at, worker_id, last_error)
            VALUES (?, ?, datetime('now'), ?, ?)
            ON CONFLICT(version_id) DO UPDATE SET
                runtime_state = excluded.runtime_state,
                updated_at = excluded.updated_at,
                worker_id = excluded.worker_id,
                last_error = excluded.last_error
            "#,
        )
        .bind(params.version_id)
        .bind(params.runtime_state)
        .bind(params.worker_id)
        .bind(params.last_error)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_adapter_version_runtime_state(
        &self,
        version_id: &str,
    ) -> Result<Option<AdapterVersionRuntimeState>> {
        let state = sqlx::query_as::<_, AdapterVersionRuntimeState>(
            r#"
            SELECT version_id, runtime_state, updated_at, worker_id, last_error
            FROM adapter_version_runtime_state
            WHERE version_id = ?
            "#,
        )
        .bind(version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(state)
    }

    pub async fn update_adapter_version_artifact(
        &self,
        version_id: &str,
        release_state: &str,
        aos_path: Option<&str>,
        aos_hash: Option<&str>,
        data_spec_hash: Option<&str>,
        training_backend: Option<&str>,
        coreml_used: Option<bool>,
        coreml_device_type: Option<&str>,
        metrics_snapshot_id: Option<&str>,
        evaluation_summary: Option<&str>,
        actor: Option<&str>,
        reason: Option<&str>,
        train_job_id: Option<&str>,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let version = sqlx::query_as::<Sqlite, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ?
            "#,
        )
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("adapter version {}", version_id)))?;
        validate_release_transition(Some(&version.release_state), release_state)?;

        if normalize_release_state(release_state) == "active" {
            let existing: Option<String> = sqlx::query_scalar::<Sqlite, String>(
                r#"
                SELECT id FROM adapter_versions
                WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active' AND id != ?
                LIMIT 1
                "#,
            )
            .bind(&version.repo_id)
            .bind(&version.tenant_id)
            .bind(&version.branch)
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if let Some(existing_id) = existing {
                return Err(AosError::Validation(format!(
                    "branch {} already has active version {}",
                    version.branch, existing_id
                )));
            }
        }

        if normalize_release_state(release_state) == "active" {
            let existing: Option<String> = sqlx::query_scalar::<Sqlite, String>(
                r#"
                SELECT id FROM adapter_versions
                WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active' AND id != ?
                LIMIT 1
                "#,
            )
            .bind(&version.repo_id)
            .bind(&version.tenant_id)
            .bind(&version.branch)
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if let Some(existing_id) = existing {
                return Err(AosError::Validation(format!(
                    "branch {} already has active version {}",
                    version.branch, existing_id
                )));
            }
        }

        sqlx::query(
            r#"
            UPDATE adapter_versions
            SET release_state = ?,
                aos_path = COALESCE(?, aos_path),
                aos_hash = COALESCE(?, aos_hash),
                data_spec_hash = COALESCE(?, data_spec_hash),
                training_backend = COALESCE(?, training_backend),
                coreml_used = COALESCE(?, coreml_used),
                coreml_device_type = COALESCE(?, coreml_device_type),
                metrics_snapshot_id = COALESCE(?, metrics_snapshot_id),
                evaluation_summary = COALESCE(?, evaluation_summary)
            WHERE id = ?
            "#,
        )
        .bind(release_state)
        .bind(aos_path)
        .bind(aos_hash)
        .bind(data_spec_hash)
        .bind(training_backend)
        .bind(coreml_used)
        .bind(coreml_device_type)
        .bind(metrics_snapshot_id)
        .bind(evaluation_summary)
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        self.insert_version_history(
            &mut tx,
            VersionHistoryEntry {
                repo_id: &version.repo_id,
                tenant_id: &version.tenant_id,
                branch: &version.branch,
                version_id,
                old_state: Some(&version.release_state),
                new_state: release_state,
                actor,
                reason,
                train_job_id,
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Replace dataset version links for an adapter version (tenant-scoped).
    ///
    /// Captures the current effective trust_state for each dataset version as a
    /// snapshot (read-only mirror; manifest remains historical source).
    pub async fn upsert_adapter_version_dataset_versions(
        &self,
        tenant_id: &str,
        version_id: &str,
        dataset_version_ids: &[String],
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        self.upsert_adapter_version_dataset_versions_with_tx(
            &mut tx,
            tenant_id,
            version_id,
            dataset_version_ids,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Replace dataset version links using an existing transaction.
    ///
    /// This variant avoids starting a nested transaction, preventing pool exhaustion
    /// when called within an outer transaction (e.g., from `create_adapter_version`).
    pub async fn upsert_adapter_version_dataset_versions_with_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        tenant_id: &str,
        version_id: &str,
        dataset_version_ids: &[String],
    ) -> Result<()> {
        // Validate version belongs to tenant.
        let version_exists: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM adapter_versions WHERE id = ? AND tenant_id = ? LIMIT 1",
        )
        .bind(version_id)
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        if version_exists.is_none() {
            return Err(AosError::Validation(
                "adapter version not found for tenant".to_string(),
            ));
        }

        sqlx::query("DELETE FROM adapter_version_dataset_versions WHERE adapter_version_id = ?")
            .bind(version_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        for ds_ver in dataset_version_ids {
            // Snapshot trust at the time we link the dataset version.
            let trust_snapshot = self.get_effective_trust_state_with_tx(tx, ds_ver).await?;
            sqlx::query(
                "INSERT INTO adapter_version_dataset_versions (adapter_version_id, dataset_version_id, tenant_id, trust_at_training_time)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(version_id)
            .bind(ds_ver)
            .bind(tenant_id)
            .bind(trust_snapshot)
            .execute(&mut **tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        }

        let trust_state = self
            .derive_adapter_trust_state_from_dataset_versions_with_tx(tx, dataset_version_ids)
            .await?;
        self.set_adapter_trust_state(tx, version_id, &trust_state)
            .await?;

        Ok(())
    }

    pub async fn list_dataset_versions_for_adapter_version(
        &self,
        version_id: &str,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<Sqlite, String>(
            r#"
            SELECT dataset_version_id
            FROM adapter_version_dataset_versions
            WHERE adapter_version_id = ?
            ORDER BY dataset_version_id
            "#,
        )
        .bind(version_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rows)
    }

    /// List dataset versions using an existing transaction.
    pub async fn list_dataset_versions_for_adapter_version_with_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        version_id: &str,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<Sqlite, String>(
            r#"
            SELECT dataset_version_id
            FROM adapter_version_dataset_versions
            WHERE adapter_version_id = ?
            ORDER BY dataset_version_id
            "#,
        )
        .bind(version_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rows)
    }

    /// List dataset versions with captured trust snapshot for an adapter version.
    pub async fn list_dataset_versions_with_trust_for_adapter_version(
        &self,
        version_id: &str,
    ) -> Result<Vec<(String, Option<String>)>> {
        let rows = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT dataset_version_id, trust_at_training_time
            FROM adapter_version_dataset_versions
            WHERE adapter_version_id = ?
            ORDER BY dataset_version_id
            "#,
        )
        .bind(version_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(rows)
    }

    pub async fn set_adapter_version_state(
        &self,
        version_id: &str,
        release_state: &str,
        evaluation_summary: Option<&str>,
    ) -> Result<()> {
        self.set_adapter_version_state_with_metadata(
            version_id,
            release_state,
            evaluation_summary,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn set_adapter_version_state_with_metadata(
        &self,
        version_id: &str,
        release_state: &str,
        evaluation_summary: Option<&str>,
        actor: Option<&str>,
        reason: Option<&str>,
        train_job_id: Option<&str>,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let version = sqlx::query_as::<Sqlite, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ?
            "#,
        )
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("adapter version {}", version_id)))?;
        validate_release_transition(Some(&version.release_state), release_state)?;

        // Guard: prevent multiple active versions on the same branch
        let normalized_new_state = normalize_release_state(release_state);
        if normalized_new_state == "active" {
            let existing_active: Option<(String,)> = sqlx::query_as(
                r#"
                SELECT id FROM adapter_versions
                WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active' AND id != ?
                LIMIT 1
                "#,
            )
            .bind(&version.repo_id)
            .bind(&version.tenant_id)
            .bind(&version.branch)
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if existing_active.is_some() {
                tx.rollback().await.ok();
                return Err(AosError::Validation(
                    "branch already has active version; use promote_adapter_version to deprecate it first"
                        .to_string(),
                ));
            }
        }

        sqlx::query(
            r#"
            UPDATE adapter_versions
            SET release_state = ?, evaluation_summary = COALESCE(?, evaluation_summary)
            WHERE id = ?
            "#,
        )
        .bind(release_state)
        .bind(evaluation_summary)
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        self.insert_version_history(
            &mut tx,
            VersionHistoryEntry {
                repo_id: &version.repo_id,
                tenant_id: &version.tenant_id,
                branch: &version.branch,
                version_id,
                old_state: Some(&version.release_state),
                new_state: release_state,
                actor,
                reason,
                train_job_id,
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn find_active_version_for_branch(
        &self,
        repo_id: &str,
        tenant_id: &str,
        branch: &str,
    ) -> Result<Option<AdapterVersion>> {
        let version = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .bind(branch)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(version)
    }

    pub async fn promote_adapter_version(
        &self,
        tenant_id: &str,
        repo_id: &str,
        version_id: &str,
        actor: Option<&str>,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let target_version = sqlx::query_as::<Sqlite, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ?
            "#,
        )
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("adapter version {}", version_id)))?;
        if target_version.repo_id != repo_id || target_version.tenant_id != tenant_id {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "version does not belong to the provided repository or tenant".to_string(),
            ));
        }

        let branch = target_version.branch.clone();

        let target_state = normalize_release_state(&target_version.release_state);
        if target_state != "ready" && target_state != "active" {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "promotion requires version in Ready or Active state".to_string(),
            ));
        }

        let linked_datasets = self
            .list_dataset_versions_for_adapter_version_with_tx(&mut tx, version_id)
            .await?;
        let is_legacy_unpinned =
            linked_datasets.is_empty() && target_version.data_spec_hash.is_none();
        let branch_classification = target_version.branch_classification.to_ascii_lowercase();
        if is_legacy_unpinned && branch_classification != "sandbox" {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "legacy_unpinned adapters can only be promoted on sandbox branches".to_string(),
            ));
        }
        if target_version.coreml_used && target_version.coreml_device_type.is_none() {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "coreml_used=true requires coreml_device_type before promotion".to_string(),
            ));
        }
        if linked_datasets.is_empty() {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "promotion requires dataset_version_ids; synthetic/dataset-only adapters cannot be activated"
                    .to_string(),
            ));
        }
        if !linked_datasets.is_empty() && target_version.data_spec_hash.is_none() {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "data_spec_hash is required when dataset_version_ids are present".to_string(),
            ));
        }
        if target_version.coreml_used && linked_datasets.is_empty() {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "coreml-trained adapters must record dataset_version_ids".to_string(),
            ));
        }

        // Deprecate existing active version (if any)
        let existing_active: Option<AdapterVersion> = sqlx::query_as(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active'
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .bind(&branch)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(active) = existing_active {
            if active.id != target_version.id {
                validate_release_transition(Some(&active.release_state), "deprecated")?;
                sqlx::query(
                    r#"
                    UPDATE adapter_versions
                    SET release_state = 'deprecated'
                    WHERE id = ?
                    "#,
                )
                .bind(&active.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

                self.insert_version_history(
                    &mut tx,
                    VersionHistoryEntry {
                        repo_id,
                        tenant_id,
                        branch: &branch,
                        version_id: &active.id,
                        old_state: Some(&active.release_state),
                        new_state: "deprecated",
                        actor,
                        reason,
                        train_job_id: None,
                    },
                )
                .await?;
            }
        }

        // Promote target version
        if normalize_release_state(&target_version.release_state) != "active" {
            validate_release_transition(Some(&target_version.release_state), "active")?;
            sqlx::query(
                r#"
                UPDATE adapter_versions
                SET release_state = 'active'
                WHERE id = ? AND tenant_id = ?
                "#,
            )
            .bind(version_id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            self.insert_version_history(
                &mut tx,
                VersionHistoryEntry {
                    repo_id,
                    tenant_id,
                    branch: &branch,
                    version_id,
                    old_state: Some(&target_version.release_state),
                    new_state: "active",
                    actor,
                    reason,
                    train_job_id: None,
                },
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// List repositories grouped by base model.
    pub async fn list_adapter_repositories_grouped(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<RepositoryGroup>> {
        let repos = self
            .list_adapter_repositories(tenant_id, None, None)
            .await?;
        let mut grouped = Vec::<RepositoryGroup>::new();

        for repo in repos {
            if let Some(group) = grouped
                .iter_mut()
                .find(|g| g.base_model_id == repo.base_model_id)
            {
                group.repositories.push(repo);
            } else {
                grouped.push(RepositoryGroup {
                    base_model_id: repo.base_model_id.clone(),
                    repositories: vec![repo],
                });
            }
        }

        // Stable ordering within groups by name for deterministic output
        for group in &mut grouped {
            group
                .repositories
                .sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        }

        // Order groups by base_model_id for consistent responses
        grouped.sort_by(|a, b| a.base_model_id.cmp(&b.base_model_id));
        Ok(grouped)
    }

    /// Resolve a version selector to a concrete version record.
    ///
    /// Selector formats:
    /// - None or "" => default branch (Active, else latest Ready)
    /// - "branch" => Active on branch, else latest Ready
    /// - "branch@version" => exact branch + version match
    /// - "tag" => match by version across branches (prefers Active, then Ready, then Deprecated)
    pub async fn resolve_adapter_version(
        &self,
        tenant_id: &str,
        repo_id: &str,
        selector: Option<&str>,
    ) -> Result<Option<AdapterVersion>> {
        let repo = match self.get_adapter_repository(tenant_id, repo_id).await? {
            Some(r) => r,
            None => return Ok(None),
        };

        let selector = selector.unwrap_or_default().trim();

        if let Some(tag) = selector.strip_prefix("tag:") {
            return self
                .find_version_by_tag(tenant_id, repo_id, tag.trim())
                .await;
        }

        // branch@version
        if let Some((branch, version)) = selector.split_once('@') {
            return self
                .get_version_by_branch_version(tenant_id, repo_id, branch.trim(), version.trim())
                .await;
        }

        // Branch-only selector (default)
        let branch = if selector.is_empty() {
            repo.default_branch.clone()
        } else {
            selector.to_string()
        };

        if let Some(version) = self
            .select_active_or_ready(tenant_id, repo_id, &branch)
            .await?
        {
            return Ok(Some(version));
        }

        // If branch resolution failed and selector was not empty, treat it as tag lookup
        if !selector.is_empty() {
            return self.find_version_by_tag(tenant_id, repo_id, selector).await;
        }

        Ok(None)
    }

    /// Roll back a branch to a target version (Ready or Deprecated → Active).
    pub async fn rollback_adapter_branch(
        &self,
        tenant_id: &str,
        repo_id: &str,
        branch: &str,
        target_version_id: &str,
        actor: Option<&str>,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let target_version = sqlx::query_as::<Sqlite, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ?
            "#,
        )
        .bind(target_version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("adapter version {}", target_version_id)))?;
        if target_version.repo_id != repo_id
            || target_version.tenant_id != tenant_id
            || target_version.branch != branch
        {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "rollback target must belong to the provided repository/branch".to_string(),
            ));
        }

        let target_state_lower = normalize_release_state(&target_version.release_state);
        if target_state_lower != "ready" && target_state_lower != "deprecated" {
            return Err(AosError::Validation(
                "rollback target must be Ready or Deprecated".to_string(),
            ));
        }

        // Mark current active as deprecated (if any)
        let current_active: Option<AdapterVersion> = sqlx::query_as(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'active'
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .bind(branch)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(active) = current_active.as_ref() {
            validate_release_transition(Some(&active.release_state), "deprecated")?;
            sqlx::query("UPDATE adapter_versions SET release_state = 'deprecated' WHERE id = ?")
                .bind(&active.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;

            self.insert_version_history(
                &mut tx,
                VersionHistoryEntry {
                    repo_id,
                    tenant_id,
                    branch,
                    version_id: &active.id,
                    old_state: Some(&active.release_state),
                    new_state: "deprecated",
                    actor,
                    reason,
                    train_job_id: None,
                },
            )
            .await?;
        }

        // Promote target to active
        validate_release_transition(Some(&target_version.release_state), "active")?;
        sqlx::query("UPDATE adapter_versions SET release_state = 'active' WHERE id = ?")
            .bind(target_version_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        self.insert_version_history(
            &mut tx,
            VersionHistoryEntry {
                repo_id,
                tenant_id,
                branch,
                version_id: target_version_id,
                old_state: Some(&target_version.release_state),
                new_state: "active",
                actor,
                reason,
                train_job_id: None,
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mark a promotion as failed and restore the last deprecated version on the branch, if any.
    pub async fn rollback_failed_promotion(
        &self,
        tenant_id: &str,
        repo_id: &str,
        branch: &str,
        failed_version_id: &str,
        actor: Option<&str>,
        reason: Option<&str>,
    ) -> Result<Option<String>> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        let failed_version = sqlx::query_as::<Sqlite, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ?
            "#,
        )
        .bind(failed_version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        .ok_or_else(|| AosError::NotFound(format!("adapter version {}", failed_version_id)))?;
        if failed_version.repo_id != repo_id
            || failed_version.tenant_id != tenant_id
            || failed_version.branch != branch
        {
            tx.rollback().await.ok();
            return Err(AosError::Validation(
                "failed version does not belong to the provided repository/branch".to_string(),
            ));
        }

        // Mark the failed version as failed (idempotent for already-failed states).
        if normalize_release_state(&failed_version.release_state) != "failed" {
            validate_release_transition(Some(&failed_version.release_state), "failed")?;
            sqlx::query(
                r#"
                UPDATE adapter_versions
                SET release_state = 'failed'
                WHERE id = ?
                "#,
            )
            .bind(failed_version_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            self.insert_version_history(
                &mut tx,
                VersionHistoryEntry {
                    repo_id,
                    tenant_id,
                    branch,
                    version_id: failed_version_id,
                    old_state: Some(&failed_version.release_state),
                    new_state: "failed",
                    actor,
                    reason,
                    train_job_id: None,
                },
            )
            .await?;
        }

        // Find the most recent deprecated version on the branch (previous active).
        let previous_active: Option<AdapterVersion> = sqlx::query_as(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE repo_id = ? AND tenant_id = ? AND branch = ? AND release_state = 'deprecated' AND id != ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .bind(branch)
        .bind(failed_version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(previous) = previous_active.as_ref() {
            validate_release_transition(Some(&previous.release_state), "active")?;
            sqlx::query(
                r#"
                UPDATE adapter_versions
                SET release_state = 'active'
                WHERE id = ?
                "#,
            )
            .bind(&previous.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            self.insert_version_history(
                &mut tx,
                VersionHistoryEntry {
                    repo_id,
                    tenant_id,
                    branch,
                    version_id: &previous.id,
                    old_state: Some(&previous.release_state),
                    new_state: "active",
                    actor,
                    reason,
                    train_job_id: None,
                },
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(previous_active.map(|v| v.id))
    }

    /// Tag a version for selector-based resolution.
    pub async fn upsert_adapter_version_tag(
        &self,
        tenant_id: &str,
        version_id: &str,
        tag_name: &str,
    ) -> Result<()> {
        let version = self.get_adapter_version(tenant_id, version_id).await?;
        let version = match version {
            Some(v) => v,
            None => {
                return Err(AosError::NotFound(format!(
                    "adapter version {}",
                    version_id
                )))
            }
        };

        let id = Uuid::now_v7().to_string();

        sqlx::query(
            r#"
            INSERT INTO adapter_version_tags (id, version_id, repo_id, tenant_id, tag_name)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(repo_id, tag_name) DO UPDATE SET
                version_id = excluded.version_id,
                tenant_id = excluded.tenant_id
            "#,
        )
        .bind(&id)
        .bind(version_id)
        .bind(&version.repo_id)
        .bind(tenant_id)
        .bind(tag_name)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Archive a repository, preventing new versions unless explicitly overridden.
    pub async fn archive_adapter_repository(&self, tenant_id: &str, repo_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE adapter_repositories
            SET archived = 1
            WHERE id = ? AND tenant_id = ?
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn select_active_or_ready(
        &self,
        tenant_id: &str,
        repo_id: &str,
        branch: &str,
    ) -> Result<Option<AdapterVersion>> {
        // Prefer Active
        if let Some(active) = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE tenant_id = ? AND repo_id = ? AND branch = ? AND release_state = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(repo_id)
        .bind(branch)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?
        {
            if is_serveable_version(&active) {
            return Ok(Some(active));
            }
        }

        // Fallback to latest Ready (semver-desc)
        let ready = self
            .list_adapter_versions_for_repo(tenant_id, repo_id, Some(branch), Some(&["ready"][..]))
            .await?;

        Ok(ready.into_iter().find(is_serveable_version))
    }

    async fn find_version_by_tag(
        &self,
        tenant_id: &str,
        repo_id: &str,
        tag: &str,
    ) -> Result<Option<AdapterVersion>> {
        let tagged_version: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT avt.version_id
            FROM adapter_version_tags avt
            WHERE avt.repo_id = ? AND avt.tenant_id = ? AND avt.tag_name = ?
            "#,
        )
        .bind(repo_id)
        .bind(tenant_id)
        .bind(tag)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some((version_id,)) = tagged_version {
            // Tag lookups should return the version regardless of state.
            // Tags are often used for release management (rollback targets, audit trails).
            return self.get_adapter_version(tenant_id, &version_id).await;
        }

        Ok(None)
    }

    async fn get_version_by_branch_version(
        &self,
        tenant_id: &str,
        repo_id: &str,
        branch: &str,
        version: &str,
    ) -> Result<Option<AdapterVersion>> {
        let version = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE tenant_id = ? AND repo_id = ? AND branch = ? AND version = ?
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(repo_id)
        .bind(branch)
        .bind(version)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        // Exact-match lookups should return the version regardless of state.
        // This allows resolving deprecated/retired versions for auditing, debugging, etc.
        Ok(version)
    }

    /// List adapter artifact paths and hashes for a tenant.
    pub async fn list_adapter_artifacts_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT aos_path, aos_hash
            FROM adapter_versions
            WHERE tenant_id = ?
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(rows)
    }

    /// List all adapter versions for reconciler checks.
    pub async fn list_all_adapter_versions(&self) -> Result<Vec<AdapterVersion>> {
        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let rows = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list adapter versions: {}", e)))?;

        Ok(rows)
    }

    /// Count adapter versions for a tenant.
    pub async fn count_adapter_versions_for_tenant(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) as cnt FROM adapter_versions WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(self.pool())
                .await
                .unwrap_or(0);
        Ok(count)
    }

    // ========================================================================
    // Adapter Publish + Attach Modes v1
    // ========================================================================

    /// Publish an adapter version with attach mode configuration.
    ///
    /// This marks the adapter version as published and configures its attach mode.
    /// - If `attach_mode` is "requires_dataset", validates the dataset version exists
    ///   and was used in training this adapter.
    /// - Sets `published_at` timestamp and `release_state` to "active".
    pub async fn publish_adapter_version(
        &self,
        tenant_id: &str,
        repo_id: &str,
        version_id: &str,
        attach_mode: &str,
        required_scope_dataset_version_id: Option<&str>,
        short_description: Option<&str>,
        _actor: Option<&str>,
    ) -> Result<()> {
        // Validate attach_mode
        if attach_mode != "free" && attach_mode != "requires_dataset" {
            return Err(AosError::Validation(format!(
                "invalid attach_mode: '{}', must be 'free' or 'requires_dataset'",
                attach_mode
            )));
        }

        // Validate requires_dataset constraints
        if attach_mode == "requires_dataset" {
            let ds_version_id = required_scope_dataset_version_id.ok_or_else(|| {
                AosError::Validation(
                    "required_scope_dataset_version_id is required when attach_mode is 'requires_dataset'".to_string()
                )
            })?;

            // Verify dataset version exists and belongs to tenant
            let ds_exists: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM training_dataset_versions WHERE id = ? AND tenant_id = ?",
            )
            .bind(ds_version_id)
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if ds_exists.is_none() {
                return Err(AosError::NotFound(format!(
                    "dataset version '{}' not found for tenant",
                    ds_version_id
                )));
            }

            // Verify dataset version was used in training this adapter
            let linked: Option<(String,)> = sqlx::query_as(
                r#"
                SELECT adapter_version_id
                FROM adapter_version_dataset_versions
                WHERE adapter_version_id = ? AND dataset_version_id = ?
                "#,
            )
            .bind(version_id)
            .bind(ds_version_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if linked.is_none() {
                return Err(AosError::Validation(
                    "required_scope_dataset_version_id must be a dataset version used in training this adapter".to_string()
                ));
            }
        } else if required_scope_dataset_version_id.is_some() {
            return Err(AosError::Validation(
                "required_scope_dataset_version_id must be NULL when attach_mode is 'free'"
                    .to_string(),
            ));
        }

        // Validate short_description length
        if let Some(desc) = short_description {
            if desc.len() > 280 {
                return Err(AosError::Validation(
                    "short_description must be 280 characters or less".to_string(),
                ));
            }
        }

        // Update version with publish fields
        let result = sqlx::query(
            r#"
            UPDATE adapter_versions
            SET attach_mode = ?,
                required_scope_dataset_version_id = ?,
                short_description = ?,
                published_at = datetime('now'),
                release_state = 'active'
            WHERE id = ? AND tenant_id = ? AND repo_id = ?
            "#,
        )
        .bind(attach_mode)
        .bind(required_scope_dataset_version_id)
        .bind(short_description)
        .bind(version_id)
        .bind(tenant_id)
        .bind(repo_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to publish adapter version: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "adapter version '{}' not found",
                version_id
            )));
        }

        Ok(())
    }

    /// Archive an adapter version.
    ///
    /// Sets `is_archived = true`. Archived versions are hidden from normal use
    /// but retain their lifecycle_state for audit purposes.
    pub async fn archive_adapter_version(&self, tenant_id: &str, version_id: &str) -> Result<()> {
        let result = sqlx::query(
            "UPDATE adapter_versions SET is_archived = 1 WHERE id = ? AND tenant_id = ?",
        )
        .bind(version_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to archive adapter version: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "adapter version '{}' not found",
                version_id
            )));
        }

        Ok(())
    }

    /// Unarchive an adapter version.
    ///
    /// Sets `is_archived = false`, making the version visible again.
    pub async fn unarchive_adapter_version(&self, tenant_id: &str, version_id: &str) -> Result<()> {
        let result = sqlx::query(
            "UPDATE adapter_versions SET is_archived = 0 WHERE id = ? AND tenant_id = ?",
        )
        .bind(version_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to unarchive adapter version: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!(
                "adapter version '{}' not found",
                version_id
            )));
        }

        Ok(())
    }

    /// Get the attach mode for an adapter version.
    ///
    /// Returns `(attach_mode, required_scope_dataset_version_id)` or None if not found.
    pub async fn get_adapter_version_attach_mode(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<(String, Option<String>)>> {
        let result: Option<(String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT attach_mode, required_scope_dataset_version_id
            FROM adapter_versions
            WHERE id = ? AND tenant_id = ?
            "#,
        )
        .bind(version_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get attach mode: {}", e)))?;

        Ok(result)
    }

    /// Get a single adapter version by ID with all publish fields.
    pub async fn get_adapter_version_full(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<AdapterVersion>> {
        let result = sqlx::query_as::<_, AdapterVersion>(
            r#"
            SELECT id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
                   manifest_schema_version, parent_version_id, code_commit_sha,
                   data_spec_hash, training_backend, coreml_used, coreml_device_type,
                   adapter_trust_state, release_state, metrics_snapshot_id, evaluation_summary, created_at,
                   attach_mode, required_scope_dataset_version_id, is_archived, published_at, short_description
            FROM adapter_versions
            WHERE id = ? AND tenant_id = ?
            "#
        )
        .bind(version_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter version: {}", e)))?;

        Ok(result)
    }
}

/// Repositories grouped by base model identifier (if any).
#[derive(Debug, Clone)]
pub struct RepositoryGroup {
    pub base_model_id: Option<String>,
    pub repositories: Vec<AdapterRepository>,
}

fn compare_versions_desc(a: &AdapterVersion, b: &AdapterVersion) -> std::cmp::Ordering {
    let parsed_a = parse_semver(&a.version);
    let parsed_b = parse_semver(&b.version);

    match (parsed_a, parsed_b) {
        (Some(va), Some(vb)) => vb.cmp(&va).then_with(|| b.created_at.cmp(&a.created_at)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.created_at.cmp(&a.created_at),
    }
}

fn parse_semver(version: &str) -> Option<Version> {
    let trimmed = version.trim_start_matches('v');
    Version::parse(trimmed).ok()
}

// Version history is captured via adapter_version_history (migration 0177).
