//! Workers management page
//!
//! Comprehensive worker management with detailed status, metrics,
//! spawn controls, and lifecycle management.

mod components;
pub mod dialogs;
mod utils;

use crate::api::ApiClient;
use crate::components::{
    BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, ErrorDisplay, Spinner,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use adapteros_api_types::SpawnWorkerRequest;
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::{IconPlus, IconRefresh, IconX};
use components::{WorkerDetailPanel, WorkerDetailView, WorkersList, WorkersSummary};
use dialogs::{PlanOption, SpawnWorkerDialog};
use utils::{WorkerHealthRecord, WorkerHealthSummary};

/// Workers management page
#[component]
pub fn Workers() -> impl IntoView {
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

    view! {
        <div class="space-y-6">
            // Header with title and actions
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Workers"</h1>
                    <p class="text-muted-foreground mt-1">
                        "Manage inference workers, monitor health, and control lifecycle"
                    </p>
                </div>
                <div class="flex items-center gap-2">
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
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| show_spawn_dialog.set(true))
                    >
                        <IconPlus/>
                        "Spawn Worker"
                    </Button>
                </div>
            </div>

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
                let health_map: HashMap<String, WorkerHealthRecord> = match worker_health.get() {
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
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(workers_data) => {
                        let health_summary = match worker_health.get() {
                            LoadingState::Loaded(ref summary) => Some(summary.clone()),
                            _ => None,
                        };
                        view! {
                            // Summary cards
                            <WorkersSummary
                                workers=workers_data.clone()
                                health_summary=health_summary
                            />

                            // Workers list
                            <WorkersList
                                workers=workers_data.clone()
                                selected_worker=selected_worker
                                health_map=health_map.clone()
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
                            />

                            // Worker detail panel
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
                                on_retry=refetch_workers
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Worker detail page (for direct navigation)
#[component]
pub fn WorkerDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();

    let worker_id = move || params.with(|p| p.get("id").unwrap_or_default());

    // Fetch worker details
    let (worker, refetch_worker) = use_api_resource({
        let worker_id = worker_id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id();
            async move { client.get_worker(&id).await }
        }
    });

    // Fetch worker metrics
    let (metrics, refetch_metrics) = use_api_resource({
        let worker_id = worker_id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id();
            async move { client.get_worker_metrics(&id).await }
        }
    });

    // Set up polling for metrics
    Effect::new(move |_| {
        let interval_handle = gloo_timers::callback::Interval::new(3_000, move || {
            refetch_metrics.run(());
        });
        std::mem::forget(interval_handle);
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
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
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
