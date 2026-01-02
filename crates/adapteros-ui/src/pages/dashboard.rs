//! Dashboard page

use crate::api::{use_sse_json, ApiClient, SseState};
use crate::components::{Badge, BadgeVariant, Card, Shell, Spinner, StatusIndicator, StatusColor};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{SystemMetricsResponse, SystemStatusResponse, StatusIndicator as ApiStatusIndicator, InferenceReadyState, WorkerResponse};
use leptos::prelude::*;
use std::sync::Arc;

/// Dashboard page
#[component]
pub fn Dashboard() -> impl IntoView {
    // Fetch system status
    let (status, refetch) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.system_status().await
    });

    // Fetch workers list
    let (workers, refetch_workers) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.list_workers().await
    });

    // Live metrics from SSE stream - updated in real-time
    let live_metrics: RwSignal<Option<SystemMetricsResponse>> = RwSignal::new(None);

    // SSE connection for real-time metrics updates
    let (sse_status, _sse_reconnect) = use_sse_json::<SystemMetricsResponse, _>(
        "/api/v1/stream/metrics",
        move |metrics| {
            live_metrics.set(Some(metrics));
        },
    );

    // Refetch functions stored for use in closures
    let refetch_signal = StoredValue::new(refetch);
    let refetch_workers_signal = StoredValue::new(refetch_workers);

    // Refetch all data (SSE reconnection handled separately due to non-Send types)
    let refetch_all = move || {
        refetch_signal.with_value(|f| f());
        refetch_workers_signal.with_value(|f| f());
    };

    view! {
        <Shell>
            <div class="space-y-6">
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-4">
                        <h1 class="text-3xl font-bold tracking-tight">"Dashboard"</h1>
                        <SseIndicator state=sse_status/>
                    </div>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                        on:click=move |_| refetch_all()
                    >
                        "Refresh"
                    </button>
                </div>

                {move || {
                    match status.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            let workers_data = match workers.get() {
                                LoadingState::Loaded(w) => w,
                                _ => Vec::new(),
                            };
                            view! {
                                <DashboardContent
                                    status=data
                                    workers=workers_data
                                    live_metrics=live_metrics
                                />
                            }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                    <p class="text-destructive">{e.to_string()}</p>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </div>
        </Shell>
    }
}

/// SSE connection status indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2">
            {move || {
                let current_state = state.get();
                let (color, label) = match current_state {
                    SseState::Connected => (StatusColor::Green, "Live"),
                    SseState::Connecting => (StatusColor::Yellow, "Connecting"),
                    SseState::Error => (StatusColor::Red, "Error"),
                    SseState::CircuitOpen => (StatusColor::Red, "Circuit Open"),
                    SseState::Disconnected => (StatusColor::Gray, "Offline"),
                };

                view! {
                    <StatusIndicator
                        color=color
                        pulsing={current_state == SseState::Connected}
                        label=label.to_string()
                    />
                }
            }}
        </div>
    }
}

