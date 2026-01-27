//! SSE streaming handlers for training progress
//!
//! This module contains Server-Sent Events (SSE) streaming handlers
//! for real-time training progress updates.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::Json,
    Extension,
};
use futures_util::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;
use tracing::{info, warn};

use crate::types::TrainingProgressEvent;

// Note: These types will be provided by the parent crate when integrated
// For now, we define the handler signature that will work with adapteros-server-api

/// Stream real-time training progress for a job
///
/// Provides an SSE stream of training progress updates including epoch,
/// loss, tokens processed, and status. The stream terminates when the
/// job reaches a terminal state (completed, failed, or cancelled).
///
/// # Arguments
///
/// * `state` - Application state containing database and services
/// * `claims` - Authenticated user claims
/// * `job_id` - Training job ID to stream progress for
///
/// # Returns
///
/// SSE stream of training progress events
#[cfg(feature = "full-handlers")]
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/jobs/{job_id}/progress",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "SSE stream of training progress"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Training job not found")
    )
)]
pub async fn stream_training_progress<S, C, E>(
    State(state): State<S>,
    Extension(claims): Extension<C>,
    Path(job_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<E>)>
where
    S: TrainingStreamState + Clone + Send + Sync + 'static,
    C: TrainingClaims + Send + Sync + 'static,
    E: serde::Serialize + From<StreamError>,
{
    // Validate permissions
    if !claims.can_view_training() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(E::from(StreamError::AccessDenied)),
        ));
    }

    // Validate job exists and user has access
    let job = state.get_training_job(&job_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(E::from(StreamError::DatabaseError(e.to_string()))),
        )
    })?;

    let job = job.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(E::from(StreamError::NotFound(job_id.clone()))),
        )
    })?;

    // Validate tenant isolation
    if let Some(ref job_tenant_id) = job.tenant_id {
        if !claims.can_access_tenant(job_tenant_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(E::from(StreamError::AccessDenied)),
            ));
        }
    } else if !claims.is_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(E::from(StreamError::AccessDenied)),
        ));
    }

    info!(job_id = %job_id, "Starting training progress SSE stream");

    // Create SSE stream
    let stream = create_progress_stream(state, job_id);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Trait for state that supports training streaming
#[cfg(feature = "full-handlers")]
pub trait TrainingStreamState {
    type Job: TrainingJobInfo;
    type Error: std::error::Error;

    fn get_training_job(
        &self,
        job_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Self::Job>, Self::Error>> + Send;
}

/// Trait for job information needed by streaming
#[cfg(feature = "full-handlers")]
pub trait TrainingJobInfo {
    fn tenant_id(&self) -> Option<&str>;
    fn status(&self) -> &str;
    fn progress_json(&self) -> &str;
}

/// Trait for claims that support training operations
#[cfg(feature = "full-handlers")]
pub trait TrainingClaims {
    fn can_view_training(&self) -> bool;
    fn can_access_tenant(&self, tenant_id: &str) -> bool;
    fn is_admin(&self) -> bool;
}

/// Errors that can occur during streaming
#[derive(Debug)]
pub enum StreamError {
    AccessDenied,
    NotFound(String),
    DatabaseError(String),
}

/// Create an SSE stream for training progress
///
/// This is the core streaming implementation that polls the database
/// for job status and emits progress events.
#[cfg(feature = "full-handlers")]
fn create_progress_stream<S>(
    state: S,
    job_id: String,
) -> impl Stream<Item = Result<Event, Infallible>>
where
    S: TrainingStreamState + Clone + Send + Sync + 'static,
{
    stream::unfold((state, job_id), |(state, job_id)| async move {
        // Poll interval
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Get latest job state
        let job = match state.get_training_job(&job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
                // Job was deleted - end stream
                let event = Event::default().event("error").data("Job not found");
                return Some((Ok(event), (state, job_id)));
            }
            Err(e) => {
                warn!(job_id = %job_id, error = %e, "Failed to get training job in progress stream");
                let event = Event::default()
                    .event("error")
                    .data(format!("Database error: {}", e));
                return Some((Ok(event), (state, job_id)));
            }
        };

        let status = job.status().to_lowercase();

        // Parse progress data from JSON
        let progress_data: Option<serde_json::Value> =
            serde_json::from_str(job.progress_json()).ok();

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

        // Check if job is in terminal state
        if status == "completed" || status == "failed" || status == "cancelled" {
            // Send final event and terminate
            return None;
        }

        Some((Ok(event), (state, job_id)))
    })
}

// Simplified version for when full handlers are not needed
// This will be the actual implementation used by adapteros-server-api

/// Simplified stream creation for use with AppState directly
pub fn create_training_progress_stream<DB>(
    db: DB,
    job_id: String,
) -> impl Stream<Item = Result<Event, Infallible>>
where
    DB: TrainingJobDb + Clone + Send + Sync + 'static,
{
    stream::unfold((db, job_id), |(db, job_id)| async move {
        // Poll interval
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Get latest job state
        let job = match db.get_training_job(&job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => {
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

        // Check if job is in terminal state
        if status == "completed" || status == "failed" || status == "cancelled" {
            return None;
        }

        Some((Ok(event), (db, job_id)))
    })
}

/// Trait for database operations needed by streaming
pub trait TrainingJobDb {
    fn get_training_job(
        &self,
        job_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<TrainingJobRecord>, adapteros_core::AosError>>
           + Send;
}

/// Minimal job record for streaming
#[derive(Debug, Clone)]
pub struct TrainingJobRecord {
    pub id: String,
    pub status: String,
    pub progress_json: String,
    pub tenant_id: Option<String>,
}
