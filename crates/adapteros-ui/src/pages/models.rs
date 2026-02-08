//! Models page
//!
//! Model management with list view and status display.

use crate::api::{
    report_error_with_toast, AllModelsStatusResponse, ApiClient, ApiError, ModelLoadStatus,
    ModelStatusResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, CopyableId, ErrorDisplay, PageScaffold,
    PageScaffoldActions, Spinner, SplitPanel, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use crate::utils::format_datetime;
use leptos::prelude::*;
use std::sync::Arc;

/// Models management page
#[component]
pub fn Models() -> impl IntoView {
    // Selected model ID for detail panel
    let selected_model_id = RwSignal::new(None::<String>);

    // Fetch base model status list
    let (models, refetch_models) =
        use_api_resource(
            move |client: Arc<ApiClient>| async move { client.list_models_status().await },
        );

    let on_model_select = move |model_id: String| {
        selected_model_id.set(Some(model_id));
    };

    let on_close_detail = move || {
        selected_model_id.set(None);
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_model_id.get().is_some());

    view! {
        <PageScaffold title="Models">
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| refetch_models.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Models"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
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
                                        if matches!(&e, ApiError::Forbidden(_)) {
                                            view! {
                                                <Card>
                                                    <div class="py-6 px-4 text-sm text-muted-foreground">
                                                        "Base model status requires admin permissions."
                                                    </div>
                                                </Card>
                                            }
                                            .into_any()
                                        } else {
                                            view! {
                                                <ErrorDisplay
                                                    error=e
                                                    on_retry=refetch_models.as_callback()
                                                />
                                            }
                                            .into_any()
                                        }
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
        </PageScaffold>
    }
}

/// Model list component
#[component]
fn ModelList(
    models: AllModelsStatusResponse,
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
                    <p class="text-sm text-muted-foreground mt-1">
                        "Models must be registered, then loaded into a worker to enable chat/inference."
                    </p>
                    <p class="text-xs text-muted-foreground mt-2">
                        "If you expected to see models here, check "
                        <a class="link link-default" href="/system">"System"</a>
                        " for readiness and "
                        <a class="link link-default" href="/workers">"Workers"</a>
                        " for a connected worker."
                    </p>
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
                        <TableHead>"Model"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Load State"</TableHead>
                        <TableHead>"Memory"</TableHead>
                        <TableHead>"Loaded At"</TableHead>
                        <TableHead>"Updated"</TableHead>
                        <TableHead>"Error"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {models.models
                        .into_iter()
                        .map(|model| {
                            let model_id = model.model_id.clone();
                            let model_id_for_click = model_id.clone();

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.get().as_ref() == Some(&model_id)
                                    on:click=move |_| on_select(model_id_for_click.clone())
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{model.model_name.clone()}</p>
                                            <p class="text-xs text-muted-foreground font-mono">
                                                {model.model_id.clone().chars().take(8).collect::<String>()}"..."
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <ModelStatusBadge status=model.status/>
                                    </TableCell>
                                    <TableCell>
                                        <LoadStateBadge loaded=model.is_loaded/>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {model
                                                .memory_usage_mb
                                                .map(|m| format!("{} MB", m))
                                                .unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {model.loaded_at.clone().map(|ts| format_datetime(&ts)).unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_datetime(&model.updated_at)}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class=if model.error_message.is_some() { "text-sm text-destructive truncate max-w-60" } else { "text-sm text-muted-foreground" } title=model.error_message.clone().unwrap_or_default()>
                                            {model.error_message.clone().unwrap_or_else(|| "-".to_string())}
                                        </span>
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
fn ModelStatusBadge(status: ModelLoadStatus) -> impl IntoView {
    let (variant, label) = model_status_label(status);

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}

/// Model load state badge
#[component]
fn LoadStateBadge(loaded: bool) -> impl IntoView {
    let (variant, label) = if loaded {
        (BadgeVariant::Success, "Loaded")
    } else {
        (BadgeVariant::Secondary, "Unloaded")
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
                <h2 class="heading-3">"Model Details"</h2>
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
                                on_retry=refetch.as_callback()
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
    on_update: Refetch,
) -> impl IntoView {
    let status_variant = if model.is_loaded {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };
    let status_label = model_status_label(model.status);

    let is_loaded = model.is_loaded;
    let model_id_load = model_id.clone();
    let model_id_unload = model_id.clone();
    let (loading, set_loading) = signal(false);

    // Load model handler
    let on_load = move |_| {
        let id = model_id_load.clone();
        let on_update = on_update;
        set_loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.load_model(&id).await {
                Ok(_) => {
                    on_update.run(());
                }
                Err(e) => {
                    report_error_with_toast(&e, "Failed to load model", Some("/models"), true);
                }
            }
            set_loading.set(false);
        });
    };

    // Unload model handler
    let on_unload = move |_| {
        let id = model_id_unload.clone();
        let on_update = on_update;
        set_loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.unload_model(&id).await {
                Ok(_) => {
                    on_update.run(());
                }
                Err(e) => {
                    report_error_with_toast(&e, "Failed to unload model", Some("/models"), true);
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
                    <div class="flex items-center gap-2">
                        <Badge variant=status_variant>
                            {if model.is_loaded { "Loaded" } else { "Unloaded" }}
                        </Badge>
                        <Badge variant=status_label.0>
                            {status_label.1}
                        </Badge>
                    </div>
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

                {if matches!(model.status, ModelLoadStatus::Loading) {
                    Some(view! {
                        <p class="text-xs text-muted-foreground">
                            "Model loading. Inference will be ready once loading completes."
                        </p>
                    })
                } else if !model.is_loaded {
                    Some(view! {
                        <p class="text-xs text-muted-foreground">
                            "Load makes this model active in memory on a worker so chat/inference can run."
                        </p>
                    })
                } else {
                    None
                }}

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
                <CopyableId id=model.model_id.clone() label="Model ID".to_string() truncate=24 />
                <div class="flex justify-between">
                    <span class="text-muted-foreground">"Name"</span>
                    <span class="font-medium">{model.model_name.clone()}</span>
                </div>
                {model.model_path.clone().map(|path| {
                    let path_display = path.clone();
                    view! {
                        <div class="flex justify-between">
                            <span class="text-muted-foreground">"Path"</span>
                            <span class="font-mono text-xs truncate max-w-60" title=path>{path_display}</span>
                        </div>
                    }
                })}
                {model.loaded_at.clone().map(|ts| view! {
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Loaded At"</span>
                        <span>{format_datetime(&ts)}</span>
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

fn model_status_label(status: ModelLoadStatus) -> (BadgeVariant, &'static str) {
    match status {
        ModelLoadStatus::Ready => (BadgeVariant::Success, "Ready"),
        ModelLoadStatus::Loading => (BadgeVariant::Default, "Loading"),
        ModelLoadStatus::Unloading => (BadgeVariant::Default, "Unloading"),
        ModelLoadStatus::Checking => (BadgeVariant::Default, "Checking"),
        ModelLoadStatus::Error => (BadgeVariant::Destructive, "Error"),
        ModelLoadStatus::NoModel => (BadgeVariant::Secondary, "Unloaded"),
    }
}
