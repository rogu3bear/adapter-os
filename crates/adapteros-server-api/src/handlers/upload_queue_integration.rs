//! Integration layer between upload handlers and queue system
//!
//! This module provides utilities for:
//! - Queueing uploads for concurrent processing
//! - Checking queue status
//! - Retrieving queue metrics
//! - Handling worker failures with retry logic

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::{
    auth::Claims,
    permissions::{require_permission, Permission},
    state::AppState,
    upload_queue::{UploadQueueMetrics, UploadQueueResult},
};

/// Request to queue an upload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueueUploadRequest {
    /// Priority level for this upload (0-255, default 128)
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    128
}

/// Response containing queue status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueueStatusResponse {
    /// Item ID
    pub item_id: String,
    /// Current status
    pub status: String,
    /// Queue depth at time of check
    pub queue_depth: usize,
    /// Position in queue (if queued)
    pub queue_position: Option<usize>,
    /// Time in queue (seconds)
    pub time_in_queue: u64,
    /// Processing time if applicable (seconds)
    pub processing_time: Option<u64>,
}

/// Response containing queue metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueueMetricsResponse {
    /// Current queue depth
    pub queue_depth: usize,
    /// Maximum queue depth ever observed
    pub max_queue_depth: u64,
    /// Total items processed
    pub total_processed: u64,
    /// Total items failed
    pub total_failed: u64,
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,
    /// Per-tenant queue depths
    pub per_tenant_depths: std::collections::HashMap<String, usize>,
}

/// Queue an upload with optional priority
///
/// This endpoint allows queuing an upload for concurrent processing.
/// The upload will be processed by available workers as resources permit.
///
/// # Queue Behavior
/// - Fair scheduling across tenants
/// - Higher priority items processed first within tenant queues
/// - Automatic retry on worker failure
/// - Queue size limits prevent unbounded growth
///
/// # Metrics
/// - Queue depth tracking
/// - Per-tenant statistics
/// - Worker utilization
/// - Processing time histograms
#[utoipa::path(
    post,
    path = "/v1/uploads/queue",
    request_body = QueueUploadRequest,
    responses(
        (status = 202, description = "Upload queued successfully", body = UploadQueueResult),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Insufficient permissions"),
        (status = 429, description = "Queue full"),
        (status = 500, description = "Internal error")
    ),
    tag = "uploads",
    security(
        ("bearer" = [])
    )
)]
pub async fn queue_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<QueueUploadRequest>,
) -> Result<(StatusCode, Json<UploadQueueResult>), (StatusCode, String)> {
    // Check permissions
    if let Err(e) = require_permission(&claims, Permission::AdapterRegister) {
        warn!(
            user_id = %claims.sub,
            error = %e,
            "Upload queue request denied - insufficient permissions"
        );
        return Err((StatusCode::FORBIDDEN, e.to_string()));
    }

    let tenant_id = claims
        .tenant_id
        .clone()
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Missing tenant_id in claims".to_string(),
        ))?;

    // Validate priority
    let priority = req.priority;

    debug!(
        tenant_id = %tenant_id,
        user_id = %claims.sub,
        priority = priority,
        "Queueing upload"
    );

    // Queue the upload
    // Note: request_data would come from multipart upload in actual integration
    let request_data = Vec::new(); // Placeholder - would be actual upload data

    let result = state
        .upload_queue
        .enqueue_with_priority(tenant_id.clone(), request_data, priority)
        .await
        .map_err(|e| {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to queue upload"
            );

            if e.contains("full") {
                (StatusCode::TOO_MANY_REQUESTS, e)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            }
        })?;

    info!(
        item_id = %result.item_id,
        tenant_id = %tenant_id,
        queue_depth = result.queue_depth,
        "Upload queued successfully"
    );

    Ok((StatusCode::ACCEPTED, Json(result)))
}

/// Get status of a queued upload
///
/// Returns the current position in queue and processing status.
#[utoipa::path(
    get,
    path = "/v1/uploads/{item_id}/status",
    responses(
        (status = 200, description = "Status retrieved", body = QueueStatusResponse),
        (status = 404, description = "Item not found"),
        (status = 500, description = "Internal error")
    ),
    tag = "uploads",
    security(
        ("bearer" = [])
    )
)]
pub async fn get_upload_status(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(item_id): Path<String>,
) -> Result<Json<QueueStatusResponse>, (StatusCode, String)> {
    debug!(
        item_id = %item_id,
        "Retrieving upload status"
    );

    let status = state
        .upload_queue
        .get_status(&item_id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Upload not found".to_string()))?;

    Ok(Json(QueueStatusResponse {
        item_id: status.item_id,
        status: status.status,
        queue_depth: status.queue_depth,
        queue_position: status.queue_position,
        time_in_queue: status.time_in_queue,
        processing_time: status.processing_time,
    }))
}

/// Get queue metrics
///
/// Returns current queue statistics including:
/// - Queue depth and maximum observed depth
/// - Processing statistics (total, failed, average time)
/// - Per-tenant breakdown
///
/// Requires Admin or SRE role for detailed metrics.
#[utoipa::path(
    get,
    path = "/v1/uploads/queue/metrics",
    responses(
        (status = 200, description = "Metrics retrieved", body = QueueMetricsResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Internal error")
    ),
    tag = "uploads",
    security(
        ("bearer" = [])
    )
)]
pub async fn get_queue_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<QueueMetricsResponse>, (StatusCode, String)> {
    // Check permissions - require elevated role for metrics
    if let Err(e) = require_permission(&claims, Permission::AdapterRegister) {
        return Err((StatusCode::FORBIDDEN, e.to_string()));
    }

    debug!("Retrieving upload queue metrics");

    let metrics = state.upload_queue.get_metrics().await;

    Ok(Json(QueueMetricsResponse {
        queue_depth: metrics.queue_depth,
        max_queue_depth: metrics.max_queue_depth,
        total_processed: metrics.total_processed,
        total_failed: metrics.total_failed,
        avg_processing_time_ms: metrics.avg_processing_time_ms,
        per_tenant_depths: metrics.per_tenant_depths,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_priority() {
        assert_eq!(default_priority(), 128);
    }

    #[test]
    fn test_queue_status_response_creation() {
        let response = QueueStatusResponse {
            item_id: "test-123".to_string(),
            status: "queued".to_string(),
            queue_depth: 5,
            queue_position: Some(2),
            time_in_queue: 10,
            processing_time: None,
        };

        assert_eq!(response.item_id, "test-123");
        assert_eq!(response.queue_position, Some(2));
    }

    #[test]
    fn test_queue_metrics_response_creation() {
        let mut per_tenant = std::collections::HashMap::new();
        per_tenant.insert("tenant-1".to_string(), 2);
        per_tenant.insert("tenant-2".to_string(), 3);

        let response = QueueMetricsResponse {
            queue_depth: 5,
            max_queue_depth: 100,
            total_processed: 1000,
            total_failed: 10,
            avg_processing_time_ms: 250.5,
            per_tenant_depths: per_tenant,
        };

        assert_eq!(response.queue_depth, 5);
        assert_eq!(response.total_processed, 1000);
        assert_eq!(response.per_tenant_depths.len(), 2);
    }
}
