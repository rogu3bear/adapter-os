//! Dataset generation wizard for creating training data from raw files.
//!
//! This wizard allows users to upload a text file (e.g., README.md) and
//! generate a training dataset using local AdapterOS inference with
//! configurable strategies (QA or Summary).

#[cfg(target_arch = "wasm32")]
use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, FormField, Input, Spinner};
use adapteros_api_types::training::{GenerateDatasetResponse, GeneratedSample};
use leptos::prelude::*;

/// Outcome returned when a dataset is successfully generated
#[derive(Clone, Debug)]
pub struct GenerateDatasetOutcome {
    /// ID of the created dataset
    pub dataset_id: String,
    /// Dataset version ID
    pub dataset_version_id: Option<String>,
    /// Number of samples generated
    pub sample_count: usize,
}

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
    #[allow(dead_code)]
    fn as_str(&self) -> &'static str {
        match self {
            GenerateStrategy::Qa => "qa",
            GenerateStrategy::Summary => "summary",
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
    on_generated: Callback<GenerateDatasetOutcome>,
) -> impl IntoView {
    // Form state
    let name = RwSignal::new(String::new());
    let strategy = RwSignal::new(GenerateStrategy::Qa);
    let chunk_size = RwSignal::new("2000".to_string());
    let max_tokens = RwSignal::new("512".to_string());

    // Generation state
    let generating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let preview = RwSignal::new(Vec::<GeneratedSample>::new());
    let result = RwSignal::new(None::<GenerateDatasetResponse>);

    // File selection tracking
    let file_name = RwSignal::new(None::<String>);

    // Reset state when dialog opens
    Effect::new(move || {
        if open.get() {
            name.set(String::new());
            strategy.set(GenerateStrategy::Qa);
            chunk_size.set("2000".to_string());
            max_tokens.set("512".to_string());
            generating.set(false);
            error.set(None);
            preview.set(Vec::new());
            result.set(None);
            file_name.set(None);
        }
    });

    // File upload handler (WASM only)
    #[cfg(target_arch = "wasm32")]
    let handle_file_select = {
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;

            let target = ev.target().unwrap();
            let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();

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

                    wasm_bindgen_futures::spawn_local(async move {
                        let client = ApiClient::new();

                        // Build FormData
                        let form_data = match web_sys::FormData::new() {
                            Ok(fd) => fd,
                            Err(_) => {
                                error.set(Some("Failed to create form data".to_string()));
                                generating.set(false);
                                return;
                            }
                        };

                        if let Err(_) = form_data.append_with_blob("file", &file) {
                            error.set(Some("Failed to attach file".to_string()));
                            generating.set(false);
                            return;
                        }

                        if !name_val.is_empty() {
                            let _ = form_data.append_with_str("name", &name_val);
                        }
                        let _ = form_data.append_with_str("strategy", strategy_val.as_str());
                        let _ = form_data.append_with_str("chunk_size", &chunk_size_val);
                        let _ = form_data.append_with_str("max_tokens", &max_tokens_val);

                        match client.generate_dataset(&form_data).await {
                            Ok(resp) => {
                                preview.set(resp.preview.clone());
                                result.set(Some(resp.clone()));
                                generating.set(false);
                                // Don't auto-inject - let user review and click "Use this dataset"
                            }
                            Err(e) => {
                                error.set(Some(format!("Generation failed: {}", e)));
                                generating.set(false);
                            }
                        }
                    });
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
            on_generated,
        );
    };

    let close = move |_: ()| {
        open.set(false);
    };

    view! {
        <Show when=move || open.get()>
            // Backdrop
            <div
                class="fixed inset-0 z-50 bg-black/80"
                on:click=move |_| close(())
            />

            // Modal
            <div class="fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2 w-full max-w-2xl max-h-[85vh] overflow-y-auto">
                <div class="glass-card rounded-lg border shadow-lg p-6 space-y-4">
                    // Header
                    <div class="flex items-center justify-between">
                        <div>
                            <h2 class="text-lg font-semibold">"Generate Dataset from File"</h2>
                            <p class="text-sm text-muted-foreground">
                                "Upload a text file to generate training data using local inference"
                            </p>
                        </div>
                        <button
                            class="rounded-sm opacity-70 hover:opacity-100"
                            on:click=move |_| close(())
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path d="M18 6 6 18"/>
                                <path d="m6 6 12 12"/>
                            </svg>
                        </button>
                    </div>

                    // Error message
                    {move || error.get().map(|e| view! {
                        <div class="rounded-lg border border-destructive bg-destructive/10 p-3">
                            <p class="text-sm text-destructive">{e}</p>
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

                        <div class="grid grid-cols-3 gap-4">
                            <div class="space-y-2">
                                <label class="text-sm font-medium">"Strategy"</label>
                                <select
                                    class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm disabled:opacity-50"
                                    disabled=move || generating.get()
                                    on:change=move |ev| {
                                        let value = event_target_value(&ev);
                                        strategy.set(match value.as_str() {
                                            "summary" => GenerateStrategy::Summary,
                                            _ => GenerateStrategy::Qa,
                                        });
                                    }
                                >
                                    <option value="qa" selected=move || strategy.get() == GenerateStrategy::Qa>
                                        "Q&A Pairs"
                                    </option>
                                    <option value="summary" selected=move || strategy.get() == GenerateStrategy::Summary>
                                        "Summaries"
                                    </option>
                                </select>
                                <p class="text-xs text-muted-foreground">
                                    {move || match strategy.get() {
                                        GenerateStrategy::Qa => "Generate question-answer pairs from text",
                                        GenerateStrategy::Summary => "Generate summary instruction-response pairs",
                                    }}
                                </p>
                            </div>

                            <FormField
                                label="Chunk Size"
                                name="chunk_size"
                                help="Characters per chunk (500-10000)"
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
                                help="Max tokens per generation"
                            >
                                <Input
                                    value=max_tokens
                                    input_type="number".to_string()
                                    disabled=generating.get()
                                />
                            </FormField>
                        </div>
                    </div>

                    // File upload
                    <div class="space-y-2">
                        <label class="text-sm font-medium">"Upload File"</label>
                        <input
                            type="file"
                            accept=".txt,.md,.markdown"
                            class="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                            disabled=move || generating.get()
                            on:change=handle_file_select.clone()
                        />
                        <p class="text-xs text-muted-foreground">
                            "Supported: .txt, .md, .markdown"
                        </p>
                    </div>

                    // Generating status
                    <Show when=move || generating.get()>
                        <div class="flex items-center gap-3 p-4 rounded-lg border bg-muted/30">
                            <Spinner/>
                            <div>
                                <p class="text-sm font-medium">"Generating samples..."</p>
                                <p class="text-xs text-muted-foreground">
                                    "This may take a few minutes depending on file size"
                                </p>
                            </div>
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
                        <div class="p-4 rounded-lg border border-green-600/50 bg-green-100/30 space-y-2">
                            <p class="text-sm font-medium text-foreground">
                                "Generated "
                                <span class="font-bold">{move || result.get().map(|r| r.sample_count).unwrap_or(0)}</span>
                                " samples using "
                                <span class="font-bold">{move || result.get().map(|r| r.total_tokens_used).unwrap_or(0)}</span>
                                " tokens"
                            </p>
                            <Show when=move || result.get().map(|r| r.failed_chunks > 0).unwrap_or(false)>
                                <p class="text-xs text-amber-600">
                                    {move || result.get().map(|r| r.failed_chunks).unwrap_or(0)}
                                    " chunks failed to generate"
                                </p>
                            </Show>
                            <p class="text-xs text-muted-foreground">
                                "Dataset ID: "
                                <code class="bg-muted px-1 rounded">{move || result.get().map(|r| r.dataset_id.clone()).unwrap_or_default()}</code>
                            </p>
                        </div>
                    </Show>

                    // Actions
                    <div class="flex justify-end gap-2 pt-4 border-t">
                        <Button
                            variant=ButtonVariant::Secondary
                            on_click=Callback::new(close.clone())
                        >
                            {move || if result.get().is_some() { "Close" } else { "Cancel" }}
                        </Button>
                        <Show when=move || result.get().is_some()>
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new({
                                    let on_generated = on_generated.clone();
                                    move |_: ()| {
                                        if let Some(r) = result.get() {
                                            on_generated.run(GenerateDatasetOutcome {
                                                dataset_id: r.dataset_id.clone(),
                                                dataset_version_id: r.dataset_version_id.clone(),
                                                sample_count: r.sample_count,
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
            </div>
        </Show>
    }
}
