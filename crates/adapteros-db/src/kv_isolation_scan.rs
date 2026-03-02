//! Deterministic KV vs SQL tenant isolation scan.
//!
//! This module performs read-only scans of KV namespaces and compares
//! tenant-bearing fields against SQL ground truth to detect cross-tenant
//! leakage before KV-only cutover. It prioritizes deterministic ordering,
//! bounded resource usage (hash-based sampling), and emits structured
//! findings suitable for policy audit evidence.

use crate::messages_kv::{MessageKv, MessageKvRepository};
use crate::policy_audit_kv::PolicyAuditKvRepository;
use crate::tenants_kv::{TenantKvOps, TenantKvRepository};
use crate::Db;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_storage::entities::tenant::TenantKv;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

const DEFAULT_SAMPLE_RATE: f64 = 0.10;
const DEFAULT_MAX_FINDINGS: usize = 500;
const DEFAULT_HASH_SEED: &str = "kv_isolation_scan_seed_v1";

/// Scan configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct KvIsolationScanConfig {
    /// Probability (0.0-1.0) for sampling non-hot tables.
    pub sample_rate: f64,
    /// Maximum findings to record (across tenants).
    pub max_findings: usize,
    /// Hash seed to keep sampling deterministic.
    pub hash_seed: String,
}

impl Default for KvIsolationScanConfig {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            max_findings: DEFAULT_MAX_FINDINGS,
            hash_seed: DEFAULT_HASH_SEED.to_string(),
        }
    }
}

/// Type of issue detected during a scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KvIsolationIssue {
    /// KV entry refers to a different tenant than SQL ground truth.
    CrossTenantMismatch {
        expected_tenant: String,
        found_tenant: String,
    },
    /// KV entry exists but SQL does not.
    MissingInSql,
    /// SQL entry exists but KV does not.
    MissingInKv,
    /// Field differs between SQL and KV for the same id.
    FieldMismatch {
        field: String,
        sql_value: String,
        kv_value: String,
    },
    /// KV key prefix encodes a different tenant than the serialized value.
    PrefixValueMismatch {
        prefix_tenant: String,
        value_tenant: String,
    },
}

/// Single finding with evidence metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct KvIsolationFinding {
    pub tenant_id: String,
    pub domain: String,
    pub key: String,
    pub issue: KvIsolationIssue,
    /// Lightweight evidence blob (JSON) for audit attachment.
    pub evidence: serde_json::Value,
}

/// Per-tenant summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct KvIsolationTenantSummary {
    pub tenant_id: String,
    pub findings: usize,
    pub scanned: usize,
    pub sampled: usize,
}

/// Scan report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct KvIsolationScanReport {
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub started_at: DateTime<Utc>,
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub completed_at: DateTime<Utc>,
    pub sample_rate: f64,
    pub hash_seed: String,
    pub max_findings: usize,
    pub total_scanned: usize,
    pub total_sampled: usize,
    pub findings: Vec<KvIsolationFinding>,
    pub tenant_summaries: Vec<KvIsolationTenantSummary>,
    pub hot_tables_full_scan: Vec<String>,
}

impl KvIsolationScanReport {
    fn new(started_at: DateTime<Utc>, cfg: &KvIsolationScanConfig) -> Self {
        Self {
            started_at,
            completed_at: started_at,
            sample_rate: cfg.sample_rate,
            hash_seed: cfg.hash_seed.clone(),
            max_findings: cfg.max_findings,
            total_scanned: 0,
            total_sampled: 0,
            findings: Vec::new(),
            tenant_summaries: Vec::new(),
            hot_tables_full_scan: vec![
                "tenants_kv".to_string(),
                "messages_kv".to_string(),
                "policy_audit_kv".to_string(),
            ],
        }
    }
}

