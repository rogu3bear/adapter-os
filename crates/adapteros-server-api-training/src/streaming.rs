//! SSE streaming handlers for training progress
//!
//! This module contains Server-Sent Events (SSE) streaming utilities
//! for real-time training progress updates.
//!
//! # Usage
//!
//! The main export is `create_training_progress_stream` which creates an SSE stream
//! that polls the database for job status updates. The parent crate (adapteros-server-api)
//! wraps this in a handler with authentication and validation.
//!
//! ```ignore
//! use adapteros_server_api_training::streaming::{create_training_progress_stream, TrainingJobDb};
//!
//! // In your handler:
//! let stream = create_training_progress_stream(db.clone(), job_id);
//! Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
//! ```

use axum::response::sse::Event;
use futures_util::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;
use tracing::warn;

use crate::types::TrainingProgressEvent;

/// Create an SSE stream for training progress
///
/// This function creates a stream that:
/// 1. Polls the database every second for job status
/// 2. Emits progress events with epoch, loss, tokens processed, and status
/// 3. Terminates when the job reaches a terminal state (completed, failed, cancelled)
///
/// # Arguments
///
/// * `db` - Database client implementing `TrainingJobDb`
/// * `job_id` - Training job ID to monitor
///
/// # Returns
///
/// A `Stream` of SSE `Event`s suitable for use with `axum::response::Sse`
pub fn create_training_progress_stream<DB>(
    db: DB,
    job_id: String,
) -> impl Stream<Item = Result<Event, Infallible>>
where
    DB: TrainingJobDb + Clone + Send + Sync + 'static,
{
    stream::unfold((db, job_id), |(db, job_id)| async move {
        // Poll interval - 1 second between updates
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Get latest job state from database
        let job = match db.get_training_job(&job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                // Job was deleted - send error event
                let event = Event::default().event("error").data("Job not found");
                return Some((Ok(event), (db, job_id)));
            }
            Err(e) => {
                warn!(job_id = %job_id, error = %e, "Failed to get training job in progress stream");
                let event = Event::default()
                    .event("error")
                    .data(format!("Database error: {}", e));
                return Some((Ok(event), (db, job_id)));
            }
        };

        let status = job.status.to_lowercase();

        // Parse progress data from JSON
        let progress_data: Option<serde_json::Value> =
            serde_json::from_str(&job.progress_json).ok();

        let current_epoch = progress_data
            .as_ref()
            .and_then(|p| p.get("current_epoch"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let current_loss = progress_data
            .as_ref()
            .and_then(|p| p.get("current_loss"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

        let tokens_processed = progress_data
            .as_ref()
            .and_then(|p| p.get("tokens_processed"))
            .and_then(|v| v.as_i64());

        let progress_pct = progress_data
            .as_ref()
            .and_then(|p| p.get("progress_pct"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

        let progress_event = TrainingProgressEvent {
            epoch: current_epoch,
            loss: current_loss,
            tokens_processed,
            status: status.clone(),
            progress_pct,
        };

        let event_data = serde_json::to_string(&progress_event).unwrap_or_default();
        let event = Event::default().event("progress").data(event_data);

        // Check if job is in terminal state - if so, end the stream
        if status == "completed" || status == "failed" || status == "cancelled" {
            return None;
        }

        Some((Ok(event), (db, job_id)))
    })
}

/// Trait for database operations needed by streaming
///
/// This trait abstracts the database access pattern so that the streaming
/// implementation can work with any database backend that implements it.
pub trait TrainingJobDb {
    /// Get a training job by ID
    ///
    /// Returns `Ok(Some(job))` if found, `Ok(None)` if not found,
    /// or `Err` on database error.
    fn get_training_job(
        &self,
        job_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<TrainingJobRecord>, adapteros_core::AosError>>
           + Send;
}

/// Minimal job record for streaming
///
/// Contains only the fields needed for progress streaming.
/// The parent crate maps from `adapteros_db::training::TrainingJobRecord`
/// to this type.
#[derive(Debug, Clone)]
pub struct TrainingJobRecord {
    /// Training job ID
    pub id: String,
    /// Job status (e.g., "running", "completed", "failed")
    pub status: String,
    /// JSON-encoded progress data
    pub progress_json: String,
    /// Tenant ID for isolation checks
    pub tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Integration tests for streaming are in the parent crate
    // since they require a full database setup.

    #[test]
    fn test_progress_event_serialization() {
        let event = TrainingProgressEvent {
            epoch: 5,
            loss: 0.123,
            tokens_processed: Some(50000),
            status: "running".to_string(),
            progress_pct: 45.5,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"epoch\":5"));
        assert!(json.contains("\"status\":\"running\""));
    }
}
