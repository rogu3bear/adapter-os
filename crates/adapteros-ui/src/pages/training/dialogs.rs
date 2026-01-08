//! Training page dialog components
//!
//! Modal dialogs for creating training jobs.

use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, FormField, Input};
use crate::validation::{rules, use_form_errors, validate_field, ValidationRule};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;

/// Create job dialog
#[component]
pub fn CreateJobDialog(
    open: RwSignal<bool>,
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    // Form state
    let adapter_name = RwSignal::new(String::new());
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());
    let dataset_id = RwSignal::new(String::new());
    let category = RwSignal::new("code".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Form validation state
    let form_errors = use_form_errors();

    // File upload state
    let uploading = RwSignal::new(false);
    let upload_status = RwSignal::new(String::new());

    let on_created_clone = on_created.clone();

    // Handle file upload - uploads document then converts to dataset
    // This handler is WASM-only since it uses web_sys APIs
    #[cfg(target_arch = "wasm32")]
    let handle_file_upload = {
        let dataset_id = dataset_id.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;

            let target = ev.target().unwrap();
            let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();

            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_name = file.name();
                    uploading.set(true);
                    upload_status.set(format!("Uploading {}...", file_name));
                    error.set(None);

                    wasm_bindgen_futures::spawn_local(async move {
                        let client = ApiClient::new();

                        // Step 1: Upload document
                        match client.upload_document(&file).await {
                            Ok(doc) => {
                                upload_status.set("Processing document...".to_string());
                                let doc_id = doc.document_id.clone();

                                // Step 2: Poll until indexed (max 60 attempts = 60 seconds)
                                for _ in 0..60 {
                                    gloo_timers::future::TimeoutFuture::new(1000).await;
                                    match client.get_document(&doc_id).await {
                                        Ok(status) => {
                                            match status.status.as_str() {
                                                "indexed" => {
                                                    // Step 3: Create dataset from document
                                                    upload_status
                                                        .set("Creating dataset...".to_string());
                                                    match client
                                                        .create_dataset_from_documents(
                                                            vec![doc_id.clone()],
                                                            Some(file_name.clone()),
                                                        )
                                                        .await
                                                    {
                                                        Ok(ds) => {
                                                            dataset_id.set(ds.id);
                                                            upload_status
                                                                .set("Dataset ready!".to_string());
                                                            uploading.set(false);
                                                            return;
                                                        }
                                                        Err(e) => {
                                                            error.set(Some(format!(
                                                                "Failed to create dataset: {}",
                                                                e
                                                            )));
                                                            uploading.set(false);
                                                            upload_status.set(String::new());
                                                            return;
                                                        }
                                                    }
                                                }
                                                "failed" => {
                                                    error.set(Some(format!(
                                                        "Document processing failed: {}",
                                                        status.error_message.unwrap_or_default()
                                                    )));
                                                    uploading.set(false);
                                                    upload_status.set(String::new());
                                                    return;
                                                }
                                                _ => {
                                                    // Still processing, continue polling
                                                    upload_status.set(format!(
                                                        "Processing document ({})...",
                                                        status.status
                                                    ));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error.set(Some(format!(
                                                "Failed to check status: {}",
                                                e
                                            )));
                                            uploading.set(false);
                                            upload_status.set(String::new());
                                            return;
                                        }
                                    }
                                }
                                // Timeout
                                error.set(Some("Document processing timed out".to_string()));
                                uploading.set(false);
                                upload_status.set(String::new());
                            }
                            Err(e) => {
                                error.set(Some(format!("Upload failed: {}", e)));
                                uploading.set(false);
                                upload_status.set(String::new());
                            }
                        }
                    });
                }
            }
        }
    };

    // No-op handler for non-wasm (native) compilation
    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_upload = move |_ev: web_sys::Event| {
        // File upload not supported outside WASM
        let _ = (uploading, upload_status, error, dataset_id);
    };

    let submit = move |_: ()| {
        // Clear previous errors
        form_errors.update(|e| e.clear_all());
        error.set(None);

        // Validate all fields
        let name = adapter_name.get();
        let epochs_str = epochs.get();
        let lr_str = learning_rate.get();
        let batch_str = batch_size.get();
        let rank_str = rank.get();
        let alpha_str = alpha.get();

        let mut has_errors = false;

        // Validate adapter name
        if let Some(err) = validate_field(&name, &rules::adapter_name()) {
            form_errors.update(|e| e.set("adapter_name", err));
            has_errors = true;
        }

        // Validate epochs (1-1000)
        if let Some(err) = validate_field(
            &epochs_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 1000 },
            ],
        ) {
            form_errors.update(|e| e.set("epochs", err));
            has_errors = true;
        }

        // Validate learning rate (0 < lr <= 1)
        if let Some(err) = validate_field(&lr_str, &rules::learning_rate()) {
            form_errors.update(|e| e.set("learning_rate", err));
            has_errors = true;
        }

        // Validate batch size (1-256)
        if let Some(err) = validate_field(
            &batch_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ],
        ) {
            form_errors.update(|e| e.set("batch_size", err));
            has_errors = true;
        }

        // Validate rank (1-256, typically 4, 8, 16, 32, 64)
        if let Some(err) = validate_field(
            &rank_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ],
        ) {
            form_errors.update(|e| e.set("rank", err));
            has_errors = true;
        }

        // Validate alpha (1-512)
        if let Some(err) = validate_field(
            &alpha_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 512 },
            ],
        ) {
            form_errors.update(|e| e.set("alpha", err));
            has_errors = true;
        }

        if has_errors {
            return;
        }

        submitting.set(true);

        let epochs_val: u32 = epochs_str.parse().unwrap_or(10);
        let lr_val: f32 = lr_str.parse().unwrap_or(0.0001);
        let batch_val: u32 = batch_str.parse().unwrap_or(4);
        let rank_val: u32 = rank_str.parse().unwrap_or(8);
        let alpha_val: u32 = alpha_str.parse().unwrap_or(16);
        let ds_id = dataset_id.get();
        let cat = category.get();

        let on_created = on_created_clone.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();

            // Build the request body
            let request = serde_json::json!({
                "adapter_name": name,
                "config": {
                    "rank": rank_val,
                    "alpha": alpha_val,
                    "targets": ["q_proj", "v_proj"],
                    "epochs": epochs_val,
                    "learning_rate": lr_val,
                    "batch_size": batch_val,
                },
                "category": cat,
                "dataset_id": if ds_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(ds_id.clone()) },
                "synthetic_mode": ds_id.is_empty(),
            });

            match client
                .post::<_, TrainingJobResponse>("/v1/training/jobs", &request)
                .await
            {
                Ok(_) => {
                    submitting.set(false);
                    // Reset form
                    adapter_name.set(String::new());
                    epochs.set("10".to_string());
                    learning_rate.set("0.0001".to_string());
                    batch_size.set("4".to_string());
                    rank.set("8".to_string());
                    alpha.set("16".to_string());
                    dataset_id.set(String::new());
                    form_errors.update(|e| e.clear_all());
                    on_created();
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    submitting.set(false);
                }
            }
        });
    };

    let close = move |_: ()| {
        open.set(false);
        error.set(None);
        form_errors.update(|e| e.clear_all());
    };

    view! {
        {move || {
            if !open.get() {
                return view! {}.into_any();
            }

            view! {
                // Backdrop
                <div
                    class="fixed inset-0 z-50 bg-black/80"
                    on:click=move |_| close(())
                />

                // Dialog
                <div class="dialog-content">
                    // Header
                    <div class="flex items-center justify-between mb-4">
                        <div>
                            <h2 class="text-lg font-semibold">"New Training Job"</h2>
                            <p class="text-sm text-muted-foreground">"Configure and start a new adapter training job"</p>
                        </div>
                        <button
                            class="rounded-sm opacity-70 hover:opacity-100"
                            on:click=move |_| close(())
                        >
                            <svg
                                xmlns="http://www.w3.org/2000/svg"
                                width="24"
                                height="24"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <path d="M18 6 6 18"/>
                                <path d="m6 6 12 12"/>
                            </svg>
                        </button>
                    </div>

                    // Error message
                    {move || error.get().map(|e| view! {
                        <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                            <p class="text-sm text-destructive">{e}</p>
                        </div>
                    })}

                    // Form
                    <div class="space-y-4">
                        <FormField
                            label="Adapter Name"
                            name="adapter_name"
                            required=true
                            help="Name for the trained adapter (letters, numbers, hyphens)"
                            error=Signal::derive(move || form_errors.get().get("adapter_name").cloned())
                        >
                            <Input
                                value=adapter_name
                                placeholder="my-code-adapter".to_string()
                            />
                        </FormField>

                        <div class="space-y-2">
                            <label class="text-sm font-medium">"Category"</label>
                            <select
                                class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                on:change=move |ev| category.set(event_target_value(&ev))
                            >
                                <option value="code" selected=true>"Code"</option>
                                <option value="framework">"Framework"</option>
                                <option value="codebase">"Codebase"</option>
                                <option value="docs">"Documentation"</option>
                                <option value="domain">"Domain"</option>
                            </select>
                        </div>

                        // File upload section
                        <div class="space-y-2">
                            <label class="text-sm font-medium">"Training Data"</label>
                            <div class="space-y-3">
                                // File upload input
                                <div>
                                    <input
                                        type="file"
                                        accept=".md,.txt,.pdf"
                                        class="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                                        disabled=move || uploading.get() || submitting.get()
                                        on:change=handle_file_upload.clone()
                                    />
                                    <p class="text-xs text-muted-foreground mt-1">
                                        "Upload a document (.md, .txt, .pdf) to create a training dataset"
                                    </p>
                                </div>

                                // Upload status
                                {move || {
                                    let status = upload_status.get();
                                    if status.is_empty() {
                                        None
                                    } else {
                                        let is_ready = status.contains("ready");
                                        let class = if is_ready {
                                            "text-sm text-green-600 flex items-center gap-2"
                                        } else {
                                            "text-sm text-muted-foreground flex items-center gap-2"
                                        };
                                        Some(view! {
                                            <div class=class>
                                                {if !is_ready {
                                                    view! {
                                                        <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg class="h-4 w-4 text-green-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                                                        </svg>
                                                    }.into_any()
                                                }}
                                                <span>{status}</span>
                                            </div>
                                        })
                                    }
                                }}

                                // Or divider
                                <div class="relative">
                                    <div class="absolute inset-0 flex items-center">
                                        <span class="w-full border-t" />
                                    </div>
                                    <div class="relative flex justify-center text-xs uppercase">
                                        <span class="bg-background px-2 text-muted-foreground">"or use existing dataset"</span>
                                    </div>
                                </div>

                                // Dataset ID input
                                <Input
                                    value=dataset_id
                                    label="Dataset ID".to_string()
                                    placeholder="ds-abc123".to_string()
                                />
                            </div>
                        </div>

                        <div class="border-t pt-4 mt-4">
                            <h3 class="text-sm font-medium mb-3">"Training Parameters"</h3>
                            <div class="grid gap-4 grid-cols-2">
                                <FormField
                                    label="Epochs"
                                    name="epochs"
                                    required=true
                                    help="Number of training iterations (1-1000)"
                                    error=Signal::derive(move || form_errors.get().get("epochs").cloned())
                                >
                                    <Input
                                        value=epochs
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Learning Rate"
                                    name="learning_rate"
                                    required=true
                                    help="Step size for optimization (0.0001-0.01 recommended)"
                                    error=Signal::derive(move || form_errors.get().get("learning_rate").cloned())
                                >
                                    <Input
                                        value=learning_rate
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Batch Size"
                                    name="batch_size"
                                    required=true
                                    help="Examples per training step (1-256)"
                                    error=Signal::derive(move || form_errors.get().get("batch_size").cloned())
                                >
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="LoRA Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter rank dimension (4, 8, 16 typical)"
                                    error=Signal::derive(move || form_errors.get().get("rank").cloned())
                                >
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                    />
                                </FormField>
                            </div>
                        </div>

                        <div class="border-t pt-4 mt-4">
                            <h3 class="text-sm font-medium mb-3">"LoRA Configuration"</h3>
                            <div class="grid gap-4 grid-cols-2">
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=Signal::derive(move || form_errors.get().get("alpha").cloned())
                                >
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <div class="space-y-2">
                                    <label class="text-sm font-medium">"Target Layers"</label>
                                    <div class="text-sm text-muted-foreground p-2 bg-muted rounded-md">
                                        "q_proj, v_proj (default)"
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Footer
                    <div class="flex justify-end gap-2 mt-6">
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(close.clone())
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            loading=submitting.get()
                            on_click=Callback::new(submit.clone())
                        >
                            "Start Training"
                        </Button>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
