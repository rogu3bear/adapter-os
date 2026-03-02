//! Synthetic dataset generation wizard for creating training data from raw files.
//!
//! This wizard allows users to upload a text file (e.g., README.md) and
//! generate a training dataset using local adapterOS inference with
//! configurable strategies (QA or Summary).
//!
//! ## Features
//!
//! - Choose generation strategy (QA pairs or summaries)
//! - Configure target volume (number of examples)
//! - Optional seed prompts to guide generation
//! - Fixed seed for deterministic generation
//! - Provenance tracking (source model hash, generation receipts)

#[cfg(target_arch = "wasm32")]
use crate::api::error::format_structured_details;
use crate::api::use_api_client;
use crate::components::{
    Button, ButtonVariant, Dialog, DialogSize, FormField, InlineErrorBanner, Input, Select, Spinner,
};
use crate::pages::training::dataset_wizard::DatasetOutcome;
use crate::validation::{use_form_errors, validate_field, ValidationRule};
use adapteros_api_types::training::{GenerateDatasetResponse, GeneratedSample};
use leptos::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Generation strategy selection
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GenerateStrategy {
    /// Generate question-answer pairs
    #[default]
    Qa,
    /// Generate summary pairs
    Summary,
}

impl GenerateStrategy {
    #[cfg(target_arch = "wasm32")]
    fn as_str(&self) -> &'static str {
        match self {
            GenerateStrategy::Qa => "qa",
            GenerateStrategy::Summary => "summary",
        }
    }

    fn from_value(v: &str) -> Self {
        match v {
            "summary" => GenerateStrategy::Summary,
            _ => GenerateStrategy::Qa,
        }
    }
}