impl Db {
    /// Run a deterministic KV isolation scan against SQL ground truth.
    ///
    /// - Full scan: tenants_kv, messages_kv, policy_audit_kv
    /// - Sampling: reserved for future non-hot tables (controlled via config.sample_rate)
    /// - Read-only: no writes to SQL or KV
    pub async fn run_kv_isolation_scan(
        &self,
        config: KvIsolationScanConfig,
    ) -> Result<KvIsolationScanReport> {
        let Some(pool) = self.pool_opt() else {
            return Err(AosError::Config(
                "SQL pool required for KV isolation scan".to_string(),
            ));
        };

        let Some(kv) = self.kv_backend() else {
            return Err(AosError::Config(
                "KV backend required for KV isolation scan".to_string(),
            ));
        };

        let cfg = sanitize_config(config);
        let started = Utc::now();
        let mut report = KvIsolationScanReport::new(started, &cfg);

        // Preload SQL ground truth
        let sql_tenants = sql_tenants(pool).await?;
        let sql_messages = sql_messages(pool).await?;
        let sql_policy_audit = sql_policy_audit(pool).await?;

        let mut scanned_counts: HashMap<String, usize> = HashMap::new();

        // Hot table: tenants (full)
        let tenant_repo = TenantKvRepository::new(kv.backend().clone());
        let mut tenant_records = tenant_repo.list_tenants_kv().await.unwrap_or_default();
        tenant_records.sort_by(|a, b| a.id.cmp(&b.id));
        for tenant in tenant_records {
            report.total_scanned += 1;
            increment_scanned(&mut scanned_counts, &tenant.id);
            evaluate_tenant_entry(&tenant, &sql_tenants, &mut report);
        }

        // Hot table: messages (full)
        let _message_repo = MessageKvRepository::new(kv.backend().clone());
        let mut message_keys = kv
            .backend()
            .scan_prefix("message:")
            .await
            .unwrap_or_default();
        message_keys.sort();
        for key in message_keys {
            if let Some(kv_bytes) = kv
                .backend()
                .get(&key)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?
            {
                if let Ok(msg) = serde_json::from_slice::<MessageKv>(&kv_bytes) {
                    report.total_scanned += 1;
                    increment_scanned(&mut scanned_counts, &msg.from_tenant_id);
                    evaluate_message_entry(&msg, &sql_messages, &mut report);
                }
            }
        }

        // Hot table: policy audit (full, per-tenant)
        let policy_repo = PolicyAuditKvRepository::new(kv.backend().clone());
        let mut policy_seq_keys = kv
            .backend()
            .scan_prefix("tenant/")
            .await
            .unwrap_or_default();
        policy_seq_keys.retain(|k| k.contains("/policy_audit/seq/"));
        policy_seq_keys.sort();
        for key in policy_seq_keys {
            if let Some((prefix_tenant, seq_part)) = key.split_once("/policy_audit/seq/") {
                let tenant_id = prefix_tenant.trim_start_matches("tenant/").to_string();
                if let Some((_, entry_id)) = seq_part.rsplit_once(':') {
                    if let Some(entry) = policy_repo.get_entry(&tenant_id, entry_id).await? {
                        report.total_scanned += 1;
                        increment_scanned(&mut scanned_counts, &tenant_id);
                        evaluate_policy_entry(&tenant_id, &entry, &sql_policy_audit, &mut report);
                    }
                }
            }
        }

        // No additional sampled tables yet; keep totals aligned
        report.total_sampled = report.total_scanned;

        // Finalize summaries deterministically
        finalize_report(&mut report, scanned_counts);
        Ok(report)
    }
}

fn sanitize_config(mut cfg: KvIsolationScanConfig) -> KvIsolationScanConfig {
    if !(0.0..=1.0).contains(&cfg.sample_rate) {
        cfg.sample_rate = DEFAULT_SAMPLE_RATE;
    }
    if cfg.max_findings == 0 {
        cfg.max_findings = DEFAULT_MAX_FINDINGS;
    }
    if cfg.hash_seed.trim().is_empty() {
        cfg.hash_seed = DEFAULT_HASH_SEED.to_string();
    }
    cfg
}

async fn sql_tenants(pool: &sqlx::SqlitePool) -> Result<HashMap<String, TenantSqlRow>> {
    let mut tenants = HashMap::new();
    let rows = sqlx::query(r#"SELECT id, name, itar_flag, status, default_stack_id FROM tenants"#)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to load tenants: {}", e)))?;

    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AosError::Database(format!("tenants.id: {}", e)))?;
        let name: String = row
            .try_get("name")
            .map_err(|e| AosError::Database(format!("tenants.name for id {}: {}", id, e)))?;
        let status: String = row
            .try_get("status")
            .map_err(|e| AosError::Database(format!("tenants.status for id {}: {}", id, e)))?;
        let itar_flag: i64 = row
            .try_get("itar_flag")
            .map_err(|e| AosError::Database(format!("tenants.itar_flag for id {}: {}", id, e)))?;
        let default_stack_id: Option<String> = row.try_get("default_stack_id").map_err(|e| {
            AosError::Database(format!("tenants.default_stack_id for id {}: {}", id, e))
        })?;
        tenants.insert(
            id.clone(),
            TenantSqlRow {
                id,
                name,
                status,
                itar_flag: itar_flag != 0,
                default_stack_id,
            },
        );
    }
    Ok(tenants)
}

