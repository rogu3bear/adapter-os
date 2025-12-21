use crate::state::AppState;
use adapteros_core::{AosError, Result};
use adapteros_db::{
    KvIsolationFinding, KvIsolationScanConfig, KvIsolationScanReport, KvIsolationTenantSummary,
};
use chrono::Utc;
use tracing::warn;

/// Run a KV isolation scan, update shared snapshot state, and emit audit entries.
pub async fn run_kv_isolation_scan(
    state: &AppState,
    cfg: KvIsolationScanConfig,
    trigger: &str,
) -> Result<KvIsolationScanReport> {
    let _guard = state.kv_isolation_lock.lock().await;

    {
        let mut snapshot = state
            .kv_isolation_snapshot
            .write()
            .map_err(|_| AosError::Database("Snapshot lock poisoned".to_string()))?;
        snapshot.running = true;
        snapshot.last_started_at = Some(Utc::now().to_rfc3339());
        snapshot.last_error = None;
    }

    let scan_res = state.db.run_kv_isolation_scan(cfg.clone()).await;

    match scan_res {
        Ok(report) => {
            {
                let mut snapshot = state
                    .kv_isolation_snapshot
                    .write()
                    .map_err(|_| AosError::Database("Snapshot lock poisoned".to_string()))?;
                snapshot.running = false;
                snapshot.last_completed_at = Some(report.completed_at.to_rfc3339());
                snapshot.last_report = Some(report.clone());
            }

            record_policy_audit(state, &report, trigger).await;
            Ok(report)
        }
        Err(err) => {
            let mut snapshot = state
                .kv_isolation_snapshot
                .write()
                .map_err(|_| AosError::Database("Snapshot lock poisoned".to_string()))?;
            snapshot.running = false;
            snapshot.last_error = Some(err.to_string());
            Err(err)
        }
    }
}

/// Default scan configuration honoring environment overrides.
pub fn kv_isolation_config_from_env() -> KvIsolationScanConfig {
    let mut cfg = KvIsolationScanConfig::default();

    if let Some(val) = std::env::var("AOS_KV_ISOLATION_SAMPLE_RATE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        cfg.sample_rate = val;
    }

    if let Some(val) = std::env::var("AOS_KV_ISOLATION_MAX_FINDINGS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        cfg.max_findings = val;
    }

    if let Ok(seed) = std::env::var("AOS_KV_ISOLATION_HASH_SEED") {
        if !seed.trim().is_empty() {
            cfg.hash_seed = seed;
        }
    }

    cfg
}

async fn record_policy_audit(state: &AppState, report: &KvIsolationScanReport, trigger: &str) {
    for KvIsolationTenantSummary {
        tenant_id,
        findings,
        ..
    } in &report.tenant_summaries
    {
        if *findings == 0 {
            continue;
        }

        let evidence: Vec<_> = report
            .findings
            .iter()
            .filter(|f| &f.tenant_id == tenant_id)
            .take(5)
            .map(redact_finding)
            .collect();

        let metadata = serde_json::json!({
            "sample_rate": report.sample_rate,
            "hash_seed": report.hash_seed,
            "findings": findings,
            "evidence": evidence,
            "completed_at": report.completed_at.to_rfc3339(),
            "trigger": trigger,
        });

        let reason = format!("{findings} KV isolation findings detected ({trigger})");
        if let Err(e) = state
            .db
            .log_policy_decision(
                tenant_id,
                "isolation",
                "kv.isolation_scan",
                "alert",
                Some(&reason),
                None,
                None,
                Some("kv"),
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            warn!(tenant_id = tenant_id, error = %e, "Failed to log KV isolation audit entry");
        }
    }
}

fn redact_finding(f: &KvIsolationFinding) -> serde_json::Value {
    serde_json::json!({
        "domain": f.domain,
        "key": f.key,
        "issue": &f.issue,
    })
}
