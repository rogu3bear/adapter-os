//! Stepped wizard for creating training jobs
//!
//! 4-step wizard flow:
//! 1. Dataset - Choose data source
//! 2. Model - Select base model
//! 3. Config - Training parameters
//! 4. Review - Summary and submit

use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Card, FormField, Input};
use crate::pages::training::dataset_wizard::{DatasetUploadOutcome, DatasetUploadWizard};
use crate::pages::training::generate_wizard::{GenerateDatasetOutcome, GenerateDatasetWizard};
use crate::signals::use_notifications;
use crate::validation::{rules, use_form_errors, validate_field, FormErrors, ValidationRule};
use adapteros_api_types::TrainingJobResponse;
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
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    // Wizard step state
    let current_step = RwSignal::new(WizardStep::default());

    // Form state - persists across steps
    let adapter_name = RwSignal::new(String::new());
    let base_model_id = RwSignal::new(String::new());
    let dataset_id = RwSignal::new(String::new());
    let dataset_message = RwSignal::new(None::<String>);
    let category = RwSignal::new("code".to_string());

    // Training parameters
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());

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
        let dataset_id = dataset_id.clone();
        let dataset_message = dataset_message.clone();
        move |outcome: DatasetUploadOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            dataset_message.set(Some(format!(
                "Dataset ready: {} ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    let on_dataset_generated = {
        let dataset_id = dataset_id.clone();
        let dataset_message = dataset_message.clone();
        move |outcome: GenerateDatasetOutcome| {
            dataset_id.set(outcome.dataset_id.clone());
            dataset_message.set(Some(format!(
                "Generated dataset: {} ({} samples)",
                outcome.dataset_id, outcome.sample_count
            )));
        }
    };

    // Step validation
    let validate_dataset_step = move || -> bool {
        // Dataset step is optional - user can proceed without a dataset
        true
    };

    let validate_model_step = {
        let base_model_id = base_model_id.clone();
        let adapter_name = adapter_name.clone();
        let form_errors = form_errors.clone();
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
        let epochs = epochs.clone();
        let learning_rate = learning_rate.clone();
        let batch_size = batch_size.clone();
        let rank = rank.clone();
        let alpha = alpha.clone();
        let form_errors = form_errors.clone();
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

                let request = json!({
                    "adapter_name": name,
                    "base_model_id": model,
                    "config": {
                        "rank": rank_val,
                        "alpha": alpha_val,
                        "targets": ["q_proj", "v_proj"],
                        "epochs": epochs_val,
                        "learning_rate": lr_val,
                        "batch_size": batch_val,
                        "preferred_backend": if backend_val == "auto" { serde_json::Value::Null } else { json!(backend_val) },
                        "backend_policy": if policy_val == "auto" { serde_json::Value::Null } else { json!(policy_val) },
                        "coreml_training_fallback": if backend_val == "coreml" || policy_val == "coreml_else_fallback" { json!(fallback_val) } else { serde_json::Value::Null },
                    },
                    "category": cat,
                    "dataset_id": if ds_id.is_empty() { serde_json::Value::Null } else { json!(ds_id) },
                    "synthetic_mode": ds_id.is_empty(),
                });

                match client
                    .post::<_, TrainingJobResponse>("/v1/training/jobs", &request)
                    .await
                {
                    Ok(_) => {
                        submitting.set(false);
                        notifications.success(
                            "Training job created",
                            &format!("\"{}\" is now queued for training", name_for_toast),
                        );
                        on_created();
                    }
                    Err(e) => {
                        error.set(Some(e.to_string()));
                        submitting.set(false);
                    }
                }
            });
        }
    };

    let close = move |_: ()| {
        open.set(false);
        current_step.set(WizardStep::default());
        error.set(None);
        form_errors.update(|e| e.clear_all());
        // Reset form state
        adapter_name.set(String::new());
        base_model_id.set(String::new());
        dataset_id.set(String::new());
        dataset_message.set(None);
        epochs.set("10".to_string());
        learning_rate.set("0.0001".to_string());
        batch_size.set("4".to_string());
        rank.set("8".to_string());
        alpha.set("16".to_string());
        preferred_backend.set("auto".to_string());
        backend_policy.set("auto".to_string());
        show_advanced_backend.set(false);
    };

    view! {
        {move || {
            if !open.get() {
                return view! {}.into_any();
            }

            let step = current_step.get();

            view! {
                // Backdrop
                <div
                    class="dialog-overlay"
                    on:click=move |_| close(())
                />

                // Dialog
                <div class="dialog-content dialog-scrollable">
                    // Header with step indicator
                    <div class="flex items-center justify-between mb-2">
                        <div>
                            <h2 class="text-lg font-semibold">"New Training Job"</h2>
                            <p class="text-sm text-muted-foreground">{step.label()}</p>
                        </div>
                        <button
                            class="rounded-sm opacity-70 hover:opacity-100"
                            aria-label="Close"
                            type="button"
                            on:click=move |_| close(())
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path d="M18 6 6 18"/><path d="m6 6 12 12"/>
                            </svg>
                        </button>
                    </div>

                    <StepIndicator current=step/>

                    // Error message
                    {move || error.get().map(|e| view! {
                        <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                            <p class="text-sm text-destructive">{e}</p>
                        </div>
                    })}

                    // Step content
                    <div class="min-h-[300px]">
                        {match step {
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
                                    epochs=epochs
                                    learning_rate=learning_rate
                                    batch_size=batch_size
                                    rank=rank
                                    alpha=alpha
                                    show_advanced=show_advanced_backend
                                    preferred_backend=preferred_backend
                                    backend_policy=backend_policy
                                    coreml_fallback=coreml_training_fallback
                                    form_errors=form_errors
                                />
                            }.into_any(),
                            WizardStep::Review => view! {
                                <ReviewStepContent
                                    adapter_name=adapter_name.get()
                                    base_model_id=base_model_id.get()
                                    dataset_id=dataset_id.get()
                                    category=category.get()
                                    epochs=epochs.get()
                                    learning_rate=learning_rate.get()
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
                        on_complete=Callback::new(on_dataset_uploaded.clone())
                    />
                    <GenerateDatasetWizard
                        open=generate_wizard_open
                        on_generated=Callback::new(on_dataset_generated.clone())
                    />

                    // Footer navigation
                    <div class="flex justify-between mt-6 pt-4 border-t">
                        <div>
                            {(step != WizardStep::Dataset).then(|| view! {
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
                                on_click=Callback::new(close.clone())
                            >
                                "Cancel"
                            </Button>
                            {if step == WizardStep::Review {
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
                </div>
            }.into_any()
        }}
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
                <h3 class="text-lg font-medium mb-2">"Choose your training data"</h3>
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

/// Step 3: Training configuration
#[component]
fn ConfigStepContent(
    epochs: RwSignal<String>,
    learning_rate: RwSignal<String>,
    batch_size: RwSignal<String>,
    rank: RwSignal<String>,
    alpha: RwSignal<String>,
    show_advanced: RwSignal<bool>,
    preferred_backend: RwSignal<String>,
    backend_policy: RwSignal<String>,
    coreml_fallback: RwSignal<String>,
    form_errors: RwSignal<FormErrors>,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <div>
                <h3 class="text-sm font-medium mb-4">"Training Parameters"</h3>
                <div class="grid gap-4 sm:grid-cols-2">
                    <FormField
                        label="Epochs"
                        name="epochs"
                        required=true
                        help="Number of training iterations (1-1000)"
                        error=Signal::derive(move || form_errors.get().get("epochs").cloned())
                    >
                        <Input value=epochs input_type="number".to_string()/>
                    </FormField>
                    <FormField
                        label="Learning Rate"
                        name="learning_rate"
                        required=true
                        help="Step size (0.0001-0.01 recommended)"
                        error=Signal::derive(move || form_errors.get().get("learning_rate").cloned())
                    >
                        <Input value=learning_rate input_type="number".to_string()/>
                    </FormField>
                    <FormField
                        label="Batch Size"
                        name="batch_size"
                        required=true
                        help="Examples per step (1-256)"
                        error=Signal::derive(move || form_errors.get().get("batch_size").cloned())
                    >
                        <Input value=batch_size input_type="number".to_string()/>
                    </FormField>
                </div>
            </div>

            <div class="border-t pt-4">
                <h3 class="text-sm font-medium mb-4">"LoRA Configuration"</h3>
                <div class="grid gap-4 sm:grid-cols-2">
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

            // Backend selection - collapsed by default
            <div class="border-t pt-4">
                <button
                    class="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| show_advanced.update(|v| *v = !*v)
                >
                    <svg
                        class=move || if show_advanced.get() { "w-4 h-4 transition-transform rotate-90" } else { "w-4 h-4 transition-transform" }
                        fill="none" viewBox="0 0 24 24" stroke="currentColor"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                    </svg>
                    "Advanced: Backend Selection"
                </button>

                {move || show_advanced.get().then(|| view! {
                    <div class="mt-4 space-y-4 pl-6">
                        <p class="text-xs text-muted-foreground">
                            "By default, the system automatically selects the best available backend (Auto)."
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
                            <div class="space-y-2">
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
    epochs: String,
    learning_rate: String,
    batch_size: String,
    rank: String,
    alpha: String,
    backend: String,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            <div class="text-center py-2">
                <h3 class="text-lg font-medium">"Review Your Training Job"</h3>
                <p class="text-sm text-muted-foreground">
                    "Confirm the settings below before starting training"
                </p>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Adapter Name" value=adapter_name/>
                <ReviewRow label="Base Model" value=base_model_id/>
                <ReviewRow label="Dataset" value=if dataset_id.is_empty() { "Synthetic (auto-generated)".to_string() } else { dataset_id }/>
                <ReviewRow label="Category" value=category/>
            </div>

            <div class="rounded-lg border bg-muted/30 divide-y">
                <ReviewRow label="Epochs" value=epochs/>
                <ReviewRow label="Learning Rate" value=learning_rate/>
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
        <div class="flex justify-between py-3 px-4">
            <span class="text-sm text-muted-foreground">{label}</span>
            <span class="text-sm font-medium">{value}</span>
        </div>
    }
}
