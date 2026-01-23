//! Models page
//!
//! Model management with list view and status display.

use crate::api::client::{ApiClient, ModelListResponse, ModelStatusResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorDisplay, Spinner, SplitPanel, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Models management page
#[component]
pub fn Models() -> impl IntoView {
    // Selected model ID for detail panel
    let selected_model_id = RwSignal::new(None::<String>);

    // Fetch models
    let (models, refetch_models) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_models().await });

    let on_model_select = move |model_id: String| {
        selected_model_id.set(Some(model_id));
    };

    let on_close_detail = move || {
        selected_model_id.set(None);
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_model_id.get().is_some());

    view! {
        <div class="p-6 space-y-6">
            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Models"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Header
                            <div class="flex items-center justify-between">
                                <h1 class="text-3xl font-bold tracking-tight">"Models"</h1>
                                <div class="flex items-center gap-2">
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=Callback::new(move |_| refetch_models.run(()))
                                    >
                                        "Refresh"
                                    </Button>
                                </div>
                            </div>

                            // Model list
                            {move || {
                                match models.get() {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <div class="flex items-center justify-center py-12">
                                                <Spinner/>
                                            </div>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(data) => {
                                        view! {
                                            <ModelList
                                                models=data
                                                selected_id=selected_model_id
                                                on_select=on_model_select
                                            />
                                        }.into_any()
                                    }
                                    LoadingState::Error(e) => {
                                        view! {
                                            <ErrorDisplay
                                                error=e
                                                on_retry=refetch_models
                                            />
                                        }.into_any()
                                    }
                                }
                            }}
                        </div>
                    }
                }
                detail_panel=move || {
                    let model_id = selected_model_id.get().unwrap_or_default();
                    view! {
                        <ModelDetail
                            model_id=model_id
                            on_close=on_close_detail
                        />
                    }
                }
            />
        </div>
    }
}

