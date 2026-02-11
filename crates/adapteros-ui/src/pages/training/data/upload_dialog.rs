//! Document upload dialog component.
//!
//! Provides a drag-and-drop file upload interface for documents with:
//! - Drag/drop zone with visual feedback
//! - Upload progress tracking
//! - Post-upload checksum display
//! - Validation results (row count, token estimate, safety scan)

use crate::components::spinner::SpinnerSize;
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, Spinner};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::{api_base_url, ApiClient};
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;

/// Supported file extensions.
#[cfg(target_arch = "wasm32")]
const SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".html", ".json", ".jsonl"];

/// Maximum file size (50 MB)
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

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

    // Reset state when dialog closes
    Effect::new(move || {
        if !open.get() {
            uploading.set(false);
            upload_progress.set(0);
            error_msg.set(None);
            selected_file.set(None);
            selected_file_size.set(None);
            is_dragging.set(false);
            upload_result.set(None);
            #[cfg(target_arch = "wasm32")]
            file_ref.set(None);
        }
    });

    // Helper to process and validate a file
    #[cfg(target_arch = "wasm32")]
    let process_file = {
        move |file: web_sys::File| {
            error_msg.set(None);
            upload_result.set(None);

            // Validate file size
            let size = file.size() as u64;
            if size > MAX_FILE_SIZE {
                error_msg.set(Some(format!(
                    "File too large. Maximum size is {} MB.",
                    MAX_FILE_SIZE / 1024 / 1024
                )));
                return;
            }

            // Validate file type
            let name = file.name();
            let ext_valid = SUPPORTED_EXTENSIONS
                .iter()
                .any(|ext| name.to_lowercase().ends_with(ext));

            if !ext_valid {
                error_msg.set(Some(format!(
                    "Unsupported file type. Supported: {}",
                    SUPPORTED_EXTENSIONS.join(", ")
                )));
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

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(&api_base_url());

                // Simulate progress during upload
                upload_progress.set(30);

                match client.upload_document(&file).await {
                    Ok(response) => {
                        upload_progress.set(90);

                        // Build upload result with validation details
                        let result = UploadResult {
                            document_id: response.document_id.clone(),
                            checksum: Some(response.hash_b3.clone()),
                            row_count: response.chunk_count,
                            token_estimate: None, // Token estimate calculated later
                            safety_status: SafetyScanStatus::from_str(&response.status),
                        };

                        upload_progress.set(100);
                        upload_result.set(Some(result));
                        uploading.set(false);

                        // Brief delay to show completion before closing
                        gloo_timers::callback::Timeout::new(500, move || {
                            open.set(false);
                            success_callback.run(response.document_id);
                        })
                        .forget();
                    }
                    Err(e) => {
                        error_msg.set(Some(e.user_message()));
                        upload_progress.set(0);
                        uploading.set(false);
                    }
                }
            });
        }
    });

    let close_dialog = move |_: web_sys::MouseEvent| {
        if !uploading.get() {
            open.set(false);
        }
    };

    let close_dialog_unit = move |_: ()| {
        if !uploading.get() {
            open.set(false);
        }
    };

    // Derived signal for button disabled state
    let upload_disabled = Signal::derive(move || uploading.get() || selected_file.get().is_none());

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
        <Show when=move || open.get()>
            <div class="dialog-overlay" on:click=close_dialog>
                <div class="dialog-content upload-dialog" on:click=|ev| ev.stop_propagation()>
                    <div class="dialog-header">
                        <h2 class="dialog-title">"Upload Document"</h2>
                        <button
                            type="button"
                            class="dialog-close"
                            on:click=close_dialog
                            disabled=uploading.get()
                            aria-label="Close dialog"
                        >
                            "×"
                        </button>
                    </div>

                    <div class="dialog-body">
                        // Drag and drop zone
                        <div
                            class=move || {
                                let mut classes = "upload-dropzone".to_string();
                                if is_dragging.get() {
                                    classes.push_str(" upload-dropzone-active");
                                }
                                if selected_file.get().is_some() {
                                    classes.push_str(" upload-dropzone-has-file");
                                }
                                if uploading.get() {
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
                                {move || if selected_file.get().is_some() {
                                    view! {
                                        <div class="upload-dropzone-file">
                                            <span class="upload-dropzone-icon">"📄"</span>
                                            <span class="upload-dropzone-filename">
                                                {selected_file.get().unwrap_or_default()}
                                            </span>
                                            {move || selected_file_size.get().map(|size| {
                                                view! {
                                                    <span class="upload-dropzone-filesize">
                                                        {format_bytes(size)}
                                                    </span>
                                                }
                                            })}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="upload-dropzone-prompt">
                                            <span class="upload-dropzone-icon-large">"📁"</span>
                                            <p class="upload-dropzone-text">
                                                "Drag and drop a file here, or click to browse"
                                            </p>
                                            <p class="upload-dropzone-hint">
                                                "Supported: PDF, TXT, Markdown, HTML, JSON, JSONL"
                                            </p>
                                            <p class="upload-dropzone-hint">
                                                {format!("Maximum size: {} MB", MAX_FILE_SIZE / 1024 / 1024)}
                                            </p>
                                        </div>
                                    }.into_any()
                                }}

                                <input
                                    type="file"
                                    accept=".pdf,.txt,.md,.html,.json,.jsonl"
                                    on:change=handle_file_change
                                    disabled=uploading.get()
                                    class="upload-dropzone-input"
                                />
                            </div>
                        </div>

                        // Progress bar (shown during upload)
                        {move || {
                            let progress = upload_progress.get();
                            if uploading.get() || progress > 0 {
                                Some(view! {
                                    <div class="upload-progress">
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
                        {move || upload_result.get().map(|result| {
                            view! {
                                <Card title="Upload Results">
                                    <dl class="upload-result-list">
                                        // Checksum
                                        {result.checksum.as_ref().map(|checksum| {
                                            view! {
                                                <div class="upload-result-item">
                                                    <dt class="upload-result-label">"Checksum (BLAKE3)"</dt>
                                                    <dd class="upload-result-value font-mono text-sm">
                                                        {checksum.chars().take(16).collect::<String>()}
                                                        "..."
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
                        {move || error_msg.get().map(|err| {
                            view! {
                                <div class="upload-error">
                                    {err}
                                </div>
                            }
                        })}
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
                            {move || if uploading.get() {
                                view! {
                                    <div class="button-loading">
                                        <Spinner size=SpinnerSize::Sm />
                                        " Uploading..."
                                    </div>
                                }.into_any()
                            } else {
                                view! { "Upload" }.into_any()
                            }}
                        </Button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
