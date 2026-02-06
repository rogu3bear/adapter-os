//! Dashboard page

use crate::api::{use_sse_json_events, ActivityEventResponse, ApiClient, SseState};
use crate::boot_log;
use crate::components::inference_guidance::guidance_for;
use crate::components::status_center::use_status_center;
use crate::components::{
    Button, ButtonVariant, Card, ChartPoint, DataSeries, EmptyState, EmptyStateVariant,
    IconCheckCircle, IconPlay, IconServer, LineChart, PageScaffold, PageScaffoldActions,
    SparklineMetric, Spinner, StatusColor, StatusIconBox, StatusIndicator, StatusVariant,
    TimeSeriesData, WorkerStatusBadge,
};
use crate::hooks::{use_api_resource, use_sse_notifications, LoadingState};
use crate::signals::use_auth;
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

    let (auth_state, _) = use_auth();
    let can_view_activity = Memo::new(move |_| {
        auth_state
            .get()
            .user()
            .map(|u| u.role == "admin" || u.permissions.iter().any(|p| p == "ActivityView"))
            .unwrap_or(false)
    });

    // Fetch activity feed (permission-aware)
    let (activity, refetch_activity) = use_api_resource({
        let can_view_activity = can_view_activity.clone();
        move |client: Arc<ApiClient>| async move {
            if !can_view_activity.get_untracked() {
                Ok(Vec::new())
            } else {
                client.activity_feed(Some(20)).await
            }
        }
    });

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
            let Some(last) = last_metrics_update.try_get_value() else {
                return;
            };
            let now = js_sys::Date::now() as u64;
            if now.saturating_sub(last) < 250 {
                return;
            }
            let _ = last_metrics_update.try_set_value(now);

            // Store lightweight snapshot instead of full response
            let _ = live_metrics.try_set(Some(MetricsSnapshot::from(&metrics)));

            // Add to history with timestamp
            let _ = metrics_history.try_update(|h| h.push(&metrics, now));
        },
    );

    // Bridge SSE connection state to user notifications
    use_sse_notifications(sse_status.read_only());

    // REST fallback for metrics when SSE fails or is not connected
    let (metrics_fallback, refetch_metrics_fallback) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_metrics().await });

    // Update live_metrics from REST fallback when SSE is not providing data
    Effect::new(move || {
        let sse_state = sse_status.get();
        let has_live_metrics = live_metrics.get().is_some();

        // If SSE is not connected/working and we don't have live metrics, use REST fallback
        if !has_live_metrics
            || matches!(
                sse_state,
                SseState::Error | SseState::CircuitOpen | SseState::Disconnected
            )
        {
            if let LoadingState::Loaded(ref resp) = metrics_fallback.get() {
                let now = js_sys::Date::now() as u64;
                let _ = live_metrics.try_set(Some(MetricsSnapshot::from(resp)));
                let _ = metrics_history.try_update(|h| h.push(resp, now));
            }
        }
    });

    // Periodically refresh REST fallback when SSE is not connected
    let refetch_metrics_fallback_stored = StoredValue::new(refetch_metrics_fallback);
    Effect::new(move || {
        let sse_state = sse_status.get();

        // Only set up polling if SSE is not connected
        if matches!(
            sse_state,
            SseState::Error | SseState::CircuitOpen | SseState::Disconnected
        ) {
            // Trigger a refetch - the interval is handled by the resource itself
            let _ = refetch_metrics_fallback_stored.try_with_value(|f| f.run(()));
        }
    });

    // Refetch functions stored for use in closures
    let refetch_signal = StoredValue::new(refetch);
    let refetch_workers_signal = StoredValue::new(refetch_workers);
    let refetch_activity_signal = StoredValue::new(refetch_activity);

    // Refetch all data (SSE reconnection handled separately due to non-Send types)
    let refetch_all = move || {
        let _ = refetch_signal.try_with_value(|f| f.run(()));
        let _ = refetch_workers_signal.try_with_value(|f| f.run(()));
        let _ = refetch_activity_signal.try_with_value(|f| f.run(()));
    };

    view! {
        <PageScaffold
            title="Dashboard"
            subtitle="A live system overview of health, activity, and resource usage."
        >
            <PageScaffoldActions slot>
                <SseIndicator state=sse_status/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| refetch_all())
                    aria_label="Refresh dashboard data".to_string()
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

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
                                activity=activity
                                can_view_activity=can_view_activity
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
        </PageScaffold>
    }
}