/// Model list component
#[component]
fn ModelList(
    models: ModelListResponse,
    selected_id: RwSignal<Option<String>>,
    on_select: impl Fn(String) + Copy + Send + 'static,
) -> impl IntoView {
    if models.models.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="h-8 w-8 text-muted-foreground"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
                        </svg>
                    </div>
                    <p class="text-muted-foreground">"No models found."</p>
                    <p class="text-sm text-muted-foreground mt-1">"Import a model to get started."</p>
                </div>
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Format"</TableHead>
                        <TableHead>"Backend"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Status"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {models.models
                        .into_iter()
                        .map(|model| {
                            let model_id = model.id.clone();
                            let model_id_for_click = model_id.clone();

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.get().as_ref() == Some(&model_id)
                                    on:click=move |_| on_select(model_id_for_click.clone())
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{model.name.clone()}</p>
                                            <p class="text-xs text-muted-foreground font-mono">
                                                {model.id.clone().chars().take(8).collect::<String>()}"..."
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <Badge variant=BadgeVariant::Outline>
                                            {model.format.clone().unwrap_or_else(|| "unknown".to_string())}
                                        </Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm">
                                            {model.backend.clone().unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_size(model.size_bytes.unwrap_or(0))}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <ModelStatusBadge status=model.import_status.clone().unwrap_or_else(|| "ready".to_string())/>
                                    </TableCell>
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Model status badge
#[component]
fn ModelStatusBadge(status: String) -> impl IntoView {
    let (variant, label) = match status.to_lowercase().as_str() {
        "ready" | "imported" | "complete" => (BadgeVariant::Success, "Ready"),
        "loading" | "importing" => (BadgeVariant::Default, "Loading"),
        "unloaded" => (BadgeVariant::Secondary, "Unloaded"),
        "error" | "failed" => (BadgeVariant::Destructive, "Error"),
        _ => (BadgeVariant::Secondary, "Unknown"),
    };

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}

/// Model detail panel
#[component]
fn ModelDetail(model_id: String, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let model_id_for_fetch = model_id.clone();

    // Fetch model status
    let (model_status, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = model_id_for_fetch.clone();
        async move { client.get_model(&id).await }
    });

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="text-xl font-semibold">"Model Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            {move || {
                match model_status.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let model_id_clone = model_id.clone();
                        view! {
                            <ModelDetailContent model=data model_id=model_id_clone on_update=refetch/>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=refetch
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Model detail content
#[component]
fn ModelDetailContent(
    model: ModelStatusResponse,
    model_id: String,
    on_update: Callback<()>,
) -> impl IntoView {
    let status_variant = if model.is_loaded {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };

    let is_loaded = model.is_loaded;
    let model_id_load = model_id.clone();
    let model_id_unload = model_id.clone();
    let (loading, set_loading) = signal(false);

    // Load model handler
    let on_load = move |_| {
        let id = model_id_load.clone();
        let on_update = on_update.clone();
        set_loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.load_model(&id).await {
                Ok(_) => {
                    on_update.run(());
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to load model: {:?}", e).into());
                }
            }
            set_loading.set(false);
        });
    };

    // Unload model handler
    let on_unload = move |_| {
        let id = model_id_unload.clone();
        let on_update = on_update.clone();
        set_loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.unload_model(&id).await {
                Ok(_) => {
                    on_update.run(());
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to unload model: {:?}", e).into());
                }
            }
            set_loading.set(false);
        });
    };

    view! {
        // Status
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <Badge variant=status_variant>
                        {if model.is_loaded { "Loaded" } else { "Unloaded" }}
                    </Badge>
                    <div class="flex gap-2">
                        {move || {
                            if loading.get() {
                                view! { <Spinner /> }.into_any()
                            } else if is_loaded {
                                view! {
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=Callback::new(on_unload.clone())
                                    >
                                        "Unload"
                                    </Button>
                                }.into_any()
                            } else {
                                view! {
                                    <Button
                                        variant=ButtonVariant::Primary
                                        on_click=Callback::new(on_load.clone())
                                    >
                                        "Load"
                                    </Button>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>

                {model.error_message.clone().map(|err| view! {
                    <div class="rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{err}</p>
                    </div>
                })}
            </div>
        </Card>

        // Details
        <Card title="Details".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Model ID"</span>
                    <span class="font-mono text-xs">{model.model_id.clone()}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Name"</span>
                    <span class="font-medium">{model.model_name.clone()}</span>
                </div>
                {model.model_path.clone().map(|path| {
                    let path_display = path.clone();
                    view! {
                        <div class="flex justify-between">
                            <span class="text-muted-foreground">"Path"</span>
                            <span class="font-mono text-xs truncate max-w-truncate" title=path>{path_display}</span>
                        </div>
                    }
                })}
                {model.loaded_at.clone().map(|ts| view! {
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Loaded At"</span>
                        <span>{format_date(&ts)}</span>
                    </div>
                })}
            </div>
        </Card>

        // Memory usage
        {model.memory_usage_mb.map(|mem| {
            let uma_level = model.uma_pressure_level.clone();
            view! {
                <Card title="Resources".to_string() class="mt-4".to_string()>
                    <div class="grid gap-3 text-sm">
                        <div class="flex justify-between">
                            <span class="text-muted-foreground">"Memory Usage"</span>
                            <span class="font-medium">{format!("{} MB", mem)}</span>
                        </div>
                        {uma_level.map(|level| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"UMA Pressure"</span>
                                <span>{level}</span>
                            </div>
                        })}
                    </div>
                </Card>
            }
        })}
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Format byte size for display
fn format_size(bytes: i64) -> String {
    if bytes <= 0 {
        return "-".to_string();
    }

    let gb = bytes as f64 / 1_073_741_824.0;
    let mb = bytes as f64 / 1_048_576.0;

    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else if mb >= 1.0 {
        format!("{:.1} MB", mb)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a date string for display
fn format_date(date_str: &str) -> String {
    if date_str.len() >= 16 {
        format!("{} {}", &date_str[0..10], &date_str[11..16])
    } else {
        date_str.to_string()
    }
}
