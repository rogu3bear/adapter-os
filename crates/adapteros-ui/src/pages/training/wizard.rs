//! Stepped wizard for creating training jobs
//!
//! 4-step wizard flow:
//! 1. Knowledge - Add or choose files
//! 2. Name - Name your adapter
//! 3. Train - Default settings with optional advanced controls
//! 4. Confirm - Review and submit

use crate::api::error::format_structured_details;
use crate::api::{
    use_api_client, ApiClient, ApiError, DatasetResponse, DocumentListResponse, ModelListResponse,
};
use crate::components::{
    AsyncBoundary, Card, DialogSize, FormField, Input, Select, StepFormDialog,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use crate::pages::training::config_presets::TrainingPreset;
use crate::pages::training::dataset_wizard::{DatasetOutcome, DatasetUploadWizard};
use crate::pages::training::generate_wizard::GenerateDatasetWizard;
use crate::signals::use_notifications;
use crate::validation::{
    rules, use_field_error, use_form_state, validate_on_blur, FormState, ValidationRule,
};
use adapteros_api_types::TRAINING_DATA_CONTRACT_VERSION;
use leptos::prelude::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[path = "data/upload_dialog.rs"]
mod document_upload_dialog;
use document_upload_dialog::{DocumentUploadDialog, UploadBatchResult};

/// Wizard step enum
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WizardStep {
    /// Step 0: Knowledge source selection
    #[default]
    Knowledge,
    Name,
    Train,
    Confirm,
}

impl WizardStep {
    fn index(&self) -> usize {
        match self {
            WizardStep::Knowledge => 0,
            WizardStep::Name => 1,
            WizardStep::Train => 2,
            WizardStep::Confirm => 3,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            WizardStep::Knowledge => "Knowledge",
            WizardStep::Name => "Name",
            WizardStep::Train => "Train",
            WizardStep::Confirm => "Confirm",
        }
    }

    fn next(&self) -> Option<WizardStep> {
        match self {
            WizardStep::Knowledge => Some(WizardStep::Name),
            WizardStep::Name => Some(WizardStep::Train),
            WizardStep::Train => Some(WizardStep::Confirm),
            WizardStep::Confirm => None,
        }
    }

    fn prev(&self) -> Option<WizardStep> {
        match self {
            WizardStep::Knowledge => None,
            WizardStep::Name => Some(WizardStep::Knowledge),
            WizardStep::Train => Some(WizardStep::Name),
            WizardStep::Confirm => Some(WizardStep::Train),
        }
    }
}

const STEPS: [WizardStep; 4] = [
    WizardStep::Knowledge,
    WizardStep::Name,
    WizardStep::Train,
    WizardStep::Confirm,
];

#[cfg(target_arch = "wasm32")]
async fn wait_for_document_indexed(client: &ApiClient, document_id: &str) -> Result<(), String> {
    const MAX_POLLS: usize = 80;
    const POLL_DELAY_MS: u32 = 1500;

    for _ in 0..MAX_POLLS {
        match client.get_document(document_id).await {
            Ok(document) => match document.status.as_str() {
                "indexed" => return Ok(()),
                "failed" | "error" => {
                    return Err(document.error_message.unwrap_or_else(|| {
                        "One of your files could not be prepared. Please try another file."
                            .to_string()
                    }))
                }
                _ => {}
            },
            Err(e) => return Err(e.user_message()),
        }
        gloo_timers::future::TimeoutFuture::new(POLL_DELAY_MS).await;
    }

    Err("Your files are still preparing. Please try again in a moment.".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn wait_for_document_indexed(_client: &ApiClient, _document_id: &str) -> Result<(), String> {
    Ok(())
}

fn map_training_submit_error(error: &ApiError) -> String {
    match error.code() {
        Some("DATASET_TRUST_BLOCKED")
        | Some("DATASET_TRUST_NEEDS_APPROVAL")
        | Some("VALIDATION_ERROR") => "Your files need a quick review before training.".to_string(),
        Some("TRAINING_CAPACITY_LIMIT")
        | Some("MEMORY_PRESSURE_CRITICAL")
        | Some("CAPACITY_CHECK_ERROR")
        | Some("BACKPRESSURE")
        | Some("MEMORY_PRESSURE")
        | Some("OUT_OF_MEMORY")
        | Some("SERVICE_UNAVAILABLE") => {
            "Training is busy. Try again in a few minutes.".to_string()
        }
        _ => format_structured_details(error),
    }
}

/// Training job creation wizard
#[component]
pub fn CreateJobWizard(
    open: RwSignal<bool>,
    on_created: impl Fn(String) + Clone + Send + Sync + 'static,
    /// Optional initial dataset ID (e.g., from dataset detail page navigation)
    #[prop(optional)]
    initial_dataset_id: Option<RwSignal<Option<String>>>,
    /// Optional source document ID (e.g., from document-to-training workflow)
    #[prop(optional)]
    source_document_id: Option<RwSignal<Option<String>>>,
    /// Optional initial base model ID (from deep-link query params)
    #[prop(optional)]
    initial_base_model_id: Option<RwSignal<Option<String>>>,
    /// Optional initial preferred backend (from deep-link query params)
    #[prop(optional)]
    initial_preferred_backend: Option<RwSignal<Option<String>>>,
    /// Optional initial backend policy (from deep-link query params)
    #[prop(optional)]
    initial_backend_policy: Option<RwSignal<Option<String>>>,
    /// Optional initial epochs (from deep-link query params)
    #[prop(optional)]
    initial_epochs: Option<RwSignal<Option<String>>>,
    /// Optional initial learning rate (from deep-link query params)
    #[prop(optional)]
    initial_learning_rate: Option<RwSignal<Option<String>>>,
    /// Optional initial batch size (from deep-link query params)
    #[prop(optional)]
    initial_batch_size: Option<RwSignal<Option<String>>>,
    /// Optional initial LoRA rank (from deep-link query params)
    #[prop(optional)]
    initial_rank: Option<RwSignal<Option<String>>>,
    /// Optional initial LoRA alpha (from deep-link query params)
    #[prop(optional)]
    initial_alpha: Option<RwSignal<Option<String>>>,
) -> impl IntoView {
    let is_active = Arc::new(AtomicBool::new(true));
    {
        let is_active = Arc::clone(&is_active);
        on_cleanup(move || {
            is_active.store(false, Ordering::Relaxed);
        });
    }

    // Wizard step state
    let current_step = RwSignal::new(WizardStep::default());

    // Form state - persists across steps
    let adapter_name = RwSignal::new(String::new());
    let skill_purpose = RwSignal::new(String::new());
    let base_model_id = RwSignal::new(String::new());
    let dataset_id = RwSignal::new(String::new());
    let dataset_message = RwSignal::new(None::<String>);

    // Initialize dataset_id from initial_dataset_id if provided
    if let Some(init_ds) = initial_dataset_id {
        Effect::new(move || {
            if let Some(ds_id) = init_ds.try_get().flatten() {
                let _ = dataset_id.try_set(ds_id.clone());
                let _ = dataset_message
                    .try_set(Some(format!("Using selected training data: {}", ds_id)));
            }
        });
    }

    // Pre-populate from source document if provided
    if let Some(src_doc) = source_document_id {
        Effect::new(move || {
            if let Some(doc_id) = src_doc.try_get().flatten() {
                let _ = dataset_message.try_set(Some(format!(
                    "Document {} is available. Add files or choose an uploaded file.",
                    doc_id
                )));
            }
        });
    }
    // Initialize base_model_id from deep-link if provided
    if let Some(init_model) = initial_base_model_id {
        Effect::new(move || {
            if let Some(model) = init_model.try_get().flatten() {
                let _ = base_model_id.try_set(model);
            }
        });
    }

    let category = RwSignal::new("code".to_string());

    // Training parameters with preset support
    let training_preset = RwSignal::new("qa".to_string());
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let validation_split = RwSignal::new("0.15".to_string());
    let early_stopping = RwSignal::new(true);
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());

    // Dataset sample count for time estimation (updated when dataset is selected)
    let dataset_sample_count = RwSignal::new(None::<usize>);

    // Train step advanced options
    let show_train_advanced = RwSignal::new(false);
    let show_knowledge_advanced = RwSignal::new(false);
    let preferred_backend = RwSignal::new("auto".to_string());
    let backend_policy = RwSignal::new("auto".to_string());
    let coreml_training_fallback = RwSignal::new("mlx".to_string());
    let document_upload_open = RwSignal::new(false);
    let selected_document_id = RwSignal::new(String::new());
    let creating_document_dataset = RwSignal::new(false);

    // Lifted state: survives step transitions so data isn't re-fetched
    // and UI toggles aren't lost when navigating between steps.
    let (models, _refetch_models) = use_api_resource(
        |client: std::sync::Arc<ApiClient>| async move { client.list_models().await },
    );
    let (documents, refetch_documents) =
        use_api_resource(|client: std::sync::Arc<ApiClient>| async move {
            client.list_documents(None).await
        });
    let use_custom_model = RwSignal::new(false);
    let dataset_info = RwSignal::new(None::<DatasetResponse>);

    // Shared API client for closures and handlers
    let client = use_api_client();

    // Fetch dataset details reactively when dataset_id changes
    {
        let _client = client.clone();
        #[cfg(target_arch = "wasm32")]
        let is_active = Arc::clone(&is_active);
        Effect::new(move || {
            let id = dataset_id.get();
            if id.trim().is_empty() {
                dataset_info.set(None);
                return;
            }
            #[cfg(target_arch = "wasm32")]
            {
                let client = _client.clone();
                let is_active = Arc::clone(&is_active);
                wasm_bindgen_futures::spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    match client.get_dataset(&id).await {
                        Ok(resp) => {
                            let _ = dataset_info.try_set(Some(resp));
                        }
                        Err(_) => {
                            let _ = dataset_info.try_set(None);
                        }
                    }
                });
            }
        });
    }

    // If no base model is preselected, default to the first non-CoreML model.
    Effect::new(move || {
        if !base_model_id.get().trim().is_empty() {
            return;
        }
        if let LoadingState::Loaded(resp) = models.get() {
            if let Some(model) = resp
                .models
                .iter()
                .find(|m| m.backend.as_deref() != Some("coreml"))
                .or_else(|| resp.models.first())
            {
                let _ = base_model_id.try_set(model.id.clone());
            }
        }
    });

    // Initialize training parameters from deep-link props
    if let Some(init_epochs) = initial_epochs {
        Effect::new(move || {
            if let Some(v) = init_epochs.try_get().flatten() {
                let _ = epochs.try_set(v);
            }
        });
    }
    if let Some(init_lr) = initial_learning_rate {
        Effect::new(move || {
            if let Some(v) = init_lr.try_get().flatten() {
                let _ = learning_rate.try_set(v);
            }
        });
    }
    if let Some(init_bs) = initial_batch_size {
        Effect::new(move || {
            if let Some(v) = init_bs.try_get().flatten() {
                let _ = batch_size.try_set(v);
            }
        });
    }
    if let Some(init_rank) = initial_rank {
        Effect::new(move || {
            if let Some(v) = init_rank.try_get().flatten() {
                let _ = rank.try_set(v);
            }
        });
    }
    if let Some(init_alpha) = initial_alpha {
        Effect::new(move || {
            if let Some(v) = init_alpha.try_get().flatten() {
                let _ = alpha.try_set(v);
            }
        });
    }
    if let Some(init_backend) = initial_preferred_backend {
        Effect::new(move || {
            if let Some(v) = init_backend.try_get().flatten() {
                let _ = preferred_backend.try_set(v);
            }
        });
    }
    if let Some(init_policy) = initial_backend_policy {
        Effect::new(move || {
            if let Some(v) = init_policy.try_get().flatten() {
                let _ = backend_policy.try_set(v);
            }
        });
    }

    // Wizard state
    let dataset_wizard_open = RwSignal::new(false);
    let generate_wizard_open = RwSignal::new(false);
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let form_state = use_form_state();
    let notifications = use_notifications();

    let on_created_clone = on_created.clone();

    // Dataset callback — unified for upload, document conversion, and generation
    let on_dataset_ready = Callback::new(move |outcome: DatasetOutcome| {
        let sample_count = (outcome.sample_count > 0).then_some(outcome.sample_count);
        let source = if outcome.is_synthetic {
            "Generated examples"
        } else {
            "Your files"
        };
        let message = if let Some(count) = sample_count {
            format!("{} are ready ({} examples).", source, count)
        } else {
            format!("{} are ready.", source)
        };
        dataset_id.set(outcome.dataset_id);
        dataset_sample_count.set(sample_count);
        dataset_message.set(Some(message));
    });

    // Convert a selected document into a dataset for training.
    let use_document_for_dataset = {
        let client = client.clone();
        let is_active = Arc::clone(&is_active);
        let on_dataset_ready = on_dataset_ready.clone();
        Callback::new(move |document_id: String| {
            let document_id = document_id.trim().to_string();
            if document_id.is_empty() || creating_document_dataset.get() {
                return;
            }

            creating_document_dataset.set(true);
            error.set(None);
            dataset_message.set(Some("Preparing your files…".to_string()));

            let client = client.clone();
            let is_active = Arc::clone(&is_active);
            let on_dataset_ready = on_dataset_ready.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                if let Err(message) = wait_for_document_indexed(&client, &document_id).await {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = error.try_set(Some(message));
                    let _ = creating_document_dataset.try_set(false);
                    return;
                }
                match client
                    .create_dataset_from_documents(vec![document_id.clone()], None)
                    .await
                {
                    Ok(dataset) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let sample_count = match client.get_dataset_statistics(&dataset.id).await {
                            Ok(stats) if stats.num_examples > 0 => stats.num_examples as usize,
                            _ => 0,
                        };
                        on_dataset_ready.run(DatasetOutcome {
                            dataset_id: dataset.id.clone(),
                            dataset_version_id: dataset.dataset_version_id.clone(),
                            sample_count,
                            is_synthetic: false,
                            source_hash: dataset
                                .dataset_hash_b3
                                .clone()
                                .or(dataset.hash_b3.clone()),
                            receipt_count: 0,
                        });
                        let _ = creating_document_dataset.try_set(false);
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = error.try_set(Some(format_structured_details(&e)));
                        let _ = creating_document_dataset.try_set(false);
                    }
                }
            });
        })
    };

    let on_document_upload_success = {
        let on_dataset_ready = on_dataset_ready.clone();
        Callback::new(move |result: UploadBatchResult| {
            refetch_documents.run(());
            if let Some(dataset_id_value) = result.dataset_id {
                on_dataset_ready.run(DatasetOutcome {
                    dataset_id: dataset_id_value,
                    dataset_version_id: None,
                    sample_count: 0,
                    is_synthetic: false,
                    source_hash: None,
                    receipt_count: 0,
                });
            } else if let Some(document_id) = result.document_ids.first() {
                use_document_for_dataset.run(document_id.clone());
            }
        })
    };

    // Step validation
    let validate_name_step = move || -> bool {
        let adapter_name_rules = rules::adapter_name();
        let name = adapter_name.get();
        validate_on_blur("adapter_name", &name, &adapter_name_rules, form_state)
    };

    let validate_knowledge_step = move || -> bool {
        let dataset_rules = [ValidationRule::Pattern {
            pattern: r"^\s*\S.*$",
            message: "Add your files or choose an uploaded file before continuing",
        }];
        let dataset = dataset_id.get();
        validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state)
    };

    let validate_config_step = {
        move || -> bool {
            let mut valid = true;

            let epochs_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 1000 },
            ];
            let epochs_value = epochs.get();
            if !validate_on_blur("epochs", &epochs_value, &epochs_rules, form_state) {
                valid = false;
            }

            let learning_rate_value = learning_rate.get();
            let learning_rate_rules = rules::learning_rate();
            if !validate_on_blur(
                "learning_rate",
                &learning_rate_value,
                &learning_rate_rules,
                form_state,
            ) {
                valid = false;
            }

            let validation_split_rules = [ValidationRule::Range { min: 0.0, max: 0.5 }];
            let validation_split_value = validation_split.get();
            if !validate_on_blur(
                "validation_split",
                &validation_split_value,
                &validation_split_rules,
                form_state,
            ) {
                valid = false;
            }

            let batch_size_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ];
            let batch_size_value = batch_size.get();
            if !validate_on_blur(
                "batch_size",
                &batch_size_value,
                &batch_size_rules,
                form_state,
            ) {
                valid = false;
            }

            let rank_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ];
            let rank_value = rank.get();
            if !validate_on_blur("rank", &rank_value, &rank_rules, form_state) {
                valid = false;
            }

            let alpha_rules = [
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 512 },
            ];
            let alpha_value = alpha.get();
            if !validate_on_blur("alpha", &alpha_value, &alpha_rules, form_state) {
                valid = false;
            }

            valid
        }
    };

    let validate_train_step = move || -> bool { validate_config_step() };

    // Navigation
    let go_next = move |_: ()| {
        let step = current_step.get();
        let can_proceed = match step {
            WizardStep::Knowledge => validate_knowledge_step(),
            WizardStep::Name => validate_name_step(),
            WizardStep::Train => validate_train_step(),
            WizardStep::Confirm => true,
        };

        if can_proceed {
            if let Some(next) = step.next() {
                current_step.set(next);
            }
        }
    };

    let go_back = move |_: ()| {
        if let Some(prev) = current_step.get().prev() {
            current_step.set(prev);
            form_state.update(|state| state.clear_all());
        }
    };

    // Submit handler
    let submit = {
        let on_created = on_created_clone.clone();
        let notifications = notifications.clone();
        let client = client.clone();
        move |_: ()| {
            submitting.set(true);
            error.set(None);

            let name = adapter_name.get();
            let name_for_toast = name.clone();
            let model = base_model_id.get();
            let ds_id = dataset_id.get();
            let epochs_val: u32 = epochs.get().parse().unwrap_or(10);
            let lr_val: f32 = learning_rate.get().parse().unwrap_or(0.0001);
            let val_split: f32 = validation_split.get().parse().unwrap_or(0.0);
            let batch_val: u32 = batch_size.get().parse().unwrap_or(4);
            let rank_val: u32 = rank.get().parse().unwrap_or(8);
            let alpha_val: u32 = alpha.get().parse().unwrap_or(16);
            let backend_val = preferred_backend.get();
            let policy_val = backend_policy.get();
            let fallback_val = coreml_training_fallback.get();

            let on_created = on_created.clone();
            let notifications = notifications.clone();
            let client = client.clone();
            let is_active = Arc::clone(&is_active);

            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                let model_id = model.trim().to_string();
                if model_id.is_empty() {
                    let _ = error.try_set(Some(
                        "A training model is still loading. Please try again in a moment."
                            .to_string(),
                    ));
                    let _ = submitting.try_set(false);
                    return;
                }

                let dataset_id = ds_id.trim().to_string();
                if dataset_id.is_empty() {
                    let _ = error.try_set(Some(
                        "Add training examples before starting this adapter.".to_string(),
                    ));
                    let _ = submitting.try_set(false);
                    return;
                }

                let request = json!({
                    "adapter_name": name,
                    "base_model_id": model_id,
                    "training_config": {
                        "rank": rank_val,
                        "alpha": alpha_val,
                        "targets": ["q_proj", "v_proj"],
                        "training_contract_version": TRAINING_DATA_CONTRACT_VERSION,
                        "pad_token_id": 0,
                        "ignore_index": -100,
                        "epochs": epochs_val,
                        "learning_rate": lr_val,
                        "batch_size": batch_val,
                        "validation_split": if val_split > 0.0 { json!(val_split) } else { serde_json::Value::Null },
                        "preferred_backend": if backend_val == "auto" { serde_json::Value::Null } else { json!(backend_val) },
                        "backend_policy": if policy_val == "auto" { serde_json::Value::Null } else { json!(policy_val) },
                        "coreml_training_fallback": if backend_val == "coreml" || policy_val == "coreml_else_fallback" { json!(fallback_val) } else { serde_json::Value::Null },
                        "early_stopping": early_stopping.get_untracked(),
                    },
                });

                match client.create_adapter_from_dataset(&dataset_id, &request).await {
                    Ok(response) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = submitting.try_set(false);
                        let job_href = format!("/training?job_id={}", response.id);
                        notifications.success_with_action(
                            "Adapter training started",
                            &format!(
                                "\"{}\" is now training. You can watch progress live.",
                                name_for_toast
                            ),
                            "View training",
                            &job_href,
                        );
                        if is_active.load(Ordering::Relaxed) {
                            on_created(response.id);
                        }
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = error.try_set(Some(map_training_submit_error(&e)));
                        let _ = submitting.try_set(false);
                    }
                }
            });
        }
    };

    let reset_form = move || {
        current_step.set(WizardStep::default());
        error.set(None);
        form_state.update(|state| state.clear_all());
        submitting.set(false);
        dataset_wizard_open.set(false);
        generate_wizard_open.set(false);
        // Reset form state
        adapter_name.set(String::new());
        skill_purpose.set(String::new());
        base_model_id.set(String::new());
        dataset_id.set(String::new());
        dataset_message.set(None);
        dataset_sample_count.set(None);
        // Reset to QA preset defaults
        training_preset.set("qa".to_string());
        epochs.set("10".to_string());
        learning_rate.set("0.0001".to_string());
        validation_split.set("0.15".to_string());
        early_stopping.set(true);
        batch_size.set("4".to_string());
        rank.set("8".to_string());
        alpha.set("16".to_string());
        preferred_backend.set("auto".to_string());
        backend_policy.set("auto".to_string());
        show_train_advanced.set(false);
        show_knowledge_advanced.set(false);
        document_upload_open.set(false);
        selected_document_id.set(String::new());
        creating_document_dataset.set(false);
        // Reset lifted step state
        use_custom_model.set(false);
        dataset_info.set(None);
    };

    // Reset form when dialog closes
    let was_open = StoredValue::new(open.get_untracked());
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let prev = was_open.get_value();
        was_open.set_value(is_open);
        if prev && !is_open {
            reset_form();
        }
    });

    let step_labels = STEPS
        .iter()
        .map(|step| step.label().to_string())
        .collect::<Vec<_>>();

    view! {
        <StepFormDialog
            open=open
            title="Create Adapter".to_string()
            current_step=Signal::derive(move || current_step.try_get().unwrap_or_default().index())
            total_steps=STEPS.len()
            step_labels=step_labels
            loading=Signal::derive(move || submitting.try_get().unwrap_or(false))
            on_next=Callback::new(go_next)
            on_back=Callback::new(go_back)
            on_submit=Callback::new(submit.clone())
            submit_label="Start training".to_string()
            size=DialogSize::Lg
            scrollable=true
        >
            // Error message
            {move || error.try_get().flatten().map(|e| view! {
                <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                    <p class="text-sm text-destructive">{e}</p>
                </div>
            })}

            // Step content
            <div class="wizard-step-content min-w-0">
                {move || match current_step.try_get().unwrap_or_default() {
                    WizardStep::Knowledge => view! {
                        <DatasetStepContent
                            dataset_id=dataset_id
                            dataset_message=dataset_message
                            dataset_sample_count=dataset_sample_count
                            dataset_info=dataset_info
                            show_knowledge_advanced=show_knowledge_advanced
                            document_upload_open=document_upload_open
                            selected_document_id=selected_document_id
                            creating_document_dataset=creating_document_dataset
                            documents=documents
                            on_use_document=use_document_for_dataset
                            dataset_wizard_open=dataset_wizard_open
                            generate_wizard_open=generate_wizard_open
                            form_state=form_state
                        />
                    }.into_any(),
                    WizardStep::Name => view! {
                        <NameStepContent
                            adapter_name=adapter_name
                            skill_purpose=skill_purpose
                            form_state=form_state
                        />
                    }.into_any(),
                    WizardStep::Train => view! {
                        <TrainStepContent
                            training_preset=training_preset
                            epochs=epochs
                            learning_rate=learning_rate
                            validation_split=validation_split
                            early_stopping=early_stopping
                            batch_size=batch_size
                            rank=rank
                            alpha=alpha
                            show_advanced=show_train_advanced
                            preferred_backend=preferred_backend
                            backend_policy=backend_policy
                            coreml_fallback=coreml_training_fallback
                            form_state=form_state
                            sample_count=dataset_sample_count.try_get().flatten()
                        />
                    }.into_any(),
                    WizardStep::Confirm => view! {
                        <ReviewStepContent
                            adapter_name=adapter_name.try_get().unwrap_or_default()
                            base_model_id=base_model_id.try_get().unwrap_or_default()
                            dataset_id=dataset_id.try_get().unwrap_or_default()
                            category=category.try_get().unwrap_or_default()
                            preset=training_preset.try_get().unwrap_or_default()
                            epochs=epochs.try_get().unwrap_or_default()
                            learning_rate=learning_rate.try_get().unwrap_or_default()
                            validation_split=validation_split.try_get().unwrap_or_default()
                            early_stopping=early_stopping.try_get().unwrap_or(true)
                            batch_size=batch_size.try_get().unwrap_or_default()
                            rank=rank.try_get().unwrap_or_default()
                            alpha=alpha.try_get().unwrap_or_default()
                            backend=preferred_backend.try_get().unwrap_or_default()
                        />
                    }.into_any(),
                }}
            </div>

            // Embedded wizards (modals inside modal)
            <DocumentUploadDialog
                open=document_upload_open
                on_success=on_document_upload_success
                allow_multiple=true
                auto_create_dataset=true
                prefer_training_dataset_upload=true
            />
            <DatasetUploadWizard
                open=dataset_wizard_open
                on_complete=on_dataset_ready.clone()
            />
            <GenerateDatasetWizard
                open=generate_wizard_open
                on_generated=on_dataset_ready
            />
        </StepFormDialog>
    }
}

