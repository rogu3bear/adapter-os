//! Workers page view components
//!
//! Subcomponents for displaying worker lists, details, and metrics.

use crate::api::{ApiClient, WorkerMetricsResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Spinner, StatusColor,
    StatusIndicator, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_navigate, LoadingState};
use adapteros_api_types::WorkerResponse;
use leptos::prelude::*;
use std::sync::Arc;

use super::utils::{
    format_timestamp, format_uptime, short_hash, short_id, status_badge_variant, WORKERS_PAGE_SIZE,
};
use crate::components::{IconPause, IconRefresh, IconServer, IconStop, IconX};

// ============================================================================
// Summary Cards
// ============================================================================

#[component]
pub fn WorkersSummary(workers: Vec<WorkerResponse>) -> impl IntoView {
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
pub fn WorkersList(
    workers: Vec<WorkerResponse>,
    selected_worker: RwSignal<Option<String>>,
    on_drain: Callback<String>,
    on_stop: Callback<String>,
) -> impl IntoView {
    let total = workers.len();

    // Client-side pagination to reduce DOM nodes
    let visible_count = RwSignal::new(WORKERS_PAGE_SIZE.min(total));

    let show_more = move |_| {
        visible_count.update(|c| *c = (*c + WORKERS_PAGE_SIZE).min(total));
    };

    view! {
        <Card
            title="Workers".to_string()
            description="All registered workers and their current status".to_string()
        >
            {if workers.is_empty() {
                view! {
                    <div class="text-center py-12">
                        <IconServer class="h-12 w-12 mx-auto text-muted-foreground mb-4"/>
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
                            {move || {
                                let count = visible_count.get();
                                let on_drain = on_drain.clone();
                                let on_stop = on_stop.clone();
                                workers.iter().take(count).map(|worker| {
                                    let worker_id = worker.id.clone();
                                    let worker_id_drain = worker.id.clone();
                                    let worker_id_stop = worker.id.clone();
                                    let on_drain = on_drain.clone();
                                    let on_stop = on_stop.clone();
                                    let is_healthy = worker.status == "healthy";
                                    let is_draining = worker.status == "draining";

                                    view! {
                                        <WorkerRow
                                            worker=worker.clone()
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
                                }).collect::<Vec<_>>()
                            }}
                        </TableBody>
                    </Table>

                    // Show more button if there are hidden items
                    {move || {
                        let count = visible_count.get();
                        let remaining = total.saturating_sub(count);
                        if remaining > 0 {
                            view! {
                                <div class="flex items-center justify-center py-4 border-t">
                                    <button
                                        class="text-sm text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
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
                }.into_any()
            }}
        </Card>
    }
}

#[component]
pub fn WorkerRow(
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

    let short_worker_id = if worker.id.len() > 12 {
        format!("{}...", &worker.id[..12])
    } else {
        worker.id.clone()
    };

    let backend = worker
        .backend
        .clone()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());
    let model = worker
        .model_id
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "Not assigned".to_string());
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
                    class="font-mono text-sm text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                    title=worker.id.clone()
                    on:click=move |_| on_select.run(())
                >
                    {short_worker_id.clone()}
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
                        <Button
                            variant=ButtonVariant::Ghost
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| on_drain.run(()))
                        >
                            <IconPause/>
                            "Drain"
                        </Button>
                    })}
                    {show_stop.then(|| view! {
                        <Button
                            variant=ButtonVariant::Destructive
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| on_stop.run(()))
                        >
                            <IconStop/>
                            "Stop"
                        </Button>
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
pub fn WorkerDetailPanel(worker: WorkerResponse, on_close: Callback<()>) -> impl IntoView {
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
                        <Button
                            variant=ButtonVariant::Secondary
                            size=ButtonSize::Sm
                            on_click=Callback::new({
                                let worker_id = worker_id.clone();
                                move |_| navigate(&format!("/workers/{}", worker_id))
                            })
                        >
                            "View Full Details"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            size=ButtonSize::IconSm
                            aria_label="Close details".to_string()
                            on_click=Callback::new(move |_| on_close.run(()))
                        >
                            <IconX/>
                        </Button>
                    </div>
                </div>

                // Info grid
                <div class="grid grid-cols-2 gap-4">
                    <DetailItem label="Worker ID" value=worker.id.clone()/>
                    <DetailItem label="Node ID" value=worker.node_id.clone()/>
                    <DetailItem label="Tenant ID" value=worker.tenant_id.clone()/>
                    <DetailItem label="Plan ID" value=worker.plan_id.clone()/>
                    <DetailItem label="Backend" value=worker.backend.clone().filter(|b| !b.is_empty()).unwrap_or_else(|| "Unknown".to_string())/>
                    <DetailItem label="Model" value=worker.model_id.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "Not assigned".to_string())/>
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
pub fn DetailItem(label: &'static str, value: String) -> impl IntoView {
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
pub fn WorkerDetailView(
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
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new(move |_| on_refresh.run(()))
                    >
                        <IconRefresh/>
                        "Refresh"
                    </Button>
                    {is_healthy.then(|| {
                        let worker_id = worker_id.clone();
                        let on_refresh = on_refresh.clone();
                        view! {
                            <Button
                                variant=ButtonVariant::Secondary
                                disabled=Signal::from(action_loading)
                                on_click=Callback::new(move |_| {
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
                                })
                            >
                                <IconPause/>
                                "Drain"
                            </Button>
                        }
                    })}
                    {(is_healthy || is_draining).then(|| {
                        let worker_id = worker.id.clone();
                        view! {
                            <Button
                                variant=ButtonVariant::Destructive
                                disabled=Signal::from(action_loading)
                                on_click=Callback::new(move |_| {
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
                                })
                            >
                                <IconStop/>
                                "Stop"
                            </Button>
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
                    <DetailItem label="Backend" value=worker.backend.clone().filter(|b| !b.is_empty()).unwrap_or_else(|| "Unknown".to_string())/>
                    <DetailItem label="Model ID" value=worker.model_id.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "Not assigned".to_string())/>
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
                        else if pct > 70.0 { "bg-status-warning" }
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

// ============================================================================
// Worker Metrics Components
// ============================================================================

#[component]
pub fn WorkerMetricsPanel(metrics: WorkerMetricsResponse) -> impl IntoView {
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
pub fn WorkerMetricsCard(metrics: WorkerMetricsResponse) -> impl IntoView {
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
pub fn MetricTile(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="p-4 rounded-lg border">
            <p class="text-xs text-muted-foreground mb-1">{label}</p>
            <p class="text-xl font-bold">{value}</p>
        </div>
    }
}

#[component]
pub fn ResourceBar(label: &'static str, value: String, percentage: f64) -> impl IntoView {
    let color = if percentage > 90.0 {
        "bg-destructive"
    } else if percentage > 70.0 {
        "bg-status-warning"
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
