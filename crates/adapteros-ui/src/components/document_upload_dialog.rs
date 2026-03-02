//! Reusable document upload dialog.

use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Dialog};
use crate::utils::{format_bytes, status_display_label};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::use_api_client;
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;

fn status_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "indexed" | "ready" => BadgeVariant::Success,
        "processing" | "uploaded" | "chunked" | "embedded" => BadgeVariant::Warning,
        "failed" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

/// Upload a document and return its `document_id` on success.
#[component]
pub fn DocumentUploadDialog(open: RwSignal<bool>, on_success: Callback<String>) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    let client = use_api_client();
    const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

    #[cfg(target_arch = "wasm32")]
    // Keep in sync with backend `detect_document_kind()` (.md and .markdown are both supported).
    const SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md", ".markdown"];

    let uploading = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);
    let selected_file_name = RwSignal::new(None::<String>);
    let selected_file_size = RwSignal::new(None::<u64>);
    let upload_status = RwSignal::new(None::<String>);
    let uploaded_status = RwSignal::new(None::<String>);

    #[cfg(not(target_arch = "wasm32"))]
    let _ = on_success;

    #[cfg(target_arch = "wasm32")]
    let file_ref: RwSignal<Option<SendWrapper<web_sys::File>>> = RwSignal::new(None);

    Effect::new(move || {
        if !open.try_get().unwrap_or(true) {
            let _ = uploading.try_set(false);
            let _ = error_msg.try_set(None);
            let _ = selected_file_name.try_set(None);
            let _ = selected_file_size.try_set(None);
            let _ = upload_status.try_set(None);
            let _ = uploaded_status.try_set(None);
            #[cfg(target_arch = "wasm32")]
            let _ = file_ref.try_set(None);
        }
    });

    #[cfg(target_arch = "wasm32")]
    let handle_file_change = {
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            if let Some(file) = files.get(0) {
                let size = file.size() as u64;
                let name = file.name();
                let name_lower = name.to_lowercase();

                if size > MAX_FILE_SIZE {
                    error_msg.set(Some(format!(
                        "File too large. Maximum size is {} MB.",
                        MAX_FILE_SIZE / 1024 / 1024
                    )));
                    selected_file_name.set(None);
                    selected_file_size.set(None);
                    file_ref.set(None);
                    input.set_value("");
                    return;
                }

                let ext_ok = SUPPORTED_EXTENSIONS
                    .iter()
                    .any(|ext| name_lower.ends_with(ext));
                if !ext_ok {
                    error_msg.set(Some(format!(
                        "Unsupported file type. Supported: {}",
                        SUPPORTED_EXTENSIONS.join(", ")
                    )));
                    selected_file_name.set(None);
                    selected_file_size.set(None);
                    file_ref.set(None);
                    input.set_value("");
                    return;
                }

                error_msg.set(None);
                selected_file_name.set(Some(name));
                selected_file_size.set(Some(size));
                file_ref.set(Some(SendWrapper::new(file)));
                input.set_value("");
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_change = |_ev: web_sys::Event| {};

    let handle_upload = Callback::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file_wrapper) = file_ref.get() else {
                error_msg.set(Some("Please select a file first.".into()));
                return;
            };
            uploading.set(true);
            error_msg.set(None);
            upload_status.set(Some("Uploading document...".into()));
            uploaded_status.set(None);

            let file = file_wrapper.take();
            let on_success = on_success;
            let open = open;
            let client = client.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match client.upload_document(&file).await {
                    Ok(response) => {
                        let _ = upload_status
                            .try_set(Some("Upload complete. Indexing started.".into()));
                        let _ = uploaded_status.try_set(Some(response.status.clone()));
                        let _ = uploading.try_set(false);
                        let _ = open.try_set(false);
                        on_success.run(response.document_id);
                    }
                    Err(e) => {
                        let _ = error_msg.try_set(Some(e.user_message()));
                        let _ = upload_status.try_set(None);
                        let _ = uploading.try_set(false);
                    }
                }
            });
        }
    });

    let upload_disabled = Signal::derive(move || {
        uploading.try_get().unwrap_or(false) || selected_file_name.try_get().flatten().is_none()
    });

    view! {
        <Dialog
            open=open
            title="Upload Document"
            description="Upload a document to index for RAG retrieval."
        >
            <div class="space-y-4 py-2">
                <div class="space-y-2">
                    <label for="documents-upload-file" class="text-sm font-medium">"File"</label>
                    <input
                        id="documents-upload-file"
                        type="file"
                        accept=".pdf,.txt,.md,.markdown"
                        class="block w-full text-sm"
                        disabled=move || uploading.try_get().unwrap_or(false)
                        on:change=handle_file_change
                    />
                    <p class="text-xs text-muted-foreground">
                        "Supported: PDF, TXT, Markdown · Max 100 MB"
                    </p>
                    {move || selected_file_name.try_get().flatten().map(|name| {
                        let size = selected_file_size.try_get().flatten().unwrap_or_default();
                        view! {
                            <div class="text-sm text-muted-foreground">
                                {name} " · " {format_bytes(size as i64)}
                            </div>
                        }
                    })}
                </div>

                {move || upload_status.try_get().flatten().map(|status| view! {
                    <div class="text-sm text-muted-foreground">{status}</div>
                })}

                {move || uploaded_status.try_get().flatten().map(|status| {
                    let status_raw = status.clone();
                    let status_label = status_display_label(&status);
                    view! {
                        <div class="flex items-center gap-2 text-sm">
                            <span class="text-muted-foreground">"Indexing Status"</span>
                            <span title=status_raw.clone()>
                                <Badge variant=status_badge_variant(&status_raw)>{status_label}</Badge>
                            </span>
                        </div>
                    }
                })}

                {move || error_msg.try_get().flatten().map(|err| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                        {err}
                    </div>
                })}
            </div>

            <div class="flex justify-end gap-2">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| open.set(false))
                    disabled=Signal::derive(move || uploading.try_get().unwrap_or(false))
                >
                    "Cancel"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    loading=Signal::derive(move || uploading.try_get().unwrap_or(false))
                    disabled=upload_disabled
                    on_click=handle_upload
                >
                    "Upload"
                </Button>
            </div>
        </Dialog>
    }
}
