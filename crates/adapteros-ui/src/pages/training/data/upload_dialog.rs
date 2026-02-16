//! Document upload dialog component.
//!
//! Provides a drag-and-drop file upload interface for documents with:
//! - Drag/drop zone with visual feedback
//! - Upload progress tracking
//! - Post-upload checksum display
//! - Validation results (row count, token estimate, safety scan)

use crate::components::spinner::SpinnerSize;
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, Dialog, DialogSize, Spinner};
use leptos::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[cfg(target_arch = "wasm32")]
use crate::api::{api_base_url, ApiClient};
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

/// Document upload dialog with drag/drop support.
#[component]
pub fn DocumentUploadDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Callback when upload completes successfully
    on_success: Callback<String>,
) -> impl IntoView {
    let uploading = RwSignal::new(false);
    let upload_progress = RwSignal::new(0u8); // 0-100 progress percentage
    let error_msg = RwSignal::new(None::<String>);
    let selected_file = RwSignal::new(None::<String>);
    let selected_file_size = RwSignal::new(None::<u64>);
    let is_dragging = RwSignal::new(false);
    let upload_result = RwSignal::new(None::<UploadResult>);

    #[cfg(target_arch = "wasm32")]
    let file_ref: RwSignal<Option<SendWrapper<web_sys::File>>> = RwSignal::new(None);
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
            let _ = error_msg.try_set(None);
            let _ = selected_file.try_set(None);
            let _ = selected_file_size.try_set(None);
            let _ = is_dragging.try_set(false);
            let _ = upload_result.try_set(None);
            #[cfg(target_arch = "wasm32")]
            let _ = file_ref.try_set(None);
        }
    });

    // Helper to process and validate a file
    #[cfg(target_arch = "wasm32")]
    let process_file = {
        move |file: web_sys::File| {
            error_msg.set(None);
            upload_result.set(None);
            selected_file.set(None);
            selected_file_size.set(None);
            file_ref.set(None);

            // Validate file size
            let size = file.size() as u64;
            let name = file.name();
            if let Err(validation_error) =
                validate_document_upload_input(&name, size, MAX_FILE_SIZE, SUPPORTED_EXTENSIONS)
            {
                error_msg.set(Some(validation_error));
                return;
            }

            selected_file.set(Some(name));
            selected_file_size.set(Some(size));
            file_ref.set(Some(SendWrapper::new(file)));
        }
    };

    #[cfg(target_arch = "wasm32")]
    let handle_file_change = {
        let process_file = process_file.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        process_file(file);
                    }
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
        let process_file = process_file.clone();
        move |ev: web_sys::DragEvent| {
            ev.prevent_default();
            is_dragging.set(false);

            if let Some(data_transfer) = ev.data_transfer() {
                if let Some(files) = data_transfer.files() {
                    if let Some(file) = files.get(0) {
                        process_file(file);
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
            let Some(file_wrapper) = file_ref.get() else {
                error_msg.set(Some("Please select a file first".into()));
                return;
            };

            uploading.set(true);
            upload_progress.set(10); // Show initial progress
            error_msg.set(None);

            let file = file_wrapper.take();
            let open = open;
            let uploading = uploading;
            let upload_progress = upload_progress;
            let error_msg = error_msg;
            let upload_result = upload_result;
            let success_callback = on_success.clone();
            let is_active = Arc::clone(&is_active);

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(&api_base_url());
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }

                // Simulate progress during upload
                let _ = upload_progress.try_set(30);

                match client.upload_document(&file).await {
                    Ok(response) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = upload_progress.try_set(90);

                        // Build upload result with validation details
                        let result = UploadResult {
                            document_id: response.document_id.clone(),
                            checksum: Some(response.hash_b3.clone()),
                            row_count: response.chunk_count,
                            token_estimate: None, // Token estimate calculated later
                            safety_status: SafetyScanStatus::from_str(&response.status),
                        };

                        let _ = upload_progress.try_set(100);
                        let _ = upload_result.try_set(Some(result));
                        let _ = uploading.try_set(false);

                        // Brief delay to show completion before closing
                        let document_id = response.document_id.clone();
                        let is_active = Arc::clone(&is_active);
                        gloo_timers::callback::Timeout::new(500, move || {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            open.set(false);
                            success_callback.run(document_id);
                        })
                        .forget();
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = error_msg.try_set(Some(e.user_message()));
                        let _ = upload_progress.try_set(0);
                        let _ = uploading.try_set(false);
                    }
                }
            });
        }
    });

    let close_dialog_unit = move |_: ()| {
        if !uploading.get() {
            open.set(false);
        }
    };

    // Derived signal for button disabled state
    let upload_disabled = Signal::derive(move || uploading.try_get().unwrap_or(false) || selected_file.try_get().flatten().is_none());

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
                        if selected_file.try_get().flatten().is_some() {
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
                            when=move || selected_file.try_get().flatten().is_some()
                            fallback=move || view! {
                                <div class="upload-dropzone-prompt">
                                    <span class="upload-dropzone-icon-large">"📁"</span>
                                    <p class="upload-dropzone-text">
                                        "Drag and drop a file here, or click to browse"
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
                                    {move || selected_file.try_get().flatten().unwrap_or_default()}
                                </span>
                                {move || selected_file_size.try_get().flatten().map(|size| {
                                    view! {
                                        <span class="upload-dropzone-filesize">
                                            {format_bytes(size)}
                                        </span>
                                    }
                                })}
                            </div>
                        </Show>

                        <input
                            type="file"
                            accept=".pdf,.txt,.md,.markdown"
                            on:change=handle_file_change
                            disabled=uploading.get()
                            class="upload-dropzone-input"
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

                    // Upload result with validation details
                    {move || upload_result.try_get().flatten().map(|result| {
                        view! {
                            <Card title="Upload Results">
                                <dl class="upload-result-list">
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
                        fallback=move || view! { "Upload" }
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
        assert!(
            result
                .expect_err("oversized file should be rejected")
                .contains("File too large")
        );
    }
}
