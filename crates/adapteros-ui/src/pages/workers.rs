//! Workers management page
//!
//! Comprehensive worker management with detailed status, metrics,
//! spawn controls, and lifecycle management.

use crate::api::{ApiClient, WorkerMetricsResponse};
use crate::components::{
    Badge, BadgeVariant, Card, Dialog, Input, Select, Spinner, StatusColor, StatusIndicator, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_navigate, LoadingState};
use adapteros_api_types::{NodeResponse, SpawnWorkerRequest, WorkerResponse};
use leptos::prelude::*;
use std::sync::Arc;

/// Workers management page
#[component]
pub fn Workers() -> impl IntoView {
    // Fetch workers list
    let (workers, refetch_workers) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_workers().await });

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

    // Store refetch for sharing
    let refetch_workers_signal = StoredValue::new(refetch_workers);

    // Set up polling interval (every 5 seconds for workers)
    Effect::new(move |_| {
        let interval_handle = gloo_timers::callback::Interval::new(5_000, move || {
            refetch_workers_signal.with_value(|f| f());
        });
        std::mem::forget(interval_handle);
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
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| refetch_workers_signal.with_value(|f| f())
                    >
                        <RefreshIcon/>
                        "Refresh"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                        on:click=move |_| show_spawn_dialog.set(true)
                    >
                        <PlusIcon/>
                        "Spawn Worker"
                    </button>
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
                            <CloseIcon/>
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

                match workers_state {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(workers_data) => {
                        view! {
                            // Summary cards
                            <WorkersSummary workers=workers_data.clone()/>

                            // Workers list
                            <WorkersList
                                workers=workers_data.clone()
                                selected_worker=selected_worker
                                on_drain=Callback::new({
                                    let refetch = refetch_workers_signal;
                                    move |worker_id: String| {
                                        action_loading.set(true);
                                        let client = ApiClient::new();
                                        let worker_id = worker_id.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            match client.drain_worker(&worker_id).await {
                                                Ok(_) => {
                                                    action_error.set(None);
                                                    refetch.with_value(|f| f());
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
                                    let refetch = refetch_workers_signal;
                                    move |worker_id: String| {
                                        action_loading.set(true);
                                        let client = ApiClient::new();
                                        let worker_id = worker_id.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            match client.stop_worker(&worker_id).await {
                                                Ok(_) => {
                                                    action_error.set(None);
                                                    refetch.with_value(|f| f());
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
                                worker.map(|w| view! {
                                    <WorkerDetailPanel
                                        worker=w
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
                                    let refetch = refetch_workers_signal;
                                    move |request: SpawnWorkerRequest| {
                                        action_loading.set(true);
                                        show_spawn_dialog.set(false);
                                        let client = ApiClient::new();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            match client.spawn_worker(&request).await {
                                                Ok(_) => {
                                                    action_error.set(None);
                                                    refetch.with_value(|f| f());
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
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive font-medium">"Failed to load workers"</p>
                                <p class="text-sm text-destructive/80 mt-1">{e.to_string()}</p>
                            </div>
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
    let navigate = use_navigate();

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

    let refetch_worker_signal = StoredValue::new(refetch_worker);
    let refetch_metrics_signal = StoredValue::new(refetch_metrics);

    // Set up polling for metrics
    Effect::new(move |_| {
        let interval_handle = gloo_timers::callback::Interval::new(3_000, move || {
            refetch_metrics_signal.with_value(|f| f());
        });
        std::mem::forget(interval_handle);
    });

    view! {
        <div class="space-y-6">
            // Back button
            <div class="flex items-center gap-4">
                <button
                    class="inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                    on:click=move |_| navigate("/workers")
                >
                    <BackIcon/>
                    "Back to Workers"
                </button>
            </div>

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
                                    refetch_worker_signal.with_value(|f| f());
                                    refetch_metrics_signal.with_value(|f| f());
                                })
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive font-medium">"Failed to load worker"</p>
                                <p class="text-sm text-destructive/80 mt-1">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

// ============================================================================
// Summary Cards
// ============================================================================

#[component]
fn WorkersSummary(workers: Vec<WorkerResponse>) -> impl IntoView {
    let total = workers.len();
    let healthy = workers.iter().filter(|w| w.status == "healthy").count();
    let draining = workers.iter().filter(|w| w.status == "draining").count();
    let error = workers
        .iter()
        .filter(|w| w.status == "error" || w.status == "stopped")
        .count();

    // Calculate total cache usage
    let total_cache_used: u32 = workers.iter().filter_map(|w| w.cache_used_mb).sum();
    let total_cache_max: u32 = workers.iter().filter_map(|w| w.cache_max_mb).sum();

    // Count unique backends (available for future use)
    let _backends: std::collections::HashSet<_> =
        workers.iter().filter_map(|w| w.backend.as_ref()).collect();

    view! {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-5">
            <Card title="Total Workers".to_string()>
                <div class="text-2xl font-bold">{total}</div>
                <p class="text-xs text-muted-foreground">"Registered workers"</p>
            </Card>

            <Card title="Healthy".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator color=StatusColor::Green pulsing=true/>
                    <span class="text-2xl font-bold">{healthy}</span>
                </div>
                <p class="text-xs text-muted-foreground">"Ready for inference"</p>
            </Card>

            <Card title="Draining".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator color=StatusColor::Yellow/>
                    <span class="text-2xl font-bold">{draining}</span>
                </div>
                <p class="text-xs text-muted-foreground">"Gracefully stopping"</p>
            </Card>

            <Card title="Error/Stopped".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator color=StatusColor::Red/>
                    <span class="text-2xl font-bold">{error}</span>
                </div>
                <p class="text-xs text-muted-foreground">"Need attention"</p>
            </Card>

            <Card title="Cache Usage".to_string()>
                <div class="text-2xl font-bold">
                    {if total_cache_max > 0 {
                        format!("{:.0}%", (total_cache_used as f64 / total_cache_max as f64) * 100.0)
                    } else {
                        "-".to_string()
                    }}
                </div>
                <p class="text-xs text-muted-foreground">
                    {format!("{} / {} MB", total_cache_used, total_cache_max)}
                </p>
            </Card>
        </div>
    }
}

// ============================================================================
// Workers List
// ============================================================================

#[component]
fn WorkersList(
    workers: Vec<WorkerResponse>,
    selected_worker: RwSignal<Option<String>>,
    on_drain: Callback<String>,
    on_stop: Callback<String>,
) -> impl IntoView {
    view! {
        <Card
            title="Workers".to_string()
            description="All registered workers and their current status".to_string()
        >
            {if workers.is_empty() {
                view! {
                    <div class="text-center py-12">
                        <ServerIcon class="h-12 w-12 mx-auto text-muted-foreground mb-4"/>
                        <p class="text-lg font-medium">"No Workers"</p>
                        <p class="text-muted-foreground mt-1">
                            "Spawn a worker to start processing inference requests"
                        </p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"ID"</TableHead>
                                <TableHead>"Node"</TableHead>
                                <TableHead>"Status"</TableHead>
                                <TableHead>"Backend"</TableHead>
                                <TableHead>"Model"</TableHead>
                                <TableHead>"Cache"</TableHead>
                                <TableHead>"Last Seen"</TableHead>
                                <TableHead class="text-right">"Actions"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {workers.into_iter().map(|worker| {
                                let worker_id = worker.id.clone();
                                let worker_id_drain = worker.id.clone();
                                let worker_id_stop = worker.id.clone();
                                let on_drain = on_drain.clone();
                                let on_stop = on_stop.clone();
                                let is_healthy = worker.status == "healthy";
                                let is_draining = worker.status == "draining";

                                view! {
                                    <WorkerRow
                                        worker=worker
                                        on_select=Callback::new(move |_| {
                                            selected_worker.set(Some(worker_id.clone()));
                                        })
                                        on_drain=Callback::new(move |_| {
                                            on_drain.run(worker_id_drain.clone());
                                        })
                                        on_stop=Callback::new(move |_| {
                                            on_stop.run(worker_id_stop.clone());
                                        })
                                        show_drain=is_healthy
                                        show_stop=is_healthy || is_draining
                                    />
                                }
                            }).collect::<Vec<_>>()}
                        </TableBody>
                    </Table>
                }.into_any()
            }}
        </Card>
    }
}

#[component]
fn WorkerRow(
    worker: WorkerResponse,
    on_select: Callback<()>,
    on_drain: Callback<()>,
    on_stop: Callback<()>,
    show_drain: bool,
    show_stop: bool,
) -> impl IntoView {
    let status_variant = match worker.status.as_str() {
        "healthy" => BadgeVariant::Success,
        "draining" => BadgeVariant::Warning,
        "registered" => BadgeVariant::Secondary,
        "error" | "stopped" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    };

    let short_id = if worker.id.len() > 12 {
        format!("{}...", &worker.id[..12])
    } else {
        worker.id.clone()
    };

    let backend = worker.backend.clone().unwrap_or_else(|| "-".to_string());
    let model = worker.model_id.clone().unwrap_or_else(|| "-".to_string());
    let short_model = if model.len() > 20 {
        format!("{}...", &model[..20])
    } else {
        model.clone()
    };

    let cache_display = match (worker.cache_used_mb, worker.cache_max_mb) {
        (Some(used), Some(max)) => {
            let pct = if max > 0 {
                (used as f64 / max as f64) * 100.0
            } else {
                0.0
            };
            format!("{}/{} MB ({:.0}%)", used, max, pct)
        }
        _ => "-".to_string(),
    };

    let last_seen = worker
        .last_seen_at
        .clone()
        .unwrap_or_else(|| "-".to_string());

    view! {
        <TableRow class="cursor-pointer hover:bg-muted/50">
            <TableCell>
                <button
                    class="font-mono text-sm text-primary hover:underline"
                    title=worker.id.clone()
                    on:click=move |_| on_select.run(())
                >
                    {short_id.clone()}
                </button>
            </TableCell>
            <TableCell>
                <span class="text-sm">{worker.node_id.clone()}</span>
            </TableCell>
            <TableCell>
                <Badge variant=status_variant>
                    {worker.status.clone()}
                </Badge>
            </TableCell>
            <TableCell>
                <span class="text-sm">{backend}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm font-mono" title=model>{short_model}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm">{cache_display}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{format_timestamp(&last_seen)}</span>
            </TableCell>
            <TableCell class="text-right">
                <div class="flex items-center justify-end gap-1">
                    {show_drain.then(|| view! {
                        <button
                            class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium hover:bg-accent"
                            on:click=move |_| on_drain.run(())
                        >
                            <PauseIcon/>
                            "Drain"
                        </button>
                    })}
                    {show_stop.then(|| view! {
                        <button
                            class="inline-flex items-center gap-1 rounded-md bg-destructive px-2 py-1 text-xs font-medium text-destructive-foreground hover:bg-destructive/90"
                            on:click=move |_| on_stop.run(())
                        >
                            <StopIcon/>
                            "Stop"
                        </button>
                    })}
                </div>
            </TableCell>
        </TableRow>
    }
}

// ============================================================================
// Worker Detail Panel (slide-out)
// ============================================================================

#[component]
fn WorkerDetailPanel(worker: WorkerResponse, on_close: Callback<()>) -> impl IntoView {
    let navigate = use_navigate();
    let worker_id = worker.id.clone();

    // Fetch metrics for this worker
    let (metrics, _refetch_metrics) = use_api_resource({
        let worker_id = worker.id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id.clone();
            async move { client.get_worker_metrics(&id).await }
        }
    });

    view! {
        <Card title="Worker Details".to_string()>
            <div class="space-y-6">
                // Header
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-3">
                        <span class="font-mono text-lg">{short_id(&worker.id)}</span>
                        <Badge variant=status_badge_variant(&worker.status)>
                            {worker.status.clone()}
                        </Badge>
                    </div>
                    <div class="flex items-center gap-2">
                        <button
                            class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-3 py-1 text-sm font-medium hover:bg-accent"
                            on:click={
                                let worker_id = worker_id.clone();
                                move |_| navigate(&format!("/workers/{}", worker_id))
                            }
                        >
                            "View Full Details"
                        </button>
                        <button
                            class="p-2 hover:bg-muted rounded"
                            on:click=move |_| on_close.run(())
                        >
                            <CloseIcon/>
                        </button>
                    </div>
                </div>

                // Info grid
                <div class="grid grid-cols-2 gap-4">
                    <DetailItem label="Worker ID" value=worker.id.clone()/>
                    <DetailItem label="Node ID" value=worker.node_id.clone()/>
                    <DetailItem label="Tenant ID" value=worker.tenant_id.clone()/>
                    <DetailItem label="Plan ID" value=worker.plan_id.clone()/>
                    <DetailItem label="Backend" value=worker.backend.clone().unwrap_or("-".to_string())/>
                    <DetailItem label="Model" value=worker.model_id.clone().unwrap_or("-".to_string())/>
                    <DetailItem label="PID" value=worker.pid.map(|p| p.to_string()).unwrap_or("-".to_string())/>
                    <DetailItem label="UDS Path" value=worker.uds_path.clone()/>
                    <DetailItem label="Started At" value=format_timestamp(&worker.started_at)/>
                    <DetailItem label="Last Seen" value=worker.last_seen_at.clone().map(|t| format_timestamp(&t)).unwrap_or("-".to_string())/>
                </div>

                // Capabilities
                {
                    let caps = worker.capabilities.clone();
                    (!caps.is_empty()).then(move || {
                        let cap_views: Vec<_> = caps.iter().map(|cap| {
                            let cap_text = cap.clone();
                            view! {
                                <Badge variant=BadgeVariant::Secondary>
                                    {cap_text}
                                </Badge>
                            }
                        }).collect();
                        view! {
                            <div>
                                <h4 class="text-sm font-medium mb-2">"Capabilities"</h4>
                                <div class="flex flex-wrap gap-2">
                                    {cap_views}
                                </div>
                            </div>
                        }
                    })
                }

                // Cache info
                <div>
                    <h4 class="text-sm font-medium mb-2">"Cache"</h4>
                    <div class="grid grid-cols-2 gap-4">
                        <DetailItem
                            label="Used"
                            value=worker.cache_used_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())
                        />
                        <DetailItem
                            label="Max"
                            value=worker.cache_max_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())
                        />
                        <DetailItem
                            label="Pinned Entries"
                            value=worker.cache_pinned_entries.map(|e| e.to_string()).unwrap_or("-".to_string())
                        />
                        <DetailItem
                            label="Active Entries"
                            value=worker.cache_active_entries.map(|e| e.to_string()).unwrap_or("-".to_string())
                        />
                    </div>
                </div>

                // Metrics (if loaded)
                {move || {
                    match metrics.get() {
                        LoadingState::Loaded(m) => view! {
                            <WorkerMetricsPanel metrics=m/>
                        }.into_any(),
                        LoadingState::Loading => view! {
                            <div class="flex items-center gap-2 text-muted-foreground">
                                <Spinner/>
                                <span>"Loading metrics..."</span>
                            </div>
                        }.into_any(),
                        _ => view! {}.into_any(),
                    }
                }}
            </div>
        </Card>
    }
}

#[component]
fn DetailItem(label: &'static str, value: String) -> impl IntoView {
    let value_clone = value.clone();
    view! {
        <div>
            <p class="text-xs text-muted-foreground">{label}</p>
            <p class="text-sm font-mono truncate" title=value>{value_clone}</p>
        </div>
    }
}

// ============================================================================
// Worker Detail View (full page)
// ============================================================================

#[component]
fn WorkerDetailView(
    worker: WorkerResponse,
    metrics: Option<WorkerMetricsResponse>,
    on_refresh: Callback<()>,
) -> impl IntoView {
    let action_loading = RwSignal::new(false);
    let action_error = RwSignal::new(Option::<String>::None);

    let is_healthy = worker.status == "healthy";
    let is_draining = worker.status == "draining";
    let worker_id = worker.id.clone();

    view! {
        <div class="space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <h1 class="text-2xl font-bold font-mono">{short_id(&worker.id)}</h1>
                    <Badge variant=status_badge_variant(&worker.status)>
                        {worker.status.clone()}
                    </Badge>
                </div>
                <div class="flex items-center gap-2">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| on_refresh.run(())
                    >
                        <RefreshIcon/>
                        "Refresh"
                    </button>
                    {is_healthy.then(|| {
                        let worker_id = worker_id.clone();
                        let on_refresh = on_refresh.clone();
                        view! {
                            <button
                                class="inline-flex items-center gap-2 rounded-md bg-secondary px-4 py-2 text-sm font-medium text-secondary-foreground hover:bg-secondary/80"
                                disabled=action_loading.get()
                                on:click=move |_| {
                                    action_loading.set(true);
                                    let client = ApiClient::new();
                                    let worker_id = worker_id.clone();
                                    let on_refresh = on_refresh.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.drain_worker(&worker_id).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                on_refresh.run(());
                                            }
                                            Err(e) => {
                                                action_error.set(Some(format!("Failed to drain: {}", e)));
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            >
                                <PauseIcon/>
                                "Drain"
                            </button>
                        }
                    })}
                    {(is_healthy || is_draining).then(|| {
                        let worker_id = worker.id.clone();
                        view! {
                            <button
                                class="inline-flex items-center gap-2 rounded-md bg-destructive px-4 py-2 text-sm font-medium text-destructive-foreground hover:bg-destructive/90"
                                disabled=action_loading.get()
                                on:click=move |_| {
                                    action_loading.set(true);
                                    let client = ApiClient::new();
                                    let worker_id = worker_id.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.stop_worker(&worker_id).await {
                                            Ok(_) => {
                                                // Navigate using window.location
                                                if let Some(window) = web_sys::window() {
                                                    let _ = window.location().set_href("/workers");
                                                }
                                            }
                                            Err(e) => {
                                                action_error.set(Some(format!("Failed to stop: {}", e)));
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            >
                                <StopIcon/>
                                "Stop"
                            </button>
                        }
                    })}
                </div>
            </div>

            // Error banner
            {move || action_error.get().map(|e| view! {
                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                    <p class="text-destructive">{e}</p>
                </div>
            })}

            // Basic info card
            <Card title="Worker Information".to_string()>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-6">
                    <DetailItem label="Worker ID" value=worker.id.clone()/>
                    <DetailItem label="Node ID" value=worker.node_id.clone()/>
                    <DetailItem label="Tenant ID" value=worker.tenant_id.clone()/>
                    <DetailItem label="Plan ID" value=worker.plan_id.clone()/>
                    <DetailItem label="Backend" value=worker.backend.clone().unwrap_or("-".to_string())/>
                    <DetailItem label="Model ID" value=worker.model_id.clone().unwrap_or("-".to_string())/>
                    <DetailItem label="Model Hash" value=worker.model_hash.clone().map(|h| short_hash(&h)).unwrap_or("-".to_string())/>
                    <DetailItem label="Model Loaded" value=if worker.model_loaded { "Yes".to_string() } else { "No".to_string() }/>
                    <DetailItem label="PID" value=worker.pid.map(|p| p.to_string()).unwrap_or("-".to_string())/>
                    <DetailItem label="UDS Path" value=worker.uds_path.clone()/>
                    <DetailItem label="Started At" value=format_timestamp(&worker.started_at)/>
                    <DetailItem label="Last Seen" value=worker.last_seen_at.clone().map(|t| format_timestamp(&t)).unwrap_or("-".to_string())/>
                </div>

                // Capabilities
                {
                    let caps = worker.capabilities.clone();
                    (!caps.is_empty()).then(move || {
                        let cap_views: Vec<_> = caps.iter().map(|cap| {
                            let cap_text = cap.clone();
                            view! {
                                <Badge variant=BadgeVariant::Secondary>
                                    {cap_text}
                                </Badge>
                            }
                        }).collect();
                        view! {
                            <div class="mt-6 pt-6 border-t">
                                <h4 class="text-sm font-medium mb-3">"Capabilities"</h4>
                                <div class="flex flex-wrap gap-2">
                                    {cap_views}
                                </div>
                            </div>
                        }
                    })
                }
            </Card>

            // Cache card
            <Card title="Cache Status".to_string()>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-6">
                    <div>
                        <p class="text-xs text-muted-foreground mb-1">"Memory Used"</p>
                        <p class="text-2xl font-bold">
                            {worker.cache_used_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())}
                        </p>
                    </div>
                    <div>
                        <p class="text-xs text-muted-foreground mb-1">"Memory Max"</p>
                        <p class="text-2xl font-bold">
                            {worker.cache_max_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())}
                        </p>
                    </div>
                    <div>
                        <p class="text-xs text-muted-foreground mb-1">"Pinned Entries"</p>
                        <p class="text-2xl font-bold">
                            {worker.cache_pinned_entries.map(|e| e.to_string()).unwrap_or("-".to_string())}
                        </p>
                    </div>
                    <div>
                        <p class="text-xs text-muted-foreground mb-1">"Active Entries"</p>
                        <p class="text-2xl font-bold">
                            {worker.cache_active_entries.map(|e| e.to_string()).unwrap_or("-".to_string())}
                        </p>
                    </div>
                </div>

                // Cache usage bar
                {worker.cache_used_mb.zip(worker.cache_max_mb).map(|(used, max)| {
                    let pct = if max > 0 { (used as f64 / max as f64) * 100.0 } else { 0.0 };
                    let color = if pct > 90.0 { "bg-destructive" }
                        else if pct > 70.0 { "bg-yellow-500" }
                        else { "bg-primary" };
                    view! {
                        <div class="mt-6">
                            <div class="flex items-center justify-between mb-2">
                                <span class="text-sm text-muted-foreground">"Usage"</span>
                                <span class="text-sm font-medium">{format!("{:.1}%", pct)}</span>
                            </div>
                            <div class="h-2 bg-muted rounded-full overflow-hidden">
                                <div
                                    class=format!("h-full {} transition-all duration-300", color)
                                    style=format!("width: {}%", pct.min(100.0))
                                />
                            </div>
                        </div>
                    }
                })}
            </Card>

            // Metrics card
            {metrics.map(|m| view! {
                <WorkerMetricsCard metrics=m/>
            })}
        </div>
    }
}

#[component]
fn WorkerMetricsPanel(metrics: WorkerMetricsResponse) -> impl IntoView {
    view! {
        <div>
            <h4 class="text-sm font-medium mb-3">"Performance Metrics"</h4>
            <div class="grid grid-cols-2 gap-4">
                <DetailItem
                    label="Requests Processed"
                    value=metrics.requests_processed.to_string()
                />
                <DetailItem
                    label="Requests/sec"
                    value=format!("{:.2}", metrics.requests_per_second)
                />
                <DetailItem
                    label="Avg Latency"
                    value=metrics.avg_latency_ms.map(|l| format!("{:.1} ms", l)).unwrap_or("-".to_string())
                />
                <DetailItem
                    label="P99 Latency"
                    value=metrics.p99_latency_ms.map(|l| format!("{:.1} ms", l)).unwrap_or("-".to_string())
                />
            </div>
        </div>
    }
}

#[component]
fn WorkerMetricsCard(metrics: WorkerMetricsResponse) -> impl IntoView {
    view! {
        <Card title="Performance Metrics".to_string()>
            <div class="grid grid-cols-2 md:grid-cols-4 gap-6">
                // Request metrics
                <MetricTile
                    label="Requests Processed"
                    value=metrics.requests_processed.to_string()
                />
                <MetricTile
                    label="Requests/sec"
                    value=format!("{:.2}", metrics.requests_per_second)
                />
                <MetricTile
                    label="Avg Latency"
                    value=metrics.avg_latency_ms.map(|l| format!("{:.1} ms", l)).unwrap_or("-".to_string())
                />
                <MetricTile
                    label="P99 Latency"
                    value=metrics.p99_latency_ms.map(|l| format!("{:.1} ms", l)).unwrap_or("-".to_string())
                />

                // Resource metrics
                <MetricTile
                    label="CPU Usage"
                    value=metrics.cpu_utilization_pct.map(|p| format!("{:.1}%", p)).unwrap_or("-".to_string())
                />
                <MetricTile
                    label="Memory"
                    value=match (metrics.memory_used_mb, metrics.memory_limit_mb) {
                        (Some(used), Some(limit)) => format!("{}/{} MB", used, limit),
                        (Some(used), None) => format!("{} MB", used),
                        _ => "-".to_string(),
                    }
                />
                <MetricTile
                    label="GPU Memory"
                    value=match (metrics.gpu_memory_used_mb, metrics.gpu_memory_total_mb) {
                        (Some(used), Some(total)) => format!("{}/{} MB", used, total),
                        (Some(used), None) => format!("{} MB", used),
                        _ => "-".to_string(),
                    }
                />
                <MetricTile
                    label="GPU Utilization"
                    value=metrics.gpu_utilization_pct.map(|p| format!("{:.1}%", p)).unwrap_or("-".to_string())
                />

                // Cache metrics
                <MetricTile
                    label="Uptime"
                    value=format_uptime(metrics.uptime_seconds)
                />
                <MetricTile
                    label="Cache Entries"
                    value=metrics.cache_entries.map(|e| e.to_string()).unwrap_or("-".to_string())
                />
                <MetricTile
                    label="Cache Hit Rate"
                    value=metrics.cache_hit_rate.map(|r| format!("{:.1}%", r * 100.0)).unwrap_or("-".to_string())
                />
            </div>

            // Resource usage charts (simplified bar representation)
            <div class="mt-6 pt-6 border-t space-y-4">
                <h4 class="text-sm font-medium">"Resource Usage"</h4>

                // Memory bar
                {metrics.memory_used_mb.zip(metrics.memory_limit_mb).map(|(used, limit)| {
                    let pct = if limit > 0 { (used as f64 / limit as f64) * 100.0 } else { 0.0 };
                    view! {
                        <ResourceBar
                            label="Memory"
                            value=format!("{} MB / {} MB", used, limit)
                            percentage=pct
                        />
                    }
                })}

                // GPU Memory bar
                {metrics.gpu_memory_used_mb.zip(metrics.gpu_memory_total_mb).map(|(used, total)| {
                    let pct = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };
                    view! {
                        <ResourceBar
                            label="GPU Memory"
                            value=format!("{} MB / {} MB", used, total)
                            percentage=pct
                        />
                    }
                })}

                // GPU Utilization bar
                {metrics.gpu_utilization_pct.map(|pct| {
                    view! {
                        <ResourceBar
                            label="GPU Utilization"
                            value=format!("{:.1}%", pct)
                            percentage=pct
                        />
                    }
                })}

                // CPU Utilization bar
                {metrics.cpu_utilization_pct.map(|pct| {
                    view! {
                        <ResourceBar
                            label="CPU Utilization"
                            value=format!("{:.1}%", pct)
                            percentage=pct
                        />
                    }
                })}
            </div>
        </Card>
    }
}

#[component]
fn MetricTile(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="p-4 rounded-lg border">
            <p class="text-xs text-muted-foreground mb-1">{label}</p>
            <p class="text-xl font-bold">{value}</p>
        </div>
    }
}

#[component]
fn ResourceBar(label: &'static str, value: String, percentage: f64) -> impl IntoView {
    let color = if percentage > 90.0 {
        "bg-destructive"
    } else if percentage > 70.0 {
        "bg-yellow-500"
    } else {
        "bg-primary"
    };

    view! {
        <div>
            <div class="flex items-center justify-between mb-1">
                <span class="text-sm">{label}</span>
                <span class="text-sm text-muted-foreground">{value}</span>
            </div>
            <div class="h-2 bg-muted rounded-full overflow-hidden">
                <div
                    class=format!("h-full {} transition-all duration-300", color)
                    style=format!("width: {}%", percentage.min(100.0))
                />
            </div>
        </div>
    }
}

// ============================================================================
// Spawn Worker Dialog
// ============================================================================

/// Local plan option type for spawn form
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanOption {
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub status: String,
}

#[component]
fn SpawnWorkerDialog(
    open: RwSignal<bool>,
    nodes: Vec<NodeResponse>,
    plans: Vec<PlanOption>,
    on_spawn: Callback<SpawnWorkerRequest>,
) -> impl IntoView {
    // Form state
    let tenant_id = RwSignal::new(String::new());
    let node_id = RwSignal::new(String::new());
    let plan_id = RwSignal::new(String::new());
    let uds_path = RwSignal::new(String::new());

    // Validation
    let is_valid = move || {
        !tenant_id.get().is_empty()
            && !node_id.get().is_empty()
            && !plan_id.get().is_empty()
            && !uds_path.get().is_empty()
    };

    // Build node options
    let node_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a node...".to_string()))
            .chain(
                nodes
                    .iter()
                    .map(|n| (n.id.clone(), format!("{} ({})", n.hostname, n.id))),
            )
            .collect();

    // Build plan options
    let plan_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "Select a plan...".to_string()))
            .chain(plans.iter().map(|p| {
                (
                    p.id.clone(),
                    format!("{} ({})", short_hash(&p.manifest_hash_b3), p.id),
                )
            }))
            .collect();

    // Auto-generate UDS path when node is selected
    Effect::new(move || {
        let node = node_id.get();
        if !node.is_empty() && uds_path.get().is_empty() {
            let timestamp = js_sys::Date::now() as u64;
            uds_path.set(format!(
                "/tmp/aos-worker-{}-{}.sock",
                short_id(&node),
                timestamp
            ));
        }
    });

    view! {
        <Dialog
            open=open
            title="Spawn New Worker".to_string()
            description="Configure and spawn a new inference worker".to_string()
        >
            <div class="space-y-4">
                <Input
                    value=tenant_id
                    label="Tenant ID".to_string()
                    placeholder="Enter tenant ID".to_string()
                />

                <Select
                    value=node_id
                    options=node_options
                    label="Node".to_string()
                />

                <Select
                    value=plan_id
                    options=plan_options
                    label="Plan".to_string()
                />

                <Input
                    value=uds_path
                    label="UDS Path".to_string()
                    placeholder="/tmp/aos-worker.sock".to_string()
                />

                <div class="flex justify-end gap-2 pt-4">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| {
                            open.set(false);
                            // Reset form
                            tenant_id.set(String::new());
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                        disabled=move || !is_valid()
                        on:click=move |_| {
                            let request = SpawnWorkerRequest {
                                tenant_id: tenant_id.get(),
                                node_id: node_id.get(),
                                plan_id: plan_id.get(),
                                uds_path: uds_path.get(),
                            };
                            on_spawn.run(request);
                            // Reset form
                            tenant_id.set(String::new());
                            node_id.set(String::new());
                            plan_id.set(String::new());
                            uds_path.set(String::new());
                        }
                    >
                        "Spawn Worker"
                    </button>
                </div>
            </div>
        </Dialog>
    }
}

// ============================================================================
// Icons
// ============================================================================

#[component]
fn RefreshIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
        </svg>
    }
}

#[component]
fn PlusIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
        </svg>
    }
}

#[component]
fn CloseIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
        </svg>
    }
}

#[component]
fn BackIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 19l-7-7m0 0l7-7m-7 7h18"/>
        </svg>
    }
}

#[component]
fn PauseIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 9v6m4-6v6m7-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </svg>
    }
}

#[component]
fn StopIcon() -> impl IntoView {
    view! {
        <svg
            class="h-4 w-4"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z"/>
        </svg>
    }
}

#[component]
fn ServerIcon(#[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <svg
            class=class
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
        >
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"/>
        </svg>
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_timestamp(timestamp: &str) -> String {
    if timestamp == "-" || timestamp.is_empty() {
        return "-".to_string();
    }
    if timestamp.contains('T') {
        if let Some(time_part) = timestamp.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
            return time.to_string();
        }
    }
    timestamp.to_string()
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

fn short_hash(hash: &str) -> String {
    if hash.len() > 8 {
        format!("{}...", &hash[..8])
    } else {
        hash.to_string()
    }
}

fn status_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "healthy" => BadgeVariant::Success,
        "draining" => BadgeVariant::Warning,
        "registered" => BadgeVariant::Secondary,
        "error" | "stopped" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}
