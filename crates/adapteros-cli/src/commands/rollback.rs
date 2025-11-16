//! Rollback to previous checkpoint

use crate::output::OutputWriter;
use adapteros_db::Db;
use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Serialize)]
struct RollbackResult {
    tenant: String,
    cpid: String,
    workers_restarted: usize,
}

pub async fn run(tenant: &str, cpid: &str, output: &OutputWriter) -> Result<()> {
    output.info(format!("Rolling back tenant {} to CPID: {}", tenant, cpid));

    // Connect to database
    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    // Look up tenant
    let tenant_record = db
        .get_tenant(tenant)
        .await
        .context("Failed to get tenant")?
        .ok_or_else(|| anyhow::anyhow!("Tenant not found: {}", tenant))?;

    output.success(format!("Tenant found: {}", tenant_record.name));

    // Look up current active CP pointer
    let current_cp = db
        .get_active_cp_pointer(&tenant_record.id)
        .await
        .context("Failed to get current CP pointer")?;

    if let Some(current) = current_cp {
        output.kv("Current CPID", &current.name);
    }

    // Look up target CP pointer
    let target_cp = db
        .get_cp_pointer_by_name(cpid)
        .await
        .context("Failed to get target CP pointer")?
        .ok_or_else(|| anyhow::anyhow!("Target CPID not found: {}", cpid))?;

    // Verify target CP has valid plan
    let target_plan = db
        .get_plan(&target_cp.plan_id)
        .await
        .context("Failed to get target plan")?
        .ok_or_else(|| anyhow::anyhow!("Target plan not found: {}", target_cp.plan_id))?;

    output.success("Target CP verified");
    output.kv("Plan ID", &target_plan.plan_id_b3);

    // Update CP pointers: set all to inactive, then target to active
    db.deactivate_all_cp_pointers(&tenant_record.id)
        .await
        .context("Failed to deactivate CP pointers")?;

    db.activate_cp_pointer(&target_cp.id)
        .await
        .context("Failed to activate target CP pointer")?;

    output.success("CP pointer updated");

    // Query all workers for tenant
    let workers = db
        .list_workers_by_tenant(&tenant_record.id)
        .await
        .context("Failed to list workers")?;

    output.success(format!("Found {} workers to restart", workers.len()));

    if workers.is_empty() {
        output.warning("No workers to restart");
    } else {
        // Restart each worker
        let client = reqwest::Client::new();

        for worker in &workers {
            output.verbose(format!(
                "Restarting worker {} (PID: {:?})...",
                worker.id, worker.pid
            ));

            // Get node info
            let node = db
                .get_node(&worker.node_id)
                .await
                .context("Failed to get node")?
                .ok_or_else(|| anyhow::anyhow!("Node not found: {}", worker.node_id))?;

            // Stop old worker
            if let Some(pid) = worker.pid {
                let stop_url = format!("{}/workers/{}", node.agent_endpoint, pid);
                match client.delete(&stop_url).send().await {
                    Ok(response) if response.status().is_success() => {
                        output.verbose(format!("Stopped old worker (PID: {})", pid));
                    }
                    Ok(response) => {
                        let status = response.status();
                        let error_text = response.text().await.unwrap_or_default();
                        output.warning(format!(
                            "Failed to stop old worker: {} - {}",
                            status, error_text
                        ));
                    }
                    Err(e) => {
                        output.warning(format!("Failed to contact node runtime: {}", e));
                    }
                }
            }

            // Start new worker with rollback plan
            let spawn_req = serde_json::json!({
                "tenant_id": worker.tenant_id,
                "plan_id": target_plan.plan_id_b3,
                "uid": 1000, // Default UID, should be configurable
                "gid": 1000, // Default GID, should be configurable
            });

            let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);
            match client.post(&spawn_url).json(&spawn_req).send().await {
                Ok(response) if response.status().is_success() => {
                    let spawn_response: serde_json::Value = response
                        .json()
                        .await
                        .context("Failed to parse spawn response")?;
                    let new_pid = spawn_response["pid"].as_i64().unwrap_or(0);

                    output.verbose(format!("Started new worker (PID: {})", new_pid));
                }
                Ok(response) => {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    output.error(format!(
                        "Failed to spawn new worker: {} - {}",
                        status, error_text
                    ));
                }
                Err(e) => {
                    output.error(format!("Failed to contact node runtime: {}", e));
                }
            }
        }

        output.success("Worker restart completed");
    }

    output.blank();
    output.success("Rollback completed");
    output.kv("Tenant", tenant);
    output.kv("Active CPID", cpid);

    if output.is_json() {
        let result = RollbackResult {
            tenant: tenant.to_string(),
            cpid: cpid.to_string(),
            workers_restarted: workers.len(),
        };
        output.json(&result)?;
    }

    Ok(())
}
