//! Process Monitoring page
//!
//! Real-time process monitoring with alerts, anomalies, and health metrics.

use crate::api::types::{WorkerHealthSummaryResponse, WorkerHealthSummaryWorker};
use crate::api::{
    report_error_with_toast, use_api_client, ApiClient, ComponentStatus, ProcessAlertResponse,
    ProcessAnomalyResponse, ProcessHealthMetricResponse, ReadyzCheck, ReadyzChecks, ReadyzResponse,
    SystemHealthResponse, SystemReadyResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, EmptyState, EmptyStateVariant,
    ErrorDisplay, LineChart, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, SkeletonCard,
    SkeletonStatsGrid, Spinner, TabButton, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{
    use_api_resource, use_live_system_metrics, use_polling, LiveSystemMetricsHandle, LoadingState,
    MetricViewMode,
};
use crate::signals::{use_refetch_signal, RefetchTopic};
use crate::utils::humanize;
use adapteros_api_types::HealthResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Process Monitoring page with tabs for alerts, anomalies, and health metrics
#[component]
pub fn Monitoring() -> impl IntoView {
    // Shared API client for action handlers (StoredValue makes Arc<ApiClient> Copy for reactive closures)
    let client = StoredValue::new(use_api_client());

    // Active tab state
    let active_tab = RwSignal::new("alerts");

    // Fetch process alerts
    let (alerts, refetch_alerts) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_process_alerts(None).await
    });

    // Fetch process anomalies
    let (anomalies, refetch_anomalies) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.list_process_anomalies(None).await
        });

    // Fetch health metrics
    let (health_metrics, refetch_health) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.get_process_health_metrics(None).await
        });

    // Shared global live metrics + worker summary projection for top panel.
    let live_metrics = use_live_system_metrics();
    let (worker_health_summary, refetch_worker_health_summary) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.worker_health_summary().await
        });

    // Fetch system overview (includes active sessions count)
    let (system_overview, refetch_overview) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.get_system_overview().await
        });

    // Fetch health endpoints
    let (healthz, refetch_healthz) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.get_with_status::<HealthResponse>("/healthz").await
    });
    let (readyz, refetch_readyz) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.get_with_status::<ReadyzResponse>("/readyz").await
    });
    let (healthz_all, refetch_healthz_all) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client.get::<SystemHealthResponse>("/healthz/all").await
        });
    let (system_ready, refetch_system_ready) =
        use_api_resource(move |client: Arc<ApiClient>| async move {
            client
                .get_with_status::<SystemReadyResponse>("/system/ready")
                .await
        });

    // SSE-driven refresh from Shell's health lifecycle stream.
    let health_refetch_counter = use_refetch_signal(RefetchTopic::Health);
    Effect::new(move || {
        let Some(counter) = health_refetch_counter.try_get() else {
            return;
        };
        if counter > 0 {
            refetch_health.run(());
            refetch_overview.run(());
            refetch_worker_health_summary.run(());
            refetch_healthz.run(());
            refetch_readyz.run(());
            refetch_healthz_all.run(());
            refetch_system_ready.run(());
        }
    });

    // Set up polling (every 10 seconds)
    let _ = use_polling(10_000, move || async move {
        refetch_alerts.run(());
        refetch_anomalies.run(());
        refetch_health.run(());
        refetch_overview.run(());
        refetch_worker_health_summary.run(());
        refetch_healthz.run(());
        refetch_readyz.run(());
        refetch_healthz_all.run(());
        refetch_system_ready.run(());
    });

    // Count active alerts
    let active_alert_count = Signal::derive(move || match alerts.try_get().unwrap_or_default() {
        LoadingState::Loaded(ref a) => a.iter().filter(|x| x.status == "active").count(),
        _ => 0,
    });

    // Count unresolved anomalies
    let unresolved_anomaly_count =
        Signal::derive(move || match anomalies.try_get().unwrap_or_default() {
            LoadingState::Loaded(ref a) => a.iter().filter(|x| x.status != "resolved").count(),
            _ => 0,
        });

    view! {
        <PageScaffold
            title="Monitoring"
            subtitle="Process health, alerts, and anomalies across workers."
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Observe"),
                PageBreadcrumbItem::current("Monitoring"),
            ]
        >
            <PageScaffoldActions slot>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| {
                        refetch_alerts.run(());
                        refetch_anomalies.run(());
                        refetch_health.run(());
                        refetch_overview.run(());
                        refetch_worker_health_summary.run(());
                        refetch_healthz.run(());
                        refetch_readyz.run(());
                        refetch_healthz_all.run(());
                        refetch_system_ready.run(());
                    })
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            // Inline status bar
            <div class="flex flex-wrap items-center gap-3 md:gap-6 px-4 py-2 glass-tier-1 rounded-lg text-sm">
                <span class="flex items-center gap-2">
                    <span class="text-muted-foreground">"Alerts"</span>
                    <Badge variant=BadgeVariant::Destructive>
                        {move || active_alert_count.try_get().unwrap_or_default().to_string()}
                    </Badge>
                </span>
                <span class="border-l border-border h-4"></span>
                <span class="flex items-center gap-2">
                    <span class="text-muted-foreground">"Anomalies"</span>
                    <Badge variant=BadgeVariant::Warning>
                        {move || unresolved_anomaly_count.try_get().unwrap_or_default().to_string()}
                    </Badge>
                </span>
                <span class="border-l border-border h-4"></span>
                <span class="flex items-center gap-2">
                    <span class="text-muted-foreground">"Sessions"</span>
                    {move || {
                        let count = match system_overview.try_get().unwrap_or_default() {
                            LoadingState::Loaded(ref overview) => overview.active_sessions,
                            _ => 0,
                        };
                        view! { <Badge variant=BadgeVariant::Secondary>{count.to_string()}</Badge> }
                    }}
                </span>
                <span class="border-l border-border h-4"></span>
                <span class="flex items-center gap-2">
                    <span class="text-muted-foreground">"Health"</span>
                    {move || match healthz.try_get().unwrap_or_default() {
                        LoadingState::Loaded((status_code, data)) => {
                            let variant = health_status_variant(status_code, &data.status);
                            view! { <Badge variant=variant>{data.status}</Badge> }.into_any()
                        }
                        LoadingState::Loading | LoadingState::Idle => view! { <Spinner/> }.into_any(),
                        LoadingState::Error(_) => view! { <Badge variant=BadgeVariant::Destructive>"Error"</Badge> }.into_any(),
                    }}
                </span>
            </div>

            // Health endpoints
            <HealthEndpointsCard
                healthz=healthz.try_get().unwrap_or_default()
                readyz=readyz.try_get().unwrap_or_default()
                healthz_all=healthz_all.try_get().unwrap_or_default()
                system_ready=system_ready.try_get().unwrap_or_default()
            />

            // Tab navigation
            <nav role="tablist" class="tab-nav" aria-label="Monitoring tabs">
                <TabButton tab="alerts" label="Alerts" active=active_tab tab_id="alerts" badge_count=active_alert_count/>
                <TabButton tab="anomalies" label="Anomalies" active=active_tab tab_id="anomalies" badge_count=unresolved_anomaly_count/>
                <TabButton tab="health" label="Health Metrics" active=active_tab tab_id="health"/>
            </nav>

            // Tab content
            <div class="py-4">
                {move || {
                    match active_tab.try_get().unwrap_or_default() {
                        "alerts" => {
                            match alerts.try_get().unwrap_or_default() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="space-y-3">
                                            <SkeletonCard has_header=true />
                                            <SkeletonCard has_header=true />
                                            <SkeletonCard has_header=true />
                                        </div>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    let alerts_data: Vec<ProcessAlertResponse> = data;
                                    if alerts_data.is_empty() {
                                        view! {
                                            <EmptyState
                                                title="No Alerts"
                                                description="No process alerts have been triggered. Your system is running smoothly."
                                                variant=EmptyStateVariant::Empty
                                                icon="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"
                                            />
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="space-y-3">
                                                {alerts_data.into_iter().map(|alert| {
                                                    let alert_id = alert.id.clone();
                                                    let is_active = alert.status == "active";
                                                    let severity_variant = match alert.severity.as_str() {
                                                        "critical" => BadgeVariant::Destructive,
                                                        "warning" => BadgeVariant::Warning,
                                                        _ => BadgeVariant::Secondary,
                                                    };
                                                    let status_variant = if is_active {
                                                        BadgeVariant::Destructive
                                                    } else {
                                                        BadgeVariant::Success
                                                    };
                                                    let acknowledging = RwSignal::new(false);

                                                    view! {
                                                        <Card>
                                                            <div class="flex items-start justify-between">
                                                                <div class="flex-1">
                                                                    <div class="flex items-center gap-2 mb-2">
                                                                        <Badge variant=severity_variant>{alert.severity.clone()}</Badge>
                                                                        <Badge variant=status_variant>{alert.status.clone()}</Badge>
                                                                        <span class="text-xs text-muted-foreground">"Worker: "{adapteros_id::short_id(&alert.worker_id)}</span>
                                                                    </div>
                                                                    <p class="text-sm font-medium">{alert.message.clone()}</p>
                                                                    <p class="text-xs text-muted-foreground mt-1">"Triggered: "{alert.triggered_at.clone()}</p>
                                                                </div>
                                                                {is_active.then(|| {
                                                                    let alert_id = alert_id.clone();
                                                                    view! {
                                                                        <Button
                                                                            variant=ButtonVariant::Outline
                                                                            size=ButtonSize::Sm
                                                                            loading=acknowledging.try_get().unwrap_or(false)
                                                                            on_click=Callback::new(move |_| {
                                                                                let alert_id = alert_id.clone();
                                                                                let client = client.get_value();
                                                                                let _ = acknowledging.try_set(true);
                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                    match client.acknowledge_alert(&alert_id).await {
                                                                                        Ok(_) => {
                                                                                            refetch_alerts.run(());
                                                                                        }
                                                                                        Err(e) => {
                                                                                            report_error_with_toast(&e, "Failed to acknowledge alert", Some("/monitoring"), true);
                                                                                        }
                                                                                    }
                                                                                    let _ = acknowledging.try_set(false);
                                                                                });
                                                                            })
                                                                        >
                                                                            "Acknowledge"
                                                                        </Button>
                                                                    }
                                                                })}
                                                            </div>
                                                        </Card>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                                LoadingState::Error(e) => {
                                    view! {
                                        <ErrorDisplay
                                            error=e
                                            on_retry=Callback::new(move |_| refetch_alerts.run(()))
                                        />
                                    }.into_any()
                                }
                            }
                        }
                        "anomalies" => {
                            match anomalies.try_get().unwrap_or_default() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="space-y-3">
                                            <SkeletonCard has_header=true />
                                            <SkeletonCard has_header=true />
                                            <SkeletonCard has_header=true />
                                        </div>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    let anomalies_data: Vec<ProcessAnomalyResponse> = data;
                                    if anomalies_data.is_empty() {
                                        view! {
                                            <EmptyState
                                                title="No Anomalies"
                                                description="No anomalies have been detected in your processes."
                                                variant=EmptyStateVariant::Empty
                                                icon="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
                                            />
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="space-y-3">
                                                {anomalies_data.into_iter().map(|anomaly| {
                                                    let severity_variant = match anomaly.severity.as_str() {
                                                        "critical" => BadgeVariant::Destructive,
                                                        "high" => BadgeVariant::Warning,
                                                        "medium" => BadgeVariant::Secondary,
                                                        _ => BadgeVariant::Outline,
                                                    };
                                                    let status_variant = match anomaly.status.as_str() {
                                                        "resolved" => BadgeVariant::Success,
                                                        "investigating" => BadgeVariant::Warning,
                                                        _ => BadgeVariant::Destructive,
                                                    };
                                                    let resolved_at = anomaly.resolved_at.clone();

                                                    view! {
                                                        <Card>
                                                            <div class="flex items-start justify-between">
                                                                <div class="flex-1">
                                                                    <div class="flex items-center gap-2 mb-2">
                                                                        <Badge variant=severity_variant>{anomaly.severity.clone()}</Badge>
                                                                        <Badge variant=status_variant>{anomaly.status.clone()}</Badge>
                                                                        <span class="text-xs text-muted-foreground">{humanize(&anomaly.anomaly_type)}</span>
                                                                    </div>
                                                                    <p class="text-sm font-medium">{anomaly.description.clone()}</p>
                                                                    <div class="flex items-center gap-4 mt-2 text-xs text-muted-foreground">
                                                                        <span>"Worker: "{adapteros_id::short_id(&anomaly.worker_id)}</span>
                                                                        <span>"Detected: "{anomaly.detected_at.clone()}</span>
                                                                        {resolved_at.map(|r| view! {
                                                                            <span>"Resolved: "{r}</span>
                                                                        })}
                                                                    </div>
                                                                </div>
                                                            </div>
                                                        </Card>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                                LoadingState::Error(e) => {
                                    view! {
                                        <ErrorDisplay
                                            error=e
                                            on_retry=refetch_anomalies.as_callback()
                                        />
                                    }.into_any()
                                }
                            }
                        }
                        _ => {
                            // Health metrics tab
                            match health_metrics.try_get().unwrap_or_default() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="space-y-4">
                                            <MonitoringLivePerformancePanel
                                                live_metrics=live_metrics
                                                worker_health_summary=worker_health_summary
                                            />
                                            <SkeletonStatsGrid count=6 />
                                        </div>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    let metrics_data: Vec<ProcessHealthMetricResponse> = data;
                                    if metrics_data.is_empty() {
                                        view! {
                                            <div class="space-y-4">
                                                <MonitoringLivePerformancePanel
                                                    live_metrics=live_metrics
                                                    worker_health_summary=worker_health_summary
                                                />
                                                <EmptyState
                                                    title="No Health Metrics"
                                                    description="No health metrics are being collected. Start some workers to see metrics."
                                                    variant=EmptyStateVariant::Empty
                                                    icon="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"
                                                />
                                            </div>
                                        }.into_any()
                                    } else {
                                        // Group metrics by worker
                                        let grouped = group_metrics_by_worker(metrics_data);
                                        view! {
                                            <div class="space-y-4">
                                                <MonitoringLivePerformancePanel
                                                    live_metrics=live_metrics
                                                    worker_health_summary=worker_health_summary
                                                />
                                                <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
                                                    {grouped.into_iter().map(|(worker_id, worker_metrics)| view! {
                                                        <WorkerHealthCard worker_id=worker_id metrics=worker_metrics/>
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            </div>
                                        }.into_any()
                                    }
                                }
                                LoadingState::Error(e) => {
                                    view! {
                                        <div class="space-y-4">
                                            <MonitoringLivePerformancePanel
                                                live_metrics=live_metrics
                                                worker_health_summary=worker_health_summary
                                            />
                                            <ErrorDisplay
                                                error=e
                                                on_retry=refetch_health.as_callback()
                                            />
                                        </div>
                                    }.into_any()
                                }
                            }
                        }
                    }
                }}
            </div>
        </PageScaffold>
    }
}

#[derive(Clone, Debug, PartialEq)]
struct WorkerPerformanceProjection {
    worker_id: String,
    throughput_rps_recent: f64,
    avg_latency_ms: f64,
}

#[component]
fn MonitoringLivePerformancePanel(
    live_metrics: LiveSystemMetricsHandle,
    worker_health_summary: ReadSignal<LoadingState<WorkerHealthSummaryResponse>>,
) -> impl IntoView {
    let metric_mode = RwSignal::new(MetricViewMode::Throughput);
    let global_chart_data = Memo::new(move |_| {
        let mode = metric_mode.try_get().unwrap_or_default();
        live_metrics
            .history
            .with(|history| history.series_for_mode(mode))
    });

    let target_workers: RwSignal<Vec<WorkerPerformanceProjection>> = RwSignal::new(Vec::new());
    let display_workers: RwSignal<Vec<WorkerPerformanceProjection>> = RwSignal::new(Vec::new());

    Effect::new(move || {
        if let Some(LoadingState::Loaded(summary)) = worker_health_summary.try_get() {
            let top_workers = top_five_workers_by_throughput(&summary.workers);
            let _ = target_workers.try_set(top_workers.clone());
            let _ = display_workers.try_update(|current| {
                if current.is_empty() {
                    *current = top_workers.clone();
                }
            });
        }
    });

    let _ = use_polling(100, move || async move {
        let target = target_workers.try_get().unwrap_or_default();
        if target.is_empty() {
            return;
        }

        let current = display_workers.try_get().unwrap_or_default();
        let next = lerp_worker_projections(&current, &target, 0.35);
        let _ = display_workers.try_set(next);
    });

    view! {
        <Card title="Live Performance".to_string() description="Global metrics with top worker performance.".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between gap-3">
                    <div class="inline-flex rounded-md border border-border bg-muted/20 p-1 transition-colors duration-200">
                        <button
                            type="button"
                            class=move || {
                                if metric_mode.try_get().unwrap_or_default() == MetricViewMode::Throughput {
                                    "rounded px-3 py-1 text-xs font-medium bg-background text-foreground shadow-sm transition-all duration-200"
                                } else {
                                    "rounded px-3 py-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-all duration-200"
                                }
                            }
                            on:click=move |_| metric_mode.set(MetricViewMode::Throughput)
                        >
                            "Throughput"
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if metric_mode.try_get().unwrap_or_default() == MetricViewMode::Latency {
                                    "rounded px-3 py-1 text-xs font-medium bg-background text-foreground shadow-sm transition-all duration-200"
                                } else {
                                    "rounded px-3 py-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-all duration-200"
                                }
                            }
                            on:click=move |_| metric_mode.set(MetricViewMode::Latency)
                        >
                            "Latency"
                        </button>
                    </div>
                    <span class="text-xs text-muted-foreground transition-opacity duration-200">
                        {move || match metric_mode.try_get().unwrap_or_default() {
                            MetricViewMode::Throughput => "Requests/sec",
                            MetricViewMode::Latency => "Latency (ms)",
                        }}
                    </span>
                </div>

                <LineChart
                    data=Signal::from(global_chart_data)
                    title="Global Live Performance".to_string()
                    height=170.0
                    show_points=false
                    class="transition-opacity duration-200".to_string()
                />

                {move || {
                    match live_metrics.display_metrics.try_get().flatten() {
                        Some(metrics) => view! {
                            <div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-3 text-xs">
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"CPU "</span>
                                    <span class="font-mono font-medium">{format!("{:.1}%", metrics.cpu_usage)}</span>
                                </div>
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"Memory "</span>
                                    <span class="font-mono font-medium">{format!("{:.1}%", metrics.memory_usage)}</span>
                                </div>
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"GPU "</span>
                                    <span class="font-mono font-medium">{format!("{:.1}%", metrics.gpu_utilization)}</span>
                                </div>
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"Current Throughput "</span>
                                    <span class="font-mono font-medium">{format!("{:.1} req/s", metrics.requests_per_second)}</span>
                                </div>
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"Current Latency "</span>
                                    <span class="font-mono font-medium">{format!("{:.0} ms", metrics.avg_latency_ms)}</span>
                                </div>
                                <div class="rounded-md border border-border bg-muted/10 px-3 py-2 transition-colors duration-200">
                                    <span class="text-muted-foreground">"Active Workers "</span>
                                    <span class="font-mono font-medium">{metrics.active_workers}</span>
                                </div>
                            </div>
                        }.into_any(),
                        None => view! {
                            <div class="flex items-center gap-2 text-xs text-muted-foreground">
                                <Spinner/>
                                <span>"Waiting for live metrics stream..."</span>
                            </div>
                        }.into_any(),
                    }
                }}

                <div class="border-t border-border pt-3 space-y-2">
                    <div class="flex items-center justify-between text-xs text-muted-foreground">
                        <span>
                            {move || {
                                match metric_mode.try_get().unwrap_or_default() {
                                    MetricViewMode::Throughput => "Top workers by throughput (recent)",
                                    MetricViewMode::Latency => "Top workers by latency (recent)",
                                }
                            }}
                        </span>
                        {move || {
                            let count = target_workers.try_get().unwrap_or_default().len();
                            view! { <span>{format!("{} workers", count)}</span> }
                        }}
                    </div>
                    {move || {
                        match worker_health_summary.try_get().unwrap_or_default() {
                            LoadingState::Idle | LoadingState::Loading => view! {
                                <div class="flex items-center gap-2 text-xs text-muted-foreground">
                                    <Spinner/>
                                    <span>"Loading worker performance..."</span>
                                </div>
                            }.into_any(),
                            LoadingState::Error(_) => view! {
                                <div class="text-xs text-muted-foreground">
                                    "Worker summary unavailable."
                                </div>
                            }.into_any(),
                            LoadingState::Loaded(_) => {
                                let workers = display_workers.try_get().unwrap_or_default();
                                if workers.is_empty() {
                                    view! {
                                        <div class="text-xs text-muted-foreground">
                                            "No worker performance data."
                                        </div>
                                    }.into_any()
                                } else {
                                    let mode = metric_mode.try_get().unwrap_or_default();
                                    let max_value = workers
                                        .iter()
                                        .map(|worker| project_worker_value(worker, mode))
                                        .fold(0.0, f64::max)
                                        .max(1.0);
                                    view! {
                                        <div class="space-y-2">
                                            {workers.into_iter().map(|worker| {
                                                let value = project_worker_value(&worker, mode);
                                                let width = (value / max_value * 100.0).clamp(5.0, 100.0);
                                                let value_label = format_projected_worker_value(value, mode);
                                                view! {
                                                    <div class="rounded-md border border-border px-3 py-2 transition-all duration-200 ease-out">
                                                        <div class="flex items-center justify-between text-xs">
                                                            <span class="font-medium">{adapteros_id::short_id(&worker.worker_id)}</span>
                                                            <span class="font-mono transition-opacity duration-200">{value_label}</span>
                                                        </div>
                                                        <div class="mt-2 h-1.5 rounded bg-muted/50 overflow-hidden">
                                                            <div
                                                                class="h-full bg-primary/70 transition-all duration-300 ease-out"
                                                                style={format!("width: {:.2}%;", width)}
                                                            ></div>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }
                        }
                    }}
                </div>
            </div>
        </Card>
    }
}

fn top_five_workers_by_throughput(
    workers: &[WorkerHealthSummaryWorker],
) -> Vec<WorkerPerformanceProjection> {
    use std::cmp::Ordering;

    let mut projected: Vec<WorkerPerformanceProjection> = workers
        .iter()
        .map(|worker| WorkerPerformanceProjection {
            worker_id: worker.worker_id.clone(),
            throughput_rps_recent: worker.throughput_rps_recent.unwrap_or_default(),
            avg_latency_ms: worker.avg_latency_ms.unwrap_or_default(),
        })
        .collect();

    projected.sort_by(|left, right| {
        right
            .throughput_rps_recent
            .partial_cmp(&left.throughput_rps_recent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.worker_id.cmp(&right.worker_id))
    });
    projected.truncate(5);
    projected
}

fn project_worker_value(worker: &WorkerPerformanceProjection, mode: MetricViewMode) -> f64 {
    match mode {
        MetricViewMode::Throughput => worker.throughput_rps_recent,
        MetricViewMode::Latency => worker.avg_latency_ms,
    }
}

fn format_projected_worker_value(value: f64, mode: MetricViewMode) -> String {
    match mode {
        MetricViewMode::Throughput => format!("{:.1} req/s", value),
        MetricViewMode::Latency => format!("{:.0} ms", value),
    }
}

fn lerp_worker_projections(
    current: &[WorkerPerformanceProjection],
    target: &[WorkerPerformanceProjection],
    factor: f64,
) -> Vec<WorkerPerformanceProjection> {
    target
        .iter()
        .map(|target_worker| {
            if let Some(current_worker) = current
                .iter()
                .find(|worker| worker.worker_id == target_worker.worker_id)
            {
                WorkerPerformanceProjection {
                    worker_id: target_worker.worker_id.clone(),
                    throughput_rps_recent: lerp_f64(
                        current_worker.throughput_rps_recent,
                        target_worker.throughput_rps_recent,
                        factor,
                    ),
                    avg_latency_ms: lerp_f64(
                        current_worker.avg_latency_ms,
                        target_worker.avg_latency_ms,
                        factor,
                    ),
                }
            } else {
                target_worker.clone()
            }
        })
        .collect()
}

fn lerp_f64(current: f64, target: f64, factor: f64) -> f64 {
    current + (target - current) * factor.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(
        id: &str,
        throughput_rps_recent: f64,
        avg_latency_ms: f64,
    ) -> WorkerHealthSummaryWorker {
        WorkerHealthSummaryWorker {
            worker_id: id.to_string(),
            throughput_rps_recent: Some(throughput_rps_recent),
            avg_latency_ms: Some(avg_latency_ms),
            ..Default::default()
        }
    }

    #[test]
    fn sorts_top_five_workers_by_throughput_desc() {
        let workers = vec![
            worker("worker-c", 9.0, 40.0),
            worker("worker-a", 12.0, 30.0),
            worker("worker-f", 7.0, 50.0),
            worker("worker-b", 12.0, 20.0),
            worker("worker-d", 4.0, 25.0),
            worker("worker-e", 8.0, 35.0),
        ];

        let top = top_five_workers_by_throughput(&workers);
        let ids: Vec<&str> = top.iter().map(|w| w.worker_id.as_str()).collect();

        assert_eq!(top.len(), 5);
        assert_eq!(
            ids,
            vec!["worker-a", "worker-b", "worker-c", "worker-e", "worker-f"]
        );
    }

    #[test]
    fn projection_follows_selected_toggle_mode() {
        let worker = WorkerPerformanceProjection {
            worker_id: "worker-1".to_string(),
            throughput_rps_recent: 15.5,
            avg_latency_ms: 82.0,
        };

        assert!((project_worker_value(&worker, MetricViewMode::Throughput) - 15.5).abs() < 0.001);
        assert!((project_worker_value(&worker, MetricViewMode::Latency) - 82.0).abs() < 0.001);
    }
}

/// Returns true if a LoadingState contains a network-level error (backend unreachable).
fn is_network_error<T>(state: &LoadingState<T>) -> bool {
    matches!(state, LoadingState::Error(e) if matches!(e, crate::api::ApiError::Network(_)))
}

/// Health endpoints summary card
#[component]
fn HealthEndpointsCard(
    healthz: LoadingState<(u16, HealthResponse)>,
    readyz: LoadingState<(u16, ReadyzResponse)>,
    healthz_all: LoadingState<SystemHealthResponse>,
    system_ready: LoadingState<(u16, SystemReadyResponse)>,
) -> impl IntoView {
    // Detect when all endpoints failed with network errors (backend unreachable)
    let all_network_errors = is_network_error(&healthz)
        && is_network_error(&readyz)
        && is_network_error(&healthz_all)
        && is_network_error(&system_ready);

    if all_network_errors {
        return view! {
            <Card title="Health Endpoints".to_string() description="Live status from /healthz, /readyz, /system/ready".to_string()>
                <div class="rounded-lg border border-destructive bg-destructive/10 p-6 text-center space-y-2">
                    <div class="flex items-center justify-center gap-2 text-destructive">
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
                            <line x1="12" y1="9" x2="12" y2="13"/>
                            <line x1="12" y1="17" x2="12.01" y2="17"/>
                        </svg>
                        <span class="font-medium">"Backend not available"</span>
                    </div>
                    <p class="text-sm text-muted-foreground">
                        "Cannot reach the control plane. Start the backend with "
                        <code class="font-mono text-xs bg-muted px-1 py-0.5 rounded">"./start"</code>
                        " or check the server logs."
                    </p>
                </div>
            </Card>
        }.into_any();
    }

    view! {
        <Card title="Health Endpoints".to_string() description="Live status from /healthz, /readyz, /system/ready".to_string()>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Endpoint"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Details"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {match healthz {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/healthz"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell>
                                    <span
                                        class="inline-block h-4 w-28 rounded bg-muted/60 animate-pulse"
                                        aria-hidden="true"
                                    ></span>
                                    <span class="sr-only">"Loading endpoint status"</span>
                                </TableCell>
                            </TableRow>
                        }.into_any(),
                        LoadingState::Loaded((status_code, data)) => {
                            let variant = health_status_variant(status_code, &data.status);
                            let details = match &data.build_id {
                                Some(build) => format!("HTTP {} | v{} | {}", status_code, data.version, build),
                                None => format!("HTTP {} | v{}", status_code, data.version),
                            };
                            view! {
                                <TableRow>
                                    <TableCell><span class="font-mono text-sm">"/healthz"</span></TableCell>
                                    <TableCell>
                                        <Badge variant=variant>{data.status}</Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{details}</span>
                                    </TableCell>
                                </TableRow>
                            }.into_any()
                        }
                        LoadingState::Error(e) => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/healthz"</span></TableCell>
                                <TableCell><Badge variant=BadgeVariant::Destructive>"Error"</Badge></TableCell>
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match readyz {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/readyz"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell>
                                    <span
                                        class="inline-block h-4 w-28 rounded bg-muted/60 animate-pulse"
                                        aria-hidden="true"
                                    ></span>
                                    <span class="sr-only">"Loading endpoint status"</span>
                                </TableCell>
                            </TableRow>
                        }.into_any(),
                        LoadingState::Loaded((status_code, data)) => {
                            let (variant, label) = if data.ready {
                                (BadgeVariant::Success, "ready")
                            } else if status_code >= 500 {
                                (BadgeVariant::Destructive, "not ready")
                            } else {
                                (BadgeVariant::Warning, "degraded")
                            };
                            let summary = readiness_checks_summary(&data.checks);
                            view! {
                                <TableRow>
                                    <TableCell><span class="font-mono text-sm">"/readyz"</span></TableCell>
                                    <TableCell>
                                        <Badge variant=variant>{label}</Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format!("HTTP {} | {}", status_code, summary)}
                                        </span>
                                    </TableCell>
                                </TableRow>
                            }.into_any()
                        }
                        LoadingState::Error(e) => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/readyz"</span></TableCell>
                                <TableCell><Badge variant=BadgeVariant::Destructive>"Error"</Badge></TableCell>
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match system_ready {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/system/ready"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell>
                                    <span
                                        class="inline-block h-4 w-28 rounded bg-muted/60 animate-pulse"
                                        aria-hidden="true"
                                    ></span>
                                    <span class="sr-only">"Loading endpoint status"</span>
                                </TableCell>
                            </TableRow>
                        }.into_any(),
                        LoadingState::Loaded((status_code, data)) => {
                            let (variant, label) = if data.ready {
                                (BadgeVariant::Success, "ready")
                            } else if data.maintenance {
                                (BadgeVariant::Warning, "maintenance")
                            } else {
                                (BadgeVariant::Destructive, "not ready")
                            };
                            let reason = if data.reason.is_empty() { "ready".to_string() } else { data.reason.clone() };
                            view! {
                                <TableRow>
                                    <TableCell><span class="font-mono text-sm">"/system/ready"</span></TableCell>
                                    <TableCell>
                                        <Badge variant=variant>{label}</Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format!("HTTP {} | {}", status_code, reason)}
                                        </span>
                                    </TableCell>
                                </TableRow>
                            }.into_any()
                        }
                        LoadingState::Error(e) => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/system/ready"</span></TableCell>
                                <TableCell><Badge variant=BadgeVariant::Destructive>"Error"</Badge></TableCell>
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match healthz_all {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/healthz/all"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell>
                                    <span
                                        class="inline-block h-4 w-28 rounded bg-muted/60 animate-pulse"
                                        aria-hidden="true"
                                    ></span>
                                    <span class="sr-only">"Loading endpoint status"</span>
                                </TableCell>
                            </TableRow>
                        }.into_any(),
                        LoadingState::Loaded(data) => {
                            let (variant, label) = component_status_badge(data.overall_status);
                            view! {
                                <TableRow>
                                    <TableCell><span class="font-mono text-sm">"/healthz/all"</span></TableCell>
                                    <TableCell>
                                        <Badge variant=variant>{label}</Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format!("{} components", data.components.len())}
                                        </span>
                                    </TableCell>
                                </TableRow>
                            }.into_any()
                        }
                        LoadingState::Error(e) => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/healthz/all"</span></TableCell>
                                <TableCell><Badge variant=BadgeVariant::Destructive>"Error"</Badge></TableCell>
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}
                </TableBody>
            </Table>
        </Card>
    }.into_any()
}

fn component_status_badge(status: ComponentStatus) -> (BadgeVariant, &'static str) {
    match status {
        ComponentStatus::Healthy => (BadgeVariant::Success, "healthy"),
        ComponentStatus::Degraded => (BadgeVariant::Warning, "degraded"),
        ComponentStatus::Unhealthy => (BadgeVariant::Destructive, "unhealthy"),
    }
}

fn readiness_checks_summary(checks: &ReadyzChecks) -> String {
    format!(
        "db: {} | worker: {} | models: {}",
        check_label(&checks.db),
        check_label(&checks.worker),
        check_label(&checks.models_seeded),
    )
}

fn check_label(check: &ReadyzCheck) -> &'static str {
    if check.ok {
        "ok"
    } else {
        "fail"
    }
}

fn health_status_variant(status_code: u16, status: &str) -> BadgeVariant {
    let status_lower = status.to_lowercase();
    if status_code >= 500 || status_lower.contains("failed") {
        BadgeVariant::Destructive
    } else if status_lower.contains("degrad")
        || status_lower.contains("boot")
        || status_lower.contains("drain")
        || status_lower.contains("maintenance")
    {
        BadgeVariant::Warning
    } else {
        BadgeVariant::Success
    }
}

/// Group metrics by worker ID
fn group_metrics_by_worker(
    metrics: Vec<ProcessHealthMetricResponse>,
) -> Vec<(String, Vec<ProcessHealthMetricResponse>)> {
    use std::collections::HashMap;
    let mut map: HashMap<String, Vec<ProcessHealthMetricResponse>> = HashMap::new();
    for metric in metrics {
        map.entry(metric.worker_id.clone())
            .or_default()
            .push(metric);
    }
    let mut result: Vec<_> = map.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Worker health card showing metrics for a single worker
#[component]
fn WorkerHealthCard(worker_id: String, metrics: Vec<ProcessHealthMetricResponse>) -> impl IntoView {
    let metrics_count = metrics.len();
    let display_metrics: Vec<_> = metrics.into_iter().take(5).collect();

    view! {
        <Card>
            <div class="space-y-3">
                <div class="flex items-center justify-between">
                    <span class="font-medium">{adapteros_id::short_id(&worker_id)}</span>
                    <Badge variant=BadgeVariant::Outline>{metrics_count.to_string()}" metrics"</Badge>
                </div>
                <div class="space-y-2">
                    {display_metrics.into_iter().map(|m| {
                        let unit = m.metric_unit.clone().unwrap_or_default();
                        let metric_name = match m.metric_name.as_str() {
                            "cpu_usage" => "CPU (Unified)",
                            "memory_usage" => "Memory (Unified)",
                            n => n,
                        };
                        view! {
                            <div class="flex items-center justify-between text-sm">
                                <span class="text-muted-foreground">{metric_name.to_string()}</span>
                                <span class="font-mono">{format!("{:.2}{}", m.metric_value, unit)}</span>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>
        </Card>
    }
}
