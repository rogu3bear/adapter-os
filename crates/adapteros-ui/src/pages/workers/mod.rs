//! Workers management page
//!
//! Comprehensive worker management with detailed status, metrics,
//! spawn controls, and lifecycle management.
//!
//! ## Layout
//!
//! Uses PageScaffold for consistent page structure and SplitPanel for
//! list-detail layout:
//! - Left: WorkersList with click-to-select
//! - Right: WorkerDetailPanel showing full details for selected worker
//! - Summary cards above the split panel

mod components;
pub mod dialogs;
mod utils;
pub(crate) use utils::is_terminal_worker_status;

use crate::api::{report_error_with_toast, ApiClient};
use crate::components::{
    Button, ButtonLink, ButtonSize, ButtonVariant, ConfirmationDialog, ConfirmationSeverity,
    ErrorDisplay, InlineErrorBanner, LoadingDisplay, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, PageScaffoldPrimaryAction, RefreshButton, SkeletonCard, SkeletonTable,
    SplitPanel, SplitRatio,
};
use crate::hooks::{
    use_api, use_api_resource, use_cached_api_resource, use_polling, use_system_status, CacheTtl,
    LoadingState,
};
use crate::signals::{use_notifications, use_refetch_signal, RefetchTopic};
use adapteros_api_types::SpawnWorkerRequest;
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use adapteros_api_types::StatusIndicator as ApiStatusIndicator;

use crate::components::{IconPlus, IconRefresh};
use components::{WorkerDetailPanel, WorkerDetailView, WorkersList, WorkersSummary};
use dialogs::{PlanOption, SpawnWorkerDialog};
use utils::{is_recent_timestamp, WorkerHealthRecord, WorkerHealthSummary};