/// SSE connection status indicator
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2">
            {move || {
                let current_state = state.try_get().unwrap_or(SseState::Disconnected);
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
    activity: ReadSignal<LoadingState<Vec<ActivityEventResponse>>>,
    can_view_activity: Memo<bool>,
) -> impl IntoView {
    let status_center = use_status_center();

    let is_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let db_status = matches!(status.readiness.checks.db.status, ApiStatusIndicator::Ready);
    let inference_needs_attention = !matches!(status.inference_ready, InferenceReadyState::True);
    let inference_guidance = inference_needs_attention
        .then(|| guidance_for(status.inference_ready, status.inference_blockers.first()));

    let inference_text = match status.inference_ready {
        InferenceReadyState::True => "Ready",
        InferenceReadyState::False => "Not Ready",
        InferenceReadyState::Unknown => "Unknown",
    };

    let healthy_workers = workers.iter().filter(|w| w.status == "healthy").count();
    let total_workers = workers.len();

    view! {
        // ═══════════════════════════════════════════════════════════════════════════
        // CRITICAL PATH: System Health at a Glance
        // These 3 cards are the "is everything OK?" indicators - always visible first
        // ═══════════════════════════════════════════════════════════════════════════
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            // System Status Card - Primary health indicator
            <Card title="System Status".to_string()>
                <div class="flex items-center gap-3">
                    <StatusIconBox status=StatusVariant::from_bool(is_ready)>
                        <IconCheckCircle class="h-5 w-5".to_string() />
                    </StatusIconBox>
                    <div>
                        <StatusIndicator
                            color=StatusVariant::from_bool(is_ready).to_status_color()
                            pulsing=is_ready
                            label=if is_ready { "Ready".to_string() } else { "Not Ready".to_string() }
                        />
                        <p class="text-xs text-muted-foreground mt-1">{status.timestamp.clone()}</p>
                    </div>
                </div>
            </Card>

            // Inference Status - The most actionable card
            <Card title="Inference".to_string()>
                <div class="flex items-center gap-3">
                    <StatusIconBox status=match status.inference_ready {
                        InferenceReadyState::True => StatusVariant::Success,
                        InferenceReadyState::False => StatusVariant::Error,
                        InferenceReadyState::Unknown => StatusVariant::Warning,
                    }>
                        <IconPlay class="h-5 w-5".to_string() />
                    </StatusIconBox>
                    <div>
                        <div class="text-2xl font-bold">{inference_text}</div>
                        <p class="text-xs text-muted-foreground">"Inference status"</p>
                        {if let Some(guidance) = inference_guidance {
                            let action = guidance.action;
                            Some(view! {
                                <div class="mt-2 space-y-2">
                                    <p class="text-xs text-muted-foreground">{guidance.reason}</p>
                                    <div class="flex flex-wrap items-center gap-2">
                                        <a
                                            href=action.href
                                            class="btn btn-outline btn-sm"
                                        >
                                            {action.label}
                                        </a>
                                        {status_center.map(|ctx| view! {
                                                <button
                                                    class="text-xs text-muted-foreground hover:text-foreground"
                                                    on:click=move |_| ctx.open()
                                                >
                                                    "Why?"
                                                </button>
                                            })}
                                    </div>
                                </div>
                            })
                        } else {
                            None
                        }}
                    </div>
                </div>
            </Card>

            // Database Status
            <Card title="Database".to_string()>
                <div class="flex items-center gap-3">
                    <StatusIconBox status=StatusVariant::from_bool(db_status)>
                        <IconServer class="h-5 w-5".to_string() />
                    </StatusIconBox>
                    <div>
                        <div class="text-2xl font-bold">
                            {if db_status { "Connected" } else { "Disconnected" }}
                        </div>
                        <p class="text-xs text-muted-foreground">"Database connection"</p>
                    </div>
                </div>
            </Card>
        </div>

        // ═══════════════════════════════════════════════════════════════════════════
        // MAIN CONTENT: Two-column layout for desktop, stacked for mobile
        // Left: Performance (metrics + charts) | Right: Operations (activity + workers)
        // ═══════════════════════════════════════════════════════════════════════════
        <div class="grid gap-6 mt-6 lg:grid-cols-5">
            // Left Column: Performance Metrics (wider - 3/5 on desktop)
            <div class="lg:col-span-3 space-y-6">
                // Live Metrics Section
                <LiveMetricsSection metrics=live_metrics history=metrics_history/>
            </div>

            // Right Column: Operations (narrower - 2/5 on desktop)
            <div class="lg:col-span-2 space-y-6">
                // Workers List - with count in header
                <Card title=format!("Workers ({}/{})", healthy_workers, total_workers)>
                    {if workers.is_empty() {
                        view! {
                            <EmptyState
                                variant=EmptyStateVariant::Empty
                                title="No Workers Registered".to_string()
                                description="Workers handle inference requests. Start a worker to begin processing.".to_string()
                            />
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-2">
                                {workers.into_iter().map(|worker| {
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
                                            <WorkerStatusBadge status=worker.status.clone() />
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </Card>

                // Activity feed
                <Card title="Recent Activity".to_string()>
                    {move || {
                        if !can_view_activity.get() {
                            return view! {
                                <div class="text-sm text-muted-foreground">
                                    "Activity requires permission."
                                </div>
                            }.into_any();
                        }
                        match activity.get() {
                            LoadingState::Idle | LoadingState::Loading => view! {
                                <div class="text-sm text-muted-foreground">"Loading activity..."</div>
                            }.into_any(),
                            LoadingState::Error(_) => view! {
                                <div class="text-sm text-muted-foreground">"Activity unavailable."</div>
                            }.into_any(),
                            LoadingState::Loaded(events) => {
                                if events.is_empty() {
                                    view! {
                                        <div class="text-sm text-muted-foreground">"No recent activity."</div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="space-y-2 max-h-80 overflow-y-auto">
                                            {events.into_iter().map(|event| {
                                                let target = event.target_type.clone().unwrap_or_else(|| "system".to_string());
                                                let when = event.created_at.clone();
                                                view! {
                                                    <div class="flex items-center justify-between rounded-md border border-input px-3 py-2">
                                                        <div>
                                                            <div class="text-sm font-medium">{event.event_type}</div>
                                                            <div class="text-xs text-muted-foreground">{target}</div>
                                                        </div>
                                                        <div class="text-xs text-muted-foreground">{when}</div>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }
                        }
                    }}
                </Card>
            </div>
        </div>
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
        <div class="space-y-6">
            // ─────────────────────────────────────────────────────────────────────
            // Performance Metrics: Request handling (the most watched metrics)
            // ─────────────────────────────────────────────────────────────────────
            <Card title="Request Performance".to_string() description="Real-time inference throughput".to_string()>
                {move || {
                    match metrics.get() {
                        Some(m) => view! {
                            <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                                // Requests/sec - primary metric
                                <SparklineMetric
                                    label="Requests/sec".to_string()
                                    value=format!("{:.1}", m.requests_per_second)
                                    values=Signal::from(rps_sparkline)
                                    show_trend=true
                                />

                                // Latency - performance indicator
                                <SparklineMetric
                                    label="Avg Latency".to_string()
                                    value=format!("{:.0} ms", m.avg_latency_ms)
                                    unit="ms".to_string()
                                    values=Signal::from(latency_sparkline)
                                    show_trend=true
                                />

                                // Active Workers & Sessions
                                {
                                    let sessions_trend = m.active_sessions.map(|s| format!("{} sessions", s));
                                    view! {
                                        <MetricCard
                                            label="Active Workers".to_string()
                                            value=m.active_workers.to_string()
                                            trend=sessions_trend
                                        />
                                    }
                                }
                            </div>
                        }.into_any(),
                        None => view! {
                            <div class="flex items-center justify-center py-6 text-muted-foreground gap-2">
                                <Spinner/>
                                <span class="text-sm">"Connecting to metrics stream..."</span>
                            </div>
                        }.into_any(),
                    }
                }}
            </Card>

            // ─────────────────────────────────────────────────────────────────────
            // Resource Utilization: Hardware metrics
            // ─────────────────────────────────────────────────────────────────────
            <Card title="Resource Utilization".to_string()>
                {move || {
                    match metrics.get() {
                        Some(m) => view! {
                            <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                                // CPU
                                <SparklineMetric
                                    label="CPU Usage".to_string()
                                    value=format!("{:.1}%", m.cpu_usage)
                                    values=Signal::from(cpu_sparkline)
                                    show_trend=true
                                />

                                // Memory
                                <SparklineMetric
                                    label="Memory".to_string()
                                    value=format!("{:.1}%", m.memory_usage)
                                    values=Signal::from(memory_sparkline)
                                    show_trend=true
                                />

                                // GPU
                                <SparklineMetric
                                    label="GPU".to_string()
                                    value=format!("{:.1}%", m.gpu_utilization)
                                    values=Signal::from(gpu_sparkline)
                                    show_trend=true
                                />

                                // Load Average
                                <MetricCard
                                    label="Load Avg".to_string()
                                    value=format!("{:.2}", m.load_1min)
                                    trend=format!("5m: {:.2} 15m: {:.2}", m.load_5min, m.load_15min)
                                />
                            </div>

                            // Uptime inline at bottom
                            <div class="mt-4 pt-3 border-t border-border flex justify-end">
                                <span class="text-xs text-muted-foreground">
                                    "Uptime: "{format_uptime(m.uptime_seconds)}
                                </span>
                            </div>
                        }.into_any(),
                        None => view! {
                            <div class="h-20"></div>
                        }.into_any(),
                    }
                }}
            </Card>

            // ─────────────────────────────────────────────────────────────────────
            // Time Series Charts: Historical view
            // ─────────────────────────────────────────────────────────────────────
            <div class="grid gap-6 sm:grid-cols-2">
                // Throughput chart
                <LineChart
                    data=Signal::from(throughput_data)
                    title="Throughput".to_string()
                    y_label="req/s".to_string()
                    height=180.0
                    show_points=false
                />

                // Latency chart
                <LineChart
                    data=Signal::from(latency_data)
                    title="Latency".to_string()
                    y_label="ms".to_string()
                    height=180.0
                    show_points=false
                />
            </div>
        </div>
    }
}

/// Trend direction for metric cards
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TrendDirection {
    #[default]
    Neutral,
    Up,
    Down,
}

/// Individual metric card for live metrics display
#[component]
fn MetricCard(
    label: String,
    value: String,
    #[prop(optional, into)] trend: MaybeProp<String>,
    #[prop(optional)] trend_direction: TrendDirection,
) -> impl IntoView {
    let trend = trend.get();
    let trend_class = match trend_direction {
        TrendDirection::Up => "trend-up",
        TrendDirection::Down => "trend-down",
        TrendDirection::Neutral => "trend-neutral",
    };

    let trend_icon = match trend_direction {
        TrendDirection::Up => Some(view! {
            <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M7 17l5-5 5 5M7 7l5 5 5-5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
        }),
        TrendDirection::Down => Some(view! {
            <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M7 7l5 5 5-5M7 17l5-5 5 5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
        }),
        TrendDirection::Neutral => None,
    };

    view! {
        <div class="rounded-lg border p-4 bg-card">
            <p class="text-xs text-muted-foreground mb-1">{label}</p>
            <p class="text-2xl font-bold">{value}</p>
            {trend.map(|t| view! {
                <div class=format!("flex items-center gap-1 text-xs mt-1 {}", trend_class)>
                    {trend_icon.clone()}
                    <span>{t}</span>
                </div>
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
