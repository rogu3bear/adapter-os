use adapteros_db::index_health::{DbstatIndexSummary, SqlitePageStats, TenantIndexCoverage};
use adapteros_server_api::AppState;
use sqlx::Row;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
struct DbIndexMonitorConfig {
    tick_secs: u64,
    dbstat_secs: u64,
    dbstat_top_n: usize,
    maintenance_enabled: bool,
    optimize_secs: u64,
    analyze_secs: u64,
    reindex_enabled: bool,
    reindex_cooldown_secs: u64,
    vacuum_enabled: bool,
    vacuum_cooldown_secs: u64,
    max_in_flight_for_light_maintenance: usize,
    max_in_flight_for_heavy_maintenance: usize,
    fragmentation_warn_ratio: f64,
    fragmentation_critical_ratio: f64,
    index_unused_warn_ratio: f64,
    index_unused_critical_ratio: f64,
    probe_warn_secs: f64,
    probe_critical_secs: f64,
}

impl DbIndexMonitorConfig {
    fn from_env() -> Self {
        fn env_u64(key: &str, default: u64) -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(default)
        }
        fn env_usize(key: &str, default: usize) -> usize {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default)
        }
        fn env_f64(key: &str, default: f64) -> f64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(default)
        }
        fn env_bool(key: &str, default: bool) -> bool {
            std::env::var(key)
                .ok()
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(default)
        }

        Self {
            tick_secs: env_u64("AOS_DB_INDEX_MONITOR_SECS", 60),
            dbstat_secs: env_u64("AOS_DB_INDEX_DBSTAT_SECS", 3600),
            dbstat_top_n: env_usize("AOS_DB_INDEX_DBSTAT_TOP_N", 10),
            maintenance_enabled: env_bool("AOS_DB_INDEX_MAINTENANCE_ENABLED", true),
            optimize_secs: env_u64("AOS_DB_INDEX_OPTIMIZE_SECS", 3600),
            analyze_secs: env_u64("AOS_DB_INDEX_ANALYZE_SECS", 21600),
            reindex_enabled: env_bool("AOS_DB_INDEX_REINDEX_ENABLED", false),
            reindex_cooldown_secs: env_u64("AOS_DB_INDEX_REINDEX_COOLDOWN_SECS", 21600),
            vacuum_enabled: env_bool("AOS_DB_INDEX_VACUUM_ENABLED", false),
            vacuum_cooldown_secs: env_u64("AOS_DB_INDEX_VACUUM_COOLDOWN_SECS", 604800),
            max_in_flight_for_light_maintenance: env_usize("AOS_DB_INDEX_MAX_IN_FLIGHT_LIGHT", 10),
            max_in_flight_for_heavy_maintenance: env_usize("AOS_DB_INDEX_MAX_IN_FLIGHT_HEAVY", 0),
            fragmentation_warn_ratio: env_f64("AOS_DB_INDEX_FRAGMENTATION_WARN", 0.15),
            fragmentation_critical_ratio: env_f64("AOS_DB_INDEX_FRAGMENTATION_CRITICAL", 0.30),
            index_unused_warn_ratio: env_f64("AOS_DB_INDEX_UNUSED_WARN", 0.20),
            index_unused_critical_ratio: env_f64("AOS_DB_INDEX_UNUSED_CRITICAL", 0.35),
            probe_warn_secs: env_f64("AOS_DB_INDEX_PROBE_WARN_SECS", 0.05),
            probe_critical_secs: env_f64("AOS_DB_INDEX_PROBE_CRITICAL_SECS", 0.25),
        }
    }
}

#[derive(Debug, Clone)]
struct ProbeSpec {
    name: &'static str,
    sql: &'static str,
}

fn plan_uses_index(details: &[String]) -> bool {
    let haystack = details.join(" | ").to_uppercase();
    haystack.contains("USING INDEX")
        || haystack.contains("USING COVERING INDEX")
        || haystack.contains("USING INTEGER PRIMARY KEY")
        || haystack.contains("USING ROWID")
}

