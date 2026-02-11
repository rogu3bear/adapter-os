//! Stepped wizard for creating training jobs
//!
//! 4-step wizard flow:
//! 1. Dataset - Choose data source
//! 2. Model - Select base model
//! 3. Config - Training parameters
//! 4. Review - Summary and submit

use crate::api::error::format_structured_details;
use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Card, Dialog, DialogSize, FormField, Input};
use crate::pages::training::config_presets::{TrainingConfigPresets, TrainingPreset};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use crate::pages::training::generate_wizard::{GenerateDatasetOutcome, GenerateDatasetWizard};
use crate::signals::use_notifications;
use crate::validation::{rules, use_form_errors, validate_field, FormErrors, ValidationRule};
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

/// Step indicator component
#[component]
fn StepIndicator(current: WizardStep) -> impl IntoView {
    view! {
        <div class="flex items-center justify-center gap-2 mb-6">
            {STEPS.iter().enumerate().map(|(i, step)| {
                let is_current = *step == current;
                let is_complete = step.index() < current.index();
                let is_last = i == STEPS.len() - 1;

                view! {
                    <div class="flex items-center">
                        <div class=move || {
                            let base = "flex items-center justify-center w-8 h-8 rounded-full text-sm font-medium transition-colors";
                            if is_current {
                                format!("{} bg-primary text-primary-foreground", base)
                            } else if is_complete {
                                format!("{} bg-primary/20 text-primary", base)
                            } else {
                                format!("{} bg-muted text-muted-foreground", base)
                            }
                        }>
                            {if is_complete {
                                view! {
                                    <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                    </svg>
                                }.into_any()
                            } else {
                                view! { <span>{i + 1}</span> }.into_any()
                            }}
                        </div>
                        <span class=move || {
                            let base = "ml-2 text-sm hidden sm:inline";
                            if is_current {
                                format!("{} font-medium text-foreground", base)
                            } else {
                                format!("{} text-muted-foreground", base)
                            }
                        }>
                            {step.label()}
                        </span>
                        {(!is_last).then(|| view! {
                            <div class="w-8 h-px bg-border mx-3"/>
                        })}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
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
            if let Some(ds_id) = init_ds.get() {
                dataset_id.set(ds_id.clone());
                dataset_message.set(Some(format!("Using dataset: {}", ds_id)));
            }
        });
    }

    // Pre-populate from source document if provided
    if let Some(src_doc) = source_document_id {
        Effect::new(move || {
            if let Some(doc_id) = src_doc.get() {
                dataset_message.set(Some(format!(
                    "From document: {} — generate or upload a dataset",
                    doc_id
                )));
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

    // Wizard state
    let dataset_wizard_open = RwSignal::new(false);
    let generate_wizard_open = RwSignal::new(false);
    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let form_errors = use_form_errors();
    let notifications = use_notifications();

    let on_created_clone = on_created.clone();

    // Dataset callbacks
    let on_dataset_uploaded = {
        move |outcome: DatasetUploadOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            dataset_sample_count.set(Some(outcome.sample_count));
            dataset_message.set(Some(format!(
                "Dataset ready: {} ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    let on_dataset_generated = {
        move |outcome: GenerateDatasetOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            dataset_sample_count.set(Some(outcome.sample_count));
            dataset_message.set(Some(format!(
                "Generated dataset: {} ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    // Step validation
    let validate_dataset_step = move || -> bool {
        form_errors.update(|e| e.clear_all());
        if dataset_id.get().trim().is_empty() {
            form_errors.update(|e| {
                e.set(
                    "dataset_id",
                    "Select or generate a dataset before continuing".to_string(),
                )
            });
            return false;
        }
        true
    };

    let validate_model_step = {
        move || -> bool {
            form_errors.update(|e| e.clear_all());
            let mut valid = true;

            let name = adapter_name.get();
            if let Some(err) = validate_field(&name, &rules::adapter_name()) {
                form_errors.update(|e| e.set("adapter_name", err));
                valid = false;
            }

            let model = base_model_id.get();
            if model.trim().is_empty() {
                form_errors
                    .update(|e| e.set("base_model_id", "Base model is required".to_string()));
                valid = false;
            }

            valid
        }
    };

    let validate_config_step = {
        move || -> bool {
            form_errors.update(|e| e.clear_all());
            let mut valid = true;

            if let Some(err) = validate_field(
                &epochs.get(),
                &[
                    ValidationRule::Required,
                    ValidationRule::IntRange { min: 1, max: 1000 },
                ],
            ) {
                form_errors.update(|e| e.set("epochs", err));
                valid = false;
            }

            if let Some(err) = validate_field(&learning_rate.get(), &rules::learning_rate()) {
                form_errors.update(|e| e.set("learning_rate", err));
                valid = false;
            }

            // Validate validation_split is in range [0.0, 0.5]
            if let Ok(split) = validation_split.get().parse::<f32>() {
                if !(0.0..=0.5).contains(&split) {
                    form_errors.update(|e| {
                        e.set(
                            "validation_split",
                            "Validation split must be between 0 and 0.5".to_string(),
                        )
                    });
                    valid = false;
                }
            }

            if let Some(err) = validate_field(
                &batch_size.get(),
                &[
                    ValidationRule::Required,
                    ValidationRule::IntRange { min: 1, max: 256 },
                ],
            ) {
                form_errors.update(|e| e.set("batch_size", err));
                valid = false;
            }

            if let Some(err) = validate_field(
                &rank.get(),
                &[
                    ValidationRule::Required,
                    ValidationRule::IntRange { min: 1, max: 256 },
                ],
            ) {
                form_errors.update(|e| e.set("rank", err));
                valid = false;
            }

            if let Some(err) = validate_field(
                &alpha.get(),
                &[
                    ValidationRule::Required,
                    ValidationRule::IntRange { min: 1, max: 512 },
                ],
            ) {
                form_errors.update(|e| e.set("alpha", err));
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
            form_errors.update(|e| e.clear_all());
        }
    };

    // Submit handler
    let submit = {
        let on_created = on_created_clone.clone();
        let notifications = notifications.clone();
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

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::new();

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
        form_errors.update(|e| e.clear_all());
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
    };

    // Reset form when dialog closes
    let was_open = StoredValue::new(open.get_untracked());
    Effect::new(move || {
        let is_open = open.get();
        let prev = was_open.get_value();
        was_open.set_value(is_open);
        if prev && !is_open {
            reset_form();
        }
    });

    view! {
        <Dialog
            open=open
            title="New Training Job".to_string()
            description=Signal::derive(move || current_step.get().label().to_string()).get()
            size=DialogSize::Lg
            scrollable=true
        >
            <StepIndicator current=current_step.get()/>

            // Error message
            {move || error.get().map(|e| view! {
                <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                    <p class="text-sm text-destructive">{e}</p>
                </div>
            })}

            // Step content
            <div class="wizard-step-content min-w-0">
                {move || match current_step.get() {
                    WizardStep::Dataset => view! {
                        <DatasetStepContent
                            dataset_id=dataset_id
                            dataset_message=dataset_message
                            dataset_wizard_open=dataset_wizard_open
                            generate_wizard_open=generate_wizard_open
                        />
                    }.into_any(),
                    WizardStep::Model => view! {
                        <ModelStepContent
                            adapter_name=adapter_name
                            base_model_id=base_model_id
                            category=category
                            form_errors=form_errors
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
                            form_errors=form_errors
                            sample_count=dataset_sample_count.get()
                        />
                    }.into_any(),
                    WizardStep::Review => view! {
                        <ReviewStepContent
                            adapter_name=adapter_name.get()
                            base_model_id=base_model_id.get()
                            dataset_id=dataset_id.get()
                            category=category.get()
                            preset=training_preset.get()
                            epochs=epochs.get()
                            learning_rate=learning_rate.get()
                            validation_split=validation_split.get()
                            early_stopping=early_stopping.get()
                            batch_size=batch_size.get()
                            rank=rank.get()
                            alpha=alpha.get()
                            backend=preferred_backend.get()
                        />
                    }.into_any(),
                }}
            </div>

            // Embedded wizards (modals inside modal)
            <DatasetUploadWizard
                open=dataset_wizard_open
                on_complete=Callback::new(on_dataset_uploaded)
            />
            <GenerateDatasetWizard
                open=generate_wizard_open
                on_generated=Callback::new(on_dataset_generated)
            />

            // Footer navigation
            <div class="flex justify-between mt-6 pt-4 border-t">
                <div>
                    {move || (current_step.get() != WizardStep::Dataset).then(|| view! {
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(go_back)
                        >
                            "Back"
                        </Button>
                    })}
                </div>
                <div class="flex gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| open.set(false))
                    >
                        "Cancel"
                    </Button>
                    {move || if current_step.get() == WizardStep::Review {
                        view! {
                            <Button
                                variant=ButtonVariant::Primary
                                loading=submitting.get()
                                on_click=Callback::new(submit.clone())
                            >
                                "Start Training"
                            </Button>
                        }.into_any()
                    } else {
                        view! {
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(go_next)
                            >
                                "Next"
                            </Button>
                        }.into_any()
                    }}
                </div>
            </div>
        </Dialog>
    }
}

/// Step 1: Dataset selection
#[component]
fn DatasetStepContent(
    dataset_id: RwSignal<String>,
    dataset_message: RwSignal<Option<String>>,
    dataset_wizard_open: RwSignal<bool>,
    generate_wizard_open: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <div class="text-center py-4">
                <h3 class="heading-4 mb-2">"Choose your training data"</h3>
                <p class="text-sm text-muted-foreground">
                    "Select how you want to provide training data. You can also skip this step to use synthetic data."
                </p>
            </div>

            // Dataset ready message
            {move || dataset_message.get().map(|msg| view! {
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
                <Input
                    value=dataset_id
                    placeholder="ds-abc123".to_string()
                />
            </div>
        </div>
    }
}

/// Step 2: Model selection
#[component]
fn ModelStepContent(
    adapter_name: RwSignal<String>,
    base_model_id: RwSignal<String>,
    category: RwSignal<String>,
    form_errors: RwSignal<FormErrors>,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <FormField
                label="Adapter Name"
                name="adapter_name"
                required=true
                help="A unique name for your trained adapter (letters, numbers, hyphens)"
                error=Signal::derive(move || form_errors.get().get("adapter_name").cloned())
            >
                <Input
                    value=adapter_name
                    placeholder="my-code-adapter".to_string()
                />
            </FormField>

            <FormField
                label="Base Model"
                name="base_model_id"
                required=true
                help="The foundation model to fine-tune"
                error=Signal::derive(move || form_errors.get().get("base_model_id").cloned())
            >
                <Input
                    value=base_model_id
                    placeholder="qwen2.5-coder-base".to_string()
                />
            </FormField>

            <div class="space-y-2">
                <label class="text-sm font-medium">"Category"</label>
                <select
                    class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                    prop:value=Signal::derive(move || category.get())
                    on:change=move |ev| category.set(event_target_value(&ev))
                >
                    <option value="code">"Code"</option>
                    <option value="framework">"Framework"</option>
                    <option value="codebase">"Codebase"</option>
                    <option value="docs">"Documentation"</option>
                    <option value="domain">"Domain"</option>
                </select>
                <p class="text-xs text-muted-foreground">
                    "Categorize your adapter for easier discovery"
                </p>
            </div>
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
    form_errors: RwSignal<FormErrors>,
    sample_count: Option<usize>,
) -> impl IntoView {
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
                        class=move || if show_advanced.get() { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced Options"
                </button>

                {move || show_advanced.get().then(|| view! {
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
                                    error=Signal::derive(move || form_errors.get().get("batch_size").cloned())
                                >
                                    <Input value=batch_size input_type="number".to_string()/>
                                </FormField>
                                <FormField
                                    label="Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter dimension (4, 8, 16 typical)"
                                    error=Signal::derive(move || form_errors.get().get("rank").cloned())
                                >
                                    <Input value=rank input_type="number".to_string()/>
                                </FormField>
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=Signal::derive(move || form_errors.get().get("alpha").cloned())
                                >
                                    <Input value=alpha input_type="number".to_string()/>
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
                                    </select>
                                </div>
                                <div class="space-y-2">
                                    <label class="text-sm font-medium">"Backend Policy"</label>
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || backend_policy.get())
                                        on:change=move |ev| backend_policy.set(event_target_value(&ev))
                                    >
                                        <option value="auto">"Auto"</option>
                                        <option value="coreml_only">"CoreML Only"</option>
                                        <option value="coreml_else_fallback">"CoreML with Fallback"</option>
                                    </select>
                                </div>
                            </div>
                            {move || (preferred_backend.get() == "coreml" || backend_policy.get() == "coreml_else_fallback").then(|| view! {
                                <div class="mt-4 space-y-2">
                                    <label class="text-sm font-medium">"Fallback Backend"</label>
                                    <select
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                        prop:value=Signal::derive(move || coreml_fallback.get())
                                        on:change=move |ev| coreml_fallback.set(event_target_value(&ev))
                                    >
                                        <option value="mlx">"MLX"</option>
                                        <option value="metal">"Metal"</option>
                                    </select>
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
