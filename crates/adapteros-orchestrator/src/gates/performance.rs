//! Performance gate: verifies latency and throughput budgets

use crate::{Gate, OrchestratorConfig};
use adapteros_db::Db;
use anyhow::Result;

#[derive(Debug)]
pub struct PerformanceGate {
    pub latency_p95_ms_max: u64,
    pub throughput_tokens_per_s_min: u64,
    pub router_overhead_pct_max: f64,
}

impl Default for PerformanceGate {
    fn default() -> Self {
        Self {
            latency_p95_ms_max: 24,
            throughput_tokens_per_s_min: 40,
            router_overhead_pct_max: 8.0,
        }
    }
}

#[async_trait::async_trait]
impl Gate for PerformanceGate {
    fn name(&self) -> String {
        "Performance".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Connect to database
        let db = Db::connect(&config.db_path).await?;

        // Get latest audit for this CPID (query directly)
        let audit = sqlx::query_as::<_, adapteros_db::audits::Audit>(
            "SELECT id, tenant_id, cpid, suite_name, bundle_id, result_json, status, created_at 
             FROM audits 
             WHERE cpid = ? 
             ORDER BY created_at DESC 
             LIMIT 1",
        )
        .bind(&config.cpid)
        .fetch_optional(db.pool())
        .await?
        .ok_or_else(|| anyhow::anyhow!("No audit found for CPID: {}", config.cpid))?;

        // Parse audit results
        let result: serde_json::Value = serde_json::from_str(&audit.result_json)?;
        let perf = &result["performance"];

        let latency_p95 = perf["latency_p95_ms"].as_u64().unwrap_or(1000);
        let throughput = perf["throughput_tokens_per_s"].as_u64().unwrap_or(0);
        let router_overhead = perf["router_overhead_pct"].as_f64().unwrap_or(100.0);

        // Check budgets
        let mut failures = Vec::new();

        if latency_p95 > self.latency_p95_ms_max {
            failures.push(format!(
                "Latency p95 {}ms > {}ms",
                latency_p95, self.latency_p95_ms_max
            ));
        }

        if throughput < self.throughput_tokens_per_s_min {
            failures.push(format!(
                "Throughput {} tokens/s < {} tokens/s",
                throughput, self.throughput_tokens_per_s_min
            ));
        }

        if router_overhead > self.router_overhead_pct_max {
            failures.push(format!(
                "Router overhead {:.1}% > {:.1}%",
                router_overhead, self.router_overhead_pct_max
            ));
        }

        if !failures.is_empty() {
            anyhow::bail!("Performance budgets failed: {}", failures.join(", "));
        }

        println!(
            "    Latency p95: {}ms (budget: {}ms)",
            latency_p95, self.latency_p95_ms_max
        );
        println!(
            "    Throughput: {} tokens/s (budget: {} tokens/s)",
            throughput, self.throughput_tokens_per_s_min
        );
        println!(
            "    Router overhead: {:.1}% (budget: {:.1}%)",
            router_overhead, self.router_overhead_pct_max
        );

        Ok(())
    }
}
