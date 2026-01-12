//! Dashboard page

use crate::api::{use_sse_json_events, ApiClient, SseState};
use crate::boot_log;
use crate::components::{
    Badge, BadgeVariant, Card, ChartPoint, DataSeries, LineChart, SparklineMetric, Spinner,
    StatusColor, StatusIndicator, TimeSeriesData,
};
use crate::hooks::{use_api_resource, use_sse_notifications, LoadingState};
use adapteros_api_types::{
    InferenceReadyState, StatusIndicator as ApiStatusIndicator, SystemMetricsResponse,
    SystemStatusResponse, WorkerResponse,
};
use leptos::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Maximum number of data points to keep in history for charts
const METRICS_HISTORY_SIZE: usize = 60;

/// Lightweight snapshot of metrics for display - avoids cloning full response
#[derive(Clone)]
struct MetricsSnapshot {
    cpu_usage: f32,
    memory_usage: f32,
    gpu_utilization: f32,
    requests_per_second: f32,
    avg_latency_ms: f32,
    active_workers: i32,
    active_sessions: Option<i32>,
    uptime_seconds: u64,
    load_1min: f64,
    load_5min: f64,
    load_15min: f64,
}

impl From<&SystemMetricsResponse> for MetricsSnapshot {
    fn from(m: &SystemMetricsResponse) -> Self {
        Self {
            cpu_usage: m.cpu_usage_percent.unwrap_or(m.cpu_usage),
            memory_usage: m.memory_usage_percent.unwrap_or(m.memory_usage),
            gpu_utilization: m.gpu_utilization,
            requests_per_second: m.requests_per_second,
            avg_latency_ms: m.avg_latency_ms,
            active_workers: m.active_workers,
            active_sessions: m.active_sessions,
            uptime_seconds: m.uptime_seconds,
            load_1min: m.load_average.load_1min,
            load_5min: m.load_average.load_5min,
            load_15min: m.load_average.load_15min,
        }
    }
}

/// A single timestamped metrics entry for history tracking.
#[derive(Clone, Copy)]
struct TimestampedMetrics {
    timestamp: u64,
    cpu_usage: f64,
    memory_usage: f64,
    gpu_utilization: f64,
    requests_per_second: f64,
    avg_latency_ms: f64,
}

impl TimestampedMetrics {
    fn from_response(metrics: &SystemMetricsResponse, timestamp: u64) -> Self {
        Self {
            timestamp,
            cpu_usage: metrics.cpu_usage_percent.unwrap_or(metrics.cpu_usage) as f64,
            memory_usage: metrics.memory_usage_percent.unwrap_or(metrics.memory_usage) as f64,
            gpu_utilization: metrics.gpu_utilization as f64,
            requests_per_second: metrics.requests_per_second as f64,
            avg_latency_ms: metrics.avg_latency_ms as f64,
        }
    }
}

/// Metrics history for chart visualization using a single synchronized buffer.
#[derive(Clone, Default)]
struct MetricsHistory {
    snapshots: VecDeque<TimestampedMetrics>,
    /// Version counter to enable cheap change detection
    version: u64,
}

impl MetricsHistory {
    fn push(&mut self, metrics: &SystemMetricsResponse, timestamp: u64) {
        self.snapshots
            .push_back(TimestampedMetrics::from_response(metrics, timestamp));

        // Single trim operation
        while self.snapshots.len() > METRICS_HISTORY_SIZE {
            self.snapshots.pop_front();
        }

        // Increment version for change detection
        self.version = self.version.wrapping_add(1);
    }

    /// Extract a single metric field as Vec<f64> for sparklines.
    fn extract<F>(&self, f: F) -> Vec<f64>
    where
        F: Fn(&TimestampedMetrics) -> f64,
    {
        self.snapshots.iter().map(f).collect()
    }

    fn to_time_series<F>(&self, name: &str, extractor: F) -> TimeSeriesData
    where
        F: Fn(&TimestampedMetrics) -> f64,
    {
        let points: Vec<ChartPoint> = self
            .snapshots
            .iter()
            .map(|s| ChartPoint::new(s.timestamp, extractor(s)))
            .collect();

        let mut data = TimeSeriesData::new();
        data.series.push(DataSeries {
            name: name.to_string(),
            points,
            color: String::new(),
        });
        data
    }

    fn throughput_series(&self) -> TimeSeriesData {
        self.to_time_series("Requests/sec", |s| s.requests_per_second)
    }

    fn latency_series(&self) -> TimeSeriesData {
        self.to_time_series("Latency (ms)", |s| s.avg_latency_ms)
    }
}

