//! Adapters page
//!
//! Displays the list of registered adapters with an expand/collapse detail panel.
//! Automatically refreshes when the global `RefetchTopic::Adapters` signal is
//! triggered (e.g., after training job completion).
//!
//! ## Layout
//!
//! Uses a split panel layout:
//! - Left: Paginated adapter list with click-to-select
//! - Right: AdapterDetailPanel showing full details for selected adapter
//!
//! ## Drawer Behavior
//!
//! - Click adapter row to open detail panel
//! - Click close button or press Escape to close
//! - Mobile: Full-screen overlay with back button

use crate::api::use_api_client;
use crate::api::{report_error_with_toast, ApiClient};
use crate::components::layout::nav_group_label_for_route;
use crate::components::{
    AdapterDetailPanel, AsyncBoundary, AsyncBoundaryWithErrorRender, Badge, BadgeVariant, Button,
    ButtonSize, ButtonType, ButtonVariant, Card, CopyableId, EmptyState, EmptyStateVariant,
    ErrorDisplay, Input, Link, PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    PageScaffoldPrimaryAction, SkeletonTable, SplitPanel, SplitRatio, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::contexts::use_in_flight;
use crate::hooks::{use_api_resource, use_cached_api_resource, CacheTtl, LoadingState};
use crate::signals::refetch::{use_refetch_signal, RefetchTopic};
use crate::signals::{try_use_route_context, use_ui_profile, SelectedEntity};
use crate::utils::{chat_path_with_adapter, format_datetime, humanize};
use adapteros_api_types::{
    AdapterResponse, LifecycleState, TrainingJobResponse, TrainingListParams,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use std::sync::Arc;

/// Path to open the training wizard for new adapter creation
const NEW_ADAPTER_PATH: &str = "/training?open_wizard=1";

/// Adapters list page with split-panel detail drawer
#[component]
pub fn Adapters() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/adapters").unwrap_or("Build");
    // State: selected adapter ID (None = detail panel closed)
    let selected_id = RwSignal::new(None::<String>);

    let (adapters, refetch) = use_cached_api_resource(
        "adapters_list",
        CacheTtl::LIST,
        |client: Arc<ApiClient>| async move { client.list_adapters().await },
    );

    let refetch_signal = StoredValue::new(refetch);

    // Subscribe to global adapter refetch topic (triggered on training completion)
    let adapter_refetch_counter = use_refetch_signal(RefetchTopic::Adapters);

    // Refetch when the global counter increments
    Effect::new(move || {
        let Some(counter) = adapter_refetch_counter.try_get() else {
            return;
        };
        // Skip initial effect run (counter starts at 0)
        if counter > 0 {
            refetch.run(());
        }
    });

    // Derive selected adapter from list (memoized to avoid recomputation)
    let selected_adapter = Memo::new(move |_| {
        let id = selected_id.try_get()??;
        match adapters.try_get()? {
            LoadingState::Loaded(data) => data.iter().find(|a| a.id == id).cloned(),
            _ => None,
        }
    });

    // Publish selection to RouteContext for contextual actions in Command Palette
    {
        Effect::new(move || {
            if let Some(route_ctx) = try_use_route_context() {
                if let Some(id) = selected_id.try_get().flatten() {
                    // Find the adapter name and status from loaded data
                    if let LoadingState::Loaded(data) =
                        adapters.try_get().unwrap_or(LoadingState::Idle)
                    {
                        if let Some(adapter) = data.iter().find(|a| a.id == id) {
                            route_ctx.set_selected(SelectedEntity::with_status(
                                "adapter",
                                id.clone(),
                                adapter.name.clone(),
                                lifecycle_stage_label(adapter.lifecycle_state).to_string(),
                            ));
                        } else {
                            route_ctx.set_selected(SelectedEntity::new("adapter", id.clone(), id));
                        }
                    } else {
                        route_ctx.set_selected(SelectedEntity::new("adapter", id.clone(), id));
                    }
                } else {
                    route_ctx.clear_selected();
                }
            }
        });
    }

    // Loading state
    let is_loading = Signal::derive(move || {
        matches!(
            adapters.try_get().unwrap_or(LoadingState::Idle),
            LoadingState::Idle | LoadingState::Loading
        )
    });

    // Has selection for split panel
    let has_selection = Signal::derive(move || selected_id.try_get().flatten().is_some());

    // Callbacks
    let on_select = Callback::new(move |id: String| {
        selected_id.set(Some(id));
    });

    let on_close_detail = Callback::new(move |_: ()| {
        selected_id.set(None);
    });

    let on_refetch_detail = Callback::new(move |_: ()| {
        refetch.run(());
    });

    view! {
        <PageScaffold
            title="Adapters"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/adapters"),
                PageBreadcrumbItem::current("Adapters"),
            ]
            full_width=true
        >
            <PageScaffoldPrimaryAction slot>
                {
                    let navigate = use_navigate();
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| {
                                navigate(NEW_ADAPTER_PATH, Default::default());
                            })
                        >
                            "Create Adapter"
                        </Button>
                    }
                }
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    size=ButtonSize::Sm
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            {move || {
                match adapters.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonTable rows=5 columns=5 /> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                            />
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let mut adapters_for_list = data.clone();
                        adapters_for_list.sort_by_key(|a| a.lifecycle_state.sort_key());
                        view! {
                            <SplitPanel
                                has_selection=has_selection
                                on_close=on_close_detail
                                back_label="Back to Adapters"
                                ratio=SplitRatio::TwoFifthsThreeFifths
                                list_panel=move || {
                                    let data = adapters_for_list.clone();
                                    view! {
                                        <AdaptersListInteractive
                                            adapters=data
                                            selected_id=selected_id
                                            on_select=on_select
                                        />
                                    }
                                }
                                detail_panel=move || {
                                    view! {
                                        <AdapterDetailPanel
                                            adapter=selected_adapter
                                            loading=is_loading
                                            on_close=on_close_detail
                                            on_refetch=on_refetch_detail
                                        />
                                    }
                                }
                            />
                        }.into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}

/// Page size for client-side pagination (reduces initial DOM nodes)
const PAGE_SIZE: usize = 25;

/// Interactive adapter list with selection support for split panel layout.
#[component]
fn AdaptersListInteractive(
    adapters: Vec<AdapterResponse>,
    #[prop(into)] selected_id: RwSignal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let total = adapters.len();
    let in_flight = use_in_flight();
    let navigate = use_navigate();
    let (recent_training_jobs, _) = use_cached_api_resource(
        "adapters_recent_training_jobs",
        CacheTtl::LIST,
        |client: Arc<ApiClient>| async move {
            let params = TrainingListParams {
                page: Some(1),
                page_size: Some(100),
                ..Default::default()
            };
            client
                .list_training_jobs(Some(&params))
                .await
                .map(|resp| resp.jobs)
        },
    );

    if adapters.is_empty() {
        let navigate = navigate.clone();
        return view! {
            <Card class="mt-4".to_string()>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No adapters yet".to_string()
                    description="Create your first adapter to add a new capability, then start a conversation right away.".to_string()
                    action_label="Create Adapter".to_string()
                    on_action=Callback::new(move |_| {
                        navigate(NEW_ADAPTER_PATH, Default::default());
                    })
                />
            </Card>
        }
        .into_any();
    }

    // Client-side pagination to reduce DOM nodes
    let visible_count = RwSignal::new(PAGE_SIZE.min(total));

    let show_more = move |_| {
        visible_count.update(|c| *c = (*c + PAGE_SIZE).min(total));
    };

    // Access in-flight IDs directly from context (already a HashSet)
    let in_flight_ids = in_flight.adapter_ids;

    // Clone adapters once for the closure
    let adapters_for_rows = adapters.clone();
    let nav_stored = StoredValue::new(navigate.clone());

    view! {
        <Card data_testid="adapters-list-card".to_string()>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Adapter"</TableHead>
                        <TableHead>"Glance"</TableHead>
                        <TableHead>"Last Training Source"</TableHead>
                        <TableHead>"Provenance"</TableHead>
                        <TableHead>""</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {move || {
                        let count = visible_count.try_get().unwrap_or(PAGE_SIZE);
                        let current_selected = selected_id.try_get().flatten();
                        let current_in_flight = in_flight_ids.try_get().unwrap_or_default();
                        let mut jobs_by_adapter_id = HashMap::<String, TrainingJobResponse>::new();
                        let mut jobs_by_name = HashMap::<String, TrainingJobResponse>::new();
                        if let LoadingState::Loaded(jobs) =
                            recent_training_jobs.try_get().unwrap_or(LoadingState::Idle)
                        {
                            for job in jobs {
                                if let Some(adapter_id) =
                                    job.adapter_id.clone().filter(|value| !value.trim().is_empty())
                                {
                                    record_latest_training_job(&mut jobs_by_adapter_id, adapter_id, &job);
                                }
                                if !job.adapter_name.trim().is_empty() {
                                    record_latest_training_job(
                                        &mut jobs_by_name,
                                        job.adapter_name.clone(),
                                        &job,
                                    );
                                }
                            }
                        }
                        adapters_for_rows.iter().take(count).map(|adapter| {
                            let id = adapter.id.clone();
                            let id_for_click = id.clone();
                            let name = adapter.name.clone();
                            let lifecycle = adapter.lifecycle_state;
                            let tier = adapter.tier.clone();
                            let adapter_id = adapter.adapter_id.clone();
                            let current_version = adapter.version.clone();
                            let runtime_label = runtime_state_label(adapter.runtime_state.as_deref());
                            let is_selected = current_selected.as_ref() == Some(&id);
                            let is_in_flight = current_in_flight.contains(&id);

                            // Lifecycle badge variant
                            let lifecycle_variant = lifecycle_badge_variant(lifecycle);
                            let lifecycle_label = lifecycle_stage_label(lifecycle);
                            let latest_job = jobs_by_adapter_id
                                .get(&adapter_id)
                                .or_else(|| jobs_by_name.get(&name));

                            let base_model = latest_job
                                .and_then(|job| job.base_model_id.clone())
                                .unwrap_or_else(|| "Not captured in recent training jobs".to_string());

                            let (training_source_summary, training_source_status) =
                                if let Some(job) = latest_job {
                                    let summary = job
                                        .dataset_version_trust
                                        .as_ref()
                                        .and_then(|entries| entries.first())
                                        .map(|entry| {
                                            if let Some(dataset_name) = entry.dataset_name.clone() {
                                                format!("{} ({})", dataset_name, entry.dataset_version_id)
                                            } else {
                                                format!(
                                                    "Dataset version {}",
                                                    entry.dataset_version_id
                                                )
                                            }
                                        })
                                        .or_else(|| {
                                            job.dataset_version_ids.as_ref().and_then(|versions| {
                                                versions.first().map(|selection| {
                                                    format!(
                                                        "Dataset version {}",
                                                        selection.dataset_version_id
                                                    )
                                                })
                                            })
                                        })
                                        .or_else(|| {
                                            job.dataset_id
                                                .as_ref()
                                                .map(|dataset_id| format!("Dataset {}", dataset_id))
                                        })
                                        .unwrap_or_else(|| {
                                            "Training source is not attached to this job".to_string()
                                        });

                                    (
                                        summary,
                                        format!(
                                            "Latest job: {}",
                                            humanize(job.status.as_str())
                                        ),
                                    )
                                } else {
                                    (
                                        "No recent training job found in this list view".to_string(),
                                        "Open adapter details for deeper lineage".to_string(),
                                    )
                                };

                            let provenance_count = latest_job
                                .and_then(|job| {
                                    job.dataset_version_ids.as_ref().map(|versions| versions.len())
                                })
                                .unwrap_or(0);
                            let provenance_label = if provenance_count == 0 {
                                "No linked dataset versions".to_string()
                            } else if provenance_count == 1 {
                                "1 linked dataset version".to_string()
                            } else {
                                format!("{} linked dataset versions", provenance_count)
                            };

                            let id_for_keydown = id_for_click.clone();
                            let row_label = format!("Select adapter {}", name);
                            view! {
                                <tr
                                    class="table-row table-row-interactive cursor-pointer"
                                    data-state=if is_selected { "selected" } else { "" }
                                    role="button"
                                    tabindex=0
                                    aria-label=row_label
                                    aria-pressed=is_selected
                                    on:click=move |_| {
                                        on_select.run(id_for_click.clone());
                                    }
                                    on:keydown=move |e: web_sys::KeyboardEvent| {
                                        let key = e.key();
                                        if key == "Enter" || key == " " || key == "Spacebar" {
                                            e.prevent_default();
                                            e.stop_propagation();
                                            on_select.run(id_for_keydown.clone());
                                        }
                                    }
                                >
                                    <TableCell>
                                        <div class="space-y-1">
                                            <p class="font-medium">{name}</p>
                                            <p class="text-xs text-muted-foreground">
                                                {format!("Tier: {}", humanize(&tier))}
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <div class="space-y-1">
                                            <div class="flex flex-wrap items-center gap-2">
                                                <Badge variant=lifecycle_variant>
                                                    {lifecycle_label}
                                                </Badge>
                                                <Badge variant=BadgeVariant::Secondary>
                                                    {runtime_label}
                                                </Badge>
                                                {is_in_flight.then(|| view! {
                                                    <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
                                                })}
                                            </div>
                                            <p class="text-xs text-muted-foreground">{format!("Adapter version: {}", current_version)}</p>
                                            <p class="text-xs text-muted-foreground">{format!("Base model: {}", base_model)}</p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <div class="space-y-1">
                                            <p class="text-sm">{training_source_summary}</p>
                                            <p class="text-xs text-muted-foreground">{training_source_status}</p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <p class="text-sm">{provenance_label}</p>
                                        <p class="text-xs text-muted-foreground">
                                            "Counts dataset version links from the latest visible training job."
                                        </p>
                                    </TableCell>
                                    <TableCell>
                                        {
                                            let chat_id = id.clone();
                                            view! {
                                                <div on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                                                    <Button
                                                        variant=ButtonVariant::Ghost
                                                        size=ButtonSize::Sm
                                                        on_click=Callback::new(move |_| {
                                                            let path = chat_path_with_adapter(&chat_id);
                                                            nav_stored.with_value(|nav| nav(&path, Default::default()));
                                                        })
                                                    >
                                                        "Open Studio"
                                                    </Button>
                                                </div>
                                            }
                                        }
                                    </TableCell>
                                </tr>
                            }
                        }).collect::<Vec<_>>()
                    }}
                </TableBody>
            </Table>

            // Show more button if there are hidden items
            {move || {
                let count = visible_count.try_get().unwrap_or(PAGE_SIZE);
                let remaining = total.saturating_sub(count);
                if remaining > 0 {
                    let aria_label = if remaining == 1 {
                        "Show 1 more adapter".to_string()
                    } else {
                        format!("Show {} more adapters", remaining)
                    };
                    view! {
                        <div class="flex items-center justify-center py-4 border-t">
                            <button
                                class="text-sm text-primary hover:underline"
                                aria-label=aria_label
                                on:click=show_more
                            >
                                {format!("Show more ({} remaining)", remaining)}
                            </button>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </Card>
    }
    .into_any()
}

/// Validate adapter ID format
/// Valid IDs: alphanumeric with hyphens/underscores, 1-128 chars
fn validate_adapter_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("Adapter ID is required");
    }
    if id.len() > 128 {
        return Err("Adapter ID exceeds maximum length");
    }
    // Allow alphanumeric, hyphens, underscores
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Adapter ID contains invalid characters");
    }
    Ok(())
}

/// Adapter detail page
#[component]
pub fn AdapterDetail() -> impl IntoView {
    let nav_label =
        nav_group_label_for_route(use_ui_profile().get_untracked(), "/adapters").unwrap_or("Build");
    let params = use_params_map();

    // Extract adapter ID from URL - must be called unconditionally
    let adapter_id = Memo::new(move |_| {
        let params_map = params.try_get().unwrap_or_default();
        params_map.get("id").unwrap_or_default()
    });

    // Always call use_api_resource (hooks must be called unconditionally)
    // Use get_untracked() to avoid reactive tracking warnings
    let (adapter, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = adapter_id.get_untracked();
        async move {
            if let Err(validation_err) = validate_adapter_id(&id) {
                let api_err = crate::api::ApiError::Validation(validation_err.to_string());
                report_error_with_toast(&api_err, "Invalid adapter ID", Some("/adapters"), false);
                return Err(api_err);
            }

            let result = client.get_adapter(&id).await;

            if let Err(ref e) = result {
                report_error_with_toast(e, "Failed to load adapter", Some("/adapters"), false);
            }
            result
        }
    });

    let refetch_signal = StoredValue::new(refetch);

    // Derive adapter name for breadcrumb (shows ID until loaded)
    let adapter_name_for_breadcrumb =
        Signal::derive(move || adapter_id.try_get().unwrap_or_default());

    view! {
        <PageScaffold
            title="Adapter Details"
            breadcrumbs=vec![
                PageBreadcrumbItem::new(nav_label, "/adapters"),
                PageBreadcrumbItem::new("Adapters", "/adapters"),
                PageBreadcrumbItem::current(adapter_name_for_breadcrumb.get()),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                {
                    let navigate = use_navigate();
                    let id_for_chat = adapter_id;
                    view! {
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| {
                                let path = chat_path_with_adapter(&id_for_chat.get_untracked());
                                navigate(&path, Default::default());
                            })
                        >
                            "Start Conversation"
                        </Button>
                    }
                }
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| refetch.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <AsyncBoundaryWithErrorRender
                state=adapter
                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                render=move |data| view! {
                    <AdapterDetailContent
                        adapter=data
                        on_refetch=Callback::new(move |_| refetch_signal.with_value(|f| f.run(())))
                    />
                }
                render_error=move |e, retry| {
                    if let crate::api::ApiError::Validation(msg) = &e {
                        let error_msg = msg.clone();
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <div class="text-center">
                                    <h2 class="heading-3 mb-2 text-destructive">"Invalid Adapter ID"</h2>
                                    <p class="text-muted-foreground mb-4">{error_msg}</p>
                                    <Link href="/adapters">
                                        "← Back to Adapters"
                                    </Link>
                                </div>
                            </div>
                        }.into_any()
                    } else if let Some(retry_cb) = retry {
                        view! { <ErrorDisplay error=e on_retry=retry_cb /> }.into_any()
                    } else {
                        view! { <ErrorDisplay error=e /> }.into_any()
                    }
                }
            />
        </PageScaffold>
    }
}