/// Wizard for generating datasets from uploaded files
#[component]
pub fn GenerateDatasetWizard(
    /// Signal controlling dialog visibility
    open: RwSignal<bool>,
    /// Callback when dataset generation completes successfully
    #[prop(into)]
    on_generated: Callback<DatasetOutcome>,
) -> impl IntoView {
    // Form state
    let name = RwSignal::new(String::new());
    let strategy = RwSignal::new(GenerateStrategy::Qa);
    let strategy_value = RwSignal::new("qa".to_string());
    let chunk_size = RwSignal::new("2000".to_string());
    let max_tokens = RwSignal::new("512".to_string());
    let target_volume = RwSignal::new(String::new()); // empty = all chunks
    let generation_seed = RwSignal::new(String::new()); // empty = non-deterministic
    let seed_prompts = RwSignal::new(String::new()); // newline-separated
    let show_advanced = RwSignal::new(false);

    // Generation state
    let generating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let preview = RwSignal::new(Vec::<GeneratedSample>::new());
    let result = RwSignal::new(None::<GenerateDatasetResponse>);

    // File selection tracking
    let file_name = RwSignal::new(None::<String>);

    // Validation
    let form_errors = use_form_errors();

    // Async safety: guard against setting signals after component unmount
    let is_active = Arc::new(AtomicBool::new(true));
    on_cleanup({
        let is_active = Arc::clone(&is_active);
        move || is_active.store(false, Ordering::Relaxed)
    });

    // Sync strategy string signal → strategy enum
    Effect::new(move || {
        let val = strategy_value.try_get().unwrap_or_default();
        let _ = strategy.try_set(GenerateStrategy::from_value(&val));
    });

    let reset_form = move || {
        name.set(String::new());
        strategy.set(GenerateStrategy::Qa);
        strategy_value.set("qa".to_string());
        chunk_size.set("2000".to_string());
        max_tokens.set("512".to_string());
        target_volume.set(String::new());
        generation_seed.set(String::new());
        seed_prompts.set(String::new());
        show_advanced.set(false);
        generating.set(false);
        error.set(None);
        preview.set(Vec::new());
        result.set(None);
        file_name.set(None);
        form_errors.update(|e| e.clear_all());
    };

    // Reset state on open→close transition (matches CreateJobWizard StoredValue pattern)
    let was_open = StoredValue::new(open.get_untracked());
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let prev = was_open.get_value();
        was_open.set_value(is_open);
        // Reset when dialog closes (was open, now closed)
        if prev && !is_open {
            reset_form();
        }
    });

    // Shared API client
    let _client = use_api_client();

    // Validate fields before generation
    let _validate_form = move || -> bool {
        form_errors.update(|e| e.clear_all());
        let mut valid = true;

        if let Some(err) = validate_field(
            &chunk_size.get(),
            &[
                ValidationRule::Required,
                ValidationRule::IntRange {
                    min: 500,
                    max: 10000,
                },
            ],
        ) {
            form_errors.update(|e| e.set("chunk_size", err));
            valid = false;
        }

        if let Some(err) = validate_field(
            &max_tokens.get(),
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 4096 },
            ],
        ) {
            form_errors.update(|e| e.set("max_tokens", err));
            valid = false;
        }

        // target_volume is optional, but if present validate range
        let tv = target_volume.get();
        if !tv.trim().is_empty() {
            if let Some(err) = validate_field(
                &tv,
                &[ValidationRule::IntRange {
                    min: 1,
                    max: 100000,
                }],
            ) {
                form_errors.update(|e| e.set("target_volume", err));
                valid = false;
            }
        }

        valid
    };

    // File upload handler (WASM only)
    #[cfg(target_arch = "wasm32")]
    let handle_file_select = {
        let is_active = Arc::clone(&is_active);
        let client = _client.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;

            let Some(target) = ev.target() else {
                tracing::error!("handle_file_select: no event target");
                return;
            };
            let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
                tracing::error!("handle_file_select: target is not an HtmlInputElement");
                return;
            };

            // Validate form before starting generation
            if !_validate_form() {
                input.set_value("");
                return;
            }

            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let selected_file_name = file.name();
                    file_name.set(Some(selected_file_name.clone()));
                    generating.set(true);
                    error.set(None);
                    preview.set(Vec::new());
                    result.set(None);

                    let name_val = name.get_untracked();
                    let strategy_val = strategy.get_untracked();
                    let chunk_size_val = chunk_size.get_untracked();
                    let max_tokens_val = max_tokens.get_untracked();
                    let target_volume_val = target_volume.get_untracked();
                    let generation_seed_val = generation_seed.get_untracked();
                    let seed_prompts_val = seed_prompts.get_untracked();
                    let is_active = Arc::clone(&is_active);
                    let client = client.clone();

                    wasm_bindgen_futures::spawn_local(async move {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }

                        // Build FormData
                        let form_data = match web_sys::FormData::new() {
                            Ok(fd) => fd,
                            Err(_) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ =
                                    error.try_set(Some("Failed to create form data".to_string()));
                                let _ = generating.try_set(false);
                                return;
                            }
                        };

                        if form_data.append_with_blob("file", &file).is_err() {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = error.try_set(Some("Failed to attach file".to_string()));
                            let _ = generating.try_set(false);
                            return;
                        }

                        if !name_val.is_empty() {
                            let _ = form_data.append_with_str("name", &name_val);
                        }
                        let _ = form_data.append_with_str("strategy", strategy_val.as_str());
                        let _ = form_data.append_with_str("chunk_size", &chunk_size_val);
                        let _ = form_data.append_with_str("max_tokens", &max_tokens_val);

                        // Add optional fields
                        if !target_volume_val.is_empty() {
                            let _ = form_data.append_with_str("target_volume", &target_volume_val);
                        }
                        if !generation_seed_val.is_empty() {
                            let _ =
                                form_data.append_with_str("generation_seed", &generation_seed_val);
                        }
                        if !seed_prompts_val.is_empty() {
                            let _ = form_data.append_with_str("seed_prompts", &seed_prompts_val);
                        }

                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }

                        match client.generate_dataset(&form_data).await {
                            Ok(resp) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ = preview.try_set(resp.preview.clone());
                                let _ = result.try_set(Some(resp.clone()));
                                let _ = generating.try_set(false);
                            }
                            Err(e) => {
                                if !is_active.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ = error.try_set(Some(format_structured_details(&e)));
                                let _ = generating.try_set(false);
                            }
                        }
                    });
                    input.set_value("");
                }
            }
        }
    };

    // No-op for non-WASM
    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_select = move |_ev: web_sys::Event| {
        let _ = (
            generating,
            error,
            preview,
            result,
            file_name,
            name,
            strategy,
            chunk_size,
            max_tokens,
            target_volume,
            generation_seed,
            seed_prompts,
            show_advanced,
            on_generated,
            form_errors,
            strategy_value,
        );
    };

    let close = move |_: ()| {
        open.set(false);
    };

    view! {
        <Dialog
            open=open
            title="Generate Dataset from File".to_string()
            description="Upload a text file to generate training data using local inference".to_string()
            size=DialogSize::Lg
            scrollable=true
        >
                <div class="space-y-4">

                    // Error banner (inline, matching CreateJobWizard pattern)
                    {move || error.try_get().flatten().map(|e| view! {
                        <div class="mb-4">
                            <InlineErrorBanner message=e/>
                        </div>
                    })}

                    // Configuration
                    <div class="space-y-4">
                        <FormField
                            label="Dataset Name"
                            name="name"
                            help="Optional - auto-generated from filename if empty"
                        >
                            <Input
                                value=name
                                placeholder="my-generated-dataset".to_string()
                                disabled=generating.get()
                            />
                        </FormField>

                        <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
                            <FormField
                                label="Strategy"
                                name="strategy"
                                help=Signal::derive(move || match strategy.try_get().unwrap_or_default() {
                                    GenerateStrategy::Qa => "Generate question-answer pairs from text".to_string(),
                                    GenerateStrategy::Summary => "Generate summary instruction-response pairs".to_string(),
                                }).get()
                            >
                                <Select
                                    value=strategy_value
                                    options=vec![
                                        ("qa".to_string(), "Q&A Pairs".to_string()),
                                        ("summary".to_string(), "Summaries".to_string()),
                                    ]
                                    disabled=Signal::derive(move || generating.try_get().unwrap_or(false))
                                />
                            </FormField>

                            <FormField
                                label="Chunk Size"
                                name="chunk_size"
                                help="Characters per chunk (500-10000)"
                                error=Signal::derive(move || form_errors.try_get().unwrap_or_default().get("chunk_size").cloned())
                            >
                                <Input
                                    value=chunk_size
                                    input_type="number".to_string()
                                    disabled=generating.get()
                                />
                            </FormField>

                            <FormField
                                label="Max Tokens"
                                name="max_tokens"
                                help="Max tokens per generation (1-4096)"
                                error=Signal::derive(move || form_errors.try_get().unwrap_or_default().get("max_tokens").cloned())
                            >
                                <Input
                                    value=max_tokens
                                    input_type="number".to_string()
                                    disabled=generating.get()
                                />
                            </FormField>
                        </div>

                        // Volume control
                        <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                            <FormField
                                label="Target Volume"
                                name="target_volume"
                                help="Number of examples to generate (empty = all chunks)"
                                error=Signal::derive(move || form_errors.try_get().unwrap_or_default().get("target_volume").cloned())
                            >
                                <Input
                                    value=target_volume
                                    input_type="number".to_string()
                                    placeholder="All chunks".to_string()
                                    disabled=generating.get()
                                />
                            </FormField>

                            <FormField
                                label="Generation Seed"
                                name="generation_seed"
                                help="Fixed seed for deterministic generation (optional)"
                            >
                                <Input
                                    value=generation_seed
                                    input_type="number".to_string()
                                    placeholder="Random".to_string()
                                    disabled=generating.get()
                                />
                            </FormField>
                        </div>

                        // Advanced: Seed prompts toggle
                        <div class="space-y-2">
                            <button
                                type="button"
                                class="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                                disabled=move || generating.get()
                                on:click=move |_| show_advanced.set(!show_advanced.get())
                            >
                                <svg
                                    xmlns="http://www.w3.org/2000/svg"
                                    width="16"
                                    height="16"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    class=move || if show_advanced.get() { "rotate-90 transition-transform" } else { "transition-transform" }
                                >
                                    <path d="m9 18 6-6-6-6"/>
                                </svg>
                                "Seed Prompts (Advanced)"
                            </button>

                            <Show when=move || show_advanced.get()>
                                <div class="space-y-2 pl-6">
                                    <FormField
                                        label="Seed Prompts"
                                        name="seed_prompts"
                                        help="Each line provides context for one chunk's generation"
                                    >
                                        <textarea
                                            class="flex min-h-24 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                                            placeholder="Enter seed prompts (one per line) to guide generation..."
                                            disabled=move || generating.get()
                                            prop:value=move || seed_prompts.get()
                                            on:input=move |ev| seed_prompts.set(event_target_value(&ev))
                                        />
                                    </FormField>
                                </div>
                            </Show>
                        </div>
                    </div>

                    // File upload
                    <div class="space-y-2">
                        <label for="generate-wizard-upload-file" class="text-sm font-medium">
                            "Upload File"
                        </label>
                        <input
                            id="generate-wizard-upload-file"
                            type="file"
                            accept=".txt,.md,.markdown"
                            class="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                            disabled=move || generating.get()
                            on:change=handle_file_select
                        />
                        <p class="text-xs text-muted-foreground">
                            "Supported: .txt, .md, .markdown"
                        </p>
                    </div>

                    // Generating status
                    <Show when=move || generating.get()>
                        <div class="flex items-center justify-between gap-3 p-4 rounded-lg border bg-muted/30">
                            <div class="flex items-center gap-3">
                                <Spinner/>
                                <div>
                                    <p class="text-sm font-medium">"Generating samples..."</p>
                                    <p class="text-xs text-muted-foreground">
                                        {move || file_name.try_get().flatten().map(|f|
                                            format!("Processing {} - this may take a few minutes depending on file size", f)
                                        ).unwrap_or_else(|| "This may take a few minutes depending on file size".to_string())}
                                    </p>
                                </div>
                            </div>
                            <Button
                                variant=ButtonVariant::Outline
                                on_click=Callback::new(move |_| {
                                    generating.set(false);
                                    error.set(Some("Generation cancelled by user".to_string()));
                                })
                            >
                                "Cancel"
                            </Button>
                        </div>
                    </Show>

                    // Preview table
                    <Show when=move || !preview.get().is_empty()>
                        <div class="space-y-2">
                            <h3 class="text-sm font-medium">
                                "Preview ("{move || preview.get().len()}" samples)"
                            </h3>
                            <div class="max-h-64 overflow-y-auto rounded-lg border">
                                <table class="w-full text-sm">
                                    <thead class="bg-muted/50 sticky top-0">
                                        <tr>
                                            <th class="p-2 text-left font-medium w-1/2">"Instruction"</th>
                                            <th class="p-2 text-left font-medium w-1/2">"Response"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <For
                                            each=move || preview.get()
                                            key=|s| s.source_chunk_index
                                            children=move |sample| {
                                                view! {
                                                    <tr class="border-t">
                                                        <td class="p-2 align-top">
                                                            <p class="line-clamp-3">{sample.instruction}</p>
                                                        </td>
                                                        <td class="p-2 align-top">
                                                            <p class="line-clamp-3">{sample.response}</p>
                                                        </td>
                                                    </tr>
                                                }
                                            }
                                        />
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    </Show>

                    // Result summary
                    <Show when=move || result.get().is_some()>
                        <div class="p-4 rounded-lg border border-status-success/50 bg-status-success/10 space-y-3">
                            <div class="flex items-center gap-2">
                                <span class="inline-flex items-center rounded-md bg-purple-100 px-2 py-1 text-xs font-medium text-purple-700">
                                    "Synthetic"
                                </span>
                                <p class="text-sm font-medium text-foreground">
                                    "Generated "
                                    <span class="font-bold">{move || result.get().map(|r| r.sample_count).unwrap_or(0)}</span>
                                    " samples using "
                                    <span class="font-bold">{move || result.get().map(|r| r.total_tokens_used).unwrap_or(0)}</span>
                                    " tokens"
                                </p>
                            </div>
                            <Show when=move || result.get().map(|r| r.failed_chunks > 0).unwrap_or(false)>
                                <p class="text-xs text-amber-600">
                                    {move || result.get().map(|r| r.failed_chunks).unwrap_or(0)}
                                    " chunks failed to generate"
                                </p>
                            </Show>
                            <div class="text-xs text-muted-foreground space-y-1">
                                <p>
                                    "Dataset ID: "
                                    <code class="bg-muted px-1 rounded">{move || result.get().map(|r| r.dataset_id.clone()).unwrap_or_default()}</code>
                                </p>
                                // Provenance info
                                <Show when=move || result.get().and_then(|r| r.source_model_hash.clone()).is_some()>
                                    <p>
                                        "Source Model: "
                                        <code class="bg-muted px-1 rounded">
                                            {move || result.get().and_then(|r| r.source_model_hash.clone()).map(|h| adapteros_id::format_hash_short(&h)).unwrap_or_default()}
                                        </code>
                                    </p>
                                </Show>
                                <Show when=move || result.get().map(|r| !r.generation_receipt_digests.is_empty()).unwrap_or(false)>
                                    <p>
                                        "Receipts collected: "
                                        <span class="font-medium">{move || result.get().map(|r| r.generation_receipt_digests.len()).unwrap_or(0)}</span>
                                    </p>
                                </Show>
                                <Show when=move || result.get().and_then(|r| r.generation_seed).is_some()>
                                    <p>
                                        "Deterministic seed: "
                                        <code class="bg-muted px-1 rounded">{move || result.get().and_then(|r| r.generation_seed).unwrap_or(0)}</code>
                                    </p>
                                </Show>
                            </div>
                        </div>
                    </Show>

                    // Actions
                    <div class="flex justify-end gap-2 pt-4 border-t">
                        <Button
                            variant=ButtonVariant::Secondary
                            on_click=Callback::new(close)
                        >
                            {move || if result.get().is_some() { "Close" } else { "Cancel" }}
                        </Button>
                        <Show when=move || result.get().is_some()>
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new({
                                    move |_: ()| {
                                        if let Some(r) = result.get() {
                                            on_generated.run(DatasetOutcome {
                                                dataset_id: r.dataset_id.clone(),
                                                dataset_version_id: r.dataset_version_id.clone(),
                                                sample_count: r.sample_count,
                                                is_synthetic: true,
                                                source_hash: r.source_model_hash.clone(),
                                                receipt_count: r.generation_receipt_digests.len(),
                                            });
                                            open.set(false);
                                        }
                                    }
                                })
                            >
                                "Use this dataset"
                            </Button>
                        </Show>
                    </div>
                </div>
        </Dialog>
    }
}