/// Dashboard page
#[component]
pub fn Dashboard() -> impl IntoView {
    boot_log("route", "Dashboard rendered");

    // Fetch system status
    let (status, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    // Log first successful status fetch
    let logged_first_status = StoredValue::new(false);
    Effect::new(move || {
        if let LoadingState::Loaded(_) = status.get() {
            if !logged_first_status.get_value() {
                logged_first_status.set_value(true);
                boot_log("api", "first /v1/system/status success");
            }
        }
    });

    // Fetch workers list
    let (workers, refetch_workers) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_workers().await });

    // Live metrics snapshot - lightweight struct for display (avoids full response clone)
    let live_metrics: RwSignal<Option<MetricsSnapshot>> = RwSignal::new(None);

    // Metrics history for charts
    let metrics_history: RwSignal<MetricsHistory> = RwSignal::new(MetricsHistory::default());
    let last_metrics_update = StoredValue::new(0u64);

    // SSE connection for real-time metrics updates
    let (sse_status, _sse_reconnect) = use_sse_json_events::<SystemMetricsResponse, _>(
        "/v1/stream/metrics",
        &["metrics"],
        move |metrics| {
            let now = js_sys::Date::now() as u64;
            let last = last_metrics_update.get_value();
            if now.saturating_sub(last) < 250 {
                return;
            }
            last_metrics_update.set_value(now);

            // Store lightweight snapshot instead of full response
            live_metrics.set(Some(MetricsSnapshot::from(&metrics)));

            // Add to history with timestamp
            metrics_history.update(|h| h.push(&metrics, now));
        },
    );

    // Bridge SSE connection state to user notifications
    use_sse_notifications(sse_status.read_only());

    // Refetch functions stored for use in closures
    let refetch_signal = StoredValue::new(refetch);
    let refetch_workers_signal = StoredValue::new(refetch_workers);

    // Refetch all data (SSE reconnection handled separately due to non-Send types)
    let refetch_all = move || {
        refetch_signal.with_value(|f| f());
        refetch_workers_signal.with_value(|f| f());
    };

    view! {
        <div class="p-6 space-y-6">
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
                                metrics_history=metrics_history
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
    live_metrics: RwSignal<Option<MetricsSnapshot>>,
    metrics_history: RwSignal<MetricsHistory>,
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
        <div class="grid gap-4 sm:grid-cols-2 md:grid-cols-2 lg:grid-cols-4">
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

        // Live Metrics Section - Updated in real-time via SSE with charts
        <LiveMetricsSection metrics=live_metrics history=metrics_history/>

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

/// Live metrics section - displays real-time metrics from SSE stream with charts
#[component]
fn LiveMetricsSection(
    metrics: RwSignal<Option<MetricsSnapshot>>,
    history: RwSignal<MetricsHistory>,
) -> impl IntoView {
    // Use Memo for sparkline data - only recomputes when history version changes
    let cpu_sparkline = Memo::new(move |_| history.with(|h| h.extract(|s| s.cpu_usage)));
    let memory_sparkline = Memo::new(move |_| history.with(|h| h.extract(|s| s.memory_usage)));
    let gpu_sparkline = Memo::new(move |_| history.with(|h| h.extract(|s| s.gpu_utilization)));
    let rps_sparkline = Memo::new(move |_| history.with(|h| h.extract(|s| s.requests_per_second)));
    let latency_sparkline = Memo::new(move |_| history.with(|h| h.extract(|s| s.avg_latency_ms)));

    // Time series for the line charts - memoized to avoid redundant computation
    let throughput_data = Memo::new(move |_| history.with(|h| h.throughput_series()));
    let latency_data = Memo::new(move |_| history.with(|h| h.latency_series()));

    view! {
        <div class="space-y-6 mt-6">
            // Metric cards with sparklines
            <Card title="Live Metrics".to_string() description="Real-time system metrics via SSE".to_string()>
                {move || {
                    match metrics.get() {
                        Some(m) => view! {
                            <div class="grid gap-4 sm:grid-cols-2 md:grid-cols-2 lg:grid-cols-4">
                                // CPU with sparkline
                                <SparklineMetric
                                    label="CPU Usage".to_string()
                                    value=format!("{:.1}%", m.cpu_usage)
                                    values=Signal::from(cpu_sparkline)
                                    show_trend=true
                                />

                                // Memory with sparkline
                                <SparklineMetric
                                    label="Memory Usage".to_string()
                                    value=format!("{:.1}%", m.memory_usage)
                                    values=Signal::from(memory_sparkline)
                                    show_trend=true
                                />

                                // GPU with sparkline
                                <SparklineMetric
                                    label="GPU Utilization".to_string()
                                    value=format!("{:.1}%", m.gpu_utilization)
                                    values=Signal::from(gpu_sparkline)
                                    show_trend=true
                                />

                                // Requests/sec with sparkline
                                <SparklineMetric
                                    label="Requests/sec".to_string()
                                    value=format!("{:.1}", m.requests_per_second)
                                    values=Signal::from(rps_sparkline)
                                    show_trend=true
                                />

                                // Latency with sparkline
                                <SparklineMetric
                                    label="Avg Latency".to_string()
                                    value=format!("{:.0} ms", m.avg_latency_ms)
                                    unit="ms".to_string()
                                    values=Signal::from(latency_sparkline)
                                    show_trend=true
                                />

                                // Active Workers (no sparkline needed)
                                <MetricCard
                                    label="Active Workers".to_string()
                                    value=m.active_workers.to_string()
                                    trend=m.active_sessions.map(|s| format!("{} sessions", s))
                                />

                                // Uptime
                                <MetricCard
                                    label="Uptime".to_string()
                                    value=format_uptime(m.uptime_seconds)
                                    trend=None
                                />

                                // Load Average
                                <MetricCard
                                    label="Load Average".to_string()
                                    value=format!("{:.2}", m.load_1min)
                                    trend=Some(format!("5m: {:.2} 15m: {:.2}", m.load_5min, m.load_15min))
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

            // Time series charts
            <div class="grid gap-6 md:grid-cols-2">
                // Throughput chart
                <LineChart
                    data=Signal::from(throughput_data)
                    title="Throughput".to_string()
                    y_label="req/s".to_string()
                    height=200.0
                    show_points=false
                />

                // Latency chart
                <LineChart
                    data=Signal::from(latency_data)
                    title="Latency".to_string()
                    y_label="ms".to_string()
                    height=200.0
                    show_points=false
                />
            </div>
        </div>
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
