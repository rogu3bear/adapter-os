//! Process Monitoring page
//!
//! Real-time process monitoring with alerts, anomalies, and health metrics.

use crate::api::{
    report_error_with_toast, ApiClient, ComponentStatus, ProcessAlertResponse,
    ProcessAnomalyResponse, ProcessHealthMetricResponse, ReadyzCheck, ReadyzChecks, ReadyzResponse,
    SystemHealthResponse, SystemReadyResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, EmptyState, EmptyStateVariant,
    ErrorDisplay, LoadingDisplay, Spinner, TabButton, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use adapteros_api_types::HealthResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Process Monitoring page with tabs for alerts, anomalies, and health metrics
#[component]
pub fn Monitoring() -> impl IntoView {
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

    // Set up polling (every 10 seconds)
    let _ = use_polling(10_000, move || async move {
        refetch_alerts.run(());
        refetch_anomalies.run(());
        refetch_health.run(());
        refetch_overview.run(());
        refetch_healthz.run(());
        refetch_readyz.run(());
        refetch_healthz_all.run(());
        refetch_system_ready.run(());
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
        <div class="shell-page space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Metrics"</h1>
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
                        refetch_overview.run(());
                        refetch_healthz.run(());
                        refetch_readyz.run(());
                        refetch_healthz_all.run(());
                        refetch_system_ready.run(());
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
                            let count = match system_overview.get() {
                                LoadingState::Loaded(ref overview) => overview.active_sessions,
                                _ => 0,
                            };
                            view! { <Badge variant=BadgeVariant::Secondary>{count.to_string()}</Badge> }
                        }}
                    </div>
                </Card>
                <Card>
                    <div class="flex items-center justify-between">
                        <span class="text-sm font-medium text-muted-foreground">"Health Status"</span>
                        {move || match healthz.get() {
                            LoadingState::Loaded((status_code, data)) => {
                                let variant = health_status_variant(status_code, &data.status);
                                view! { <Badge variant=variant>{data.status}</Badge> }.into_any()
                            }
                            LoadingState::Loading | LoadingState::Idle => view! { <Spinner/> }.into_any(),
                            LoadingState::Error(_) => view! { <Badge variant=BadgeVariant::Destructive>"Error"</Badge> }.into_any(),
                        }}
                    </div>
                </Card>
            </div>

            // Health endpoints
            <HealthEndpointsCard
                healthz=healthz.get()
                readyz=readyz.get()
                healthz_all=healthz_all.get()
                system_ready=system_ready.get()
            />

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
                    match active_tab.get() {
                        "alerts" => {
                            match alerts.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <LoadingDisplay message="Loading alerts..."/>
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
                                                                                            report_error_with_toast(&e, "Failed to acknowledge alert", Some("/monitoring"), true);
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
                                        <LoadingDisplay message="Loading anomalies..."/>
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
                                        <LoadingDisplay message="Loading health metrics..."/>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    let metrics_data: Vec<ProcessHealthMetricResponse> = data;
                                    if metrics_data.is_empty() {
                                        view! {
                                            <EmptyState
                                                title="No Health Metrics"
                                                description="No health metrics are being collected. Start some workers to see metrics."
                                                variant=EmptyStateVariant::Empty
                                                icon="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"
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

/// Health endpoints summary card
#[component]
fn HealthEndpointsCard(
    healthz: LoadingState<(u16, HealthResponse)>,
    readyz: LoadingState<(u16, ReadyzResponse)>,
    healthz_all: LoadingState<SystemHealthResponse>,
    system_ready: LoadingState<(u16, SystemReadyResponse)>,
) -> impl IntoView {
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
                                <TableCell><span class="text-sm text-muted-foreground">"Loading..."</span></TableCell>
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
                                <TableCell><span class="text-sm text-destructive">{e.to_string()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match readyz {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/readyz"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell><span class="text-sm text-muted-foreground">"Loading..."</span></TableCell>
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
                                <TableCell><span class="text-sm text-destructive">{e.to_string()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match system_ready {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/system/ready"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell><span class="text-sm text-muted-foreground">"Loading..."</span></TableCell>
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
                                <TableCell><span class="text-sm text-destructive">{e.to_string()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match healthz_all {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <TableRow>
                                <TableCell><span class="font-mono text-sm">"/healthz/all"</span></TableCell>
                                <TableCell><Spinner/></TableCell>
                                <TableCell><span class="text-sm text-muted-foreground">"Loading..."</span></TableCell>
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
                                <TableCell><span class="text-sm text-destructive">{e.to_string()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}
                </TableBody>
            </Table>
        </Card>
    }
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