fn lifecycle_badge_variant(state: LifecycleState) -> BadgeVariant {
    match state {
        LifecycleState::Active => BadgeVariant::Success,
        LifecycleState::Staging => BadgeVariant::Warning,
        LifecycleState::Deprecated => BadgeVariant::Warning,
        LifecycleState::Retired => BadgeVariant::Destructive,
        LifecycleState::Draft => BadgeVariant::Secondary,
        _ => BadgeVariant::Secondary,
    }
}

fn lifecycle_stage_label(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::Draft => "Draft",
        LifecycleState::Staging => "Reviewed",
        LifecycleState::Active => "Production",
        LifecycleState::Deprecated => "Paused",
        LifecycleState::Retired => "Retired",
        _ => "Unknown",
    }
}

fn runtime_state_label(runtime_state: Option<&str>) -> &'static str {
    match runtime_state {
        Some("hot") => "Ready",
        Some("warm") => "Warming",
        Some("cold") => "Standby",
        Some("resident") => "Pinned in Memory",
        Some("unloaded") => "Not Loaded",
        _ => "Unknown",
    }
}

fn record_latest_training_job(
    index: &mut HashMap<String, TrainingJobResponse>,
    key: String,
    candidate: &TrainingJobResponse,
) {
    let should_replace = index
        .get(&key)
        .map(|existing| candidate.created_at > existing.created_at)
        .unwrap_or(true);
    if should_replace {
        index.insert(key, candidate.clone());
    }
}

