//! Models page
//!
//! Model management with list view and status display.

use crate::api::{
    report_error_with_toast, AllModelsStatusResponse, ApiClient, ApiError, BaseModelStatusResponse,
    ModelLoadStatus, ModelStatusResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, CopyableId, ErrorDisplay, LoadingDisplay,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, SkeletonTable, Spinner, SplitPanel,
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState, Refetch};
use crate::utils::format_datetime;
use leptos::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

/// Models management page
#[component]
pub fn Models() -> impl IntoView {
    // Selected model ID for detail panel
    let selected_model_id = RwSignal::new(None::<String>);

    // Fetch base model status list (models with load status)
    let (models_status, refetch_models_status) = use_api_resource(
        move |client: Arc<ApiClient>| async move { client.list_models_status().await },
    );

    // Also fetch registered models (may include models not yet loaded)
    let (registered_models, refetch_registered) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_models().await });

    let refetch_all = move |()| {
        refetch_models_status.run(());
        refetch_registered.run(());
    };

    // Merge status data with registered models: if a registered model has no
    // status entry, synthesize one with NoModel status so it appears in the list.
    let merged_models = Signal::derive(move || {
        let status_state = models_status.try_get().unwrap_or(LoadingState::Loading);
        let registered_state = registered_models.try_get().unwrap_or(LoadingState::Loading);

        match status_state {
            LoadingState::Idle | LoadingState::Loading => LoadingState::Loading,
            LoadingState::Error(e) => {
                // If status endpoint failed but registered models loaded,
                // still show registered models with unknown status
                if let LoadingState::Loaded(reg) = registered_state {
                    if !reg.models.is_empty() {
                        let models: Vec<BaseModelStatusResponse> = reg
                            .models
                            .iter()
                            .map(|m| BaseModelStatusResponse {
                                model_id: m.id.clone(),
                                model_name: m.name.clone(),
                                model_path: None,
                                status: ModelLoadStatus::NoModel,
                                loaded_at: None,
                                unloaded_at: None,
                                error_message: None,
                                memory_usage_mb: None,
                                is_loaded: false,
                                updated_at: m.updated_at.clone().unwrap_or_default(),
                            })
                            .collect();
                        return LoadingState::Loaded(AllModelsStatusResponse {
                            schema_version: String::new(),
                            active_model_count: 0,
                            total_memory_mb: 0,
                            available_memory_mb: None,
                            models,
                        });
                    }
                }
                LoadingState::Error(e)
            }
            LoadingState::Loaded(mut data) => {
                // Merge registered models that have no status entry
                if let LoadingState::Loaded(reg) = registered_state {
                    let known_ids: HashSet<String> =
                        data.models.iter().map(|m| m.model_id.clone()).collect();
                    for m in &reg.models {
                        if !known_ids.contains(&m.id) {
                            data.models.push(BaseModelStatusResponse {
                                model_id: m.id.clone(),
                                model_name: m.name.clone(),
                                model_path: None,
                                status: ModelLoadStatus::NoModel,
                                loaded_at: None,
                                unloaded_at: None,
                                error_message: None,
                                memory_usage_mb: None,
                                is_loaded: false,
                                updated_at: m.updated_at.clone().unwrap_or_default(),
                            });
                        }
                    }
                }
                LoadingState::Loaded(data)
            }
        }
    });

    let on_model_select = move |model_id: String| {
        selected_model_id.set(Some(model_id));
    };

    let on_close_detail = move || {
        selected_model_id.set(None);
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_model_id.get().is_some());

    view! {
        <PageScaffold
            title="Models"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Deploy", "/models"),
                PageBreadcrumbItem::current("Models"),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| refetch_all(()))
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
                                match merged_models.try_get().unwrap_or(LoadingState::Loading) {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <SkeletonTable rows=5 columns=4/>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(mut data) => {
                                        data.models.sort_by_key(|m| match m.status {
                                            ModelLoadStatus::Ready => 0,
                                            ModelLoadStatus::Loading => 1,
                                            ModelLoadStatus::Checking => 2,
                                            ModelLoadStatus::Unloading => 3,
                                            ModelLoadStatus::NoModel => 4,
                                            ModelLoadStatus::Error => 5,
                                        });
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
                                                    on_retry=Callback::new(move |_| refetch_all(()))
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
                    let model_id = selected_model_id.try_get().flatten().unwrap_or_default();
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
        // Check if the system reports active models despite empty list (worker connected, model pending)
        let has_active_context = models.active_model_count > 0 || models.total_memory_mb > 0;

        return if has_active_context {
            view! {
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
                        <p class="text-muted-foreground">"Worker connected, model pending registration"</p>
                        <p class="text-sm text-muted-foreground mt-1">
                            "A worker is active but no model has been registered yet. Seed a model with "
                            <code class="font-mono text-xs bg-muted px-1 py-0.5 rounded">"aosctl models seed"</code>
                            " to begin."
                        </p>
                    </div>
                </Card>
            }
            .into_any()
        } else {
            view! {
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
                        <p class="text-muted-foreground">"No models registered."</p>
                        <p class="text-sm text-muted-foreground mt-1">
                            "Seed a model with "
                            <code class="font-mono text-xs bg-muted px-1 py-0.5 rounded">"aosctl models seed"</code>
                            ", then load it into a worker to enable inference."
                        </p>
                        <p class="text-xs text-muted-foreground mt-2">
                            "Check "
                            <a class="link link-default" href="/system">"System"</a>
                            " for readiness and "
                            <a class="link link-default" href="/workers">"Workers"</a>
                            " for a connected worker."
                        </p>
                    </div>
                </Card>
            }
            .into_any()
        };
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
                                    class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&model_id)
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
                match model_status.try_get().unwrap_or(LoadingState::Loading) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading model details..."/>
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
            let _ = set_loading.try_set(false);
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
            let _ = set_loading.try_set(false);
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
                            if loading.try_get().unwrap_or(false) {
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