#[component]
fn DashboardContent(
    status: SystemStatusResponse,
    workers: Vec<WorkerResponse>,
    live_metrics: RwSignal<Option<SystemMetricsResponse>>,
) -> impl IntoView {
    let is_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let db_status = matches!(status.readiness.checks.db.status, ApiStatusIndicator::Ready);

    let inference_text = match status.inference_ready {
        InferenceReadyState::True => "Ready",
        InferenceReadyState::False => "Not Ready",
        InferenceReadyState::Unknown => "Unknown",
    };

    let healthy_workers = workers.iter().filter(|w| w.status == "healthy").count();
    let total_workers = workers.len();

    view! {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
            // System Status Card
            <Card title="System Status".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator
                        color=if is_ready { StatusColor::Green } else { StatusColor::Red }
                        pulsing=is_ready
                        label=if is_ready { "Ready".to_string() } else { "Not Ready".to_string() }
                    />
                </div>
            </Card>

            // Inference Status
            <Card title="Inference".to_string()>
                <div class="text-2xl font-bold">
                    {inference_text}
                </div>
                <p class="text-xs text-muted-foreground">"Inference status"</p>
            </Card>

            // Database Status
            <Card title="Database".to_string()>
                <div class="text-2xl font-bold">
                    {if db_status { "Connected" } else { "Disconnected" }}
                </div>
                <p class="text-xs text-muted-foreground">"Database connection"</p>
            </Card>

            // Workers Status
            <Card title="Workers".to_string()>
                <div class="text-2xl font-bold">
                    {format!("{} / {}", healthy_workers, total_workers)}
                </div>
                <p class="text-xs text-muted-foreground">"Healthy workers"</p>
            </Card>
        </div>

        // Live Metrics Section - Updated in real-time via SSE
        <LiveMetricsSection metrics=live_metrics/>

        // Workers List
        <Card title="Workers".to_string() class="mt-6".to_string()>
            {if workers.is_empty() {
                view! {
                    <p class="text-muted-foreground">"No workers registered"</p>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-2">
                        {workers.into_iter().map(|worker| {
                            let status_variant = match worker.status.as_str() {
                                "healthy" => BadgeVariant::Success,
                                "draining" => BadgeVariant::Warning,
                                "error" | "stopped" => BadgeVariant::Destructive,
                                _ => BadgeVariant::Secondary,
                            };
                            view! {
                                <div class="flex items-center justify-between p-2 rounded-lg border">
                                    <div class="flex items-center gap-3">
                                        <div>
                                            <p class="font-medium text-sm">{worker.id.clone()}</p>
                                            <p class="text-xs text-muted-foreground">
                                                {worker.backend.clone().unwrap_or_else(|| "Unknown backend".to_string())}
                                            </p>
                                        </div>
                                    </div>
                                    <Badge variant=status_variant>
                                        {worker.status.clone()}
                                    </Badge>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </Card>

        // Last Update
        <Card title="Last Update".to_string() class="mt-6".to_string()>
            <p class="text-muted-foreground">{status.timestamp}</p>
        </Card>
    }
}

/// Live metrics section - displays real-time metrics from SSE stream
#[component]
fn LiveMetricsSection(metrics: RwSignal<Option<SystemMetricsResponse>>) -> impl IntoView {
    view! {
        <Card title="Live Metrics".to_string() description="Real-time system metrics via SSE".to_string() class="mt-6".to_string()>
            {move || {
                match metrics.get() {
                    Some(m) => view! {
                        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                            <MetricCard
                                label="CPU Usage".to_string()
                                value=format!("{:.1}%", m.cpu_usage_percent.unwrap_or(m.cpu_usage))
                                trend=None
                            />
                            <MetricCard
                                label="Memory Usage".to_string()
                                value=format!("{:.1}%", m.memory_usage_percent.unwrap_or(m.memory_usage))
                                trend=None
                            />
                            <MetricCard
                                label="GPU Utilization".to_string()
                                value=format!("{:.1}%", m.gpu_utilization)
                                trend=None
                            />
                            <MetricCard
                                label="Requests/sec".to_string()
                                value=format!("{:.1}", m.requests_per_second)
                                trend=None
                            />
                            <MetricCard
                                label="Avg Latency".to_string()
                                value=format!("{:.0} ms", m.avg_latency_ms)
                                trend=m.latency_p95_ms.map(|p95| format!("P95: {:.0} ms", p95))
                            />
                            <MetricCard
                                label="Active Workers".to_string()
                                value=m.active_workers.to_string()
                                trend=m.active_sessions.map(|s| format!("{} sessions", s))
                            />
                            <MetricCard
                                label="Uptime".to_string()
                                value=format_uptime(m.uptime_seconds)
                                trend=None
                            />
                            <MetricCard
                                label="Load Average".to_string()
                                value=format!("{:.2}", m.load_average.load_1min)
                                trend=Some(format!("5m: {:.2} 15m: {:.2}", m.load_average.load_5min, m.load_average.load_15min))
                            />
                        </div>
                    }.into_any(),
                    None => view! {
                        <div class="flex items-center justify-center py-8 text-muted-foreground">
                            <div class="flex items-center gap-2">
                                <Spinner/>
                                <span>"Waiting for live metrics..."</span>
                            </div>
                        </div>
                    }.into_any(),
                }
            }}
        </Card>
    }
}

/// Individual metric card for live metrics display
#[component]
fn MetricCard(label: String, value: String, trend: Option<String>) -> impl IntoView {
    view! {
        <div class="rounded-lg border p-4 bg-card">
            <p class="text-xs text-muted-foreground mb-1">{label}</p>
            <p class="text-2xl font-bold">{value}</p>
            {trend.map(|t| view! {
                <p class="text-xs text-muted-foreground mt-1">{t}</p>
            })}
        </div>
    }
}

/// Format uptime in human-readable format
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