fn health_from_signals(
    page_stats: Option<&SqlitePageStats>,
    tenant_coverage: &[TenantIndexCoverage],
    dbstat: Option<&DbstatIndexSummary>,
    probe_results: &[(bool, bool, f64)],
    cfg: &DbIndexMonitorConfig,
) -> (u8, bool) {
    let mut status: u8 = 0;

    if let Some(stats) = page_stats {
        if stats.freelist_ratio >= cfg.fragmentation_critical_ratio {
            status = status.max(2);
        } else if stats.freelist_ratio >= cfg.fragmentation_warn_ratio {
            status = status.max(1);
        }
    }

    if let Some(s) = dbstat {
        if s.total_index_unused_ratio >= cfg.index_unused_critical_ratio {
            status = status.max(2);
        } else if s.total_index_unused_ratio >= cfg.index_unused_warn_ratio {
            status = status.max(1);
        }
    }

    let tenant_missing = tenant_coverage
        .iter()
        .any(|t| !t.table_exists || !t.has_tenant_id_column || !t.has_leading_tenant_id_index);
    if tenant_missing {
        status = status.max(1);
    }

    for (success, used_index, dur_secs) in probe_results {
        if !*success || !*used_index {
            status = status.max(2);
            continue;
        }
        if *dur_secs >= cfg.probe_critical_secs {
            status = status.max(2);
        } else if *dur_secs >= cfg.probe_warn_secs {
            status = status.max(1);
        }
    }

    let regression_detected = status > 0;
    (status, regression_detected)
}