/// Workers management page
#[component]
pub fn Workers() -> impl IntoView {
    const ACTIVE_WINDOW_SECS: u64 = 5 * 60;

    // Dialog state
    let show_spawn_dialog = RwSignal::new(false);
    let selected_worker = RwSignal::new(Option::<String>::None);
    let show_history = RwSignal::new(false);
    let action_loading = RwSignal::new(false);
    let (system_status, _) = use_system_status();
    let system_not_ready = Memo::new(move |_| {
        !matches!(
            system_status.get(),
            LoadingState::Loaded(ref s) if matches!(s.readiness.overall, ApiStatusIndicator::Ready)
        )
    });
    let action_error = RwSignal::new(Option::<String>::None);
    let pending_drain_worker = RwSignal::new(Option::<String>::None);
    let pending_stop_worker = RwSignal::new(Option::<String>::None);
    let pending_restart_worker = RwSignal::new(Option::<String>::None);
    let pending_remove_worker = RwSignal::new(Option::<String>::None);
    let show_drain_confirm = RwSignal::new(false);
    let show_stop_confirm = RwSignal::new(false);
    let show_restart_confirm = RwSignal::new(false);
    let show_remove_confirm = RwSignal::new(false);
    let notifications = use_notifications();

    // Fetch workers list (terminal entries hidden unless history is explicitly enabled, SWR-cached)
    let (workers, refetch_workers) = use_cached_api_resource(
        "workers_detail",
        CacheTtl::LIST,
        move |client: Arc<ApiClient>| {
            let include_history = show_history.get_untracked();
            async move {
                if include_history {
                    client.list_workers_with_history().await
                } else {
                    client.list_workers().await
                }
            }
        },
    );

    // Fetch worker health summary (health status + incident counts)
    let (worker_health, refetch_worker_health) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client
                .get::<WorkerHealthSummary>("/v1/workers/health/summary")
                .await
        });

    // Fetch nodes for spawn form (SWR-cached)
    let (nodes, _refetch_nodes) = use_cached_api_resource(
        "nodes_list",
        CacheTtl::LIST,
        |client: Arc<ApiClient>| async move { client.list_nodes().await },
    );

    // Fetch plans for spawn form
    let (plans, _refetch_plans) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get::<Vec<PlanOption>>("/v1/plans").await
    });

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref w)) = workers.try_get() {
            web_sys::console::log_1(&format!("[list] workers: {} items", w.len()).into());
        }
    });

    // SSE-driven refresh from Shell's health lifecycle stream.
    let workers_refetch_counter = use_refetch_signal(RefetchTopic::Workers);
    Effect::new(move || {
        let Some(counter) = workers_refetch_counter.try_get() else {
            return;
        };
        if counter > 0 {
            refetch_workers.run(());
            refetch_worker_health.run(());
        }
    });

    // Set up polling interval (every 10 seconds for workers)
    // Polling remains as keepalive fallback when SSE-driven updates are unavailable.
    let _ = use_polling(10_000, move || async move {
        refetch_workers.run(());
        refetch_worker_health.run(());
    });

    // Has selection for split panel
    let has_selection = Signal::derive(move || selected_worker.get().is_some());

    let on_close_detail = Callback::new(move |_: ()| {
        selected_worker.set(None);
    });

    let api_client = use_api();
    let spawn_worker_request = Callback::new({
        let notifications = notifications.clone();
        let api_client = api_client.clone();
        move |request: SpawnWorkerRequest| {
            action_loading.set(true);
            show_spawn_dialog.set(false);
            let client = api_client.clone();
            let notifications = notifications.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client.spawn_worker(&request).await {
                    Ok(_) => {
                        action_error.set(None);
                        notifications.success_with_action(
                            "Worker spawned",
                            "New worker is starting up.",
                            "View Workers",
                            "/workers",
                        );
                        refetch_workers.run(());
                    }
                    Err(e) => {
                        action_error.set(Some(e.user_message()));
                        report_error_with_toast(
                            &e,
                            "Failed to spawn worker",
                            Some("/workers"),
                            true,
                        );
                    }
                }
                action_loading.set(false);
            });
        }
    });
    let quick_spawn_from_defaults = Callback::new({
        move |_| {
            if action_loading.get_untracked() {
                return;
            }

            let default_node = match nodes.get_untracked() {
                LoadingState::Loaded(nodes) => nodes
                    .iter()
                    .find(|node| node.node.status.eq_ignore_ascii_case("active"))
                    .cloned()
                    .or_else(|| nodes.first().cloned()),
                _ => None,
            };
            let default_plan = match plans.get_untracked() {
                LoadingState::Loaded(plans) => plans
                    .iter()
                    .find(|plan| {
                        plan.status.eq_ignore_ascii_case("ready")
                            || plan.status.eq_ignore_ascii_case("active")
                            || plan.status.eq_ignore_ascii_case("built")
                    })
                    .cloned()
                    .or_else(|| plans.first().cloned()),
                _ => None,
            };

            let Some(node) = default_node else {
                show_spawn_dialog.set(true);
                return;
            };
            let Some(plan) = default_plan else {
                show_spawn_dialog.set(true);
                return;
            };
            if node.node.id.is_empty() || plan.id.is_empty() || plan.tenant_id.trim().is_empty() {
                show_spawn_dialog.set(true);
                return;
            }

            let timestamp = js_sys::Date::now() as u64;
            spawn_worker_request.run(SpawnWorkerRequest {
                tenant_id: plan.tenant_id,
                node_id: node.node.id.clone(),
                plan_id: plan.id,
                uds_path: format!(
                    "var/run/aos-worker-{}-{}.sock",
                    adapteros_id::short_id(&node.node.id),
                    timestamp
                ),
            });
        }
    });
    let open_advanced_spawn = Callback::new(move |_| show_spawn_dialog.set(true));

    view! {
        <PageScaffold
            title="Workers"
            subtitle="Workers run inference requests. Spawn one, monitor health, and control lifecycle.".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Observe", "/workers"),
                PageBreadcrumbItem::current("Workers"),
            ]
        >
            <PageScaffoldPrimaryAction slot>
                <Button
                    variant=ButtonVariant::Primary
                    loading=Signal::from(action_loading)
                    disabled=Signal::derive(move || system_not_ready.get())
                    on_click=quick_spawn_from_defaults
                >
                    <IconPlus/>
                    "Spawn Worker"
                </Button>
            </PageScaffoldPrimaryAction>
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| {
                        refetch_workers.run(());
                        refetch_worker_health.run(());
                    })
                >
                    <IconRefresh/>
                    "Refresh"
                </Button>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| {
                        let new_show = !show_history.get_untracked();
                        show_history.set(new_show);
                        if !new_show {
                            // If we just hid history, selection may no longer be visible.
                            selected_worker.set(None);
                        }
                        refetch_workers.run(());
                    })
                >
                    {move || {
                        let (_active, total, hidden) = match workers.get() {
                            LoadingState::Loaded(ref ws) => {
                                let total = ws.len();
                                let active = ws
                                    .iter()
                                    .filter(|w| {
                                        if is_terminal_worker_status(&w.status) {
                                            return false;
                                        }
                                        let recent_seen = w
                                            .last_seen_at
                                            .as_deref()
                                            .is_some_and(|ts| is_recent_timestamp(ts, ACTIVE_WINDOW_SECS));
                                        let recent_start =
                                            is_recent_timestamp(&w.started_at, ACTIVE_WINDOW_SECS);
                                        recent_seen || recent_start
                                    })
                                    .count();
                                let hidden = total.saturating_sub(active);
                                (active, total, hidden)
                            }
                            _ => (0, 0, 0),
                        };

                        if show_history.get() {
                            if total > 0 {
                                format!("Hide Inactive History ({})", total)
                            } else {
                                "Hide Inactive History".to_string()
                            }
                        } else if hidden > 0 {
                            format!("Show Inactive History (+{})", hidden)
                        } else {
                            "Show Inactive History".to_string()
                        }
                    }}
                </Button>
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=Signal::derive(move || action_loading.get() || system_not_ready.get())
                    on_click=open_advanced_spawn
                >
                    "Advanced Spawn"
                </Button>
            </PageScaffoldActions>

            <h2 class="sr-only" data-testid="workers-page-heading">"Workers"</h2>

            <div class="rounded-lg border border-border/60 bg-muted/20 p-3">
                <p class="text-sm font-medium">"What is a worker?"</p>
                <p class="text-xs text-muted-foreground mt-1">
                    "A worker is a runtime process that serves inference requests for a model."
                </p>
                <p class="text-xs text-muted-foreground mt-1">
                    "Use Spawn to add capacity, Drain for graceful maintenance, and Stop for immediate shutdown."
                </p>
            </div>

            // Error banner
            {move || action_error.get().map(|e| view! {
                <InlineErrorBanner
                    message=e
                    on_dismiss=Callback::new(move |_| action_error.set(None))
                />
            })}

            // Main content
            {let notifications = notifications.clone();
            move || {
                let workers_state = workers.get();
                let nodes_list = match nodes.get() {
                    LoadingState::Loaded(n) => n,
                    _ => Vec::new(),
                };
                let plans_list = match plans.get() {
                    LoadingState::Loaded(p) => p,
                    _ => Vec::new(),
                };
                let health_state = worker_health.get();
                let health_map: HashMap<String, WorkerHealthRecord> = match &health_state {
                    LoadingState::Loaded(ref summary) => summary
                        .workers
                        .iter()
                        .cloned()
                        .map(|record| (record.worker_id.clone(), record))
                        .collect(),
                    _ => HashMap::new(),
                };

                match workers_state {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                                <SkeletonCard/>
                                <SkeletonCard/>
                                <SkeletonCard/>
                                <SkeletonCard/>
                            </div>
                            <SkeletonTable rows=5 columns=5/>
                        }.into_any()
                    }
                    LoadingState::Loaded(workers_data) => {
                        let health_summary = match &health_state {
                            LoadingState::Loaded(ref summary) => Some(summary.clone()),
                            _ => None,
                        };
                        let total_all = workers_data.len();
                        let active_workers: Vec<_> = workers_data
                            .iter()
                            .filter(|w| {
                                if is_terminal_worker_status(&w.status) {
                                    return false;
                                }
                                let recent_seen = w
                                    .last_seen_at
                                    .as_deref()
                                    .is_some_and(|ts| is_recent_timestamp(ts, ACTIVE_WINDOW_SECS));
                                let recent_start =
                                    is_recent_timestamp(&w.started_at, ACTIVE_WINDOW_SECS);
                                recent_seen || recent_start
                            })
                            .cloned()
                            .collect();
                        let hidden_count = total_all.saturating_sub(active_workers.len());

                        let visible_workers = if show_history.get() {
                            workers_data.clone()
                        } else {
                            active_workers.clone()
                        };

                        let workers_for_list = visible_workers.clone();
                        let workers_for_detail = visible_workers.clone();
                        let health_map_for_detail = health_map.clone();
                        view! {
                            // Summary cards (above the split panel)
                            <WorkersSummary
                                workers=visible_workers.clone()
                                health_summary=health_summary
                            />

                            <div class="text-xs text-muted-foreground mt-2">
                                {move || {
                                    if total_all == 0 {
                                        "No workers registered. A worker runs inference requests. Next: use Spawn Worker.".to_string()
                                    } else if !show_history.get() && active_workers.is_empty() && hidden_count > 0 {
                                        format!(
                                            "No active workers right now. {} inactive in history. Next: show inactive history to inspect status, then spawn a worker.",
                                            hidden_count
                                        )
                                    } else if show_history.get() {
                                        format!(
                                            "Workers run inference requests. Showing all {} workers, including inactive history.",
                                            total_all
                                        )
                                    } else if hidden_count > 0 {
                                        format!(
                                            "Workers run inference requests. Showing {} active workers ({} inactive hidden).",
                                            active_workers.len(),
                                            hidden_count
                                        )
                                    } else {
                                        format!(
                                            "Workers run inference requests. Showing {} active workers.",
                                            active_workers.len()
                                        )
                                    }
                                }}
                            </div>

                            // Split panel: Workers list (left) + Detail panel (right)
                            <SplitPanel
                                has_selection=has_selection
                                on_close=on_close_detail
                                back_label="Back to Workers"
                                ratio=SplitRatio::TwoFifthsThreeFifths
                                list_panel=move || {
                                    let workers_data = workers_for_list.clone();
                                    let health_map = health_map.clone();
                                    view! {
                                        <WorkersList
                                            workers=workers_data
                                            selected_worker=selected_worker
                                            health_map=health_map
                                            on_drain=Callback::new({
                                                move |worker_id: String| {
                                                    pending_drain_worker.set(Some(worker_id));
                                                    show_drain_confirm.set(true);
                                                }
                                            })
                                            on_stop=Callback::new({
                                                move |worker_id: String| {
                                                    pending_stop_worker.set(Some(worker_id));
                                                    show_stop_confirm.set(true);
                                                }
                                            })
                                            on_restart=Callback::new({
                                                move |worker_id: String| {
                                                    pending_restart_worker.set(Some(worker_id));
                                                    show_restart_confirm.set(true);
                                                }
                                            })
                                            on_remove=Callback::new({
                                                move |worker_id: String| {
                                                    pending_remove_worker.set(Some(worker_id));
                                                    show_remove_confirm.set(true);
                                                }
                                            })
                                            on_spawn=quick_spawn_from_defaults
                                        />
                                    }
                                }
                                detail_panel=move || {
                                    let workers_data = workers_for_detail.clone();
                                    let health_map = health_map_for_detail.clone();
                                    view! {
                                        {move || selected_worker.get().and_then(|worker_id| {
                                            let worker = workers_data.iter().find(|w| w.id == worker_id).cloned();
                                            let health = health_map.get(&worker_id).cloned();
                                            worker.map(|w| view! {
                                                <WorkerDetailPanel
                                                    worker=w
                                                    health=health
                                                    on_close=Callback::new(move |_| selected_worker.set(None))
                                                />
                                            })
                                        })}
                                    }
                                }
                            />

                            // Spawn dialog
                            <SpawnWorkerDialog
                                open=show_spawn_dialog
                                nodes=nodes_list
                                plans=plans_list
                                loading=Signal::from(action_loading)
                                on_spawn=spawn_worker_request
                            />

                            // Drain confirmation dialog
                            {
                                let drain_desc = {
                                    let wid = pending_drain_worker.get().unwrap_or_default();
                                    let name = adapteros_id::short_id(&wid);
                                    format!("Drain worker '{}'? New requests will be rejected while existing ones complete.", name)
                                };
                                view! {
                                    <ConfirmationDialog
                                        open=show_drain_confirm
                                        title="Drain Worker"
                                        description=drain_desc
                                        severity=ConfirmationSeverity::Warning
                                        confirm_text="Drain"
                                        on_confirm=Callback::new({
                                            let api_client = api_client.clone();
                                            let notifications = notifications.clone();
                                            move |_| {
                                                show_drain_confirm.set(false);
                                                if let Some(worker_id) = pending_drain_worker.get_untracked() {
                                                    action_loading.set(true);
                                                    let client = api_client.clone();
                                                    let notifications = notifications.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.drain_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                notifications.success("Worker draining", "Worker is draining and will stop accepting new requests.");
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(e.user_message()));
                                                                report_error_with_toast(&e, "Failed to drain worker", Some("/workers"), true);
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                                pending_drain_worker.set(None);
                                            }
                                        })
                                        on_cancel=Callback::new(move |_| {
                                            show_drain_confirm.set(false);
                                            pending_drain_worker.set(None);
                                        })
                                        loading=Signal::from(action_loading)
                                    />
                                }
                            }

                            // Stop confirmation dialog
                            {
                                let stop_desc = {
                                    let wid = pending_stop_worker.get().unwrap_or_default();
                                    let name = adapteros_id::short_id(&wid);
                                    format!("Stop worker '{}'? Active inference requests will be terminated.", name)
                                };
                                view! {
                                    <ConfirmationDialog
                                        open=show_stop_confirm
                                        title="Stop Worker"
                                        description=stop_desc
                                        severity=ConfirmationSeverity::Warning
                                        confirm_text="Stop"
                                        on_confirm=Callback::new({
                                            let api_client = api_client.clone();
                                            let notifications = notifications.clone();
                                            move |_| {
                                                show_stop_confirm.set(false);
                                                if let Some(worker_id) = pending_stop_worker.get_untracked() {
                                                    action_loading.set(true);
                                                    let client = api_client.clone();
                                                    let notifications = notifications.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.stop_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                notifications.success("Worker stopped", "Worker has been stopped.");
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(e.user_message()));
                                                                report_error_with_toast(&e, "Failed to stop worker", Some("/workers"), true);
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                                pending_stop_worker.set(None);
                                            }
                                        })
                                        on_cancel=Callback::new(move |_| {
                                            show_stop_confirm.set(false);
                                            pending_stop_worker.set(None);
                                        })
                                        loading=Signal::from(action_loading)
                                    />
                                }
                            }

                            // Restart confirmation dialog
                            {
                                let restart_desc = {
                                    let wid = pending_restart_worker.get().unwrap_or_default();
                                    let name = adapteros_id::short_id(&wid);
                                    format!("Restart worker '{}'? The process will be relaunched and in-flight requests may fail.", name)
                                };
                                view! {
                                    <ConfirmationDialog
                                        open=show_restart_confirm
                                        title="Restart Worker"
                                        description=restart_desc
                                        severity=ConfirmationSeverity::Warning
                                        confirm_text="Restart"
                                        on_confirm=Callback::new({
                                            let api_client = api_client.clone();
                                            let notifications = notifications.clone();
                                            move |_| {
                                                show_restart_confirm.set(false);
                                                if let Some(worker_id) = pending_restart_worker.get_untracked() {
                                                    action_loading.set(true);
                                                    let client = api_client.clone();
                                                    let notifications = notifications.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.restart_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                notifications.success("Worker restart requested", "Worker restart has been initiated.");
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(e.user_message()));
                                                                report_error_with_toast(&e, "Failed to restart worker", Some("/workers"), true);
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                                pending_restart_worker.set(None);
                                            }
                                        })
                                        on_cancel=Callback::new(move |_| {
                                            show_restart_confirm.set(false);
                                            pending_restart_worker.set(None);
                                        })
                                        loading=Signal::from(action_loading)
                                    />
                                }
                            }

                            // Remove confirmation dialog
                            {
                                let remove_desc = {
                                    let wid = pending_remove_worker.get().unwrap_or_default();
                                    let name = adapteros_id::short_id(&wid);
                                    format!("Remove worker '{}'? This decommissions the worker record and cannot be undone.", name)
                                };
                                view! {
                                    <ConfirmationDialog
                                        open=show_remove_confirm
                                        title="Remove Worker"
                                        description=remove_desc
                                        severity=ConfirmationSeverity::Destructive
                                        confirm_text="Remove"
                                        on_confirm=Callback::new({
                                            let api_client = api_client.clone();
                                            let notifications = notifications.clone();
                                            move |_| {
                                                show_remove_confirm.set(false);
                                                if let Some(worker_id) = pending_remove_worker.get_untracked() {
                                                    action_loading.set(true);
                                                    let client = api_client.clone();
                                                    let notifications = notifications.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.remove_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                notifications.success("Worker removed", "Worker has been decommissioned and removed.");
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(e.user_message()));
                                                                report_error_with_toast(&e, "Failed to remove worker", Some("/workers"), true);
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                                pending_remove_worker.set(None);
                                            }
                                        })
                                        on_cancel=Callback::new(move |_| {
                                            show_remove_confirm.set(false);
                                            pending_remove_worker.set(None);
                                        })
                                        loading=Signal::from(action_loading)
                                    />
                                }
                            }
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=refetch_workers.as_callback()
                            />
                        }.into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}

/// Worker detail page (for direct navigation)
#[component]
pub fn WorkerDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();

    let worker_id = move || params.with(|p| p.get("id").unwrap_or_default());

    // Fetch worker details
    let (worker, refetch_worker) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let id = worker_id();
            async move {
                if id.is_empty() {
                    return Err(crate::api::ApiError::Validation(
                        "Missing worker ID in route".to_string(),
                    ));
                }
                client.get_worker(&id).await
            }
        }
    });

    // Fetch worker metrics
    let (metrics, refetch_metrics) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let id = worker_id();
            async move {
                if id.is_empty() {
                    return Err(crate::api::ApiError::Validation(
                        "Missing worker ID in route".to_string(),
                    ));
                }
                client.get_worker_metrics(&id).await
            }
        }
    });

    // Set up polling for metrics with proper cleanup
    let _ = use_polling(3_000, move || async move {
        refetch_metrics.run(());
    });

    view! {
        <PageScaffold
            title="Worker Detail"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Observe", "/workers"),
                PageBreadcrumbItem::new("Workers", "/workers"),
                PageBreadcrumbItem::current(worker_id()),
            ]
        >
            <PageScaffoldActions slot>
                <div data-testid="worker-detail-refresh-page">
                    <RefreshButton on_click=Callback::new(move |_| {
                        refetch_worker.run(());
                        refetch_metrics.run(());
                    })/>
                </div>
            </PageScaffoldActions>

            {move || {
                let worker_state = worker.get();
                let metrics_state = metrics.get();

                match worker_state {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div data-testid="worker-detail-loading-state">
                                <LoadingDisplay message="Loading worker details..."/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(w) => {
                        let metrics_data = match metrics_state {
                            LoadingState::Loaded(m) => Some(m),
                            _ => None,
                        };
                        view! {
                            <WorkerDetailView
                                worker=w
                                metrics=metrics_data
                                on_refresh=Callback::new(move |_| {
                                    refetch_worker.run(());
                                    refetch_metrics.run(());
                                })
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) if e.is_not_found() => {
                        view! {
                            <div data-testid="worker-detail-error-state">
                                <div class="flex min-h-[40vh] flex-col items-center justify-center px-4">
                                    <div class="w-full max-w-md rounded-lg border bg-card p-8 text-center shadow-sm">
                                        <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                        <h2 class="heading-3 mb-2">"Worker not found"</h2>
                                        <p class="text-muted-foreground mb-6">
                                            "This worker may have been removed or doesn't exist."
                                        </p>
                                        <ButtonLink
                                            href="/workers"
                                            variant=ButtonVariant::Primary
                                            size=ButtonSize::Md
                                        >
                                            "View all workers"
                                        </ButtonLink>
                                    </div>
                                </div>
                            </div>
                        }
                            .into_any()
                    }
                    LoadingState::Error(e) => {
                        let show_backend_hint = matches!(e.code(), Some("INTERNAL_SERVER_ERROR"));
                        view! {
                            <div data-testid="worker-detail-error-state">
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| {
                                        refetch_worker.run(());
                                        refetch_metrics.run(());
                                    })
                                />
                                {show_backend_hint.then(|| view! {
                                    <div class="mt-3 rounded-md border border-border bg-muted/30 p-3 text-sm text-muted-foreground">
                                        "Worker details are temporarily unavailable from the backend. You can refresh, return to the workers list, or retry once backend migrations are healthy."
                                    </div>
                                })}
                                <div class="mt-3">
                                    <a href="/workers" class="text-sm text-primary hover:underline">
                                        "Back to Workers"
                                    </a>
                                </div>
                            </div>
                        }
                            .into_any()
                    }
                }
            }}
        </PageScaffold>
    }
}
