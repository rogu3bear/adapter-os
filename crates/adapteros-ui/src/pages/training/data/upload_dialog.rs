//! Document upload dialog component.
//!
//! Provides a simple file upload interface for documents.

use crate::components::spinner::SpinnerSize;
use crate::components::{Button, ButtonVariant, Card, Spinner};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::{api_base_url, ApiClient};
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;

/// Supported file extensions.
#[cfg(target_arch = "wasm32")]
const SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".html", ".json"];

/// Maximum file size (50 MB)
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Document upload dialog.
#[component]
pub fn DocumentUploadDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Callback when upload completes successfully
    on_success: Callback<String>,
) -> impl IntoView {
    let uploading = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);
    let selected_file = RwSignal::new(None::<String>);

    #[cfg(target_arch = "wasm32")]
    let file_ref: RwSignal<Option<SendWrapper<web_sys::File>>> = RwSignal::new(None);

    // Reset state when dialog closes
    Effect::new(move || {
        if !open.get() {
            uploading.set(false);
            error_msg.set(None);
            selected_file.set(None);
            #[cfg(target_arch = "wasm32")]
            file_ref.set(None);
        }
    });

    #[cfg(target_arch = "wasm32")]
    let handle_file_change = {
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            error_msg.set(None);
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        // Validate file size
                        if file.size() as u64 > MAX_FILE_SIZE {
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
                        file_ref.set(Some(SendWrapper::new(file)));
                    }
                }
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_change = |_ev: web_sys::Event| {};

    let handle_upload = Callback::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file_wrapper) = file_ref.get() else {
                error_msg.set(Some("Please select a file first".into()));
                return;
            };

            uploading.set(true);
            error_msg.set(None);

            let file = file_wrapper.take();
            let open = open;
            let uploading = uploading;
            let error_msg = error_msg;
            let success_callback = on_success.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::with_base_url(&api_base_url());

                match client.upload_document(&file).await {
                    Ok(response) => {
                        uploading.set(false);
                        open.set(false);
                        success_callback.run(response.document_id);
                    }
                    Err(e) => {
                        error_msg.set(Some(e.to_string()));
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
                        <Card title="Select File">
                            <div class="upload-file-input">
                                <input
                                    type="file"
                                    accept=".pdf,.txt,.md,.html,.json"
                                    on:change=handle_file_change
                                    disabled=uploading.get()
                                    class="file-input"
                                />
                                <p class="upload-hint">
                                    "Supported formats: PDF, TXT, Markdown, HTML, JSON"
                                </p>
                                <p class="upload-hint">
                                    {format!("Maximum size: {} MB", MAX_FILE_SIZE / 1024 / 1024)}
                                </p>
                            </div>
                        </Card>

                        {move || selected_file.get().map(|name| {
                            view! {
                                <Card title="Selected File">
                                    <div class="selected-file">
                                        <span class="selected-file-icon">"📄"</span>
                                        <span class="selected-file-name">{name}</span>
                                    </div>
                                </Card>
                            }
                        })}

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
