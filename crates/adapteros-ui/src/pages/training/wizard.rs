//! Stepped wizard for creating training jobs
//!
//! 4-step wizard flow:
//! 1. Dataset - Choose data source
//! 2. Model - Select base model
//! 3. Config - Training parameters
//! 4. Review - Summary and submit

use crate::api::error::format_structured_details;
use crate::api::{use_api_client, ApiClient, DatasetResponse, ModelListResponse};
use crate::components::{
    AsyncBoundary, Card, DialogSize, FormField, Input, Select, StepFormDialog,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use crate::pages::training::config_presets::{TrainingConfigPresets, TrainingPreset};
use crate::pages::training::dataset_wizard::{DatasetOutcome, DatasetUploadWizard};
use crate::pages::training::generate_wizard::GenerateDatasetWizard;
use crate::signals::use_notifications;
use crate::validation::{
    rules, use_field_error, use_form_state, validate_on_blur, FormState, ValidationRule,
};
use adapteros_api_types::{TrainingJobResponse, TRAINING_DATA_CONTRACT_VERSION};
use leptos::prelude::*;
use serde_json::json;

/// Wizard step enum
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WizardStep {
    #[default]
    Dataset,
    Model,
    Config,
    Review,
}

impl WizardStep {
    fn index(&self) -> usize {
        match self {
            WizardStep::Dataset => 0,
            WizardStep::Model => 1,
            WizardStep::Config => 2,
            WizardStep::Review => 3,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            WizardStep::Dataset => "Dataset",
            WizardStep::Model => "Model",
            WizardStep::Config => "Configure",
            WizardStep::Review => "Review",
        }
    }

    fn next(&self) -> Option<WizardStep> {
        match self {
            WizardStep::Dataset => Some(WizardStep::Model),
            WizardStep::Model => Some(WizardStep::Config),
            WizardStep::Config => Some(WizardStep::Review),
            WizardStep::Review => None,
        }
    }

    fn prev(&self) -> Option<WizardStep> {
        match self {
            WizardStep::Dataset => None,
            WizardStep::Model => Some(WizardStep::Dataset),
            WizardStep::Config => Some(WizardStep::Model),
            WizardStep::Review => Some(WizardStep::Config),
        }
    }
}

const STEPS: [WizardStep; 4] = [
    WizardStep::Dataset,
    WizardStep::Model,
    WizardStep::Config,
    WizardStep::Review,
];

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
    // Wizard step state
    let current_step = RwSignal::new(WizardStep::default());

    // Form state - persists across steps
    let adapter_name = RwSignal::new(String::new());
    let base_model_id = RwSignal::new(String::new());
    let dataset_id = RwSignal::new(String::new());
    let dataset_message = RwSignal::new(None::<String>);

    // Initialize dataset_id from initial_dataset_id if provided
    if let Some(init_ds) = initial_dataset_id {
        Effect::new(move || {
            if let Some(ds_id) = init_ds.try_get().flatten() {
                let _ = dataset_id.try_set(ds_id.clone());
                let _ = dataset_message.try_set(Some(format!("Using dataset: {}", ds_id)));
            }
        });
    }

    // Pre-populate from source document if provided
    if let Some(src_doc) = source_document_id {
        Effect::new(move || {
            if let Some(doc_id) = src_doc.try_get().flatten() {
                let _ = dataset_message.try_set(Some(format!(
                    "From document: {} — generate or upload a dataset",
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

    // Backend selection (simplified - Auto by default)
    let show_advanced_backend = RwSignal::new(false);
    let preferred_backend = RwSignal::new("auto".to_string());
    let backend_policy = RwSignal::new("auto".to_string());
    let coreml_training_fallback = RwSignal::new("mlx".to_string());

    // Lifted state: survives step transitions so data isn't re-fetched
    // and UI toggles aren't lost when navigating between steps.
    let (models, refetch_models) = use_api_resource(
        |client: std::sync::Arc<ApiClient>| async move { client.list_models().await },
    );
    let use_custom_model = RwSignal::new(false);
    let dataset_info = RwSignal::new(None::<DatasetResponse>);
    let show_change_options = RwSignal::new(false);

    // Shared API client for closures and handlers
    let client = use_api_client();

    // Fetch dataset details reactively when dataset_id changes
    {
        let client = client.clone();
        Effect::new(move || {
            let id = dataset_id.get();
            if id.trim().is_empty() {
                dataset_info.set(None);
                return;
            }
            #[cfg(target_arch = "wasm32")]
            {
                let client = client.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match client.get_dataset(&id).await {
                        Ok(resp) => dataset_info.set(Some(resp)),
                        Err(_) => dataset_info.set(None),
                    }
                });
            }
        });
    }

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

    // Dataset callback — unified for both upload and generation
    let on_dataset_ready = {
        move |outcome: DatasetOutcome| {
            let label = if outcome.is_synthetic {
                "Generated"
            } else {
                "Uploaded"
            };
            dataset_id.set(outcome.dataset_id.clone());
            dataset_sample_count.set(Some(outcome.sample_count));
            dataset_message.set(Some(format!(
                "{} dataset: {} ({} samples)",
                label, outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    // Step validation
    let validate_dataset_step = move || -> bool {
        let dataset_rules = [ValidationRule::Pattern {
            pattern: r"^\s*\S.*$",
            message: "Select or generate a dataset before continuing",
        }];
        let dataset = dataset_id.get();
        validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state)
    };

    let validate_model_step = {
        move || -> bool {
            let mut valid = true;

            let name = adapter_name.get();
            let adapter_name_rules = rules::adapter_name();
            if !validate_on_blur("adapter_name", &name, &adapter_name_rules, form_state) {
                valid = false;
            }

            let model = base_model_id.get();
            let model_rules = [ValidationRule::Pattern {
                pattern: r"^\s*\S.*$",
                message: "Base model is required",
            }];
            if !validate_on_blur("base_model_id", &model, &model_rules, form_state) {
                valid = false;
            }

            valid
        }
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

    // Navigation
    let go_next = move |_: ()| {
        let step = current_step.get();
        let can_proceed = match step {
            WizardStep::Dataset => validate_dataset_step(),
            WizardStep::Model => validate_model_step(),
            WizardStep::Config => validate_config_step(),
            WizardStep::Review => true,
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
            let cat = category.get();
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

            wasm_bindgen_futures::spawn_local(async move {
                let dataset_id = ds_id.trim().to_string();
                if dataset_id.is_empty() {
                    error.set(Some("Dataset is required to start training".to_string()));
                    submitting.set(false);
                    return;
                }

                let request = json!({
                    "adapter_name": name,
                    "base_model_id": model,
                    "dataset_id": dataset_id,
                    "config": {
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
                    "category": cat,
                });

                match client
                    .post::<_, TrainingJobResponse>("/v1/training/jobs", &request)
                    .await
                {
                    Ok(response) => {
                        submitting.set(false);
                        let job_href = format!("/training?job_id={}", response.id);
                        notifications.success_with_action(
                            "Training job created",
                            &format!("\"{}\" is now queued for training", name_for_toast),
                            "View Job",
                            &job_href,
                        );
                        on_created(response.id);
                    }
                    Err(e) => {
                        error.set(Some(format_structured_details(&e)));
                        submitting.set(false);
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
        show_advanced_backend.set(false);
        // Reset lifted step state
        use_custom_model.set(false);
        dataset_info.set(None);
        show_change_options.set(false);
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
            title="New Training Job".to_string()
            current_step=Signal::derive(move || current_step.try_get().unwrap_or_default().index())
            total_steps=STEPS.len()
            step_labels=step_labels
            loading=Some(Signal::derive(move || submitting.try_get().unwrap_or(false)))
            on_next=Callback::new(go_next)
            on_back=Callback::new(go_back)
            on_submit=Callback::new(submit.clone())
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
                    WizardStep::Dataset => view! {
                        <DatasetStepContent
                            dataset_id=dataset_id
                            dataset_message=dataset_message
                            dataset_sample_count=dataset_sample_count
                            dataset_info=dataset_info
                            show_change_options=show_change_options
                            dataset_wizard_open=dataset_wizard_open
                            generate_wizard_open=generate_wizard_open
                            form_state=form_state
                        />
                    }.into_any(),
                    WizardStep::Model => view! {
                        <ModelStepContent
                            adapter_name=adapter_name
                            base_model_id=base_model_id
                            category=category
                            form_state=form_state
                            models=models
                            refetch_models=refetch_models
                            use_custom_model=use_custom_model
                        />
                    }.into_any(),
                    WizardStep::Config => view! {
                        <ConfigStepContent
                            training_preset=training_preset
                            epochs=epochs
                            learning_rate=learning_rate
                            validation_split=validation_split
                            early_stopping=early_stopping
                            batch_size=batch_size
                            rank=rank
                            alpha=alpha
                            show_advanced=show_advanced_backend
                            preferred_backend=preferred_backend
                            backend_policy=backend_policy
                            coreml_fallback=coreml_training_fallback
                            form_state=form_state
                            sample_count=dataset_sample_count.try_get().flatten()
                        />
                    }.into_any(),
                    WizardStep::Review => view! {
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
            <DatasetUploadWizard
                open=dataset_wizard_open
                on_complete=Callback::new(on_dataset_ready.clone())
            />
            <GenerateDatasetWizard
                open=generate_wizard_open
                on_generated=Callback::new(on_dataset_ready.clone())
            />
        </StepFormDialog>
    }
}

/// Step 1: Dataset selection — context-aware
///
/// When a dataset is pre-selected (e.g., from chat-to-training or dataset detail),
/// shows the dataset info and offers to continue, synthesize more, or change.
/// When no dataset is set, shows the original upload/generate flow.
#[component]
fn DatasetStepContent(
    dataset_id: RwSignal<String>,
    dataset_message: RwSignal<Option<String>>,
    dataset_sample_count: RwSignal<Option<usize>>,
    /// Dataset details fetched by the parent — survives step transitions.
    dataset_info: RwSignal<Option<DatasetResponse>>,
    /// "Change dataset" toggle — survives step transitions.
    show_change_options: RwSignal<bool>,
    dataset_wizard_open: RwSignal<bool>,
    generate_wizard_open: RwSignal<bool>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let has_dataset = Signal::derive(move || !dataset_id.get().trim().is_empty());

    view! {
        <div class="space-y-6">
            {move || {
                if has_dataset.get() {
                    view! {
                        <DatasetReadyView
                            dataset_id=dataset_id
                            dataset_info=dataset_info
                            dataset_message=dataset_message
                            dataset_sample_count=dataset_sample_count
                            show_change_options=show_change_options
                            dataset_wizard_open=dataset_wizard_open
                            generate_wizard_open=generate_wizard_open
                            form_state=form_state
                        />
                    }.into_any()
                } else {
                    view! {
                        <DatasetChooseView
                            dataset_id=dataset_id
                            dataset_message=dataset_message
                            dataset_wizard_open=dataset_wizard_open
                            generate_wizard_open=generate_wizard_open
                            form_state=form_state
                        />
                    }.into_any()
                }
            }}
        </div>
    }
}

/// Shown when a dataset is already selected — contextual "ready" state
#[component]
fn DatasetReadyView(
    dataset_id: RwSignal<String>,
    dataset_info: RwSignal<Option<DatasetResponse>>,
    dataset_message: RwSignal<Option<String>>,
    dataset_sample_count: RwSignal<Option<usize>>,
    show_change_options: RwSignal<bool>,
    dataset_wizard_open: RwSignal<bool>,
    generate_wizard_open: RwSignal<bool>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let dataset_error = use_field_error(form_state, "dataset_id");

    view! {
        <div class="space-y-4">
            <div class="text-center py-2">
                <h3 class="heading-4 mb-1">"Dataset ready"</h3>
                <p class="text-sm text-muted-foreground">
                    "Your training data is selected. Click Next to choose a model."
                </p>
            </div>

            // Dataset info card
            {move || {
                if let Some(info) = dataset_info.get() {
                    let display_name = info.display_name.clone().unwrap_or_else(|| info.name.clone());
                    let format_label = info.format.to_uppercase();
                    let size = info.total_size_bytes.map(format_bytes).unwrap_or_default();
                    let file_count = info.file_count.unwrap_or(0);
                    let status = info.status.clone();
                    let ds_type = info.dataset_type.clone().unwrap_or_default();

                    view! {
                        <div class="rounded-lg border border-status-success/50 bg-status-success/5 p-4">
                            <div class="flex items-start gap-4">
                                <div class="rounded-full bg-status-success/10 p-2.5 shrink-0">
                                    <svg class="w-5 h-5 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                    </svg>
                                </div>
                                <div class="min-w-0 flex-1">
                                    <p class="font-medium truncate">{display_name}</p>
                                    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1 text-xs text-muted-foreground">
                                        <span class="font-medium text-foreground/70">{format_label}</span>
                                        {(file_count > 0).then(|| view! {
                                            <span>{format!("{} file{}", file_count, if file_count == 1 { "" } else { "s" })}</span>
                                        })}
                                        {(!size.is_empty()).then(|| view! {
                                            <span>{size}</span>
                                        })}
                                        {(!ds_type.is_empty()).then(|| view! {
                                            <span class="inline-flex items-center rounded bg-muted px-1.5 py-0.5 text-xs">{ds_type}</span>
                                        })}
                                        {(status == "synthesized").then(|| view! {
                                            <span class="inline-flex items-center rounded bg-purple-100 px-1.5 py-0.5 text-xs text-purple-700">"synthetic"</span>
                                        })}
                                    </div>
                                    {move || dataset_sample_count.get().map(|count| view! {
                                        <p class="text-xs text-muted-foreground mt-1">{format!("{} training samples", count)}</p>
                                    })}
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    // Fallback: show dataset_message while info loads
                    let msg = dataset_message.get().unwrap_or_else(|| {
                        format!("Dataset: {}", dataset_id.get())
                    });
                    view! {
                        <div class="rounded-lg border border-status-success/50 bg-status-success/5 p-4 text-center">
                            <svg class="w-5 h-5 mx-auto mb-1.5 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                            </svg>
                            <p class="text-sm font-medium">{msg}</p>
                        </div>
                    }.into_any()
                }
            }}

            // Secondary actions
            <div class="grid gap-3 sm:grid-cols-2">
                <Card>
                    <button
                        class="w-full p-4 text-left hover:bg-muted/50 transition-colors rounded-lg"
                        on:click=move |_| generate_wizard_open.set(true)
                    >
                        <div class="flex items-start gap-3">
                            <div class="rounded-full bg-primary/10 p-2 shrink-0">
                                <svg class="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/>
                                </svg>
                            </div>
                            <div>
                                <h4 class="text-sm font-medium">"Synthesize More Data"</h4>
                                <p class="text-xs text-muted-foreground mt-0.5">
                                    "Generate additional Q&A pairs from a document to augment this dataset"
                                </p>
                            </div>
                        </div>
                    </button>
                </Card>

                <Card>
                    <button
                        class="w-full p-4 text-left hover:bg-muted/50 transition-colors rounded-lg"
                        on:click=move |_| show_change_options.update(|v| *v = !*v)
                    >
                        <div class="flex items-start gap-3">
                            <div class="rounded-full bg-muted p-2 shrink-0">
                                <svg class="w-4 h-4 text-muted-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                                </svg>
                            </div>
                            <div>
                                <h4 class="text-sm font-medium">"Change Dataset"</h4>
                                <p class="text-xs text-muted-foreground mt-0.5">
                                    "Upload or generate a different dataset instead"
                                </p>
                            </div>
                        </div>
                    </button>
                </Card>
            </div>

            // Expandable: change dataset options
            <Show when=move || show_change_options.try_get().unwrap_or(false)>
                <div class="border-t pt-4 space-y-4">
                    <div class="grid gap-4 sm:grid-cols-2">
                        <Card>
                            <button
                                class="w-full p-4 text-left hover:bg-muted/50 transition-colors rounded-lg"
                                on:click=move |_| {
                                    dataset_id.set(String::new());
                                    dataset_message.set(None);
                                    dataset_sample_count.set(None);
                                    dataset_wizard_open.set(true);
                                    show_change_options.set(false);
                                }
                            >
                                <div class="flex items-start gap-3">
                                    <div class="rounded-full bg-primary/10 p-2 shrink-0">
                                        <svg class="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                        </svg>
                                    </div>
                                    <div>
                                        <h4 class="text-sm font-medium">"Upload Dataset"</h4>
                                        <p class="text-xs text-muted-foreground mt-0.5">"JSONL, CSV, or text files"</p>
                                    </div>
                                </div>
                            </button>
                        </Card>
                        <Card>
                            <button
                                class="w-full p-4 text-left hover:bg-muted/50 transition-colors rounded-lg"
                                on:click=move |_| {
                                    dataset_id.set(String::new());
                                    dataset_message.set(None);
                                    dataset_sample_count.set(None);
                                    generate_wizard_open.set(true);
                                    show_change_options.set(false);
                                }
                            >
                                <div class="flex items-start gap-3">
                                    <div class="rounded-full bg-primary/10 p-2 shrink-0">
                                        <svg class="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/>
                                        </svg>
                                    </div>
                                    <div>
                                        <h4 class="text-sm font-medium">"Generate from Document"</h4>
                                        <p class="text-xs text-muted-foreground mt-0.5">"Create Q&A pairs from text files"</p>
                                    </div>
                                </div>
                            </button>
                        </Card>
                    </div>
                    <div>
                        <p class="text-xs text-muted-foreground mb-2">"Or enter a dataset ID manually:"</p>
                        <FormField
                            label="Dataset ID"
                            name="dataset_id"
                            help="Use an existing dataset ID if you already have one"
                            error=Some(dataset_error)
                        >
                            <Input
                                value=dataset_id
                                placeholder="ds-abc123".to_string()
                                on_blur=Some(Callback::new(move |_| {
                                    let dataset_rules = [ValidationRule::Pattern {
                                        pattern: r"^\s*\S.*$",
                                        message: "Select or generate a dataset before continuing",
                                    }];
                                    let dataset = dataset_id.get();
                                    let _ = validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state);
                                }))
                            />
                        </FormField>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Shown when no dataset is selected — original upload/generate flow
#[component]
fn DatasetChooseView(
    dataset_id: RwSignal<String>,
    dataset_message: RwSignal<Option<String>>,
    dataset_wizard_open: RwSignal<bool>,
    generate_wizard_open: RwSignal<bool>,
    form_state: RwSignal<FormState>,
) -> impl IntoView {
    let dataset_error = use_field_error(form_state, "dataset_id");

    view! {
        <div class="space-y-6">
            <div class="text-center py-4">
                <h3 class="heading-4 mb-2">"Choose your training data"</h3>
                <p class="text-sm text-muted-foreground">
                    "Select how you want to provide training data."
                </p>
            </div>

            // Dataset ready message (from a just-completed sub-wizard)
            {move || dataset_message.try_get().flatten().map(|msg| view! {
                <div class="rounded-lg border border-status-success/50 bg-status-success/10 p-4 text-center">
                    <svg class="w-6 h-6 mx-auto mb-2 text-status-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                    </svg>
                    <p class="text-sm font-medium">{msg}</p>
                </div>
            })}

            // Option cards
            <div class="grid gap-4 sm:grid-cols-2">
                <Card>
                    <button
                        class="w-full p-6 text-left hover:bg-muted/50 transition-colors rounded-lg"
                        on:click=move |_| dataset_wizard_open.set(true)
                    >
                        <div class="flex items-start gap-4">
                            <div class="rounded-full bg-primary/10 p-3">
                                <svg class="w-6 h-6 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/>
                                </svg>
                            </div>
                            <div>
                                <h4 class="font-medium">"Upload Dataset"</h4>
                                <p class="text-sm text-muted-foreground mt-1">
                                    "Upload JSONL, CSV, or text files with your training examples"
                                </p>
                            </div>
                        </div>
                    </button>
                </Card>

                <Card>
                    <button
                        class="w-full p-6 text-left hover:bg-muted/50 transition-colors rounded-lg"
                        on:click=move |_| generate_wizard_open.set(true)
                    >
                        <div class="flex items-start gap-4">
                            <div class="rounded-full bg-primary/10 p-3">
                                <svg class="w-6 h-6 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"/>
                                </svg>
                            </div>
                            <div>
                                <h4 class="font-medium">"Generate from Document"</h4>
                                <p class="text-sm text-muted-foreground mt-1">
                                    "Create Q&A pairs from your text or markdown files using local inference"
                                </p>
                            </div>
                        </div>
                    </button>
                </Card>
            </div>

            // Manual dataset ID
            <div class="pt-4 border-t">
                <p class="text-sm text-muted-foreground mb-3">"Or enter an existing dataset ID:"</p>
                <FormField
                    label="Dataset ID"
                    name="dataset_id"
                    help="Use an existing dataset ID if you already have one"
                    error=Some(dataset_error)
                >
                    <Input
                        value=dataset_id
                        placeholder="ds-abc123".to_string()
                        on_blur=Some(Callback::new(move |_| {
                            let dataset_rules = [ValidationRule::Pattern {
                                pattern: r"^\s*\S.*$",
                                message: "Select or generate a dataset before continuing",
                            }];
                            let dataset = dataset_id.get();
                            let _ = validate_on_blur("dataset_id", &dataset, &dataset_rules, form_state);
                        }))
                    />
                </FormField>
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

/// Step 2: Model selection
#[component]
fn ModelStepContent(
    adapter_name: RwSignal<String>,
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
    let adapter_name_error = use_field_error(form_state, "adapter_name");
    let base_model_error = use_field_error(form_state, "base_model_id");

    view! {
        <div class="space-y-6">
            <FormField
                label="Adapter Name"
                name="adapter_name"
                required=true
                help="A unique name for your trained adapter (letters, numbers, hyphens)"
                error=Some(adapter_name_error)
            >
                <Input
                    value=adapter_name
                    placeholder="my-code-adapter".to_string()
                    on_blur=Some(Callback::new(move |_| {
                        let adapter_name_rules = rules::adapter_name();
                        let value = adapter_name.get();
                        let _ = validate_on_blur("adapter_name", &value, &adapter_name_rules, form_state);
                    }))
                />
            </FormField>

            <FormField
                label="Base Model"
                name="base_model_id"
                required=true
                help="The foundation model to fine-tune"
                error=Some(base_model_error)
            >
                {move || {
                    if use_custom_model.try_get().unwrap_or(false) {
                        view! {
                            <div class="space-y-2">
                                <Input
                                    value=base_model_id
                                    placeholder="model-id".to_string()
                                    on_blur=Some(Callback::new(move |_| {
                                        let model_rules = [ValidationRule::Pattern {
                                            pattern: r"^\s*\S.*$",
                                            message: "Base model is required",
                                        }];
                                        let value = base_model_id.get();
                                        let _ = validate_on_blur("base_model_id", &value, &model_rules, form_state);
                                    }))
                                />
                                <button
                                    class="text-xs text-primary hover:underline"
                                    type="button"
                                    on:click=move |_| use_custom_model.set(false)
                                >
                                    "Back to model list"
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
                                                on_change=Some(Callback::new(move |selected: String| {
                                                    let model_rules = [ValidationRule::Pattern {
                                                        pattern: r"^\s*\S.*$",
                                                        message: "Base model is required",
                                                    }];
                                                    let _ = validate_on_blur("base_model_id", &selected, &model_rules, form_state);
                                                }))
                                            />
                                            {move || {
                                                let selected = base_model_id.get();
                                                coreml_ids.contains(&selected).then(|| view! {
                                                    <div class="rounded-md border border-warning/40 bg-warning/10 p-3">
                                                        <p class="text-xs text-warning-foreground">
                                                            "CoreML models do not support LoRA adapter training. Select an MLX model instead."
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
                label="Category"
                name="category"
                help="Categorize your adapter for easier discovery"
            >
                <Select
                    value=category
                    options=vec![
                        ("code".to_string(), "Code".to_string()),
                        ("framework".to_string(), "Framework".to_string()),
                        ("codebase".to_string(), "Codebase".to_string()),
                        ("docs".to_string(), "Documentation".to_string()),
                        ("domain".to_string(), "Domain".to_string()),
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
    let batch_size_error = use_field_error(form_state, "batch_size");
    let rank_error = use_field_error(form_state, "rank");
    let alpha_error = use_field_error(form_state, "alpha");

    view! {
        <div class="space-y-6">
            // Training preset selection with core parameters
            {if let Some(count) = sample_count {
                view! {
                    <TrainingConfigPresets
                        preset=training_preset
                        epochs=epochs
                        learning_rate=learning_rate
                        validation_split=validation_split
                        early_stopping=early_stopping
                        sample_count=count
                    />
                }.into_any()
            } else {
                view! {
                    <TrainingConfigPresets
                        preset=training_preset
                        epochs=epochs
                        learning_rate=learning_rate
                        validation_split=validation_split
                        early_stopping=early_stopping
                    />
                }.into_any()
            }}

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
                    "Advanced Options"
                </button>

                {move || show_advanced.try_get().unwrap_or(false).then(|| view! {
                    <div class="mt-4 space-y-6 pl-6">
                        // Batch size and LoRA config
                        <div>
                            <h4 class="text-sm font-medium mb-3">"LoRA & Batch Configuration"</h4>
                            <div class="grid gap-4 sm:grid-cols-3">
                                <FormField
                                    label="Batch Size"
                                    name="batch_size"
                                    required=true
                                    help="Examples per step (1-256)"
                                    error=Some(batch_size_error)
                                >
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                        on_blur=Some(Callback::new(move |_| {
                                            let batch_size_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = batch_size.get();
                                            let _ = validate_on_blur("batch_size", &value, &batch_size_rules, form_state);
                                        }))
                                    />
                                </FormField>
                                <FormField
                                    label="Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter dimension (4, 8, 16 typical)"
                                    error=Some(rank_error)
                                >
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                        on_blur=Some(Callback::new(move |_| {
                                            let rank_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 256 },
                                            ];
                                            let value = rank.get();
                                            let _ = validate_on_blur("rank", &value, &rank_rules, form_state);
                                        }))
                                    />
                                </FormField>
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=Some(alpha_error)
                                >
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                        on_blur=Some(Callback::new(move |_| {
                                            let alpha_rules = [
                                                ValidationRule::Required,
                                                ValidationRule::IntRange { min: 1, max: 512 },
                                            ];
                                            let value = alpha.get();
                                            let _ = validate_on_blur("alpha", &value, &alpha_rules, form_state);
                                        }))
                                    />
                                </FormField>
                            </div>
                        </div>

                        // Backend selection
                        <div>
                            <h4 class="text-sm font-medium mb-3">"Backend Selection"</h4>
                            <p class="text-xs text-muted-foreground mb-3">
                                "By default, the system automatically selects the best available backend."
                            </p>
                            <div class="grid gap-4 sm:grid-cols-2">
                                <FormField label="Preferred Backend" name="preferred_backend">
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
                                <FormField label="Backend Policy" name="backend_policy">
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
                                    <FormField label="Fallback Backend" name="coreml_fallback">
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
                <h3 class="heading-4">"Review Your Training Job"</h3>
                <p class="text-sm text-muted-foreground">
                    "Confirm the settings below before starting training"
                </p>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Adapter Name" value=adapter_name/>
                <ReviewRow label="Base Model" value=base_model_id/>
                <ReviewRow label="Dataset" value=if dataset_id.is_empty() { "None selected".to_string() } else { dataset_id }/>
                <ReviewRow label="Category" value=category/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Preset" value=preset_label.to_string()/>
                <ReviewRow label="Epochs" value=epochs/>
                <ReviewRow label="Learning Rate" value=learning_rate/>
                <ReviewRow label="Validation Split" value=val_split_display/>
                <ReviewRow label="Early Stopping" value=if early_stopping { "Enabled".to_string() } else { "Disabled".to_string() }/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Batch Size" value=batch_size/>
                <ReviewRow label="LoRA Rank" value=rank/>
                <ReviewRow label="LoRA Alpha" value=alpha/>
                <ReviewRow label="Backend" value=if backend == "auto" { "Auto (recommended)".to_string() } else { backend }/>
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
