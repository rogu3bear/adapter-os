//! Models page
//!
//! Model management with list view, status display, polling, and deep-link detail page.

use crate::api::{
    report_error_with_toast, AllModelsStatusResponse, ApiClient, ApiError,
    ModelArchitectureSummary, ModelListResponse, ModelLoadStatus, ModelStatusResponse,
    ModelWithStatsResponse, SeedModelRequest,
};
use crate::components::{
    AsyncBoundary, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, CopyableId, Dialog, ErrorDisplay, Input, ListEmptyCard, LoadingDisplay,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, Select, SkeletonTable, Spinner,
    SplitPanel, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api, use_api_resource, use_polling, LoadingState, Refetch};
use crate::signals::{
    try_use_route_context, use_auth, use_notifications, use_refetch_signal, RefetchTopic,
};
use crate::utils::{format_bytes, format_datetime};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Merged data types
// ============================================================================

/// Combined model data from runtime status + registered metadata.
#[derive(Clone, Debug)]
struct MergedModelRow {
    // Runtime (from BaseModelStatusResponse)
    model_id: String,
    model_name: String,
    status: ModelLoadStatus,
    memory_usage_mb: Option<i32>,
    loaded_at: Option<String>,
    // Registered metadata (from ModelWithStatsResponse, may be absent)
    format: Option<String>,
    backend: Option<String>,
    size_bytes: Option<i64>,
    quantization: Option<String>,
    adapter_count: Option<i64>,
    training_job_count: Option<i64>,
    import_status: Option<String>,
    architecture: Option<ModelArchitectureSummary>,
    capabilities: Option<Vec<String>>,
    imported_at: Option<String>,
    tenant_id: Option<String>,
}

/// Merged models data with aggregate fields.
#[derive(Clone, Debug)]
struct MergedModelsData {
    rows: Vec<MergedModelRow>,
    active_model_count: i64,
    total_memory_mb: i64,
}

// ============================================================================
// Merge logic
// ============================================================================

/// Merge runtime status with registered model metadata.
fn merge_models(
    status: &AllModelsStatusResponse,
    registered: Option<&ModelListResponse>,
) -> MergedModelsData {
    // Build lookup of registered models by ID
    let reg_map: HashMap<&str, &ModelWithStatsResponse> = registered
        .map(|r| r.models.iter().map(|m| (m.id.as_str(), m)).collect())
        .unwrap_or_default();

    let mut rows: Vec<MergedModelRow> = status
        .models
        .iter()
        .map(|s| {
            let reg = reg_map.get(s.model_id.as_str());
            MergedModelRow {
                model_id: s.model_id.clone(),
                model_name: s.model_name.clone(),
                status: s.status,
                memory_usage_mb: s.memory_usage_mb,
                loaded_at: s.loaded_at.clone(),
                format: reg.and_then(|r| r.format.clone()),
                backend: reg.and_then(|r| r.backend.clone()),
                size_bytes: reg.and_then(|r| r.size_bytes),
                quantization: reg.and_then(|r| r.quantization.clone()),
                adapter_count: reg.map(|r| r.adapter_count),
                training_job_count: reg.map(|r| r.training_job_count),
                import_status: reg.and_then(|r| r.import_status.clone()),
                architecture: reg.and_then(|r| r.architecture.clone()),
                capabilities: reg.and_then(|r| r.capabilities.clone()),
                imported_at: reg.and_then(|r| r.imported_at.clone()),
                tenant_id: reg.and_then(|r| r.tenant_id.clone()),
            }
        })
        .collect();

    // Append registered-only models (not in status) as NoModel
    if let Some(reg) = registered {
        let known_ids: std::collections::HashSet<&str> =
            status.models.iter().map(|m| m.model_id.as_str()).collect();
        for m in &reg.models {
            if !known_ids.contains(m.id.as_str()) {
                rows.push(MergedModelRow {
                    model_id: m.id.clone(),
                    model_name: m.name.clone(),
                    status: ModelLoadStatus::NoModel,
                    memory_usage_mb: None,
                    loaded_at: None,
                    format: m.format.clone(),
                    backend: m.backend.clone(),
                    size_bytes: m.size_bytes,
                    quantization: m.quantization.clone(),
                    adapter_count: Some(m.adapter_count),
                    training_job_count: Some(m.training_job_count),
                    import_status: m.import_status.clone(),
                    architecture: m.architecture.clone(),
                    capabilities: m.capabilities.clone(),
                    imported_at: m.imported_at.clone(),
                    tenant_id: m.tenant_id.clone(),
                });
            }
        }
    }

    MergedModelsData {
        rows,
        active_model_count: status.active_model_count,
        total_memory_mb: status.total_memory_mb,
    }
}