async fn sql_messages(pool: &sqlx::SqlitePool) -> Result<HashMap<String, MessageSqlRow>> {
    let mut messages = HashMap::new();
    let rows = sqlx::query(
        r#"SELECT id, workspace_id, from_user_id, from_tenant_id, thread_id FROM messages"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("Failed to load messages: {}", e)))?;

    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AosError::Database(format!("messages.id: {}", e)))?;
        let workspace_id: String = row.try_get("workspace_id").map_err(|e| {
            AosError::Database(format!("messages.workspace_id for id {}: {}", id, e))
        })?;
        let from_user_id: String = row.try_get("from_user_id").map_err(|e| {
            AosError::Database(format!("messages.from_user_id for id {}: {}", id, e))
        })?;
        let from_tenant_id: String = row.try_get("from_tenant_id").map_err(|e| {
            AosError::Database(format!("messages.from_tenant_id for id {}: {}", id, e))
        })?;
        let thread_id: Option<String> = row
            .try_get("thread_id")
            .map_err(|e| AosError::Database(format!("messages.thread_id for id {}: {}", id, e)))?;
        messages.insert(
            id.clone(),
            MessageSqlRow {
                id,
                workspace_id,
                from_user_id,
                from_tenant_id,
                thread_id,
            },
        );
    }
    Ok(messages)
}

async fn sql_policy_audit(pool: &sqlx::SqlitePool) -> Result<HashMap<String, PolicyAuditSqlRow>> {
    let mut entries = HashMap::new();
    let rows = sqlx::query(r#"SELECT id, tenant_id, chain_sequence FROM policy_audit_decisions"#)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to load policy audit: {}", e)))?;

    for row in rows {
        let id: String = row
            .try_get("id")
            .map_err(|e| AosError::Database(format!("policy_audit_decisions.id: {}", e)))?;
        let tenant_id: String = row.try_get("tenant_id").map_err(|e| {
            AosError::Database(format!(
                "policy_audit_decisions.tenant_id for id {}: {}",
                id, e
            ))
        })?;
        let chain_sequence: i64 = row.try_get("chain_sequence").map_err(|e| {
            AosError::Database(format!(
                "policy_audit_decisions.chain_sequence for id {}: {}",
                id, e
            ))
        })?;
        entries.insert(
            id.clone(),
            PolicyAuditSqlRow {
                id,
                tenant_id,
                chain_sequence,
            },
        );
    }
    Ok(entries)
}

#[allow(dead_code)]
fn should_sample(key: &str, cfg: &KvIsolationScanConfig) -> bool {
    #[allow(clippy::float_cmp)]
    if cfg.sample_rate >= 0.9999 {
        return true;
    }
    let hash = B3Hash::hash_multi(&[cfg.hash_seed.as_bytes(), key.as_bytes()]);
    let bytes = hash.to_bytes();
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    let value = u64::from_le_bytes(buf);
    let threshold = (cfg.sample_rate * u64::MAX as f64) as u64;
    value <= threshold
}

fn push_finding(report: &mut KvIsolationScanReport, finding: KvIsolationFinding) {
    if report.findings.len() >= report.max_findings {
        return;
    }
    report.findings.push(finding);
}

fn evaluate_tenant_entry(
    kv: &TenantKv,
    sql_tenants: &HashMap<String, TenantSqlRow>,
    report: &mut KvIsolationScanReport,
) {
    let Some(sql_row) = sql_tenants.get(&kv.id) else {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: kv.id.clone(),
                domain: "tenants_kv".to_string(),
                key: format!("tenant:{}", kv.id),
                issue: KvIsolationIssue::MissingInSql,
                evidence: serde_json::json!({ "kv_name": kv.name, "status": kv.status }),
            },
        );
        return;
    };

    if sql_row.id != kv.id {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: kv.id.clone(),
                domain: "tenants_kv".to_string(),
                key: format!("tenant:{}", kv.id),
                issue: KvIsolationIssue::CrossTenantMismatch {
                    expected_tenant: sql_row.id.clone(),
                    found_tenant: kv.id.clone(),
                },
                evidence: serde_json::json!({ "kv_status": kv.status, "sql_status": sql_row.status }),
            },
        );
    }

    if sql_row.name != kv.name {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: kv.id.clone(),
                domain: "tenants_kv".to_string(),
                key: format!("tenant:{}", kv.id),
                issue: KvIsolationIssue::FieldMismatch {
                    field: "name".to_string(),
                    sql_value: sql_row.name.clone(),
                    kv_value: kv.name.clone(),
                },
                evidence: serde_json::json!({ "sql": sql_row.name, "kv": kv.name }),
            },
        );
    }
}