pub async fn run_db_index_monitor(state: AppState) {
    let cfg = DbIndexMonitorConfig::from_env();

    if state.db.pool_opt().is_none() {
        info!("DB index monitor disabled (no SQL pool attached)");
        return;
    }

    let tenant_tables: [&str; 10] = [
        "adapters",
        "adapter_stacks",
        "users",
        "training_jobs",
        "documents",
        "messages",
        "chat_sessions",
        "activity_events",
        "routing_decisions",
        "telemetry_bundles",
    ];

    let probes: [ProbeSpec; 3] = [
        ProbeSpec {
            name: "adapters_by_tenant",
            sql: "SELECT 1 FROM adapters WHERE tenant_id = '__probe__' LIMIT 1",
        },
        ProbeSpec {
            name: "documents_by_tenant",
            sql: "SELECT 1 FROM documents WHERE tenant_id = '__probe__' LIMIT 1",
        },
        ProbeSpec {
            name: "training_jobs_by_tenant",
            sql: "SELECT 1 FROM training_jobs WHERE tenant_id = '__probe__' LIMIT 1",
        },
    ];

    let mut tick = tokio::time::interval(Duration::from_secs(cfg.tick_secs));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut last_dbstat_at = Instant::now().checked_sub(Duration::from_secs(cfg.dbstat_secs));
    let mut last_optimize_at = Instant::now().checked_sub(Duration::from_secs(cfg.optimize_secs));
    let mut last_analyze_at = Instant::now().checked_sub(Duration::from_secs(cfg.analyze_secs));
    let mut last_reindex_at =
        Instant::now().checked_sub(Duration::from_secs(cfg.reindex_cooldown_secs));
    let mut last_vacuum_at =
        Instant::now().checked_sub(Duration::from_secs(cfg.vacuum_cooldown_secs));

    loop {
        tick.tick().await;

        let pool = match state.db.pool_opt() {
            Some(p) => p,
            None => {
                debug!("DB index monitor skipping tick (no SQL pool attached)");
                continue;
            }
        };

        // Check for performance regressions using the new QueryPerformanceMonitor
        if let Some(monitor_guard) = state.db.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let violations = monitor.check_threshold_violations();
                if !violations.is_empty() {
                    for violation in violations {
                        let msg = format!(
                            "Query Performance Violation: {} - {:?} (Tenant: {:?})",
                            violation.query_name, violation.violation_type, violation.tenant_id
                        );
                        match violation.severity {
                            adapteros_db::query_performance::ViolationSeverity::Critical => {
                                tracing::error!(target: "db_performance", "{}", msg);
                            }
                            adapteros_db::query_performance::ViolationSeverity::Warning => {
                                tracing::warn!(target: "db_performance", "{}", msg);
                            }
                        }
                    }
                }
            }
        }

        let page_stats = match state.db.collect_sqlite_page_stats().await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, operation = "collect_sqlite_page_stats", "Failed to collect SQLite page stats");
                None
            }
        };
        if let Some(ref stats) = page_stats {
            state.metrics_exporter.set_sqlite_page_stats(stats);
        }

        let tenant_coverage = match state.db.collect_tenant_index_coverage(&tenant_tables).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, operation = "collect_tenant_index_coverage", "Failed to collect tenant index coverage");
                Vec::new()
            }
        };
        state
            .metrics_exporter
            .set_tenant_index_coverage(&tenant_coverage);

        let now = Instant::now();

        let mut dbstat_summary: Option<DbstatIndexSummary> = None;
        let should_dbstat = cfg.dbstat_secs > 0
            && last_dbstat_at
                .map(|t| now.duration_since(t) >= Duration::from_secs(cfg.dbstat_secs))
                .unwrap_or(true);
        if should_dbstat {
            match state
                .db
                .collect_dbstat_index_summary(cfg.dbstat_top_n)
                .await
            {
                Ok(s) => {
                    dbstat_summary = s;
                    last_dbstat_at = Some(now);
                }
                Err(e) => {
                    warn!(error = %e, operation = "collect_dbstat_index_summary", "Failed to collect dbstat index summary");
                    last_dbstat_at = Some(now);
                }
            }
        }
        state
            .metrics_exporter
            .set_dbstat_index_summary(dbstat_summary.as_ref());

        let mut probe_results: Vec<(bool, bool, f64)> = Vec::with_capacity(probes.len());
        for probe in &probes {
            let explain_sql = format!("EXPLAIN QUERY PLAN {}", probe.sql);
            let plan_details: Result<Vec<String>, sqlx::Error> = async {
                let rows = sqlx::query(&explain_sql).fetch_all(pool).await?;
                let mut details = Vec::with_capacity(rows.len());
                for row in rows {
                    let detail: String = row.try_get("detail")?;
                    details.push(detail);
                }
                Ok(details)
            }
            .await;

            let details = match plan_details {
                Ok(d) => d,
                Err(e) => {
                    state
                        .metrics_exporter
                        .record_index_probe_failure(probe.name, "explain_failed");
                    error!(probe = probe.name, error = %e, operation = "index_probe_explain", "Index probe EXPLAIN failed");
                    probe_results.push((false, false, 0.0));
                    continue;
                }
            };

            let used_index = plan_uses_index(&details);

            let start = Instant::now();
            let query_result: Result<Option<(i64,)>, sqlx::Error> =
                sqlx::query_as(probe.sql).fetch_optional(pool).await;
            match query_result {
                Ok(_row) => {
                    let dur_secs = start.elapsed().as_secs_f64();
                    state
                        .metrics_exporter
                        .record_index_probe_success(probe.name, used_index, dur_secs);
                    probe_results.push((true, used_index, dur_secs));
                }
                Err(e) => {
                    state
                        .metrics_exporter
                        .record_index_probe_failure(probe.name, "query_failed");
                    error!(probe = probe.name, error = %e, operation = "index_probe_query", "Index probe query failed");
                    probe_results.push((false, used_index, 0.0));
                }
            }
        }

        let (health_status, regression_detected) = health_from_signals(
            page_stats.as_ref(),
            &tenant_coverage,
            dbstat_summary.as_ref(),
            &probe_results,
            &cfg,
        );
        state
            .metrics_exporter
            .set_index_health_status(health_status);
        state
            .metrics_exporter
            .set_index_regression_detected(regression_detected);

        if !cfg.maintenance_enabled {
            continue;
        }

        let in_flight = state.in_flight_requests.load(Ordering::Relaxed);
        let in_maintenance_mode = state
            .boot_state
            .as_ref()
            .map(|b| b.is_maintenance())
            .unwrap_or(false);

        let can_run_light = in_flight <= cfg.max_in_flight_for_light_maintenance;
        let can_run_heavy =
            in_maintenance_mode || in_flight <= cfg.max_in_flight_for_heavy_maintenance;

        if cfg.optimize_secs > 0
            && last_optimize_at
                .map(|t| now.duration_since(t) >= Duration::from_secs(cfg.optimize_secs))
                .unwrap_or(true)
            && can_run_light
        {
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(true);
            let start = Instant::now();
            let res = state.db.sqlite_optimize().await;
            let dur = start.elapsed().as_secs_f64();
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(false);
            state
                .metrics_exporter
                .record_index_maintenance("optimize", res.is_ok(), dur);
            if let Err(e) = res {
                error!(error = %e, operation = "sqlite_optimize", "PRAGMA optimize failed");
            } else {
                last_optimize_at = Some(now);
            }
        }

        if cfg.analyze_secs > 0
            && last_analyze_at
                .map(|t| now.duration_since(t) >= Duration::from_secs(cfg.analyze_secs))
                .unwrap_or(true)
            && can_run_light
        {
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(true);
            let start = Instant::now();
            let res = state.db.sqlite_analyze_tables(&tenant_tables).await;
            let dur = start.elapsed().as_secs_f64();
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(false);
            state
                .metrics_exporter
                .record_index_maintenance("analyze", res.is_ok(), dur);
            if let Err(e) = res {
                error!(error = %e, operation = "sqlite_analyze_tables", tables = ?tenant_tables, "ANALYZE failed");
            } else {
                last_analyze_at = Some(now);
            }
        }

        if cfg.reindex_enabled
            && regression_detected
            && can_run_heavy
            && last_reindex_at
                .map(|t| now.duration_since(t) >= Duration::from_secs(cfg.reindex_cooldown_secs))
                .unwrap_or(true)
        {
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(true);
            let start = Instant::now();
            let res = state.db.sqlite_reindex_tables(&tenant_tables).await;
            let dur = start.elapsed().as_secs_f64();
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(false);
            state
                .metrics_exporter
                .record_index_maintenance("reindex", res.is_ok(), dur);
            if let Err(e) = res {
                error!(error = %e, operation = "sqlite_reindex_tables", tables = ?tenant_tables, "REINDEX failed");
            } else {
                last_reindex_at = Some(now);
            }
        }

        let freelist_ratio = page_stats.as_ref().map(|s| s.freelist_ratio).unwrap_or(0.0);
        if cfg.vacuum_enabled
            && freelist_ratio >= cfg.fragmentation_critical_ratio
            && can_run_heavy
            && last_vacuum_at
                .map(|t| now.duration_since(t) >= Duration::from_secs(cfg.vacuum_cooldown_secs))
                .unwrap_or(true)
        {
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(true);
            let start = Instant::now();
            let res = state.db.sqlite_vacuum().await;
            let dur = start.elapsed().as_secs_f64();
            state
                .metrics_exporter
                .set_index_maintenance_in_progress(false);
            state
                .metrics_exporter
                .record_index_maintenance("vacuum", res.is_ok(), dur);
            if let Err(e) = res {
                error!(error = %e, operation = "sqlite_vacuum", freelist_ratio, "VACUUM failed");
            } else {
                last_vacuum_at = Some(now);
            }
        }
    }
}