/// Step 1: Adapter name and purpose.
#[component]
fn NameStepContent(
    adapter_name: RwSignal<String>,
    skill_purpose: RwSignal<String>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let adapter_name_error = use_field_error(form_state, "adapter_name");

    view! {
        <div class="space-y-6">
            <div class="rounded-lg border border-border/60 bg-card/40 p-4 space-y-1">
                <p class="text-sm font-medium">"Name your adapter"</p>
                <p class="text-xs text-muted-foreground">
                    "Use a short name and, optionally, what you want it to help with."
                </p>
            </div>

            <FormField
                label="Adapter name"
                name="adapter_name"
                required=true
                help="A short name such as \"billing-help\" or \"code-review\""
                error=adapter_name_error
            >
                <Input
                    value=adapter_name
                    placeholder="billing-help".to_string()
                    on_blur=Callback::new(move |_| {
                        let adapter_name_rules = rules::adapter_name();
                        let value = adapter_name.get();
                        let _ = validate_on_blur("adapter_name", &value, &adapter_name_rules, form_state);
                    })
                />
            </FormField>

            <FormField
                label="What should it help with?"
                name="skill_purpose"
                required=false
                help="Optional plain-language goal"
            >
                <Input
                    value=skill_purpose
                    placeholder="Answers customer questions about billing and account access.".to_string()
                />
            </FormField>
        </div>
    }
}