/// Build a synthetic AllModelsStatusResponse from registered models when status endpoint fails.
fn registered_as_fallback(reg: &ModelListResponse) -> MergedModelsData {
    let rows = reg
        .models
        .iter()
        .map(|m| MergedModelRow {
            model_id: m.id.clone(),
            model_name: m.name.clone(),
            status: ModelLoadStatus::NoModel,
            memory_usage_mb: None,
            loaded_at: None,
            format: m.format.clone(),
            backend: m.backend.clone(),
            size_bytes: m.size_bytes,
            quantization: m.quantization.clone(),
            adapter_count: Some(m.adapter_count),
            training_job_count: Some(m.training_job_count),
            import_status: m.import_status.clone(),
            architecture: m.architecture.clone(),
            capabilities: m.capabilities.clone(),
            imported_at: m.imported_at.clone(),
            tenant_id: m.tenant_id.clone(),
        })
        .collect();

    MergedModelsData {
        rows,
        active_model_count: 0,
        total_memory_mb: 0,
    }
}

// ============================================================================
// Models list page
// ============================================================================

/// Models management page
#[component]
pub fn Models() -> impl IntoView {
    // Shared selection state
    let sel = crate::components::use_split_panel_selection_state();
    let selected_id = sel.selected_id;

    // Import dialog state
    let show_import_dialog = RwSignal::new(false);

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

    // SSE-driven refetch subscription
    let refetch_counter = use_refetch_signal(RefetchTopic::Models);
    let refetch_all_for_sse = refetch_all;
    Effect::new(move || {
        let _ = refetch_counter.try_get();
        refetch_all_for_sse(());
    });

    // 5-second polling for load status changes
    let refetch_status_for_poll = refetch_models_status;
    let _cancel_polling = use_polling(5_000, move || {
        refetch_status_for_poll.run(());
        async {}
    });

    // Merge status + registered data into MergedModelsData
    let merged = Signal::derive(move || {
        let status_state = models_status.try_get().unwrap_or(LoadingState::Loading);
        let registered_state = registered_models.try_get().unwrap_or(LoadingState::Loading);

        match status_state {
            LoadingState::Idle | LoadingState::Loading => LoadingState::Loading,
            LoadingState::Error(e) => {
                // Graceful degradation: if status fails but registered loaded, show registered as NoModel
                if let LoadingState::Loaded(ref reg) = registered_state {
                    if !reg.models.is_empty() {
                        return LoadingState::Loaded(registered_as_fallback(reg));
                    }
                }
                LoadingState::Error(e)
            }
            LoadingState::Loaded(ref status_data) => {
                let reg = if let LoadingState::Loaded(ref r) = registered_state {
                    Some(r)
                } else {
                    None
                };
                LoadingState::Loaded(merge_models(status_data, reg))
            }
        }
    });

    // RouteContext publishing
    Effect::new(move || {
        if let Some(route_ctx) = try_use_route_context() {
            let id = selected_id.try_get().flatten();
            if let Some(ref sel_id) = id {
                // Try to find display name from merged data
                let display_name = if let Some(LoadingState::Loaded(ref data)) = merged.try_get() {
                    data.rows
                        .iter()
                        .find(|r| &r.model_id == sel_id)
                        .map(|r| r.model_name.clone())
                } else {
                    None
                };
                let status = if let Some(LoadingState::Loaded(ref data)) = merged.try_get() {
                    data.rows
                        .iter()
                        .find(|r| &r.model_id == sel_id)
                        .map(|r| model_status_label(r.status).1.to_string())
                } else {
                    None
                };
                crate::components::publish_route_selection(
                    &route_ctx,
                    "model",
                    Some(sel_id.clone()),
                    display_name,
                    status,
                );
            } else {
                route_ctx.clear_selected();
            }
        }
    });

    // Store merged rows for the detail panel
    let merged_rows_for_detail = Signal::derive(move || {
        if let Some(LoadingState::Loaded(ref data)) = merged.try_get() {
            data.rows.clone()
        } else {
            vec![]
        }
    });

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
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_import_dialog.set(true))
                >
                    "Import Model"
                </Button>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| refetch_all(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <SeedModelDialog
                open=show_import_dialog
                on_imported=Callback::new(move |_: ()| refetch_all(()))
            />

            <SplitPanel
                has_selection=sel.has_selection
                on_close=sel.on_close
                back_label="Back to Models"
                list_panel=move || {
                    let on_select = sel.on_select;
                    view! {
                        <div class="space-y-6">
                            {move || {
                                match merged.try_get().unwrap_or(LoadingState::Loading) {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <SkeletonTable rows=5 columns=6/>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(mut data) => {
                                        data.rows.sort_by_key(|m| match m.status {
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
                                                selected_id=selected_id
                                                on_select=on_select
                                                on_import=Callback::new(move |_| show_import_dialog.set(true))
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
                    let model_id = selected_id.try_get().flatten().unwrap_or_default();
                    let merged_rows = merged_rows_for_detail.try_get().unwrap_or_default();
                    view! {
                        <ModelDetailPanel
                            model_id=model_id
                            on_close=move || selected_id.set(None)
                            merged_rows=merged_rows
                        />
                    }
                }
            />
        </PageScaffold>
    }
}

// ============================================================================
// Model list component
// ============================================================================

/// Model list component
#[component]
fn ModelList(
    models: MergedModelsData,
    selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
    on_import: Callback<()>,
) -> impl IntoView {
    if models.rows.is_empty() {
        let has_active_context = models.active_model_count > 0 || models.total_memory_mb > 0;

        return if has_active_context {
            view! {
                <ListEmptyCard
                    title="Worker connected, model pending"
                    description="A worker is active but no model has been registered yet. Import a model or run `aosctl models seed` from the CLI.".to_string()
                />
                <div class="flex justify-center mt-4">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| on_import.run(()))
                    >
                        "Import Model"
                    </Button>
                </div>
            }
            .into_any()
        } else {
            view! {
                <ListEmptyCard
                    title="No models registered"
                    description="Import a model to get started, or run `aosctl models seed` from the CLI.".to_string()
                />
                <div class="flex justify-center mt-4">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| on_import.run(()))
                    >
                        "Import Model"
                    </Button>
                </div>
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
                        <TableHead>"Format"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Adapters"</TableHead>
                        <TableHead>"Memory"</TableHead>
                        <TableHead>"Loaded At"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {models.rows
                        .into_iter()
                        .map(|row| {
                            let model_id = row.model_id.clone();
                            let model_id_for_click = model_id.clone();

                            // Format column: format + optional quantization suffix
                            let format_display = match (&row.format, &row.quantization) {
                                (Some(fmt), Some(q)) => format!("{} ({})", fmt, q),
                                (Some(fmt), None) => fmt.clone(),
                                (None, Some(q)) => q.clone(),
                                (None, None) => "-".to_string(),
                            };

                            // Size column
                            let size_display = row
                                .size_bytes
                                .map(format_bytes)
                                .unwrap_or_else(|| "-".to_string());

                            // Adapters column
                            let adapters_display = row
                                .adapter_count
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "-".to_string());

                            let model_id_for_key = model_id_for_click.clone();

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&model_id)
                                    on:click=move |_| on_select.run(model_id_for_click.clone())
                                    on:keydown=move |e: web_sys::KeyboardEvent| {
                                        if e.key() == "Enter" || e.key() == " " {
                                            e.prevent_default();
                                            on_select.run(model_id_for_key.clone());
                                        }
                                    }
                                    role="button"
                                    tabindex=0
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{row.model_name.clone()}</p>
                                            <p class="text-xs text-muted-foreground font-mono">
                                                {adapteros_id::short_id(&row.model_id)}
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <ModelStatusBadge status=row.status/>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{format_display}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{size_display}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{adapters_display}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {row
                                                .memory_usage_mb
                                                .map(|m| format!("{} MB", m))
                                                .unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {row.loaded_at.clone().map(|ts| format_datetime(&ts)).unwrap_or_else(|| "-".to_string())}
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

// ============================================================================
// Model status badge
// ============================================================================

#[component]
fn ModelStatusBadge(status: ModelLoadStatus) -> impl IntoView {
    let (variant, label) = model_status_label(status);

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}

// ============================================================================
// Split-panel detail view
// ============================================================================

/// Model detail panel (split-panel inline view).
#[component]
fn ModelDetailPanel(
    model_id: String,
    on_close: impl Fn() + Copy + 'static,
    merged_rows: Vec<MergedModelRow>,
) -> impl IntoView {
    let model_id_for_fetch = model_id.clone();

    // Fetch per-model status (has UMA pressure, etc.)
    let (model_status, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = model_id_for_fetch.clone();
        async move { client.get_model(&id).await }
    });

    // Look up merged row for enriched metadata
    let merged_row = merged_rows.iter().find(|r| r.model_id == model_id).cloned();

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"Model Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                    aria-label="Close detail panel"
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
                        let merged = merged_row.clone();
                        view! {
                            <ModelDetailContent model=data merged_row=merged on_update=refetch/>
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

// ============================================================================
// Model detail content (shared between panel and standalone page)
// ============================================================================

/// Model detail content
#[component]
fn ModelDetailContent(
    model: ModelStatusResponse,
    merged_row: Option<MergedModelRow>,
    on_update: Refetch,
) -> impl IntoView {
    let status_variant = if model.is_loaded {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };
    let status_label = model_status_label(model.status);

    let is_loaded = model.is_loaded;
    let is_loading = matches!(model.status, ModelLoadStatus::Loading);
    let is_unloading = matches!(model.status, ModelLoadStatus::Unloading);
    let lifecycle_in_progress = is_loading || is_unloading;
    let model_id_load = model.model_id.clone();
    let model_id_unload = model.model_id.clone();
    let model_name_for_toast = model.model_name.clone();
    let (loading, set_loading) = signal(false);
    let show_unload_confirm = RwSignal::new(false);

    let notifications = use_notifications();

    let (auth_state, _) = use_auth();
    let can_manage_models = Signal::derive(move || {
        auth_state
            .get()
            .user()
            .map(|u| {
                u.role.eq_ignore_ascii_case("admin") || u.role.eq_ignore_ascii_case("operator")
            })
            .unwrap_or(false)
    });
    let current_role = Signal::derive(move || {
        auth_state
            .get()
            .user()
            .map(|u| u.role.clone())
            .unwrap_or_else(|| "unknown".to_string())
    });

    // Use shared API client instead of ApiClient::new() in handlers
    let client = use_api();
    let client_load = Arc::clone(&client);
    let client_unload = Arc::clone(&client);
    let client_validate = Arc::clone(&client);

    // Load model handler
    let on_load = {
        let model_name = model_name_for_toast.clone();
        let model_id = model_id_load.clone();
        let notifications = notifications.clone();
        move |_| {
            let id = model_id.clone();
            let name = model_name.clone();
            let client = Arc::clone(&client_load);
            let notifications = notifications.clone();
            let on_update = on_update;
            set_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match client.load_model(&id).await {
                    Ok(_) => {
                        notifications.success_with_action(
                            "Model loaded",
                            &format!("{} is ready for inference.", name),
                            "View Model",
                            &format!("/models/{}", id),
                        );
                        on_update.run(());
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to load model", Some("/models"), true);
                    }
                }
                let _ = set_loading.try_set(false);
            });
        }
    };

    // Unload model handler (called from confirmation dialog)
    let do_unload = {
        let model_name = model_name_for_toast.clone();
        let model_id = model_id_unload.clone();
        let notifications = notifications.clone();
        move |_: ()| {
            show_unload_confirm.set(false);
            let id = model_id.clone();
            let name = model_name.clone();
            let client = Arc::clone(&client_unload);
            let notifications = notifications.clone();
            let on_update = on_update;
            set_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match client.unload_model(&id).await {
                    Ok(_) => {
                        notifications.success(
                            "Model unloaded",
                            &format!("{} has been removed from memory.", name),
                        );
                        on_update.run(());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to unload model",
                            Some("/models"),
                            true,
                        );
                    }
                }
                let _ = set_loading.try_set(false);
            });
        }
    };

    // Unload button opens confirmation dialog instead of calling API directly
    let on_unload = move |_| {
        show_unload_confirm.set(true);
    };

    // Validate model handler
    let (validating, set_validating) = signal(false);
    let on_validate = {
        let model_id = model.model_id.clone();
        let model_name = model_name_for_toast.clone();
        let notifications = notifications.clone();
        move |_| {
            let id = model_id.clone();
            let name = model_name.clone();
            let client = Arc::clone(&client_validate);
            let notifications = notifications.clone();
            set_validating.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match client.validate_model(&id).await {
                    Ok(_) => {
                        notifications.success(
                            "Validation passed",
                            &format!("{} passed all validation checks.", name),
                        );
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Validation failed", None, true);
                    }
                }
                let _ = set_validating.try_set(false);
            });
        }
    };

    let unload_confirm_desc = format!(
        "Unloading {} will stop any active inference using this model. Sessions in progress may be interrupted.",
        model.model_name,
    );

    // Extract fields from merged_row before the view block to avoid move issues
    let detail_imported_at = merged_row.as_ref().and_then(|r| r.imported_at.clone());
    let detail_tenant_id = merged_row.as_ref().and_then(|r| r.tenant_id.clone());

    view! {
        // Unload confirmation dialog
        <ConfirmationDialog
            open=show_unload_confirm
            title="Unload Model"
            description=unload_confirm_desc
            severity=ConfirmationSeverity::Warning
            confirm_text="Unload Model"
            on_confirm=Callback::new(do_unload)
            on_cancel=Callback::new(move |_| show_unload_confirm.set(false))
            loading=Signal::from(loading)
        />

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
                        {move || {
                            if can_manage_models.get() {
                                view! {
                                    <Badge variant=BadgeVariant::Success>
                                        "Manage Access"
                                    </Badge>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <Badge variant=BadgeVariant::Warning>
                                        "Read-only Access"
                                    </Badge>
                                }
                                .into_any()
                            }
                        }}
                    </div>
                    <div class="flex flex-col gap-1 items-end">
                        <div class="flex gap-2">
                            {move || {
                                let manage_disabled = !can_manage_models.get();
                                let request_in_flight = loading.try_get().unwrap_or(false);
                                let is_validating = validating.try_get().unwrap_or(false);
                                let action_disabled =
                                    manage_disabled || request_in_flight || lifecycle_in_progress;
                                let validate_disabled =
                                    manage_disabled || is_validating || lifecycle_in_progress;

                                if request_in_flight {
                                    view! { <Spinner /> }.into_any()
                                } else if is_unloading {
                                    view! {
                                        <Button variant=ButtonVariant::Outline disabled=true>
                                            "Validate"
                                        </Button>
                                        <Button variant=ButtonVariant::Outline disabled=true>
                                            "Unload"
                                        </Button>
                                    }.into_any()
                                } else if is_loading {
                                    view! {
                                        <Button variant=ButtonVariant::Outline disabled=true>
                                            "Validate"
                                        </Button>
                                        <Button variant=ButtonVariant::Primary disabled=true>
                                            "Load"
                                        </Button>
                                    }.into_any()
                                } else if is_loaded {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Outline
                                            disabled=validate_disabled
                                            loading=Signal::from(validating)
                                            on_click=Callback::new(on_validate.clone())
                                        >
                                            "Validate"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            disabled=action_disabled
                                            on_click=Callback::new(on_unload.clone())
                                        >
                                            "Unload"
                                        </Button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Outline
                                            disabled=validate_disabled
                                            loading=Signal::from(validating)
                                            on_click=Callback::new(on_validate.clone())
                                        >
                                            "Validate"
                                        </Button>
                                        <Button
                                            variant=ButtonVariant::Primary
                                            disabled=action_disabled
                                            on_click=Callback::new(on_load.clone())
                                        >
                                            "Load"
                                        </Button>
                                    }.into_any()
                                }
                            }}
                        </div>
                        {move || {
                            if can_manage_models.get() {
                                view! {
                                    <p class="text-xs text-muted-foreground">
                                        "Your role can load and unload models."
                                    </p>
                                }
                                .into_any()
                            } else {
                                let role = current_role.get();
                                view! {
                                    <div class="rounded-md border border-warning/40 bg-warning/10 p-3">
                                        <p class="text-xs text-warning-foreground">
                                            {format!(
                                                "Current role: {}. Load and unload actions require Admin or Operator.",
                                                role
                                            )}
                                        </p>
                                    </div>
                                }
                                .into_any()
                            }
                        }}
                    </div>
                </div>

                {if is_loading {
                    Some(view! {
                        <p class="text-xs text-muted-foreground">
                            "Model loading. Inference will be ready once loading completes."
                        </p>
                    })
                } else if is_unloading {
                    Some(view! {
                        <p class="text-xs text-muted-foreground">
                            "Model unloading. Inference using this model is being drained."
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

                {model.error_message.clone().map(|err| {
                    if lifecycle_in_progress {
                        view! {
                            <div class="rounded-lg border border-warning/40 bg-warning/10 p-3">
                                <p class="text-sm text-warning-foreground">{format!("Backend busy: {}", err)}</p>
                            </div>
                        }
                    } else {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-3">
                                <p class="text-sm text-destructive">{err}</p>
                            </div>
                        }
                    }
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
                {detail_imported_at.map(|ts| view! {
                    <div class="flex justify-between">
                        <span class="text-muted-foreground">"Imported At"</span>
                        <span>{format_datetime(&ts)}</span>
                    </div>
                })}
                {detail_tenant_id.map(|tid| view! {
                    <CopyableId id=tid label="Tenant ID".to_string() truncate=24 />
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

        // Model Info (from registered metadata)
        {merged_row.as_ref().and_then(|row| {
            let has_info = row.format.is_some() || row.backend.is_some()
                || row.quantization.is_some() || row.import_status.is_some();
            if !has_info { return None; }
            let row = row.clone();
            Some(view! {
                <Card title="Model Info".to_string() class="mt-4".to_string()>
                    <div class="grid gap-3 text-sm">
                        {row.format.clone().map(|fmt| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Format"</span>
                                <span class="font-medium">{fmt}</span>
                            </div>
                        })}
                        {row.backend.clone().map(|be| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Backend"</span>
                                <span class="font-medium">{be}</span>
                            </div>
                        })}
                        {row.quantization.clone().map(|q| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Quantization"</span>
                                <span class="font-medium">{q}</span>
                            </div>
                        })}
                        {row.import_status.clone().map(|is| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Import Status"</span>
                                <span class="font-medium">{is}</span>
                            </div>
                        })}
                    </div>
                </Card>
            })
        })}

        // Architecture (conditional)
        {merged_row.as_ref().and_then(|row| {
            row.architecture.as_ref().map(|arch| {
                let arch = arch.clone();
                view! {
                    <Card title="Architecture".to_string() class="mt-4".to_string()>
                        <div class="grid gap-3 text-sm">
                            {arch.architecture.clone().map(|name| view! {
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Architecture"</span>
                                    <span class="font-medium">{name}</span>
                                </div>
                            })}
                            {arch.num_layers.map(|n| view! {
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Layers"</span>
                                    <span class="font-medium">{n.to_string()}</span>
                                </div>
                            })}
                            {arch.hidden_size.map(|n| view! {
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Hidden Size"</span>
                                    <span class="font-medium">{n.to_string()}</span>
                                </div>
                            })}
                            {arch.vocab_size.map(|n| view! {
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Vocab Size"</span>
                                    <span class="font-medium">{n.to_string()}</span>
                                </div>
                            })}
                        </div>
                    </Card>
                }
            })
        })}

        // Statistics (from registered metadata)
        {merged_row.as_ref().and_then(|row| {
            let has_stats = row.adapter_count.is_some() || row.training_job_count.is_some();
            if !has_stats { return None; }
            let row = row.clone();
            Some(view! {
                <Card title="Statistics".to_string() class="mt-4".to_string()>
                    <div class="grid gap-3 text-sm">
                        {row.adapter_count.map(|c| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Adapters"</span>
                                <span class="font-medium">{c.to_string()}</span>
                            </div>
                        })}
                        {row.training_job_count.map(|c| view! {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Training Jobs"</span>
                                <span class="font-medium">{c.to_string()}</span>
                            </div>
                        })}
                    </div>
                </Card>
            })
        })}

        // Capabilities (from registered metadata)
        {merged_row.as_ref().and_then(|row| {
            let caps = row.capabilities.as_ref().filter(|c| !c.is_empty())?;
            let caps = caps.clone();
            Some(view! {
                <Card title="Capabilities".to_string() class="mt-4".to_string()>
                    <div class="flex flex-wrap gap-2">
                        {caps.into_iter().map(|cap| view! {
                            <Badge variant=BadgeVariant::Secondary>{cap}</Badge>
                        }).collect::<Vec<_>>()}
                    </div>
                </Card>
            })
        })}
    }
}

// ============================================================================
// Standalone model detail page (/models/:id)
// ============================================================================

/// Standalone model detail page
#[component]
pub fn ModelDetail() -> impl IntoView {
    let params = use_params_map();

    let model_id = Memo::new(move |_| {
        let params_map = params.try_get().unwrap_or_default();
        params_map.get("id").unwrap_or_default()
    });

    let (model_status, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = model_id.get_untracked();
        async move {
            if id.is_empty() {
                let err = ApiError::Validation("Missing model ID in route".to_string());
                report_error_with_toast(&err, "Missing model ID", Some("/models"), false);
                return Err(err);
            }
            let result = client.get_model(&id).await;
            if let Err(ref e) = result {
                report_error_with_toast(e, "Failed to load model", Some("/models"), false);
            }
            result
        }
    });

    // Also fetch registered model metadata so we can build a MergedModelRow
    let (registered_models, refetch_registered) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_models().await });

    let refetch_all = move |()| {
        refetch.run(());
        refetch_registered.run(());
    };

    let refetch_stored = StoredValue::new(refetch);

    let model_name_for_breadcrumb = Signal::derive(move || model_id.try_get().unwrap_or_default());

    // Build a merged row from the registered model data when available
    let merged_row_signal = Signal::derive(move || {
        let id = model_id.try_get().unwrap_or_default();
        let status_data = model_status.try_get();
        let reg_data = registered_models.try_get();

        // Find matching registered model
        let reg_model = if let Some(LoadingState::Loaded(ref reg)) = reg_data {
            reg.models.iter().find(|m| m.id == id)
        } else {
            None
        };

        // Build MergedModelRow from runtime status + registered metadata
        if let Some(LoadingState::Loaded(ref status)) = status_data {
            Some(MergedModelRow {
                model_id: status.model_id.clone(),
                model_name: status.model_name.clone(),
                status: status.status,
                memory_usage_mb: status.memory_usage_mb,
                loaded_at: status.loaded_at.clone(),
                format: reg_model.and_then(|r| r.format.clone()),
                backend: reg_model.and_then(|r| r.backend.clone()),
                size_bytes: reg_model.and_then(|r| r.size_bytes),
                quantization: reg_model.and_then(|r| r.quantization.clone()),
                adapter_count: reg_model.map(|r| r.adapter_count),
                training_job_count: reg_model.map(|r| r.training_job_count),
                import_status: reg_model.and_then(|r| r.import_status.clone()),
                architecture: reg_model.and_then(|r| r.architecture.clone()),
                capabilities: reg_model.and_then(|r| r.capabilities.clone()),
                imported_at: reg_model.and_then(|r| r.imported_at.clone()),
                tenant_id: reg_model.and_then(|r| r.tenant_id.clone()),
            })
        } else {
            None
        }
    });

    view! {
        <PageScaffold
            title="Model Details"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Deploy", "/models"),
                PageBreadcrumbItem::new("Models", "/models"),
                PageBreadcrumbItem::current(model_name_for_breadcrumb.get()),
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

            <AsyncBoundary
                state=model_status
                on_retry=Callback::new(move |_| refetch_stored.with_value(|f| f.run(())))
                render=move |data| {
                    let merged = merged_row_signal.get_untracked();
                    view! { <ModelDetailContent model=data merged_row=merged on_update=refetch/> }
                }
            />
        </PageScaffold>
    }
}

// ============================================================================
// Seed/Import model dialog
// ============================================================================

/// Dialog for importing a new model into the system.
#[component]
fn SeedModelDialog(open: RwSignal<bool>, on_imported: Callback<()>) -> impl IntoView {
    let model_path = RwSignal::new(String::new());
    let model_name = RwSignal::new(String::new());
    let format = RwSignal::new("mlx".to_string());
    let backend = RwSignal::new("mlx".to_string());

    let (loading, set_loading) = signal(false);
    let notifications = use_notifications();
    let client = use_api();

    let is_valid = move || !model_path.get().trim().is_empty();

    let format_options: Vec<(String, String)> = vec![
        ("mlx".to_string(), "MLX".to_string()),
        ("safetensors".to_string(), "SafeTensors".to_string()),
        ("gguf".to_string(), "GGUF".to_string()),
        ("pytorch".to_string(), "PyTorch".to_string()),
    ];

    let backend_options: Vec<(String, String)> = vec![
        ("mlx".to_string(), "MLX".to_string()),
        ("metal".to_string(), "Metal".to_string()),
        ("coreml".to_string(), "CoreML".to_string()),
    ];

    let reset_form = move || {
        model_path.set(String::new());
        model_name.set(String::new());
        format.set("mlx".to_string());
        backend.set("mlx".to_string());
    };

    let on_submit = {
        let notifications = notifications.clone();
        move |_| {
            let path = model_path.get().trim().to_string();
            if path.is_empty() {
                return;
            }

            // Derive name from path if not provided
            let name = {
                let n = model_name.get().trim().to_string();
                if n.is_empty() {
                    path.rsplit('/').next().unwrap_or(&path).to_string()
                } else {
                    n
                }
            };

            let request = SeedModelRequest {
                model_name: name,
                model_path: path,
                format: format.get(),
                backend: backend.get(),
                capabilities: None,
                metadata: None,
            };

            let client = Arc::clone(&client);
            let notifications = notifications.clone();
            let on_imported = on_imported;
            set_loading.set(true);
            open.set(false);

            wasm_bindgen_futures::spawn_local(async move {
                match client.seed_model(&request).await {
                    Ok(_) => {
                        notifications.success_with_action(
                            "Model imported",
                            "Model is being registered.",
                            "View Models",
                            "/models",
                        );
                        on_imported.run(());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to import model",
                            Some("/models"),
                            true,
                        );
                    }
                }
                let _ = set_loading.try_set(false);
            });

            reset_form();
        }
    };

    view! {
        <Dialog
            open=open
            title="Import Model".to_string()
            description="Register a model from the local filesystem".to_string()
        >
            <div class="space-y-4 overflow-y-auto" style="max-height: 60vh">
                <Input
                    value=model_path
                    label="Model Path".to_string()
                    placeholder="/var/models/Llama-3.2-3B-Instruct-4bit".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "Absolute path to the model directory on disk (required)"
                </p>

                <Input
                    value=model_name
                    label="Model Name".to_string()
                    placeholder="Auto-derived from path".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "Display name for the model (optional)"
                </p>

                <Select
                    value=format
                    options=format_options
                    label="Format".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "Model file format"
                </p>

                <Select
                    value=backend
                    options=backend_options
                    label="Backend".to_string()
                />
                <p class="text-xs text-muted-foreground -mt-2">
                    "Inference backend to use"
                </p>

                <div class="flex justify-end gap-2 pt-4">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| {
                            open.set(false);
                            reset_form();
                        })
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        disabled=Signal::derive(move || !is_valid() || loading.get())
                        loading=Signal::from(loading)
                        on_click=Callback::new(on_submit.clone())
                    >
                        "Import"
                    </Button>
                </div>

                <p class="text-xs text-muted-foreground border-t pt-3">
                    "You can also import models from the CLI: "
                    <code class="font-mono text-xs">"aosctl models seed"</code>
                </p>
            </div>
        </Dialog>
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
