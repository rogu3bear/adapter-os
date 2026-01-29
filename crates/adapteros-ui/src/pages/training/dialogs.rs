//! Training page dialog components
//!
//! Modal dialogs for creating training jobs.
//! Uses canonical Dialog component for ARIA compliance and keyboard handling.

use crate::api::ApiClient;
#[cfg(target_arch = "wasm32")]
use crate::api::ApiError;
use crate::components::{Button, ButtonVariant, Dialog, DialogSize, FormField, Input};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use crate::pages::training::generate_wizard::{GenerateDatasetOutcome, GenerateDatasetWizard};
use crate::validation::{rules, use_form_errors, validate_field, ValidationRule};
use adapteros_api_types::{TrainingBackendKind, TrainingBackendPolicy, TrainingJobResponse};
use leptos::prelude::*;
use serde_json::json;

/// Create job dialog
#[allow(dead_code)] // Leptos #[component] macro limitation with unused props warnings
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
    let dataset_upload_message = RwSignal::new(None::<String>);
    let dataset_wizard_open = RwSignal::new(false);
    let generate_wizard_open = RwSignal::new(false);
    let base_model_id = RwSignal::new(String::new());
    let preprocess_enabled = RwSignal::new(true);
    let coreml_model_id = RwSignal::new(String::new());
    let coreml_model_path = RwSignal::new(String::new());
    let preprocess_output = RwSignal::new("hidden_state_last".to_string());
    let preprocess_batch_size = RwSignal::new("0".to_string());
    let preprocess_max_seq_len = RwSignal::new("0".to_string());
    let preprocess_compression = RwSignal::new("none".to_string());
    let preprocess_status = RwSignal::new(None::<adapteros_api_types::PreprocessStatusResponse>);
    let status_error = RwSignal::new(None::<String>);
    let checking_status = RwSignal::new(false);

    // Backend selection state
    let preferred_backend = RwSignal::new("auto".to_string());
    let backend_policy = RwSignal::new("auto".to_string());
    let coreml_training_fallback = RwSignal::new("mlx".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Form validation state
    let form_errors = use_form_errors();

    // File upload state
    let uploading = RwSignal::new(false);
    let upload_status = RwSignal::new(String::new());
    #[cfg(target_arch = "wasm32")]
    let format_upload_error = |err: &ApiError| -> String {
        if let ApiError::Structured {
            error,
            code,
            details,
            ..
        } = err
        {
            if let Some(details) = details {
                if let Some(errors) = details.get("errors").and_then(|v| v.as_array()) {
                    let rendered: Vec<String> = errors
                        .iter()
                        .filter_map(|entry| {
                            let msg = entry
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or_default();
                            if msg.is_empty() {
                                return None;
                            }
                            let mut parts = vec![msg.to_string()];
                            if let Some(field) = entry.get("field_name").and_then(|f| f.as_str()) {
                                parts.push(format!("field {}", field));
                            }
                            if let Some(file) = entry.get("file_path").and_then(|f| f.as_str()) {
                                parts.push(file.to_string());
                            }
                            if let Some(line) = entry.get("line_number").and_then(|l| l.as_i64()) {
                                parts.push(format!("line {}", line));
                            }
                            Some(parts.join(" · "))
                        })
                        .collect();
                    if !rendered.is_empty() {
                        return format!("{}: {}", error, rendered.join("; "));
                    }
                }

                if let Some(path) = details.get("path").and_then(|p| p.as_str()) {
                    return format!("{}: {}", error, path);
                }
            }
            return format!("{} ({})", error, code);
        }
        err.to_string()
    };
    let on_dataset_uploaded = {
        let dataset_id = dataset_id.clone();
        let upload_status = upload_status.clone();
        let dataset_upload_message = dataset_upload_message.clone();
        move |outcome: DatasetUploadOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            upload_status.set(String::new());
            dataset_upload_message.set(Some(format!(
                "Dataset {} ready ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    let on_dataset_generated = {
        let dataset_id = dataset_id.clone();
        let dataset_upload_message = dataset_upload_message.clone();
        move |outcome: GenerateDatasetOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            dataset_upload_message.set(Some(format!(
                "Generated dataset {} ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    let on_created_clone = on_created.clone();

    // Handle file upload - uploads document then converts to dataset
    // This handler is WASM-only since it uses web_sys APIs
    #[cfg(target_arch = "wasm32")]
    let handle_file_upload = {
        let dataset_id = dataset_id.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;

            let Some(target) = ev.target() else {
                tracing::error!("handle_file_upload: no event target");
                return;
            };
            let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
                tracing::error!("handle_file_upload: target is not an HtmlInputElement");
                return;
            };

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
                                                            let msg = format_upload_error(&e);
                                                            error.set(Some(format!(
                                                                "Failed to create dataset: {}",
                                                                msg
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
                                let msg = format_upload_error(&e);
                                error.set(Some(format!("Upload failed: {}", msg)));
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

    let check_status = {
        let dataset_id = dataset_id.clone();
        let base_model_id = base_model_id.clone();
        let preprocess_enabled = preprocess_enabled.clone();
        let coreml_model_id = coreml_model_id.clone();
        let coreml_model_path = coreml_model_path.clone();
        let preprocess_output = preprocess_output.clone();
        let preprocess_batch_size = preprocess_batch_size.clone();
        let preprocess_max_seq_len = preprocess_max_seq_len.clone();
        let preprocess_compression = preprocess_compression.clone();
        let preprocess_status = preprocess_status.clone();
        let status_error = status_error.clone();
        let checking_status = checking_status.clone();
        move |_: ()| {
            status_error.set(None);
            preprocess_status.set(None);
            let ds_id = dataset_id.get();
            let base_model = base_model_id.get();
            if ds_id.trim().is_empty() || base_model.trim().is_empty() {
                status_error.set(Some(
                    "Dataset ID and base model ID are required to check preprocessing".to_string(),
                ));
                return;
            }

            checking_status.set(true);
            let request = json!({
                "dataset_id": ds_id,
                "base_model_id": base_model,
                "preprocessing": {
                    "enabled": preprocess_enabled.get(),
                    "coreml_model_id": if coreml_model_id.get().trim().is_empty() { serde_json::Value::Null } else { json!(coreml_model_id.get()) },
                    "coreml_model_path": if coreml_model_path.get().trim().is_empty() { serde_json::Value::Null } else { json!(coreml_model_path.get()) },
                    "output_feature": preprocess_output.get(),
                    "max_seq_len": preprocess_max_seq_len.get().parse::<u32>().unwrap_or(0),
                    "batch_size": preprocess_batch_size.get().parse::<u32>().unwrap_or(0),
                    "compression": if preprocess_compression.get() == "none" { serde_json::Value::Null } else { json!(preprocess_compression.get()) },
                }
            });

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::new();
                let result = client
                    .post::<_, adapteros_api_types::PreprocessStatusResponse>(
                        "/v1/training/preprocessing/status",
                        &request,
                    )
                    .await;
                match result {
                    Ok(resp) => {
                        preprocess_status.set(Some(resp));
                    }
                    Err(e) => {
                        status_error.set(Some(e.to_string()));
                    }
                }
                checking_status.set(false);
            });
        }
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
        let base_model = base_model_id.get();
        let coreml_id_val = coreml_model_id.get();
        let coreml_path_val = coreml_model_path.get();
        let preprocess_on = preprocess_enabled.get();

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

        if base_model.trim().is_empty() {
            form_errors.update(|e| {
                e.set(
                    "base_model_id",
                    "Base model ID is required for preprocessing".to_string(),
                )
            });
            has_errors = true;
        }

        if preprocess_on && coreml_id_val.trim().is_empty() && coreml_path_val.trim().is_empty() {
            form_errors.update(|e| {
                e.set(
                    "coreml_model",
                    "Provide a CoreML model ID or path for preprocessing".to_string(),
                )
            });
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
        let base_model_val = base_model_id.get();
        let output_feature = preprocess_output.get();
        let compression_val = preprocess_compression.get();
        let batch_pre = preprocess_batch_size.get();
        let seq_pre = preprocess_max_seq_len.get();
        let preprocess_payload = json!({
            "enabled": preprocess_on,
            "coreml_model_id": if coreml_id_val.trim().is_empty() { serde_json::Value::Null } else { json!(coreml_id_val) },
            "coreml_model_path": if coreml_path_val.trim().is_empty() { serde_json::Value::Null } else { json!(coreml_path_val) },
            "output_feature": output_feature,
            "max_seq_len": seq_pre.parse::<u32>().unwrap_or(0),
            "batch_size": batch_pre.parse::<u32>().unwrap_or(0),
            "compression": if compression_val == "none" { serde_json::Value::Null } else { json!(compression_val) },
        });

        // Backend selection values
        let backend_val = preferred_backend.get();
        let policy_val = backend_policy.get();
        let fallback_val = coreml_training_fallback.get();

        let on_created = on_created_clone.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();

            // Build the request body
            let request = serde_json::json!({
                "adapter_name": name,
                "base_model_id": base_model_val,
                "config": {
                    "rank": rank_val,
                    "alpha": alpha_val,
                    "targets": ["q_proj", "v_proj"],
                    "epochs": epochs_val,
                    "learning_rate": lr_val,
                    "batch_size": batch_val,
                    "preprocessing": preprocess_payload,
                    "preferred_backend": if backend_val == TrainingBackendKind::Auto.as_str() { serde_json::Value::Null } else { json!(backend_val) },
                    "backend_policy": if policy_val == TrainingBackendPolicy::Auto.as_str() { serde_json::Value::Null } else { json!(policy_val) },
                    "coreml_training_fallback": if backend_val == TrainingBackendKind::CoreML.as_str() || policy_val == TrainingBackendPolicy::CoremlElseFallback.as_str() { json!(fallback_val) } else { serde_json::Value::Null },
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
                    base_model_id.set(String::new());
                    coreml_model_id.set(String::new());
                    coreml_model_path.set(String::new());
                    preprocess_output.set("hidden_state_last".to_string());
                    preprocess_batch_size.set("0".to_string());
                    preprocess_max_seq_len.set("0".to_string());
                    preprocess_compression.set("none".to_string());
                    preprocess_enabled.set(true);
                    preprocess_status.set(None);
                    status_error.set(None);
                    // Reset backend selection
                    preferred_backend.set("auto".to_string());
                    backend_policy.set("auto".to_string());
                    coreml_training_fallback.set("mlx".to_string());
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
        dataset_wizard_open.set(false);
        form_errors.update(|e| e.clear_all());
    };

    view! {
        <Dialog
            open=open
            title="New Training Job"
            description="Configure and start a new adapter training job"
            size=DialogSize::Xl
            scrollable=true
        >
            // Error message
                    {move || error.get().map(|e| view! {
                        <div class="mb-4 rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                            {e}
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
                            <div class="flex items-center justify-between">
                                <div class="text-sm font-medium">"Training Data"</div>
                                <div class="flex gap-2">
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        on_click=Callback::new(move |_| generate_wizard_open.set(true))
                                    >
                                        "Generate from File"
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        on_click=Callback::new(move |_| dataset_wizard_open.set(true))
                                    >
                                        "Guided upload"
                                    </Button>
                                </div>
                            </div>
                            <p class="text-xs text-muted-foreground">
                                "Generate: use local inference to create Q&A or summary pairs from text files. "
                                "Guided: pick manifest + JSONL or structured CSV/Text with inline validation."
                            </p>
                            {move || dataset_upload_message.get().map(|msg| view! {
                                <div class="rounded-md border border-status-success/50 bg-status-success/10 p-2 text-xs text-foreground">
                                    {msg}
                                </div>
                            })}
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
                                            "text-sm text-status-success flex items-center gap-2"
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
                                                        <svg class="h-4 w-4 text-status-success" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
                                <FormField
                                    label="Base Model ID"
                                    name="base_model_id"
                                    required=true
                                    help="Foundation model identifier used for tokenizer/CoreML preprocessing"
                                    error=Signal::derive(move || form_errors.get().get("base_model_id").cloned())
                                >
                                    <Input
                                        value=base_model_id
                                        placeholder="qwen2.5-coder-base".to_string()
                                    />
                                </FormField>
                            </div>
                        </div>

                        <div class="border-t pt-4 mt-4">
                            <h3 class="text-sm font-medium mb-3">"Backend Selection"</h3>
                            <p class="text-xs text-muted-foreground mb-3">
                                "Choose the compute backend for training. Auto selects the best available."
                            </p>
                            <div class="grid gap-4 grid-cols-2">
                                <div class="space-y-2">
                                    <label class="text-sm font-medium">"Preferred Backend"</label>
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || preferred_backend.get())
                                        on:change=move |ev| preferred_backend.set(event_target_value(&ev))
                                    >
                                        <option value="auto">"Auto (recommended)"</option>
                                        <option value="mlx">"MLX"</option>
                                        <option value="coreml">"CoreML"</option>
                                        <option value="metal">"Metal"</option>
                                        <option value="cpu">"CPU"</option>
                                    </select>
                                    <p class="text-xs text-muted-foreground">
                                        "MLX: flexible, deterministic. CoreML: ANE acceleration. Metal: GPU fallback."
                                    </p>
                                </div>
                                <div class="space-y-2">
                                    <label class="text-sm font-medium">"Backend Policy"</label>
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || backend_policy.get())
                                        on:change=move |ev| backend_policy.set(event_target_value(&ev))
                                    >
                                        <option value="auto">"Auto"</option>
                                        <option value="coreml_only">"CoreML Only (fail if unavailable)"</option>
                                        <option value="coreml_else_fallback">"CoreML with Fallback"</option>
                                    </select>
                                    <p class="text-xs text-muted-foreground">
                                        "Controls how backend unavailability is handled."
                                    </p>
                                </div>
                            </div>
                            {move || (preferred_backend.get() == TrainingBackendKind::CoreML.as_str() || backend_policy.get() == TrainingBackendPolicy::CoremlElseFallback.as_str()).then(|| view! {
                                <div class="mt-3 space-y-2">
                                    <label class="text-sm font-medium">"CoreML Fallback Backend"</label>
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || coreml_training_fallback.get())
                                        on:change=move |ev| coreml_training_fallback.set(event_target_value(&ev))
                                    >
                                        <option value="mlx">"MLX (default)"</option>
                                        <option value="metal">"Metal"</option>
                                    </select>
                                    <p class="text-xs text-muted-foreground">
                                        "Backend to use when CoreML is unavailable. MLX is recommended for determinism."
                                    </p>
                                </div>
                            })}
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

                        <div class="border-t pt-4 mt-4 space-y-3">
                            <div class="flex items-center justify-between gap-3">
                                <h3 class="text-sm font-medium">"CoreML Preprocessing"</h3>
                                {move || (!preprocess_enabled.get()).then(|| view! {
                                    <span class="text-xs text-status-warning">
                                        "Disabling preprocessing will cause CoreML runs to return an error"
                                    </span>
                                })}
                            </div>
                            <div class="grid gap-4 grid-cols-2">
                                <label class="flex items-center gap-2 text-sm">
                                    <input
                                        type="checkbox"
                                        class="h-4 w-4 rounded border"
                                        prop:checked=Signal::derive(move || preprocess_enabled.get())
                                        on:change=move |ev| preprocess_enabled.set(event_target_checked(&ev))
                                    />
                                    <span>"Enable CoreML preprocessing"</span>
                                </label>
                                <FormField
                                    label="CoreML Model ID"
                                    name="coreml_model_id"
                                    help="Lookup key for cached CoreML preprocess package"
                                    error=Signal::derive(move || form_errors.get().get("coreml_model").cloned())
                                >
                                    <Input
                                        value=coreml_model_id
                                        placeholder="coreml-preprocess-id".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="CoreML Model Path"
                                    name="coreml_model_path"
                                    help="Local path to the CoreML preprocess model (mlpackage or mlmodelc)"
                                    error=Signal::derive(move || form_errors.get().get("coreml_model").cloned())
                                >
                                    <Input
                                        value=coreml_model_path
                                        placeholder="/path/to/model.mlpackage".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Output Feature"
                                    name="output_feature"
                                    help="Hidden state or embedding output to cache"
                                    error=Signal::derive(move || None::<String>)
                                >
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || preprocess_output.get())
                                        on:change=move |ev| preprocess_output.set(event_target_value(&ev))
                                    >
                                        <option value="hidden_state_last" selected=true>"hidden_states (last)"</option>
                                        <option value="embedding">"embeddings"</option>
                                        <option value="pooled">"pooled (mean)"</option>
                                    </select>
                                </FormField>
                                <FormField
                                    label="Preprocess Batch Size"
                                    name="preprocess_batch_size"
                                    help="Batch size used during preprocessing (0 = auto)"
                                    error=Signal::derive(move || None::<String>)
                                >
                                    <Input
                                        value=preprocess_batch_size
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Max Seq Length"
                                    name="preprocess_max_seq_len"
                                    help="Trim or pad sequences to this length for preprocessing (0 = input length)"
                                    error=Signal::derive(move || None::<String>)
                                >
                                    <Input
                                        value=preprocess_max_seq_len
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Compression"
                                    name="compression"
                                    help="Optional compression for cached features"
                                    error=Signal::derive(move || None::<String>)
                                >
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || preprocess_compression.get())
                                        on:change=move |ev| preprocess_compression.set(event_target_value(&ev))
                                    >
                                        <option value="none" selected=true>"None"</option>
                                        <option value="q15">"Q15 (int16 + scale)"</option>
                                    </select>
                                </FormField>
                            </div>
                            <div class="flex items-center gap-3">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    loading=checking_status.get()
                                    on_click=Callback::new(check_status.clone())
                                >
                                    "Check preprocessing cache"
                                </Button>
                                <p class="text-xs text-muted-foreground">
                                    "Surface cache manifest, dtype, and when a reprocess is needed before training."
                                </p>
                            </div>
                            {move || status_error.get().map(|msg| view! {
                                <div class="rounded border border-status-error/50 bg-status-error/10 p-2 text-sm text-status-error">{msg}</div>
                            })}
                            {move || preprocess_status.get().map(|status| view! {
                                <div class="rounded-lg border bg-muted/40 p-3 space-y-2">
                                    <div class="flex items-center justify-between">
                                        <span class="text-sm font-medium">
                                            {if status.needs_reprocess {
                                                "Reprocess required".to_string()
                                            } else if status.cache_hit {
                                                "Cache hit (ready)".to_string()
                                            } else {
                                                "Cache miss".to_string()
                                            }}
                                        </span>
                                        <span class="text-xs text-muted-foreground font-mono">{status.cache_dir.clone()}</span>
                                    </div>
                                    <div class="grid gap-2 text-xs md:grid-cols-2">
                                        <div>
                                            <p class="text-muted-foreground">"Cache key"</p>
                                            <p class="font-mono break-all">{status.cache_key_b3.clone()}</p>
                                        </div>
                                        <div>
                                            <p class="text-muted-foreground">"Manifest hash"</p>
                                            <p class="font-mono break-all">{status.manifest_hash_b3.clone()}</p>
                                        </div>
                                        <div>
                                            <p class="text-muted-foreground">"Produced at"</p>
                                            <p class="font-mono">
                                                {status.produced_at_unix_ms.map(|v| format!("{} ms", v)).unwrap_or_else(|| "unknown".to_string())}
                                            </p>
                                        </div>
                                        <div>
                                            <p class="text-muted-foreground">"Feature dtype"</p>
                                            <p class="font-mono">{status.feature_dtype.clone()}</p>
                                        </div>
                                    </div>
                                    {(!status.reasons.is_empty()).then(|| view! {
                                        <div class="text-xs">
                                            <p class="font-medium text-status-warning">"Reprocess required"</p>
                                            <ul class="list-disc pl-4 text-status-warning">
                                                {status.reasons.iter().map(|reason| {
                                                    view! { <li class="break-words">{reason.clone()}</li> }
                                                }).collect::<Vec<_>>()}
                                            </ul>
                                        </div>
                                    })}
                                </div>
                            })}
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

                    <DatasetUploadWizard
                        open=dataset_wizard_open
                        on_complete=Callback::new(on_dataset_uploaded.clone())
                    />

                    <GenerateDatasetWizard
                        open=generate_wizard_open
                        on_generated=Callback::new(on_dataset_generated.clone())
                    />

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
        </Dialog>
    }
}