/// Validate adapter response data before rendering
/// Returns error message if validation fails
fn validate_adapter_data(adapter: &AdapterResponse) -> Result<(), String> {
    let mut missing = Vec::new();

    if adapter.id.is_empty() {
        missing.push("id");
    }
    if adapter.adapter_id.is_empty() {
        missing.push("adapter_id");
    }
    if adapter.name.is_empty() {
        missing.push("name");
    }
    if adapter.hash_b3.is_empty() {
        missing.push("hash_b3");
    }
    if adapter.tier.is_empty() {
        missing.push("tier");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("Missing required fields: {}", missing.join(", ")))
    }
}

#[component]
fn AdapterDetailContent(adapter: AdapterResponse, on_refetch: Callback<()>) -> impl IntoView {
    // Validate adapter data before rendering
    if let Err(validation_error) = validate_adapter_data(&adapter) {
        report_error_with_toast(
            &crate::api::ApiError::Validation(validation_error.clone()),
            "Invalid adapter data",
            Some("/adapters"),
            false,
        );
        return view! {
            <div class="flex items-center justify-center py-12">
                <div class="text-center">
                    <h2 class="heading-3 mb-2 text-destructive">"Invalid Adapter Data"</h2>
                    <p class="text-muted-foreground mb-2">{validation_error}</p>
                    <Link href="/adapters">
                        "← Back to Adapters"
                    </Link>
                </div>
            </div>
        }
        .into_any();
    }

    let lifecycle_variant = lifecycle_badge_variant(adapter.lifecycle_state);
    let lifecycle_label = lifecycle_stage_label(adapter.lifecycle_state);

    // Rename state (allowed only for Draft and Training)
    let can_rename = matches!(
        adapter.lifecycle_state,
        LifecycleState::Draft | LifecycleState::Training
    );
    let name_editing = RwSignal::new(false);
    let name_draft = RwSignal::new(String::new());
    let renaming = RwSignal::new(false);
    let action_error = RwSignal::new(None::<String>);
    let action_success = RwSignal::new(None::<String>);
    let client = use_api_client();
    let name = adapter.name.clone();
    let adapter_id = adapter.adapter_id.clone();
    let name_for_start = name.clone();

    let start_rename = Callback::new(move |_| {
        name_draft.set(name_for_start.clone());
        name_editing.set(true);
    });
    let cancel_rename = Callback::new(move |_| {
        name_editing.set(false);
        name_draft.set(String::new());
    });
    let save_rename = Callback::new({
        let client = client.clone();
        let adapter_id = adapter_id.clone();
        move |_| {
            if renaming.get() {
                return;
            }
            let new_name = name_draft.get().trim().to_string();
            if new_name.is_empty() {
                action_error.set(Some("Name cannot be empty.".to_string()));
                return;
            }
            renaming.set(true);
            action_error.set(None);
            action_success.set(None);
            let client = client.clone();
            let adapter_id = adapter_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .patch_adapter(&adapter_id, Some(new_name.trim()))
                    .await
                {
                    Ok(_) => {
                        name_editing.set(false);
                        name_draft.set(String::new());
                        action_success.set(Some("Adapter renamed.".to_string()));
                        on_refetch.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(format!("Unable to rename: {}", e.user_message())));
                    }
                }
                renaming.set(false);
            });
        }
    });
    let clear_alias = Callback::new({
        let client = client.clone();
        let adapter_id = adapter_id.clone();
        move |_| {
            if renaming.get() {
                return;
            }
            renaming.set(true);
            action_error.set(None);
            action_success.set(None);
            let client = client.clone();
            let adapter_id = adapter_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.patch_adapter(&adapter_id, None).await {
                    Ok(_) => {
                        name_editing.set(false);
                        name_draft.set(String::new());
                        action_success.set(Some("Custom name cleared; using default.".to_string()));
                        on_refetch.run(());
                    }
                    Err(e) => {
                        action_error
                            .set(Some(format!("Unable to clear name: {}", e.user_message())));
                    }
                }
                renaming.set(false);
            });
        }
    });

    let rename_aria_label = if can_rename {
        "Rename adapter".to_string()
    } else {
        format!(
            "Rename not available for adapters in {} state",
            lifecycle_label
        )
    };

    // Extract values needed before moving into closures
    let adapter_name_for_link = adapter.name.clone();
    let intent = adapter.intent.clone();
    let languages = adapter.languages.clone();
    let framework = adapter.framework.clone();
    let framework_id = adapter.framework_id.clone();
    let framework_version = adapter.framework_version.clone();

    view! {
        // Row 1: Basic Info + Status (2-column grid)
        <div class="grid gap-4 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    {move || action_error.get().map(|msg| view! {
                        <div class="rounded-lg border border-destructive/50 bg-destructive/10 p-2">
                            <p class="text-sm text-destructive">{msg}</p>
                        </div>
                    })}
                    {move || action_success.get().map(|msg| view! {
                        <div class="rounded-lg border border-status-success/50 bg-status-success/5 p-2">
                            <p class="text-sm text-status-success">{msg}</p>
                        </div>
                    })}
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        {move || if name_editing.get() {
                            view! {
                                <div class="flex items-center gap-2" role="group" aria-label="Edit adapter name">
                                    <Input
                                        value=name_draft
                                        placeholder="Adapter name".to_string()
                                        class="flex-1".to_string()
                                    />
                                    <Button
                                        button_type=ButtonType::Button
                                        variant=ButtonVariant::Primary
                                        disabled=Signal::derive(move || renaming.get())
                                        loading=Signal::derive(move || renaming.get())
                                        on_click=save_rename
                                        aria_label="Save adapter name"
                                    >
                                        "Save"
                                    </Button>
                                    <Button
                                        button_type=ButtonType::Button
                                        variant=ButtonVariant::Ghost
                                        disabled=Signal::derive(move || renaming.get())
                                        on_click=clear_alias
                                        aria_label="Clear custom name and use default"
                                    >
                                        "Use default"
                                    </Button>
                                    <Button
                                        button_type=ButtonType::Button
                                        variant=ButtonVariant::Ghost
                                        disabled=Signal::derive(move || renaming.get())
                                        on_click=cancel_rename
                                        aria_label="Cancel editing"
                                    >
                                        "Cancel"
                                    </Button>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="flex items-center gap-2">
                                    <p class="font-medium flex-1">{name.clone()}</p>
                                    <Button
                                        button_type=ButtonType::Button
                                        variant=ButtonVariant::Ghost
                                        disabled=Signal::derive(move || !can_rename)
                                        on_click=start_rename
                                        aria_label=rename_aria_label.clone()
                                    >
                                        "Rename"
                                    </Button>
                                </div>
                            }.into_any()
                        }}
                    </div>
                    {intent.map(|intent_text| view! {
                        <div>
                            <p class="text-sm text-muted-foreground">"Intent"</p>
                            <p class="text-sm">{intent_text}</p>
                        </div>
                    })}
                    <CopyableId
                        id=adapter.adapter_id.clone()
                        label="Adapter ID".to_string()
                        display_name=adapter.display_name.clone().unwrap_or_default()
                        truncate=28
                    />
                    <div>
                        <p class="text-sm text-muted-foreground">"Hash (BLAKE3)"</p>
                        <p class="font-mono text-sm truncate">{adapter.hash_b3.clone()}</p>
                    </div>
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="flex items-center gap-2 mb-3">
                    <Badge variant=lifecycle_variant>
                        {lifecycle_label}
                    </Badge>
                    {adapter.runtime_state.clone().map(|state| view! {
                        <Badge variant=BadgeVariant::Secondary>
                            {humanize(&state)}
                        </Badge>
                    })}
                </div>
                <div class="space-y-2 text-sm">
                    <div>
                        <span class="text-muted-foreground">"Tier: "</span>
                        <span class="font-medium">{humanize(&adapter.tier)}</span>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Category: "</span>
                        <span class="font-medium">{adapter.category.as_deref().map(humanize).unwrap_or_else(|| "N/A".to_string())}</span>
                    </div>
                </div>
                <div class="mt-3 pt-3 border-t border-border/50">
                    <p class="text-xs text-muted-foreground mb-1">"Provenance"</p>
                    <Link
                        href=format!("/training?adapter_name={}", adapter_name_for_link)
                        class="text-sm text-primary hover:underline"
                    >
                        "View Training History →"
                    </Link>
                </div>
            </Card>
        </div>

        // Row 2: Tech Stack + Metadata (2-column grid)
        <div class="grid gap-4 md:grid-cols-2 mt-4">
            // Tech Stack: Languages + Framework combined (mirrors AdapterDetailPanel pattern)
            <Card title="Tech Stack".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground mb-1">"Languages"</p>
                        <div class="flex flex-wrap gap-2">
                            {if languages.is_empty() {
                                view! { <span class="text-muted-foreground text-sm">"No languages specified"</span> }.into_any()
                            } else {
                                view! {
                                    {languages.into_iter().map(|lang| view! {
                                        <Badge variant=BadgeVariant::Secondary>{lang}</Badge>
                                    }).collect::<Vec<_>>()}
                                }.into_any()
                            }}
                        </div>
                    </div>
                    {framework.map(|fw| view! {
                        <div class="space-y-2">
                            <div>
                                <p class="text-sm text-muted-foreground">"Framework"</p>
                                <p class="font-medium">{fw}</p>
                            </div>
                            {framework_id.clone().map(|fid| view! {
                                <div>
                                    <p class="text-sm text-muted-foreground">"Framework ID"</p>
                                    <p class="font-mono text-sm">{fid}</p>
                                </div>
                            })}
                            {framework_version.clone().map(|fv| view! {
                                <div>
                                    <p class="text-sm text-muted-foreground">"Framework Version"</p>
                                    <p class="font-medium">{fv}</p>
                                </div>
                            })}
                        </div>
                    })}
                </div>
            </Card>

            // Metadata
            <Card title="Metadata".to_string()>
                <div class="grid gap-4 grid-cols-2">
                    <div>
                        <p class="text-sm text-muted-foreground">"Rank"</p>
                        <p class="font-medium">{adapter.rank}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Version"</p>
                        <p class="font-medium">{adapter.version.clone()}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Created At"</p>
                        <p class="font-medium">{format_datetime(&adapter.created_at)}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Updated At"</p>
                        <p class="font-medium">{adapter.updated_at.as_deref().map(format_datetime).unwrap_or_else(|| "N/A".to_string())}</p>
                    </div>
                </div>
            </Card>
        </div>

        // Row 3: Statistics - full width (content-heavy with large metrics)
        {adapter.stats.clone().map(|stats| view! {
            <Card title="Statistics".to_string() class="mt-4".to_string()>
                <div class="grid gap-4 md:grid-cols-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Total Activations"</p>
                        <p class="text-2xl font-bold">{stats.total_activations}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selected Count"</p>
                        <p class="text-2xl font-bold">{stats.selected_count}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Selection Rate"</p>
                        <p class="text-2xl font-bold">{format!("{:.1}%", stats.selection_rate * 100.0)}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Avg Gate Value"</p>
                        <p class="text-2xl font-bold">{format!("{:.3}", stats.avg_gate_value)}</p>
                    </div>
                </div>
            </Card>
        })}
    }.into_any()
}
