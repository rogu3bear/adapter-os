//! Training metrics persistence utilities.

use adapteros_db::TrainingMetricRow;
use adapteros_lora_worker::training::TrainingResult;
use tracing::{info, warn};

/// Persist final training metrics to the database.
pub(crate) async fn persist_final_metrics(
    database: &adapteros_db::Db,
    job_id: &str,
    training_result: &TrainingResult,
) -> anyhow::Result<()> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let step = training_result.examples_processed.unwrap_or(0) as i64;
    let epoch = training_result.stopped_at_epoch.map(|e| e as i64);
    let tokens_processed = training_result.tokens_processed.unwrap_or(0) as f64;
    let tokens_per_second = training_result.tokens_per_sec as f64;

    let final_metrics = vec![
        TrainingMetricRow {
            id: uuid::Uuid::now_v7().to_string(),
            training_job_id: job_id.to_string(),
            step,
            epoch,
            metric_name: "final_loss".to_string(),
            metric_value: training_result.final_loss as f64,
            metric_timestamp: Some(timestamp.clone()),
        },
        TrainingMetricRow {
            id: uuid::Uuid::now_v7().to_string(),
            training_job_id: job_id.to_string(),
            step,
            epoch,
            metric_name: "cancelled".to_string(),
            metric_value: if training_result.cancelled { 1.0 } else { 0.0 },
            metric_timestamp: Some(timestamp.clone()),
        },
        TrainingMetricRow {
            id: uuid::Uuid::now_v7().to_string(),
            training_job_id: job_id.to_string(),
            step,
            epoch,
            metric_name: "examples_processed".to_string(),
            metric_value: training_result.examples_processed.unwrap_or(0) as f64,
            metric_timestamp: Some(timestamp),
        },
        TrainingMetricRow {
            id: uuid::Uuid::now_v7().to_string(),
            training_job_id: job_id.to_string(),
            step,
            epoch,
            metric_name: "tokens_processed".to_string(),
            metric_value: tokens_processed,
            metric_timestamp: Some(chrono::Utc::now().to_rfc3339()),
        },
        TrainingMetricRow {
            id: uuid::Uuid::now_v7().to_string(),
            training_job_id: job_id.to_string(),
            step,
            epoch,
            metric_name: "tokens_per_sec_final".to_string(),
            metric_value: tokens_per_second,
            metric_timestamp: Some(chrono::Utc::now().to_rfc3339()),
        },
    ];

    if let Err(e) = database.insert_training_metrics_batch(&final_metrics).await {
        warn!(job_id = %job_id, error = %e, "Failed to persist final training metrics (non-fatal)");
        return Err(e.into());
    }

    info!(job_id = %job_id, cancelled = training_result.cancelled, "Final training metrics persisted");
    Ok(())
}
