use crate::state::DatasetProgressEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn emit_progress(
    tx: Option<&Arc<broadcast::Sender<DatasetProgressEvent>>>,
    dataset_id: &str,
    event_type: &str,
    current_file: Option<String>,
    percentage_complete: f32,
    message: String,
    total_files: Option<i32>,
    files_processed: Option<i32>,
) {
    if let Some(sender) = tx {
        let _ = sender.send(DatasetProgressEvent {
            dataset_id: dataset_id.to_string(),
            event_type: event_type.to_string(),
            current_file,
            percentage_complete,
            total_files,
            files_processed,
            message,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }
}
