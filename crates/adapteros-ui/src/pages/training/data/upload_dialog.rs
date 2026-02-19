//! Document upload dialog component.
//!
//! Provides a drag-and-drop file upload interface for documents with:
//! - Drag/drop zone with visual feedback
//! - Upload progress tracking
//! - Post-upload checksum display
//! - Validation results (row count, token estimate, safety scan)

use crate::components::spinner::SpinnerSize;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Dialog, DialogSize, Spinner,
};
use leptos::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[cfg(target_arch = "wasm32")]
use crate::api::{api_base_url, ApiClient, ApiError};
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;

/// Supported file extensions.
#[cfg(target_arch = "wasm32")]
// Keep in sync with backend `detect_document_kind()` (.md and .markdown are both supported).
const SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".markdown"];

/// Maximum file size (100 MB)
const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

fn validate_document_upload_input(
    file_name: &str,
    size: u64,
    max_file_size: u64,
    supported_extensions: &[&str],
) -> Result<(), String> {
    if size > max_file_size {
        return Err(format!(
            "File too large. Maximum size is {} MB.",
            max_file_size / 1024 / 1024
        ));
    }

    let name_lower = file_name.to_lowercase();
    let ext_valid = supported_extensions
        .iter()
        .any(|ext| name_lower.ends_with(ext));
    if !ext_valid {
        return Err(format!(
            "Unsupported file type. Supported: {}",
            supported_extensions.join(", ")
        ));
    }

    Ok(())
}

/// Upload result with validation details.
#[derive(Clone, Debug, Default)]
pub struct UploadResult {
    /// Document ID returned from server
    pub document_id: String,
    /// BLAKE3 checksum of the uploaded file
    pub checksum: Option<String>,
    /// Row count (for structured files like JSONL)
    pub row_count: Option<i32>,
    /// Token estimate for the document
    pub token_estimate: Option<i64>,
    /// Safety scan status
    pub safety_status: SafetyScanStatus,
}

/// Batch upload completion payload.
#[derive(Clone, Debug, Default)]
pub struct UploadBatchResult {
    /// Uploaded document IDs (may be empty when training endpoint creates dataset directly).
    pub document_ids: Vec<String>,
    /// Dataset ID when dataset creation is enabled and succeeds.
    pub dataset_id: Option<String>,
}

/// Safety scan status for uploaded documents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SafetyScanStatus {
    /// Scan not yet performed
    #[default]
    Pending,
    /// Scan passed - content is safe
    Passed,
    /// Scan found warnings but content is allowed
    Warning,
    /// Scan failed - content blocked
    Failed,
}

