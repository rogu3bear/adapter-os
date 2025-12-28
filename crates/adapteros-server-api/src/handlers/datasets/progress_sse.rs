//! Dataset upload progress SSE handler.

use super::types::ProgressStreamQuery;
use crate::error_helpers::internal_error;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Json,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

/// Stream dataset upload and processing progress via SSE
///
/// This endpoint establishes a Server-Sent Events (SSE) connection that streams
/// progress events for dataset operations. Clients can connect to receive real-time
/// updates about:
/// - File upload progress (percentage, current file)
/// - Dataset validation progress (files processed, validation results)
/// - Statistics computation progress
///
/// Events are JSON objects with the following fields:
/// - `dataset_id`: The ID of the dataset being processed
/// - `event_type`: One of "upload", "validation", or "statistics"
/// - `current_file`: The file currently being processed (optional)
/// - `percentage_complete`: Overall progress as a percentage (0-100)
/// - `total_files`: Total number of files in the dataset (optional)
/// - `files_processed`: Number of files processed so far (optional)
/// - `message`: Human-readable status message
/// - `timestamp`: RFC3339 formatted timestamp
///
/// Example client usage (JavaScript):
/// ```javascript
/// const eventSource = new EventSource('/v1/datasets/upload/progress?dataset_id=abc123');
/// eventSource.onmessage = (event) => {
///   const progress = JSON.parse(event.data);
///   console.log(`${progress.message}: ${progress.percentage_complete}%`);
/// };
/// ```
#[utoipa::path(
    get,
    path = "/v1/datasets/upload/progress",
    params(
        ("dataset_id" = Option<String>, Query, description = "Optional filter by dataset ID")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of dataset progress"),
        (status = 503, description = "Progress streaming not available")
    ),
    tag = "datasets"
)]
pub async fn dataset_upload_progress(
    State(state): State<AppState>,
    Query(query): Query<ProgressStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Get progress broadcast channel from state
    let rx = state
        .dataset_progress_tx
        .as_ref()
        .ok_or_else(|| internal_error("Dataset progress streaming not available"))?
        .subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(event) => {
                // Filter by dataset_id if specified
                if let Some(ref dataset_id) = query.dataset_id {
                    if event.dataset_id != *dataset_id {
                        return None;
                    }
                }

                // Convert to SSE event
                match serde_json::to_string(&event) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
