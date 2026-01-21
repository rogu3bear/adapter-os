//! Process Monitoring page
//!
//! Real-time process monitoring with alerts, anomalies, and health metrics.

use crate::api::{
    ApiClient, ProcessAlertResponse, ProcessAnomalyResponse, ProcessHealthMetricResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ErrorDisplay, Spinner,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Process Monitoring page with tabs for alerts, anomalies, and health metrics
#[component]
pub fn Monitoring() -> impl IntoView {
    // Active tab state
    let active_tab = RwSignal::new("alerts".to_string());

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

    // Count active alerts

    // Set up polling (every 10 seconds)
    let _ = use_polling(10_000, move || async move {
        refetch_alerts.run(());
        refetch_anomalies.run(());
        refetch_health.run(());
    });

    // Count active alerts
    let active_alert_count = Signal::derive(move || match alerts.get() {
        LoadingState::Loaded(ref a) => a.iter().filter(|x| x.status == "active").count(),
        _ => 0,
    });

    // Count unresolved anomalies
    let unresolved_anomaly_count = Signal::derive(move || match anomalies.get() {
        LoadingState::Loaded(ref a) => a.iter().filter(|x| x.status != "resolved").count(),
        _ => 0,
    });

    view! {
        <div class="p-6 space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Process Monitoring"</h1>
                    <p class="text-muted-foreground mt-1">
                        "Monitor process health, alerts, and anomalies across workers"
                    </p>
                </div>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| {
                        refetch_alerts.run(());
                        refetch_anomalies.run(());
                        refetch_health.run(());
                    })
                >
                    "Refresh"
                </Button>
            </div>

            // Summary cards
            <div class="grid gap-4 md:grid-cols-4">
                <SummaryCard
                    title="Active Alerts"
                    count=active_alert_count
                    variant=BadgeVariant::Destructive
                />
                <SummaryCard
                    title="Unresolved Anomalies"
                    count=unresolved_anomaly_count
                    variant=BadgeVariant::Warning
                />
                <Card>
                    <div class="flex items-center justify-between">
                        <span class="text-sm font-medium text-muted-foreground">"Active Sessions"</span>
                        {move || {
                            let count = match health_metrics.get() {
                                LoadingState::Loaded(_) => match alerts.get() {
                                     // Placeholder: in a real app we'd fetch this from the unified metrics response
                                     _ => 0,
                                },
                                _ => 0,
                            };
                            view! { <Badge variant=BadgeVariant::Secondary>{count.to_string()}</Badge> }
                        }}
                    </div>
                </Card>
                <Card>
                    <div class="flex items-center justify-between">
                        <span class="text-sm font-medium text-muted-foreground">"Health Status"</span>
                        <Badge variant=BadgeVariant::Success>"Online"</Badge>
                    </div>
                </Card>
            </div>

            // Tab navigation
            <div class="border-b">
                <nav class="-mb-px flex space-x-8">
                    <TabButton tab="alerts" label="Alerts" active=active_tab badge_count=active_alert_count/>
                    <TabButton tab="anomalies" label="Anomalies" active=active_tab badge_count=unresolved_anomaly_count/>
                    <TabButton tab="health" label="Health Metrics" active=active_tab badge_count=Signal::derive(|| 0)/>
                </nav>
            </div>

            // Tab content
            <div class="py-4">
                {move || {
                    match active_tab.get().as_str() {
                        "alerts" => {
                            match alerts.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="flex items-center justify-center py-12">
                                            <Spinner/>
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
                                                icon="bell"
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
                                                                        <span class="text-xs text-muted-foreground">"Worker: "{alert.worker_id.clone()}</span>
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
                                                                            loading=acknowledging.get()
                                                                            on_click=Callback::new(move |_| {
                                                                                let alert_id = alert_id.clone();
                                                                                acknowledging.set(true);
                                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                                    let client = ApiClient::new();
                                                                                    match client.acknowledge_alert(&alert_id).await {
                                                                                        Ok(_) => {
                                                                                            refetch_alerts.run(());
                                                                                        }
                                                                                        Err(e) => {
                                                                                            web_sys::console::error_1(&format!("Failed to acknowledge alert: {}", e).into());
                                                                                        }
                                                                                    }
                                                                                    acknowledging.set(false);
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
                            match anomalies.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="flex items-center justify-center py-12">
                                            <Spinner/>
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
                                                icon="shield"
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
                                                                        <span class="text-xs text-muted-foreground">{anomaly.anomaly_type.clone()}</span>
                                                                    </div>
                                                                    <p class="text-sm font-medium">{anomaly.description.clone()}</p>
                                                                    <div class="flex items-center gap-4 mt-2 text-xs text-muted-foreground">
                                                                        <span>"Worker: "{anomaly.worker_id.clone()}</span>
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
                                            on_retry=refetch_anomalies
                                        />
                                    }.into_any()
                                }
                            }
                        }
                        _ => {
                            // Health metrics tab
                            match health_metrics.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="flex items-center justify-center py-12">
                                            <Spinner/>
                                        </div>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    let metrics_data: Vec<ProcessHealthMetricResponse> = data;
                                    if metrics_data.is_empty() {
                                        view! {
                                            <EmptyState
                                                title="No Health Metrics"
                                                description="No health metrics are being collected. Start some workers to see metrics."
                                                icon="activity"
                                            />
                                        }.into_any()
                                    } else {
                                        // Group metrics by worker
                                        let grouped = group_metrics_by_worker(metrics_data);
                                        view! {
                                            <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
                                                {grouped.into_iter().map(|(worker_id, worker_metrics)| view! {
                                                    <WorkerHealthCard worker_id=worker_id metrics=worker_metrics/>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                                LoadingState::Error(e) => {
                                    view! {
                                        <ErrorDisplay
                                            error=e
                                            on_retry=refetch_health
                                        />
                                    }.into_any()
                                }
                            }
                        }
                    }
                }}
            </div>
        </div>
    }
}

/// Summary card component
#[component]
fn SummaryCard(title: &'static str, count: Signal<usize>, variant: BadgeVariant) -> impl IntoView {
    view! {
        <Card>
            <div class="flex items-center justify-between">
                <span class="text-sm font-medium text-muted-foreground">{title}</span>
                <Badge variant=variant>
                    {move || count.get().to_string()}
                </Badge>
            </div>
        </Card>
    }
}

/// Tab button component
#[component]
fn TabButton(
    tab: &'static str,
    label: &'static str,
    active: RwSignal<String>,
    badge_count: Signal<usize>,
) -> impl IntoView {
    let is_active = move || active.get() == tab;

    view! {
        <button
            class=move || {
                let base = "whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors flex items-center gap-2";
                if is_active() {
                    format!("{} border-primary text-primary", base)
                } else {
                    format!("{} border-transparent text-muted-foreground hover:text-foreground hover:border-muted", base)
                }
            }
            on:click=move |_| active.set(tab.to_string())
        >
            {label}
            {move || {
                let count = badge_count.get();
                if count > 0 {
                    Some(view! {
                        <span class="ml-1 rounded-full bg-destructive/10 px-2 py-0.5 text-xs text-destructive">
                            {count.to_string()}
                        </span>
                    })
                } else {
                    None
                }
            }}
        </button>
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
                    <span class="font-medium">{worker_id}</span>
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

/// Empty state component
#[component]
fn EmptyState(title: &'static str, description: &'static str, icon: &'static str) -> impl IntoView {
    let icon_svg = match icon {
        "bell" => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"/>
            </svg>
        }.into_any(),
        "shield" => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"/>
            </svg>
        }.into_any(),
        _ => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="h-12 w-12 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/>
            </svg>
        }.into_any(),
    };

    view! {
        <div class="flex flex-col items-center justify-center py-12 text-center">
            <div class="rounded-full bg-muted p-4 mb-4">
                {icon_svg}
            </div>
            <h3 class="text-lg font-medium mb-2">{title}</h3>
            <p class="text-muted-foreground max-w-md">{description}</p>
        </div>
    }
}
