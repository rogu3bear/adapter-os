use crate::output::{create_styled_table, OutputWriter};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Subcommand, Clone)]
pub enum CoremlCommand {
    /// Show CoreML verification status across workers
    Status(CoremlStatusArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CoremlStatusArgs {
    /// Control plane base URL (default http://localhost:18080)
    #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:18080")]
    pub server_url: String,

    /// Filter by worker ID
    #[arg(long)]
    pub worker_id: Option<String>,

    /// Filter by plan ID
    #[arg(long)]
    pub plan_id: Option<String>,

    /// Filter by tenant ID
    #[arg(long)]
    pub tenant_id: Option<String>,

    /// Request timeout in seconds
    #[arg(long, default_value = "5")]
    pub timeout: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CoremlVerificationStatusResponse {
    workers: Vec<WorkerCoremlStatus>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WorkerCoremlStatus {
    worker_id: String,
    tenant_id: String,
    plan_id: String,
    status: String,
    mode: Option<String>,
    expected: Option<String>,
    actual: Option<String>,
    source: Option<String>,
    mismatch: bool,
    error: Option<String>,
}

pub async fn run(cmd: CoremlCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        CoremlCommand::Status(args) => status(args, output).await,
    }
}

async fn status(args: CoremlStatusArgs, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .build()
        .context("failed to build HTTP client")?;

    let url = format!(
        "{}/v1/debug/coreml_verification_status",
        args.server_url.trim_end_matches('/')
    );

    let resp = client.get(url).send().await.context("request failed")?;
    if resp.status() == StatusCode::NOT_FOUND {
        output.error("CoreML verification endpoint not available on server");
        return Ok(());
    }
    if !resp.status().is_success() {
        output.error(format!("Server returned {}", resp.status()));
        return Ok(());
    }

    let mut payload: CoremlVerificationStatusResponse = resp
        .json()
        .await
        .context("failed to parse CoreML verification response")?;

    if let Some(worker_id) = args.worker_id.as_ref() {
        payload.workers.retain(|w| w.worker_id == *worker_id);
    }
    if let Some(plan_id) = args.plan_id.as_ref() {
        payload.workers.retain(|w| w.plan_id == *plan_id);
    }
    if let Some(tenant_id) = args.tenant_id.as_ref() {
        payload.workers.retain(|w| w.tenant_id == *tenant_id);
    }

    if output.is_json() {
        output.json(&payload)?;
        return Ok(());
    }

    if payload.workers.is_empty() {
        output.warning("No CoreML verification data available");
        return Ok(());
    }

    let mut table = create_styled_table();
    table.set_header(vec![
        "Worker", "Tenant", "Plan", "Status", "Mode", "Expected", "Actual", "Source", "Mismatch",
    ]);

    for worker in payload.workers {
        table.add_row(vec![
            worker.worker_id,
            worker.tenant_id,
            worker.plan_id,
            worker.status,
            worker.mode.unwrap_or_else(|| "-".to_string()),
            worker.expected.unwrap_or_else(|| "-".to_string()),
            worker.actual.unwrap_or_else(|| "-".to_string()),
            worker.source.unwrap_or_else(|| "-".to_string()),
            worker.mismatch.to_string(),
        ]);
    }

    output.table(&table, None::<&Vec<WorkerCoremlStatus>>)?;
    Ok(())
}