impl SafetyScanStatus {
    pub fn label(&self) -> &'static str {
        match self {
            SafetyScanStatus::Pending => "Pending",
            SafetyScanStatus::Passed => "Passed",
            SafetyScanStatus::Warning => "Warning",
            SafetyScanStatus::Failed => "Failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "passed" | "safe" | "allowed" => SafetyScanStatus::Passed,
            "warning" | "warn" | "allowed_with_warning" => SafetyScanStatus::Warning,
            "failed" | "blocked" | "rejected" => SafetyScanStatus::Failed,
            _ => SafetyScanStatus::Pending,
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn should_fallback_training_upload(error: &ApiError) -> bool {
    error.is_not_found()
        || matches!(
            error.code(),
            Some("FEATURE_DISABLED" | "ENDPOINT_NOT_FOUND" | "NOT_IMPLEMENTED")
        )
}

#[cfg(target_arch = "wasm32")]
fn build_upload_result(response: &crate::api::DocumentResponse) -> UploadResult {
    UploadResult {
        document_id: response.document_id.clone(),
        checksum: Some(response.hash_b3.clone()),
        row_count: response.chunk_count,
        token_estimate: None, // Token estimate calculated later
        safety_status: SafetyScanStatus::from_str(&response.status),
    }
}

#[cfg(target_arch = "wasm32")]
async fn wait_for_document_indexed(client: &ApiClient, document_id: &str) -> Result<(), String> {
    // ~2 minutes max wait (80 * 1.5s). Keeps the UX responsive while allowing indexing time.
    const MAX_POLLS: usize = 80;
    const POLL_DELAY_MS: u32 = 1500;

    for _ in 0..MAX_POLLS {
        match client.get_document(document_id).await {
            Ok(document) => match document.status.as_str() {
                "indexed" => return Ok(()),
                "failed" | "error" => {
                    return Err(document
                        .error_message
                        .filter(|m| !m.trim().is_empty())
                        .unwrap_or_else(|| {
                            "One of your files could not be prepared. Please try again.".into()
                        }))
                }
                _ => {}
            },
            Err(e) => return Err(e.user_message()),
        }

        gloo_timers::future::TimeoutFuture::new(POLL_DELAY_MS).await;
    }

    Err("Your files are still being prepared. Please check the Documents tab and retry dataset creation.".into())
}

/// Document upload dialog with drag/drop support.
#[component]
pub fn DocumentUploadDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Callback when upload completes successfully
    on_success: Callback<UploadBatchResult>,
    /// Allow selecting and uploading multiple files.
    #[prop(optional)]
    allow_multiple: bool,
    /// When true, create a training dataset from uploaded file(s).
    #[prop(optional)]
    auto_create_dataset: bool,
    /// Prefer direct training dataset upload endpoint when available (single-file only).
    #[prop(optional)]
    prefer_training_dataset_upload: bool,
) -> impl IntoView {
    let uploading = RwSignal::new(false);
    let upload_progress = RwSignal::new(0u8); // 0-100 progress percentage
    let status_msg = RwSignal::new(None::<String>);
    let error_msg = RwSignal::new(None::<String>);
    let selected_files = RwSignal::new(Vec::<(String, u64)>::new());
    let is_dragging = RwSignal::new(false);
    let upload_result = RwSignal::new(None::<UploadResult>);

    #[cfg(target_arch = "wasm32")]
    let file_refs: RwSignal<Vec<SendWrapper<web_sys::File>>> = RwSignal::new(Vec::new());
    let is_active = Arc::new(AtomicBool::new(true));
    on_cleanup({
        let is_active = Arc::clone(&is_active);
        move || is_active.store(false, Ordering::Relaxed)
    });

    // Reset state when dialog closes
    Effect::new(move || {
        if !open.try_get().unwrap_or(true) {
            let _ = uploading.try_set(false);
            let _ = upload_progress.try_set(0);
            let _ = status_msg.try_set(None);
            let _ = error_msg.try_set(None);
            let _ = selected_files.try_set(Vec::new());
            let _ = is_dragging.try_set(false);
            let _ = upload_result.try_set(None);
            #[cfg(target_arch = "wasm32")]
            let _ = file_refs.try_set(Vec::new());
        }
    });

    // Helper to process and validate file selection
    #[cfg(target_arch = "wasm32")]
    let process_files = {
        move |files: web_sys::FileList| {
            error_msg.set(None);
            upload_result.set(None);
            status_msg.set(None);
            selected_files.set(Vec::new());
            file_refs.set(Vec::new());

            let mut valid_files = Vec::new();
            let mut file_details = Vec::new();
            let max_files = if allow_multiple { files.length() } else { 1 };

            for idx in 0..max_files {
                let Some(file) = files.get(idx) else {
                    continue;
                };

                let size = file.size() as u64;
                let name = file.name();
                if let Err(validation_error) =
                    validate_document_upload_input(&name, size, MAX_FILE_SIZE, SUPPORTED_EXTENSIONS)
                {
                    error_msg.set(Some(format!("{}: {}", name, validation_error)));
                    return;
                }

                file_details.push((name, size));
                valid_files.push(SendWrapper::new(file));
            }

            if valid_files.is_empty() {
                error_msg.set(Some("Please select at least one file".into()));
                return;
            }

            selected_files.set(file_details);
            file_refs.set(valid_files);
        }
    };

    #[cfg(target_arch = "wasm32")]
    let handle_file_change = {
        let process_files = process_files.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    process_files(files);
                }
                input.set_value("");
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_change = |_ev: web_sys::Event| {};

    // Drag event handlers
    #[cfg(target_arch = "wasm32")]
    let handle_drag_enter = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        is_dragging.set(true);
    };