/// Step 0: Choose knowledge source for the adapter.
#[component]
fn DatasetStepContent(
    dataset_id: RwSignal<String>,
    dataset_message: RwSignal<Option<String>>,
    dataset_sample_count: RwSignal<Option<usize>>,
    dataset_info: RwSignal<Option<DatasetResponse>>,
    show_knowledge_advanced: RwSignal<bool>,
    document_upload_open: RwSignal<bool>,
    selected_document_id: RwSignal<String>,
    creating_document_dataset: RwSignal<bool>,
    documents: ReadSignal<LoadingState<DocumentListResponse>>,
    on_use_document: Callback<String>,
    dataset_wizard_open: RwSignal<bool>,
    generate_wizard_open: RwSignal<bool>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let has_dataset = Signal::derive(move || !dataset_id.get().trim().is_empty());
    let dataset_error = use_field_error(form_state, "dataset_id");

    view! {
        <div class="space-y-6">
            <div class="text-center py-2">
                <h3 class="heading-4 mb-1">"Add knowledge for your adapter"</h3>
                <p class="text-sm text-muted-foreground">
                    "Start by adding files, or choose a file you've already uploaded."
                </p>
            </div>

            // Current dataset status
            {move || {
                if has_dataset.get() {
                    let msg = dataset_message
                        .get()
                        .unwrap_or_else(|| "Your knowledge is ready.".to_string());
                    let details = dataset_info.get();
                    view! {
                        <div class="rounded-lg border border-status-success/50 bg-status-success/5 p-4">
                            <div class="flex items-start gap-3">
                                <div class="rounded-full bg-status-success/10 p-2 shrink-0">
                                    <svg class="w-5 h-5 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                    </svg>
                                </div>
                                <div class="min-w-0 flex-1">
                                    <p class="text-sm font-medium">{msg}</p>
                                    {move || details.clone().map(|info| {
                                        let display_name = info.display_name.clone().unwrap_or_else(|| info.name.clone());
                                        view! {
                                            <p class="text-xs text-muted-foreground mt-1">
                                                {format!("Knowledge source: {}", display_name)}
                                            </p>
                                        }
                                    })}
                                    {move || dataset_sample_count.get().map(|count| {
                                        view! {
                                            <p class="text-xs text-muted-foreground mt-1">
                                                {format!("{} examples", count)}
                                            </p>
                                        }
                                    })}
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    let msg = dataset_message.get();
                    view! {
                        {msg.map(|text| view! {
                            <div class="rounded-lg border border-border/70 bg-muted/30 p-3">
                                <p class="text-sm text-muted-foreground">{text}</p>
                            </div>
                        })}
                    }.into_any()
                }
            }}

            // Primary path
            <Card>
                <button
                    class="w-full p-5 text-left hover:bg-muted/50 transition-colors rounded-lg"
                    on:click=move |_| {
                        document_upload_open.set(true);
                    }
                >
                    <div class="flex items-start gap-4">
                        <div class="rounded-full bg-primary/10 p-3">
                            <svg class="w-6 h-6 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                            </svg>
                        </div>
                        <div>
                            <h4 class="font-medium">"Add your files"</h4>
                            <p class="text-sm text-muted-foreground mt-1">
                                "Upload a document and we will prepare training examples for you."
                            </p>
                        </div>
                    </div>
                </button>
            </Card>

            // Secondary path
            <Card>
                <div class="p-5 space-y-3">
                    <h4 class="font-medium">"Use a file you already uploaded"</h4>
                    <p class="text-sm text-muted-foreground">
                        "Choose a file and we will prepare training material from it."
                    </p>
                    <AsyncBoundary
                        state=documents
                        on_retry=Callback::new(move |_| {})
                        loading_message="Loading uploaded files...".to_string()
                        render=move |resp: DocumentListResponse| {
                            let options = resp
                                .data
                                .iter()
                                .map(|doc| {
                                    let status = match doc.status.as_str() {
                                        "indexed" => "ready",
                                        "processing" => "processing",
                                        "failed" => "needs attention",
                                        _ => doc.status.as_str(),
                                    };
                                    (
                                        doc.document_id.clone(),
                                        format!("{} ({}, {})", doc.name, status, format_bytes(doc.size_bytes)),
                                    )
                                })
                                .collect::<Vec<_>>();

                            if options.is_empty() {
                                view! {
                                    <p class="text-sm text-muted-foreground">
                                        "No uploaded files yet. Use \"Add your files\" above."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-3">
                                        <Select value=selected_document_id options=options />
                                        <button
                                            type="button"
                                            class="btn btn-secondary"
                                            disabled=move || creating_document_dataset.get() || selected_document_id.get().trim().is_empty()
                                            on:click=move |_| on_use_document.run(selected_document_id.get())
                                        >
                                            {move || if creating_document_dataset.get() { "Preparing..." } else { "Use selected file" }}
                                        </button>
                                    </div>
                                }.into_any()
                            }
                        }
                    />
                </div>
            </Card>

            <div class="border-t pt-4">
                <button
                    type="button"
                    class="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| show_knowledge_advanced.update(|v| *v = !*v)
                >
                    <svg
                        class=move || if show_knowledge_advanced.get() { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced options"
                </button>

                <Show when=move || show_knowledge_advanced.get()>
                    <div class="mt-4 space-y-4 pl-6">
                        <button
                            type="button"
                            class="btn btn-secondary"
                            on:click=move |_| dataset_wizard_open.set(true)
                        >
                            "I have structured data (JSONL/CSV)"
                        </button>

                        <button
                            type="button"
                            class="btn btn-secondary"
                            on:click=move |_| generate_wizard_open.set(true)
                        >
                            "Generate examples from a document"
                        </button>

                        <FormField
                            label="Dataset ID (advanced)"
                            name="dataset_id"
                            help="Use a dataset ID that already exists"
                            error=dataset_error
                        >
                            <Input
                                value=dataset_id
                                placeholder="ds-abc123".to_string()
                                on_blur=Callback::new(move |_| {
                                    let dataset_rules = [ValidationRule::Pattern {
                                        pattern: r"^\s*\S.*$",
                                        message: "Add your files or choose an uploaded file before continuing",
                                    }];
                                    let dataset = dataset_id.get();
                                    let _ = validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state);
                                })
                            />
                        </FormField>
                    </div>
                </Show>
            </div>
        </div>
    }
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Step 2: Train setup (defaults first, advanced optional).
#[component]
fn TrainStepContent(
    form_state: RwSignal<FormState>,
    training_preset: RwSignal<String>,
    epochs: RwSignal<String>,
    learning_rate: RwSignal<String>,
    validation_split: RwSignal<String>,
    early_stopping: RwSignal<bool>,
    batch_size: RwSignal<String>,
    rank: RwSignal<String>,
    alpha: RwSignal<String>,
    show_advanced: RwSignal<bool>,
    preferred_backend: RwSignal<String>,
    backend_policy: RwSignal<String>,
    coreml_fallback: RwSignal<String>,
    sample_count: Option<usize>,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <div class="rounded-lg border border-border/60 bg-card/40 p-4 space-y-1">
                <p class="text-sm font-medium">"Default training plan"</p>
                <p class="text-xs text-muted-foreground">
                    "We will use balanced defaults. Open advanced options if you want to tune details."
                </p>
                {sample_count.map(|count| view! {
                    <p class="text-xs text-muted-foreground">{format!("Using {} examples", count)}</p>
                })}
            </div>

            <ConfigStepContent
                training_preset=training_preset
                epochs=epochs
                learning_rate=learning_rate
                validation_split=validation_split
                early_stopping=early_stopping
                batch_size=batch_size
                rank=rank
                alpha=alpha
                show_advanced=show_advanced
                preferred_backend=preferred_backend
                backend_policy=backend_policy
                coreml_fallback=coreml_fallback
                form_state=form_state
                sample_count=sample_count
            />
        </div>
    }
}

/// Train step: base model and adapter type.
#[allow(dead_code)]
#[component]
fn ModelStepContent(
    base_model_id: RwSignal<String>,
    category: RwSignal<String>,
    form_state: RwSignal<FormState>,
    /// Models resource fetched by the parent — survives step transitions.
    models: ReadSignal<LoadingState<ModelListResponse>>,
    /// Refetch handle for the models resource.
    refetch_models: Refetch,
    /// "Enter model ID manually" toggle — survives step transitions.
    use_custom_model: RwSignal<bool>,
) -> impl IntoView {
    let base_model_error = use_field_error(form_state, "base_model_id");

    view! {
        <div class="space-y-6">
            <FormField
                label="Starting model"
                name="base_model_id"
                required=true
                help="Choose the model this adapter should learn from"
                error=base_model_error
            >
                {move || {
                    if use_custom_model.try_get().unwrap_or(false) {
                        view! {
                            <div class="space-y-2">
                                <Input
                                    value=base_model_id
                                    placeholder="model-id".to_string()
                                    on_blur=Callback::new(move |_| {
                                        let model_rules = [ValidationRule::Pattern {
                                            pattern: r"^\s*\S.*$",
                                            message: "Base model is required",
                                        }];
                                        let value = base_model_id.get();
                                        let _ = validate_on_blur("base_model_id", &value, &model_rules, form_state);
                                    })
                                />
                                <button
                                    class="text-xs text-primary hover:underline"
                                    type="button"
                                    on:click=move |_| use_custom_model.set(false)
                                >
                                    "Choose from available models"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <AsyncBoundary
                                state=models
                                on_retry=Callback::new(move |_| refetch_models.run(()))
                                loading_message="Loading models...".to_string()
                                render=move |resp| {
                                    // Track which model IDs use CoreML backend
                                    let coreml_ids: Vec<String> = resp.models.iter()
                                        .filter(|m| m.backend.as_deref() == Some("coreml"))
                                        .map(|m| m.id.clone())
                                        .collect();
                                    let options: Vec<(String, String)> = resp.models.iter().map(|m| {
                                        let is_coreml = m.backend.as_deref() == Some("coreml");
                                        let label = match (&m.quantization, is_coreml) {
                                            (Some(q), true) => format!("{} ({}) — CoreML, no adapter support", m.name, q),
                                            (Some(q), false) => format!("{} ({})", m.name, q),
                                            (None, true) => format!("{} — CoreML, no adapter support", m.name),
                                            (None, false) => m.name.clone(),
                                        };
                                        (m.id.clone(), label)
                                    }).collect();
                                    view! {
                                        <div class="space-y-2">
                                            <Select
                                                value=base_model_id
                                                options=options
                                                on_change=Callback::new(move |selected: String| {
                                                    let model_rules = [ValidationRule::Pattern {
                                                        pattern: r"^\s*\S.*$",
                                                        message: "Base model is required",
                                                    }];
                                                    let _ = validate_on_blur("base_model_id", &selected, &model_rules, form_state);
                                                })
                                            />
                                            {move || {
                                                let selected = base_model_id.get();
                                                coreml_ids.contains(&selected).then(|| view! {
                                                    <div class="rounded-md border border-status-warning/40 bg-status-warning/10 p-3">
                                                        <p class="text-xs text-status-warning">
                                                            "This model cannot train new skills in this environment. Choose an MLX model instead."
                                                        </p>
                                                    </div>
                                                })
                                            }}
                                            <button
                                                class="text-xs text-primary hover:underline"
                                                type="button"
                                                on:click=move |_| use_custom_model.set(true)
                                            >
                                                "Enter model ID manually"
                                            </button>
                                        </div>
                                    }
                                }
                            />
                        }.into_any()
                    }
                }}
            </FormField>

            <FormField
                label="Adapter type"
                name="category"
                help="Used for organization and filtering"
            >
                <Select
                    value=category
                    options=vec![
                        ("code".to_string(), "Code tasks".to_string()),
                        ("framework".to_string(), "Framework guidance".to_string()),
                        ("codebase".to_string(), "Codebase helper".to_string()),
                        ("docs".to_string(), "Documentation".to_string()),
                        ("domain".to_string(), "Domain expertise".to_string()),
                    ]
                />
            </FormField>
        </div>
    }
}

/// Step 3: Training configuration with presets
#[component]
fn ConfigStepContent(
    training_preset: RwSignal<String>,
    epochs: RwSignal<String>,
    learning_rate: RwSignal<String>,
    validation_split: RwSignal<String>,
    early_stopping: RwSignal<bool>,
    batch_size: RwSignal<String>,
    rank: RwSignal<String>,
    alpha: RwSignal<String>,
    show_advanced: RwSignal<bool>,
    preferred_backend: RwSignal<String>,
    backend_policy: RwSignal<String>,
    coreml_fallback: RwSignal<String>,
    form_state: RwSignal<FormState>,
    sample_count: Option<usize>,
) -> impl IntoView {
    let preset_label = TrainingPreset::parse_str(&training_preset.get())
        .label()
        .to_string();
    let epochs_error = use_field_error(form_state, "epochs");
    let learning_rate_error = use_field_error(form_state, "learning_rate");
    let validation_split_error = use_field_error(form_state, "validation_split");
    let batch_size_error = use_field_error(form_state, "batch_size");
    let rank_error = use_field_error(form_state, "rank");
    let alpha_error = use_field_error(form_state, "alpha");

    view! {
        <div class="space-y-6">
            <Card>
                <div class="p-4 space-y-1">
                    <p class="text-sm font-medium">"Balanced default plan"</p>
                    <p class="text-xs text-muted-foreground">
                        "Training uses sensible defaults for most adapters."
                    </p>
                    <p class="text-xs text-muted-foreground">
                        {format!("Preset: {}", preset_label)}
                    </p>
                    {sample_count.map(|count| view! {
                        <p class="text-xs text-muted-foreground">{format!("Estimated input size: {} examples", count)}</p>
                    })}
                </div>
            </Card>

            // Advanced options section
            <div class="border-t pt-4">
                <button
                    type="button"
                    class="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| show_advanced.update(|v| *v = !*v)
                >
                    <svg
                        class=move || if show_advanced.try_get().unwrap_or(false) { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced options"
                </button>

                {move || show_advanced.try_get().unwrap_or(false).then(|| view! {
                    <div class="mt-4 space-y-6 pl-6">
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Training parameters"</h4>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <FormField
                                    label="Epochs"
                                    name="epochs"
                                    required=true
                                    help="Number of passes over your training examples"
                                    error=epochs_error
                                >
                                    <Input
                                        value=epochs
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let epochs_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 1000 },
                                            ];
                                            let value = epochs.get();
                                            let _ = validate_on_blur("epochs", &value, &epochs_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Learning rate"
                                    name="learning_rate"
                                    required=true
                                    help="Step size for parameter updates"
                                    error=learning_rate_error
                                >
                                    <Input
                                        value=learning_rate
                                        on_blur=Callback::new(move |_| {
                                            let learning_rate_rules = rules::learning_rate();
                                            let value = learning_rate.get();
                                            let _ = validate_on_blur(
                                                "learning_rate",
                                                &value,
                                                &learning_rate_rules,
                                                form_state,
                                            );
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Validation split"
                                    name="validation_split"
                                    required=true
                                    help="Portion of data held out for validation"
                                    error=validation_split_error
                                >
                                    <Input
                                        value=validation_split
                                        on_blur=Callback::new(move |_| {
                                            let validation_split_rules =
                                                [ValidationRule::Range { min: 0.0, max: 0.5 }];
                                            let value = validation_split.get();
                                            let _ = validate_on_blur(
                                                "validation_split",
                                                &value,
                                                &validation_split_rules,
                                                form_state,
                                            );
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Early stopping"
                                    name="early_stopping"
                                    required=false
                                    help="Stop automatically when progress plateaus"
                                >
                                    <label class="flex items-center gap-2 text-sm">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || early_stopping.get()
                                            on:change=move |ev| {
                                                use wasm_bindgen::JsCast;
                                                if let Some(input) = ev
                                                    .target()
                                                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                                {
                                                    early_stopping.set(input.checked());
                                                }
                                            }
                                        />
                                        <span>"Enable early stopping"</span>
                                    </label>
                                </FormField>
                            </div>
                        </div>

                        // Batch size and LoRA config
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Adapter settings"</h4>
                            <div class="grid gap-4 sm:grid-cols-3">
                                <FormField
                                    label="Batch Size"
                                    name="batch_size"
                                    required=true
                                    help="Examples per step (1-256)"
                                    error=batch_size_error
                                >
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let batch_size_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = batch_size.get();
                                            let _ = validate_on_blur("batch_size", &value, &batch_size_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter dimension (4, 8, 16 typical)"
                                    error=rank_error
                                >
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let rank_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = rank.get();
                                            let _ = validate_on_blur("rank", &value, &rank_rules, form_state);
                                        })
                                    />
                                </FormField>
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=alpha_error
                                >
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                        on_blur=Callback::new(move |_| {
                                            let alpha_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 512 },
                                            ];
                                            let value = alpha.get();
                                            let _ = validate_on_blur("alpha", &value, &alpha_rules, form_state);
                                        })
                                    />
                                </FormField>
                            </div>
                        </div>

                        // Backend selection
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Runtime Selection"</h4>
                            <p class="text-xs text-muted-foreground mb-3">
                                "By default, the system automatically picks the best available runtime."
                            </p>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <FormField label="Preferred Runtime" name="preferred_backend">
                                    <Select
                                        value=preferred_backend
                                        options=vec![
                                            ("auto".to_string(), "Auto (recommended)".to_string()),
                                            ("mlx".to_string(), "MLX".to_string()),
                                            ("coreml".to_string(), "CoreML".to_string()),
                                            ("metal".to_string(), "Metal".to_string()),
                                        ]
                                    />
                                </FormField>
                                <FormField label="Runtime Policy" name="backend_policy">
                                    <Select
                                        value=backend_policy
                                        options=vec![
                                            ("auto".to_string(), "Auto".to_string()),
                                            ("coreml_only".to_string(), "CoreML Only".to_string()),
                                            ("coreml_else_fallback".to_string(), "CoreML with Fallback".to_string()),
                                        ]
                                    />
                                </FormField>
                            </div>
                            {move || (preferred_backend.try_get().unwrap_or_default() == "coreml" || backend_policy.try_get().unwrap_or_default() == "coreml_else_fallback").then(|| view! {
                                <div class="mt-4">
                                    <FormField label="Fallback Runtime" name="coreml_fallback">
                                        <Select
                                            value=coreml_fallback
                                            options=vec![
                                                ("mlx".to_string(), "MLX".to_string()),
                                                ("metal".to_string(), "Metal".to_string()),
                                            ]
                                        />
                                    </FormField>
                                </div>
                            })}
                        </div>
                    </div>
                })}
            </div>
        </div>
    }
}

/// Step 4: Review and submit
#[component]
fn ReviewStepContent(
    adapter_name: String,
    base_model_id: String,
    dataset_id: String,
    category: String,
    preset: String,
    epochs: String,
    learning_rate: String,
    validation_split: String,
    early_stopping: bool,
    batch_size: String,
    rank: String,
    alpha: String,
    backend: String,
) -> impl IntoView {
    let preset_label = TrainingPreset::parse_str(&preset).label();
    let val_split_display = validation_split
        .parse::<f32>()
        .map(|v| format!("{:.0}%", v * 100.0))
        .unwrap_or_else(|_| validation_split.clone());

    view! {
        <div class="space-y-6">
            <div class="text-center py-2">
                <h3 class="heading-4">"Confirm adapter setup"</h3>
                <p class="text-sm text-muted-foreground">
                    "Review what will be created. You can go back and edit any step."
                </p>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Adapter name" value=adapter_name/>
                <ReviewRow label="Starting model" value=base_model_id/>
                <ReviewRow label="Your files" value=if dataset_id.is_empty() { "None selected".to_string() } else { dataset_id }/>
                <ReviewRow label="Adapter type" value=category/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Training plan" value=preset_label.to_string()/>
                <ReviewRow label="Epochs" value=epochs/>
                <ReviewRow label="Learning Rate" value=learning_rate/>
                <ReviewRow label="Validation split" value=val_split_display/>
                <ReviewRow label="Early stopping" value=if early_stopping { "Enabled".to_string() } else { "Disabled".to_string() }/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Batch Size" value=batch_size/>
                <ReviewRow label="Adapter Capacity" value=rank/>
                <ReviewRow label="Adapter Strength" value=alpha/>
                <ReviewRow label="Compute Runtime" value=if backend == "auto" { "Automatic (recommended)".to_string() } else { backend }/>
            </div>
        </div>
    }
}

#[component]
fn ReviewRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between gap-4 py-3 px-4">
            <span class="text-sm text-muted-foreground shrink-0">{label}</span>
            <span class="text-sm font-medium min-w-0 truncate">{value}</span>
        </div>
    }
}
