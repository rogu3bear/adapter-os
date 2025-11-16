//! Metrics gate: verifies ARR/ECS/HLR/CR thresholds

use crate::{Gate, OrchestratorConfig};
use adapteros_db::Db;
use anyhow::Result;

#[derive(Debug)]
pub struct MetricsGate {
    pub arr_min: f64,
    pub ecs5_min: f64,
    pub hlr_max: f64,
    pub cr_max: f64,
}

impl Default for MetricsGate {
    fn default() -> Self {
        Self {
            arr_min: 0.95,
            ecs5_min: 0.75,
            hlr_max: 0.03,
            cr_max: 0.01,
        }
    }
}

#[async_trait::async_trait]
impl Gate for MetricsGate {
    fn name(&self) -> String {
        "Metrics".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Connect to database
        let db = Db::connect(&config.db_path).await?;

        // Get latest audit for this CPID (query directly)
        let audit = sqlx::query_as::<_, adapteros_db::Audit>(
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
        let metrics = &result["hallucination_metrics"];

        let arr = metrics["arr"].as_f64().unwrap_or(0.0);
        let ecs5 = metrics["ecs5"].as_f64().unwrap_or(0.0);
        let hlr = metrics["hlr"].as_f64().unwrap_or(1.0);
        let cr = metrics["cr"].as_f64().unwrap_or(1.0);

        // Check thresholds
        let mut failures = Vec::new();

        if arr < self.arr_min {
            failures.push(format!("ARR {:.3} < {:.3}", arr, self.arr_min));
        }

        if ecs5 < self.ecs5_min {
            failures.push(format!("ECS@5 {:.3} < {:.3}", ecs5, self.ecs5_min));
        }

        if hlr > self.hlr_max {
            failures.push(format!("HLR {:.3} > {:.3}", hlr, self.hlr_max));
        }

        if cr > self.cr_max {
            failures.push(format!("CR {:.3} > {:.3}", cr, self.cr_max));
        }

        if !failures.is_empty() {
            anyhow::bail!("Hallucination metrics failed: {}", failures.join(", "));
        }

        tracing::info!(
            arr = format!("{:.3}", arr),
            arr_threshold = format!("{:.3}", self.arr_min),
            ecs5 = format!("{:.3}", ecs5),
            ecs5_threshold = format!("{:.3}", self.ecs5_min),
            hlr = format!("{:.3}", hlr),
            hlr_threshold = format!("{:.3}", self.hlr_max),
            cr = format!("{:.3}", cr),
            cr_threshold = format!("{:.3}", self.cr_max),
            "Hallucination metrics passed"
        );

        Ok(())
    }
}