    #[cfg(target_arch = "wasm32")]
    let handle_drag_over = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        is_dragging.set(true);
    };

    #[cfg(target_arch = "wasm32")]
    let handle_drag_leave = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        is_dragging.set(false);
    };

    #[cfg(target_arch = "wasm32")]
    let handle_drop = {
        let process_files = process_files.clone();
        move |ev: web_sys::DragEvent| {
            use wasm_bindgen::JsCast;
            ev.prevent_default();
            is_dragging.set(false);

            if let Ok(data_transfer_value) = js_sys::Reflect::get(
                ev.as_ref(),
                &wasm_bindgen::JsValue::from_str("dataTransfer"),
            ) {
                if !data_transfer_value.is_null() && !data_transfer_value.is_undefined() {
                    if let Ok(files_value) = js_sys::Reflect::get(
                        &data_transfer_value,
                        &wasm_bindgen::JsValue::from_str("files"),
                    ) {
                        if let Ok(files) = files_value.dyn_into::<web_sys::FileList>() {
                            process_files(files);
                        }
                    }
                }
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_drag_enter = |_ev: web_sys::DragEvent| {};
    #[cfg(not(target_arch = "wasm32"))]
    let handle_drag_over = |_ev: web_sys::DragEvent| {};
    #[cfg(not(target_arch = "wasm32"))]
    let handle_drag_leave = |_ev: web_sys::DragEvent| {};
    #[cfg(not(target_arch = "wasm32"))]
    let handle_drop = |_ev: web_sys::DragEvent| {};

    let handle_upload = Callback::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let file_wrappers = file_refs.get();
            if file_wrappers.is_empty() {
                error_msg.set(Some("Please select at least one file first".into()));
                return;
            }

            uploading.set(true);
            upload_progress.set(10); // Show initial progress
            status_msg.set(None);
            error_msg.set(None);

            let files: Vec<web_sys::File> =
                file_wrappers.into_iter().map(|file| file.take()).collect();
            let open = open;
            let uploading = uploading;
            let upload_progress = upload_progress;
            let status_msg = status_msg;
            let error_msg = error_msg;
            let upload_result = upload_result;
            let success_callback = on_success.clone();
            let is_active = Arc::clone(&is_active);
            let auto_create_dataset = auto_create_dataset;
            let prefer_training_dataset_upload = prefer_training_dataset_upload;

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(&api_base_url());
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }

                let files_for_upload = files;
                let mut document_ids = Vec::<String>::new();
                let mut results = Vec::<UploadResult>::new();
                let mut dataset_id = None::<String>;

                if auto_create_dataset
                    && prefer_training_dataset_upload
                    && files_for_upload.len() == 1
                {
                    if let Some(file) = files_for_upload.first() {
                        match client
                            .create_training_dataset_from_upload(file, None, None, None)
                            .await
                        {
                            Ok(dataset) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let completion = UploadBatchResult {
                                    document_ids: Vec::new(),
                                    dataset_id: Some(dataset.id),
                                };
                                let _ = upload_progress.try_set(100);
                                let _ = status_msg.try_set(None);
                                let _ = upload_result.try_set(None);
                                let _ = uploading.try_set(false);

                                let is_active = Arc::clone(&is_active);
                                gloo_timers::callback::Timeout::new(500, move || {
                                    if !is_active.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    open.set(false);
                                    success_callback.run(completion.clone());
                                })
                                .forget();
                                return;
                            }
                            Err(e) if should_fallback_training_upload(&e) => {}
                            Err(e) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ = error_msg.try_set(Some(e.user_message()));
                                let _ = status_msg.try_set(None);
                                let _ = upload_progress.try_set(0);
                                let _ = uploading.try_set(false);
                                return;
                            }
                        }
                    }
                }

                let total = files_for_upload.len().max(1) as u8;
                for (idx, file) in files_for_upload.iter().enumerate() {
                    let step_progress = 20u8.saturating_add(((idx as u8) * 50) / total);
                    let _ = upload_progress.try_set(step_progress);

                    match client.upload_document(file).await {
                        Ok(response) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            document_ids.push(response.document_id.clone());
                            results.push(build_upload_result(&response));
                        }
                        Err(e) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = error_msg.try_set(Some(e.user_message()));
                            let _ = status_msg.try_set(None);
                            let _ = upload_progress.try_set(0);
                            let _ = uploading.try_set(false);
                            return;
                        }
                    }
                }

                if auto_create_dataset && !document_ids.is_empty() {
                    let _ = upload_progress.try_set(80);
                    let _ = status_msg.try_set(Some("Preparing your files…".into()));
                    for document_id in &document_ids {
                        if let Err(message) = wait_for_document_indexed(&client, document_id).await
                        {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = error_msg.try_set(Some(message));
                            let _ = status_msg.try_set(None);
                            let _ = upload_progress.try_set(0);
                            let _ = uploading.try_set(false);
                            return;
                        }
                    }

                    let _ = upload_progress.try_set(95);
                    match client
                        .create_dataset_from_documents(document_ids.clone(), None)
                        .await
                    {
                        Ok(dataset) => {
                            dataset_id = Some(dataset.id);
                        }
                        Err(e) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = error_msg.try_set(Some(e.user_message()));
                            let _ = status_msg.try_set(None);
                            let _ = upload_progress.try_set(0);
                            let _ = uploading.try_set(false);
                            return;
                        }
                    }
                }

                let _ = upload_progress.try_set(100);
                let _ = upload_result.try_set(results.first().cloned());
                let _ = status_msg.try_set(None);
                let _ = uploading.try_set(false);

                let completion = UploadBatchResult {
                    document_ids,
                    dataset_id,
                };
                let is_active = Arc::clone(&is_active);
                gloo_timers::callback::Timeout::new(500, move || {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    open.set(false);
                    success_callback.run(completion.clone());
                })
                .forget();
            });
        }
    });

    let close_dialog_unit = move |_: ()| {
        if !uploading.get() {
            open.set(false);
        }
    };

    // Derived signal for button disabled state
    let upload_disabled = Signal::derive(move || {
        uploading.try_get().unwrap_or(false)
            || selected_files.try_get().unwrap_or_default().is_empty()
    });

    // Helper to format bytes
    let format_bytes = |bytes: u64| -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    };

    view! {
        <Dialog
            open=open
            title="Upload Document"
            size=DialogSize::Lg
        >
            <div class="dialog-body">
                // Drag and drop zone
                <div
                    class=move || {
                        let mut classes = "upload-dropzone".to_string();
                        if is_dragging.try_get().unwrap_or(false) {
                            classes.push_str(" upload-dropzone-active");
                        }
                        if !selected_files.try_get().unwrap_or_default().is_empty() {
                            classes.push_str(" upload-dropzone-has-file");
                        }
                        if uploading.try_get().unwrap_or(false) {
                            classes.push_str(" upload-dropzone-uploading");
                        }
                        classes
                    }
                    on:dragenter=handle_drag_enter
                    on:dragover=handle_drag_over
                    on:dragleave=handle_drag_leave
                    on:drop=handle_drop
                >
                    <div class="upload-dropzone-content">
                        <Show
                            when=move || !selected_files.try_get().unwrap_or_default().is_empty()
                            fallback=move || view! {
                                <div class="upload-dropzone-prompt">
                                    <span class="upload-dropzone-icon-large">"📁"</span>
                                    <p class="upload-dropzone-text">
                                        {if allow_multiple {
                                            "Drag and drop files here, or click to browse"
                                        } else {
                                            "Drag and drop a file here, or click to browse"
                                        }}
                                    </p>
                                    <p class="upload-dropzone-hint">
                                        "Supported: PDF, TXT, Markdown"
                                    </p>
                                    <p class="upload-dropzone-hint">
                                        {format!("Maximum size: {} MB", MAX_FILE_SIZE / 1024 / 1024)}
                                    </p>
                                </div>
                            }
                        >
                            <div class="upload-dropzone-file">
                                <span class="upload-dropzone-icon">"📄"</span>
                                <span class="upload-dropzone-filename">
                                    {move || {
                                        let files = selected_files.try_get().unwrap_or_default();
                                        if files.is_empty() {
                                            String::new()
                                        } else if files.len() == 1 {
                                            files[0].0.clone()
                                        } else {
                                            format!("{} files selected", files.len())
                                        }
                                    }}
                                </span>
                                {move || {
                                    let files = selected_files.try_get().unwrap_or_default();
                                    if files.is_empty() {
                                        None
                                    } else if files.len() == 1 {
                                        Some(view! {
                                            <span class="upload-dropzone-filesize">
                                                {format_bytes(files[0].1)}
                                            </span>
                                        })
                                    } else {
                                        let total_size: u64 = files.iter().map(|(_, size)| *size).sum();
                                        Some(view! {
                                            <span class="upload-dropzone-filesize">
                                                {format!("{} total", format_bytes(total_size))}
                                            </span>
                                        })
                                    }
                                }}
                            </div>
                        </Show>

                        <input
                            type="file"
                            accept=".pdf,.txt,.md,.markdown"
                            multiple=allow_multiple
                            on:change=handle_file_change
                            disabled=uploading.get()
                            class="upload-dropzone-input"
                            aria_label="Upload dataset file"
                        />
                    </div>
                </div>

                // Upload progress and status (aria-live region for screen readers)
                <div aria-live="polite" aria-atomic="true">
                    // Progress bar (shown during upload)
                    {move || {
                        let progress = upload_progress.try_get().unwrap_or(0);
                        if uploading.try_get().unwrap_or(false) || progress > 0 {
                            Some(view! {
                                <div class="upload-progress" role="progressbar"
                                    aria-valuenow=progress
                                    aria-valuemin=0
                                    aria-valuemax=100
                                >
                                    <div class="upload-progress-bar">
                                        <div
                                            class="upload-progress-fill"
                                            style=format!("width: {}%", progress)
                                        />
                                    </div>
                                    <span class="upload-progress-text">
                                        {if progress == 100 {
                                            "Complete!".to_string()
                                        } else {
                                            format!("{}%", progress)
                                        }}
                                    </span>
                                </div>
                            })
                        } else {
                            None
                        }
                    }}

                    // Upload status copy for long-running wizard path operations.
                    {move || status_msg.try_get().flatten().map(|status| {
                        view! {
                            <div class="upload-dropzone-hint">
                                {status}
                            </div>
                        }
                    })}

                    // Upload result with validation details
                    {move || upload_result.try_get().flatten().map(|result| {
                        view! {
                            <Card title="Upload Results">
                                <dl class="upload-result-list">
                                    // Document id
                                    <div class="upload-result-item">
                                        <dt class="upload-result-label">"Document ID"</dt>
                                        <dd class="upload-result-value font-mono text-sm">
                                            {result.document_id.clone()}
                                        </dd>
                                    </div>

                                    // Checksum
                                    {result.checksum.as_ref().map(|checksum| {
                                        view! {
                                            <div class="upload-result-item">
                                                <dt class="upload-result-label">"Checksum (BLAKE3)"</dt>
                                                <dd class="upload-result-value font-mono text-sm">
                                                    {adapteros_id::format_hash_short(checksum)}
                                                </dd>
                                            </div>
                                        }
                                    })}

                                    // Row/Chunk count
                                    {result.row_count.map(|count| {
                                        view! {
                                            <div class="upload-result-item">
                                                <dt class="upload-result-label">"Chunks"</dt>
                                                <dd class="upload-result-value">{count.to_string()}</dd>
                                            </div>
                                        }
                                    })}

                                    // Token estimate
                                    {result.token_estimate.map(|tokens| {
                                        view! {
                                            <div class="upload-result-item">
                                                <dt class="upload-result-label">"Token Estimate"</dt>
                                                <dd class="upload-result-value">{tokens.to_string()}</dd>
                                            </div>
                                        }
                                    })}

                                    // Safety scan status
                                    <div class="upload-result-item">
                                        <dt class="upload-result-label">"Safety Scan"</dt>
                                        <dd class="upload-result-value">
                                            <Badge variant={
                                                match result.safety_status {
                                                    SafetyScanStatus::Passed => BadgeVariant::Success,
                                                    SafetyScanStatus::Warning => BadgeVariant::Warning,
                                                    SafetyScanStatus::Failed => BadgeVariant::Destructive,
                                                    SafetyScanStatus::Pending => BadgeVariant::Default,
                                                }
                                            }>
                                                {result.safety_status.label()}
                                            </Badge>
                                        </dd>
                                    </div>
                                </dl>
                            </Card>
                        }
                    })}

                    // Error message
                    {move || error_msg.try_get().flatten().map(|err| {
                        view! {
                            <div class="upload-error" role="alert">
                                {err}
                            </div>
                        }
                    })}
                </div>
            </div>

            <div class="dialog-footer">
                <Button
                    variant=ButtonVariant::Ghost
                    disabled=uploading
                    on_click=Callback::new(close_dialog_unit)
                >
                    "Cancel"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    disabled=upload_disabled
                    on_click=handle_upload
                >
                    <Show
                        when=move || uploading.try_get().unwrap_or(false)
                        fallback=move || view! {
                            {if allow_multiple {
                                "Upload Files"
                            } else {
                                "Upload"
                            }}
                        }
                    >
                        <div class="button-loading">
                            <Spinner size=SpinnerSize::Sm />
                            " Uploading..."
                        </div>
                    </Show>
                </Button>
            </div>
        </Dialog>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".markdown"];
    const TEST_MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

    #[test]
    fn validate_document_upload_input_accepts_markdown_extension() {
        let result = validate_document_upload_input(
            "training-doc.markdown",
            1024,
            TEST_MAX_FILE_SIZE,
            TEST_SUPPORTED_EXTENSIONS,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_document_upload_input_is_case_insensitive_for_markdown() {
        let result = validate_document_upload_input(
            "TRAINING-DOC.MD",
            1024,
            TEST_MAX_FILE_SIZE,
            TEST_SUPPORTED_EXTENSIONS,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_document_upload_input_rejects_oversized_files() {
        let result = validate_document_upload_input(
            "training-doc.md",
            TEST_MAX_FILE_SIZE + 1,
            TEST_MAX_FILE_SIZE,
            TEST_SUPPORTED_EXTENSIONS,
        );
        assert!(result
            .expect_err("oversized file should be rejected")
            .contains("File too large"));
    }
}
