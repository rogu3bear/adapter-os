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

use crate::api::ApiClient;
use crate::components::{
    BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, ErrorDisplay, LoadingDisplay,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, SplitPanel, SplitRatio,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use adapteros_api_types::SpawnWorkerRequest;
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::{IconPlus, IconRefresh, IconX};
use components::{WorkerDetailPanel, WorkerDetailView, WorkersList, WorkersSummary};
use dialogs::{PlanOption, SpawnWorkerDialog};
use utils::{is_recent_timestamp, is_terminal_worker_status, WorkerHealthRecord, WorkerHealthSummary};

/// Workers management page
#[component]
pub fn Workers() -> impl IntoView {
    const ACTIVE_WINDOW_SECS: u64 = 5 * 60;

    // Fetch workers list
    let (workers, refetch_workers) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_workers().await });

    // Fetch worker health summary (health status + incident counts)
    let (worker_health, refetch_worker_health) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client
                .get::<WorkerHealthSummary>("/v1/workers/health/summary")
                .await
        });

    // Fetch nodes for spawn form
    let (nodes, _refetch_nodes) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_nodes().await });

    // Fetch plans for spawn form
    let (plans, _refetch_plans) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get::<Vec<PlanOption>>("/v1/plans").await
    });

    // Dialog state
    let show_spawn_dialog = RwSignal::new(false);
    let selected_worker = RwSignal::new(Option::<String>::None);
    let show_history = RwSignal::new(false);
    let action_loading = RwSignal::new(false);
    let action_error = RwSignal::new(Option::<String>::None);

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let LoadingState::Loaded(ref w) = workers.get() {
            web_sys::console::log_1(&format!("[list] workers: {} items", w.len()).into());
        }
    });

    // Set up polling interval (every 10 seconds for workers)
    // Using use_polling hook which properly cleans up on unmount
    let _ = use_polling(10_000, move || async move {
        refetch_workers.run(());
        refetch_worker_health.run(());
    });

    // Has selection for split panel
    let has_selection = Signal::derive(move || selected_worker.get().is_some());

    let on_close_detail = Callback::new(move |_: ()| {
        selected_worker.set(None);
    });

    view! {
        <PageScaffold
            title="Workers"
            subtitle="Manage inference workers, monitor health, and control lifecycle".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Observe", "/workers"),
                PageBreadcrumbItem::current("Workers"),
            ]
        >
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
                    })
                >
                    {move || {
                        let (active, total, hidden) = match workers.get() {
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
                                format!("Hide History ({})", total)
                            } else {
                                "Hide History".to_string()
                            }
                        } else if hidden > 0 {
                            format!("Show History (+{})", hidden)
                        } else if active > 0 {
                            "Show History".to_string()
                        } else {
                            "Show History".to_string()
                        }
                    }}
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_spawn_dialog.set(true))
                >
                    <IconPlus/>
                    "Spawn Worker"
                </Button>
            </PageScaffoldActions>

            // Error banner
            {move || action_error.get().map(|e| view! {
                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                    <div class="flex items-center justify-between">
                        <p class="text-destructive font-medium">{e}</p>
                        <button
                            class="text-destructive hover:text-destructive/80"
                            on:click=move |_| action_error.set(None)
                        >
                            <IconX/>
                        </button>
                    </div>
                </div>
            })}

            // Main content
            {move || {
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
                            <LoadingDisplay message="Loading workers..."/>
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
                            .cloned()
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
                                    if show_history.get() {
                                        format!("Showing all {} workers (including stopped/error).", total_all)
                                    } else if hidden_count > 0 {
                                        format!(
                                            "Showing {} active workers (recent) ({} hidden).",
                                            active_workers.len(),
                                            hidden_count
                                        )
                                    } else {
                                        format!("Showing {} active workers (recent).", active_workers.len())
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
                                                    action_loading.set(true);
                                                    let client = ApiClient::new();
                                                    let worker_id = worker_id.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.drain_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(format!("Failed to drain worker: {}", e)));
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                            })
                                            on_stop=Callback::new({
                                                move |worker_id: String| {
                                                    action_loading.set(true);
                                                    let client = ApiClient::new();
                                                    let worker_id = worker_id.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        match client.stop_worker(&worker_id).await {
                                                            Ok(_) => {
                                                                action_error.set(None);
                                                                refetch_workers.run(());
                                                            }
                                                            Err(e) => {
                                                                action_error.set(Some(format!("Failed to stop worker: {}", e)));
                                                            }
                                                        }
                                                        action_loading.set(false);
                                                    });
                                                }
                                            })
                                            on_spawn=Callback::new(move |_| show_spawn_dialog.set(true))
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
                                on_spawn=Callback::new({
                                    move |request: SpawnWorkerRequest| {
                                        action_loading.set(true);
                                        show_spawn_dialog.set(false);
                                        let client = ApiClient::new();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            match client.spawn_worker(&request).await {
                                                Ok(_) => {
                                                    action_error.set(None);
                                                    refetch_workers.run(());
                                                }
                                                Err(e) => {
                                                    action_error.set(Some(format!("Failed to spawn worker: {}", e)));
                                                }
                                            }
                                            action_loading.set(false);
                                        });
                                    }
                                })
                            />
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
            async move { client.get_worker(&id).await }
        }
    });

    // Fetch worker metrics
    let (metrics, refetch_metrics) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let id = worker_id();
            async move { client.get_worker_metrics(&id).await }
        }
    });

    // Set up polling for metrics with proper cleanup
    let _ = use_polling(3_000, move || async move {
        refetch_metrics.run(());
    });

    view! {
        <div class="space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Workers", "/workers"),
                BreadcrumbItem::current(worker_id()),
            ]/>

            {move || {
                let worker_state = worker.get();
                let metrics_state = metrics.get();

                match worker_state {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading worker details..."/>
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
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| {
                                    refetch_worker.run(());
                                    refetch_metrics.run(());
                                })
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