fn evaluate_message_entry(
    msg: &MessageKv,
    sql_messages: &HashMap<String, MessageSqlRow>,
    report: &mut KvIsolationScanReport,
) {
    let Some(sql_row) = sql_messages.get(&msg.id) else {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: msg.from_tenant_id.clone(),
                domain: "messages_kv".to_string(),
                key: format!("message:{}", msg.id),
                issue: KvIsolationIssue::MissingInSql,
                evidence: serde_json::json!({
                    "workspace_id": msg.workspace_id,
                    "from_tenant_id": msg.from_tenant_id,
                    "from_user_id": msg.from_user_id,
                }),
            },
        );
        return;
    };

    if sql_row.from_tenant_id != msg.from_tenant_id {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: sql_row.from_tenant_id.clone(),
                domain: "messages_kv".to_string(),
                key: format!("message:{}", msg.id),
                issue: KvIsolationIssue::CrossTenantMismatch {
                    expected_tenant: sql_row.from_tenant_id.clone(),
                    found_tenant: msg.from_tenant_id.clone(),
                },
                evidence: serde_json::json!({
                    "workspace_id": msg.workspace_id,
                    "sql_from_user_id": sql_row.from_user_id,
                    "kv_from_user_id": msg.from_user_id,
                }),
            },
        );
    }
}

fn evaluate_policy_entry(
    tenant_id: &str,
    entry: &crate::policy_audit::PolicyAuditDecision,
    sql_policy: &HashMap<String, PolicyAuditSqlRow>,
    report: &mut KvIsolationScanReport,
) {
    let Some(sql_row) = sql_policy.get(&entry.id) else {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: tenant_id.to_string(),
                domain: "policy_audit_kv".to_string(),
                key: format!("policy_audit:{}", entry.id),
                issue: KvIsolationIssue::MissingInSql,
                evidence: serde_json::json!({
                    "kv_tenant_id": entry.tenant_id,
                    "decision": entry.decision,
                    "hook": entry.hook,
                }),
            },
        );
        return;
    };

    if entry.tenant_id != sql_row.tenant_id {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: tenant_id.to_string(),
                domain: "policy_audit_kv".to_string(),
                key: format!("policy_audit:{}", entry.id),
                issue: KvIsolationIssue::CrossTenantMismatch {
                    expected_tenant: sql_row.tenant_id.clone(),
                    found_tenant: entry.tenant_id.clone(),
                },
                evidence: serde_json::json!({
                    "chain_sequence": entry.chain_sequence,
                    "sql_sequence": sql_row.chain_sequence,
                }),
            },
        );
    }

    if tenant_id != entry.tenant_id {
        push_finding(
            report,
            KvIsolationFinding {
                tenant_id: tenant_id.to_string(),
                domain: "policy_audit_kv".to_string(),
                key: format!("policy_audit:{}", entry.id),
                issue: KvIsolationIssue::PrefixValueMismatch {
                    prefix_tenant: tenant_id.to_string(),
                    value_tenant: entry.tenant_id.clone(),
                },
                evidence: serde_json::json!({
                    "chain_sequence": entry.chain_sequence,
                    "hook": entry.hook,
                }),
            },
        );
    }
}

fn increment_scanned(map: &mut HashMap<String, usize>, tenant_id: &str) {
    let counter = map.entry(tenant_id.to_string()).or_insert(0);
    *counter += 1;
}

fn finalize_report(report: &mut KvIsolationScanReport, scanned_counts: HashMap<String, usize>) {
    report.completed_at = Utc::now();

    // Aggregate per-tenant summaries
    let mut tenant_counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for finding in &report.findings {
        let entry = tenant_counts
            .entry(finding.tenant_id.clone())
            .or_insert((0, 0));
        entry.0 += 1;
    }

    for (tenant, scanned) in scanned_counts {
        let issues = tenant_counts.get(&tenant).copied().unwrap_or((0, 0));
        report.tenant_summaries.push(KvIsolationTenantSummary {
            tenant_id: tenant.clone(),
            findings: issues.0,
            scanned,
            sampled: scanned, // sampling not yet distinct for hot tables
        });
    }

    report
        .tenant_summaries
        .sort_by(|a, b| a.tenant_id.cmp(&b.tenant_id));
    // Deterministic ordering of findings
    report.findings.sort_by(|a, b| {
        a.tenant_id
            .cmp(&b.tenant_id)
            .then_with(|| a.domain.cmp(&b.domain))
            .then_with(|| a.key.cmp(&b.key))
    });
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TenantSqlRow {
    id: String,
    name: String,
    status: String,
    itar_flag: bool,
    default_stack_id: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MessageSqlRow {
    id: String,
    workspace_id: String,
    from_user_id: String,
    from_tenant_id: String,
    thread_id: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PolicyAuditSqlRow {
    id: String,
    tenant_id: String,
    chain_sequence: i64,
}
