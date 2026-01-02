//! System page
//!
//! Comprehensive system overview with status, workers, nodes, health details,
//! metrics summary, and recent events.
//!
//! Uses SSE for real-time worker status updates via `/v1/stream/workers`.

use crate::api::{use_sse_json, ApiClient, SseState};
use crate::components::{
    Badge, BadgeVariant, Card, Shell, Spinner, StatusColor, StatusIndicator, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{
    workers::WorkerStatusUpdate, ComponentCheck, DriftLevel, InferenceBlocker,
    InferenceReadyState, NodeResponse, StatusIndicator as ApiStatusIndicator,
    SystemMetricsResponse, SystemStatusResponse, WorkerResponse,
};
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// SSE event wrapper for worker updates from /v1/stream/workers
/// The server sends either a full list of workers or incremental updates
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum WorkerStreamEvent {
    /// Full list of workers (sent on connection and periodically)
    FullList { workers: Vec<WorkerResponse> },
    /// Individual worker status update
    StatusUpdate(WorkerStatusUpdate),
    /// Heartbeat/keepalive
    Heartbeat { status: String },
}

/// System overview page
#[component]
pub fn System() -> impl IntoView {
    // Fetch system status
    let (status, refetch_status) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.system_status().await
    });

    // Fetch workers list (initial load, then updated via SSE)
    let (workers, refetch_workers) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.list_workers().await
    });

    // Fetch nodes list
    let (nodes, refetch_nodes) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.list_nodes().await
    });

    // Fetch system metrics
    let (metrics, refetch_metrics) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.system_metrics().await
    });

    // Store refetch functions in signals for sharing
    let refetch_status_signal = StoredValue::new(refetch_status);
    let refetch_workers_signal = StoredValue::new(refetch_workers);
    let refetch_nodes_signal = StoredValue::new(refetch_nodes);
    let refetch_metrics_signal = StoredValue::new(refetch_metrics);

    // Real-time worker status updates via SSE
    // Maps worker_id -> (status, timestamp) for incremental updates
    let worker_status_overrides: RwSignal<HashMap<String, (String, String)>> =
        RwSignal::new(HashMap::new());

    // Track when we last received a full worker list via SSE
    let last_sse_update = RwSignal::new(Option::<String>::None);

    // SSE connection for worker status stream
    let (sse_status, _reconnect) = use_sse_json::<WorkerStreamEvent, _>(
        "/v1/stream/workers",
        move |event| {
            match event {
                WorkerStreamEvent::FullList { workers: _ } => {
                    // When we receive a full list, clear overrides and trigger refetch
                    // to get the complete worker data
                    worker_status_overrides.set(HashMap::new());
                    last_sse_update.set(Some(chrono::Utc::now().to_rfc3339()));
                    refetch_workers_signal.with_value(|f| f());
                }
                WorkerStreamEvent::StatusUpdate(update) => {
                    // Apply incremental status update
                    worker_status_overrides.update(|overrides| {
                        overrides.insert(
                            update.worker_id.clone(),
                            (update.status.clone(), update.timestamp.clone()),
                        );
                    });
                }
                WorkerStreamEvent::Heartbeat { status: _ } => {
                    // Heartbeat received, connection is healthy
                }
            }
        },
    );

    // Set up polling interval for non-worker data (every 10 seconds)
    // Worker data is now primarily updated via SSE
    Effect::new(move |_| {
        let interval_handle = gloo_timers::callback::Interval::new(10_000, move || {
            refetch_status_signal.with_value(|f| f());
            refetch_nodes_signal.with_value(|f| f());
            refetch_metrics_signal.with_value(|f| f());
            // Only refetch workers if SSE is not connected
            if sse_status.get() != SseState::Connected {
                refetch_workers_signal.with_value(|f| f());
            }
        });
        std::mem::forget(interval_handle);
    });

    view! {
        <Shell>
            <div class="space-y-6">
                // Header with title and refresh button
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-4">
                        <h1 class="text-3xl font-bold tracking-tight">"System"</h1>
                        <SseIndicator state=sse_status/>
                    </div>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                        on:click=move |_| {
                            refetch_status_signal.with_value(|f| f());
                            refetch_workers_signal.with_value(|f| f());
                            refetch_nodes_signal.with_value(|f| f());
                            refetch_metrics_signal.with_value(|f| f());
                        }
                    >
                        <RefreshIcon/>
                        "Refresh"
                    </button>
                </div>

                // Main content
                {move || {
                    let status_state = status.get();
                    let workers_state = workers.get();
                    let nodes_state = nodes.get();
                    let metrics_state = metrics.get();
                    let overrides = worker_status_overrides.get();

                    match status_state {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(status_data) => {
                            let workers_data = match workers_state {
                                LoadingState::Loaded(w) => w,
                                _ => Vec::new(),
                            };
                            let nodes_data = match nodes_state {
                                LoadingState::Loaded(n) => n,
                                _ => Vec::new(),
                            };
                            let metrics_data = match metrics_state {
                                LoadingState::Loaded(m) => Some(m),
                                _ => None,
                            };
                            view! {
                                <SystemContent
                                    status=status_data
                                    workers=workers_data
                                    nodes=nodes_data
                                    metrics=metrics_data
                                    worker_status_overrides=overrides
                                />
                            }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                    <p class="text-destructive font-medium">"Failed to load system status"</p>
                                    <p class="text-sm text-destructive/80 mt-1">{e.to_string()}</p>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </div>
        </Shell>
    }
}

/// SSE connection status indicator with detailed state display
#[component]
fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        {move || {
            let current_state = state.get();
            let (color, label, tooltip) = match current_state {
                SseState::Connected => (
                    StatusColor::Green,
                    "Live",
                    "Real-time updates active via SSE",
                ),
                SseState::Connecting => (
                    StatusColor::Yellow,
                    "Connecting...",
                    "Establishing SSE connection",
                ),
                SseState::Error => (
                    StatusColor::Red,
                    "Error",
                    "SSE connection error, will retry",
                ),
                SseState::CircuitOpen => (
                    StatusColor::Red,
                    "Circuit Open",
                    "Too many failures, circuit breaker open",
                ),
                SseState::Disconnected => (
                    StatusColor::Gray,
                    "Offline",
                    "Not connected to real-time updates",
                ),
            };

            let is_connected = current_state == SseState::Connected;
            let is_connecting = current_state == SseState::Connecting;

            view! {
                <div class="flex items-center gap-2" title=tooltip>
                    <StatusIndicator
                        color=color
                        pulsing=is_connected
                        label=label.to_string()
                    />
                    // Show spinner when connecting
                    {is_connecting.then(|| view! {
                        <span class="animate-spin h-3 w-3 border-2 border-yellow-500 border-t-transparent rounded-full"></span>
                    })}
                    // Show live indicator when connected
                    {is_connected.then(|| view! {
                        <span class="text-xs text-green-600 font-medium">"(workers)"</span>
                    })}
                </div>
            }
        }}
    }
}

/// Refresh icon SVG
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

/// Main system content with all sections
#[component]
fn SystemContent(
    status: SystemStatusResponse,
    workers: Vec<WorkerResponse>,
    nodes: Vec<NodeResponse>,
    metrics: Option<SystemMetricsResponse>,
    /// Real-time worker status overrides from SSE (worker_id -> (status, timestamp))
    #[prop(default = HashMap::new())]
    worker_status_overrides: HashMap<String, (String, String)>,
) -> impl IntoView {
    let is_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let db_status = matches!(status.readiness.checks.db.status, ApiStatusIndicator::Ready);

    let inference_ready = match status.inference_ready {
        InferenceReadyState::True => true,
        _ => false,
    };

    // Apply SSE status overrides to workers for real-time updates
    let workers_with_overrides: Vec<WorkerResponse> = workers
        .into_iter()
        .map(|mut w| {
            if let Some((status, timestamp)) = worker_status_overrides.get(&w.id) {
                w.status = status.clone();
                w.last_seen_at = Some(timestamp.clone());
            }
            w
        })
        .collect();

    let healthy_workers = workers_with_overrides
        .iter()
        .filter(|w| w.status == "healthy")
        .count();
    let total_workers = workers_with_overrides.len();

    let models_loaded = status
        .kernel
        .as_ref()
        .and_then(|k| k.adapters.as_ref())
        .and_then(|a| a.loaded)
        .unwrap_or(0);

    view! {
        // Section 1: Status Overview Cards
        <StatusOverview
            is_ready=is_ready
            db_status=db_status
            inference_ready=inference_ready
            healthy_workers=healthy_workers
            total_workers=total_workers
            models_loaded=models_loaded
        />

        // Section 2: Workers Table (with real-time SSE updates applied)
        <WorkersSection workers=workers_with_overrides.clone()/>

        // Section 3: Nodes Table
        <NodesSection nodes=nodes/>

        // Section 4: Health Details
        <HealthDetails status=status.clone()/>

        // Section 5: Metrics Summary
        <MetricsSummary metrics=metrics status=status.clone()/>

        // Section 6: Inference Blockers / Recent Events
        <InferenceBlockersSection blockers=status.inference_blockers.clone()/>

        // Section 7: Boot Status (if available)
        {status.boot.map(|boot| view! {
            <BootStatusSection boot=boot/>
        })}
    }
}

/// Status overview cards at the top
#[component]
fn StatusOverview(
    is_ready: bool,
    db_status: bool,
    inference_ready: bool,
    healthy_workers: usize,
    total_workers: usize,
    models_loaded: i64,
) -> impl IntoView {
    view! {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-5">
            // API Status
            <Card title="API Status".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator
                        color=if is_ready { StatusColor::Green } else { StatusColor::Red }
                        pulsing=is_ready
                        label=if is_ready { "Healthy".to_string() } else { "Unhealthy".to_string() }
                    />
                </div>
            </Card>

            // Database Status
            <Card title="Database".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator
                        color=if db_status { StatusColor::Green } else { StatusColor::Red }
                        pulsing=db_status
                        label=if db_status { "Connected".to_string() } else { "Disconnected".to_string() }
                    />
                </div>
            </Card>

            // Workers Count
            <Card title="Workers".to_string()>
                <div class="text-2xl font-bold">
                    {format!("{} / {}", healthy_workers, total_workers)}
                </div>
                <p class="text-xs text-muted-foreground">"Healthy / Total"</p>
            </Card>

            // Models Loaded
            <Card title="Models".to_string()>
                <div class="text-2xl font-bold">
                    {models_loaded}
                </div>
                <p class="text-xs text-muted-foreground">"Loaded adapters"</p>
            </Card>

            // Inference Ready
            <Card title="Inference".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator
                        color=if inference_ready { StatusColor::Green } else { StatusColor::Yellow }
                        pulsing=inference_ready
                        label=if inference_ready { "Ready".to_string() } else { "Not Ready".to_string() }
                    />
                </div>
            </Card>
        </div>
    }
}

/// Workers section with table
#[component]
fn WorkersSection(workers: Vec<WorkerResponse>) -> impl IntoView {
    view! {
        <Card title="Workers".to_string() description="Active worker processes and their status".to_string()>
            {if workers.is_empty() {
                view! {
                    <div class="text-center py-8">
                        <p class="text-muted-foreground">"No workers registered"</p>
                        <p class="text-sm text-muted-foreground mt-1">"Workers will appear here once they connect"</p>
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
                                <TableHead>"Last Heartbeat"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {workers.into_iter().map(|worker| {
                                view! { <WorkerRow worker=worker/> }
                            }).collect::<Vec<_>>()}
                        </TableBody>
                    </Table>
                }.into_any()
            }}
        </Card>
    }
}

/// Single worker row component with real-time status updates
#[component]
fn WorkerRow(worker: WorkerResponse) -> impl IntoView {
    let status = worker.status.as_str();

    let status_variant = match status {
        "healthy" => BadgeVariant::Success,
        "draining" => BadgeVariant::Warning,
        "error" | "stopped" => BadgeVariant::Destructive,
        "registered" | "created" => BadgeVariant::Secondary,
        _ => BadgeVariant::Secondary,
    };

    // Determine if this worker is in a transitional state
    let is_transitional = matches!(status, "draining" | "created" | "registered");
    let is_unhealthy = matches!(status, "error" | "stopped");

    let short_id = if worker.id.len() > 8 {
        format!("{}...", &worker.id[..8])
    } else {
        worker.id.clone()
    };

    let backend = worker.backend.clone().unwrap_or_else(|| "Unknown".to_string());
    let model = worker.model_id.clone().unwrap_or_else(|| "-".to_string());
    let last_seen = worker.last_seen_at.clone().unwrap_or_else(|| "-".to_string());

    // Row class for visual highlighting of state changes
    let row_class = if is_unhealthy {
        "bg-destructive/5"
    } else if is_transitional {
        "bg-yellow-500/5"
    } else {
        ""
    };

    view! {
        <TableRow class=row_class>
            <TableCell>
                <span class="font-mono text-sm" title=worker.id.clone()>{short_id}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm">{worker.node_id.clone()}</span>
            </TableCell>
            <TableCell>
                <div class="flex items-center gap-2">
                    <Badge variant=status_variant>
                        {worker.status.clone()}
                    </Badge>
                    // Show animated indicator for transitional states
                    {is_transitional.then(|| view! {
                        <span class="relative flex h-2 w-2">
                            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-yellow-400 opacity-75"></span>
                            <span class="relative inline-flex rounded-full h-2 w-2 bg-yellow-500"></span>
                        </span>
                    })}
                    // Show warning icon for error states
                    {is_unhealthy.then(|| view! {
                        <WarningIcon/>
                    })}
                </div>
            </TableCell>
            <TableCell>
                <span class="text-sm">{backend}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm font-mono">{model}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{format_timestamp(&last_seen)}</span>
            </TableCell>
        </TableRow>
    }
}

/// Expanded worker details
#[allow(dead_code)] // Leptos #[component] macro limitation: compiler can't see field usage through macro
#[component]
fn WorkerDetails(#[prop(into)] worker: WorkerResponse) -> impl IntoView {
    let WorkerResponse {
        tenant_id,
        plan_id,
        pid,
        uds_path,
        started_at,
        capabilities,
        cache_used_mb,
        cache_max_mb,
        ..
    } = worker;

    view! {
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4 py-2">
            <div>
                <p class="text-xs text-muted-foreground">"Tenant ID"</p>
                <p class="text-sm font-mono">{tenant_id.clone()}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"Plan ID"</p>
                <p class="text-sm font-mono">{plan_id.clone()}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"PID"</p>
                <p class="text-sm font-mono">{pid.map(|p| p.to_string()).unwrap_or("-".to_string())}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"UDS Path"</p>
                <p class="text-sm font-mono truncate" title=uds_path.clone()>{uds_path.clone()}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"Started At"</p>
                <p class="text-sm">{format_timestamp(&started_at)}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"Capabilities"</p>
                <p class="text-sm">{if capabilities.is_empty() { "-".to_string() } else { capabilities.join(", ") }}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"Cache Used"</p>
                <p class="text-sm">{cache_used_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())}</p>
            </div>
            <div>
                <p class="text-xs text-muted-foreground">"Cache Max"</p>
                <p class="text-sm">{cache_max_mb.map(|m| format!("{} MB", m)).unwrap_or("-".to_string())}</p>
            </div>
        </div>
    }
}

/// Nodes section
#[component]
fn NodesSection(nodes: Vec<NodeResponse>) -> impl IntoView {
    view! {
        <Card title="Nodes".to_string() description="Cluster nodes and their connectivity status".to_string()>
            {if nodes.is_empty() {
                view! {
                    <div class="text-center py-8">
                        <p class="text-muted-foreground">"No nodes registered"</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"ID"</TableHead>
                                <TableHead>"Hostname"</TableHead>
                                <TableHead>"Status"</TableHead>
                                <TableHead>"Endpoint"</TableHead>
                                <TableHead>"Last Seen"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {nodes.into_iter().map(|node| {
                                let status_variant = match node.status.as_str() {
                                    "healthy" | "active" => BadgeVariant::Success,
                                    "draining" => BadgeVariant::Warning,
                                    "error" | "offline" => BadgeVariant::Destructive,
                                    _ => BadgeVariant::Secondary,
                                };

                                let short_id = if node.id.len() > 8 {
                                    format!("{}...", &node.id[..8])
                                } else {
                                    node.id.clone()
                                };

                                view! {
                                    <TableRow>
                                        <TableCell>
                                            <span class="font-mono text-sm" title=node.id.clone()>{short_id}</span>
                                        </TableCell>
                                        <TableCell>
                                            <span class="text-sm">{node.hostname.clone()}</span>
                                        </TableCell>
                                        <TableCell>
                                            <Badge variant=status_variant>
                                                {node.status.clone()}
                                            </Badge>
                                        </TableCell>
                                        <TableCell>
                                            <span class="text-sm font-mono">{node.agent_endpoint.clone()}</span>
                                        </TableCell>
                                        <TableCell>
                                            <span class="text-sm text-muted-foreground">
                                                {node.last_seen_at.clone().map(|t| format_timestamp(&t)).unwrap_or("-".to_string())}
                                            </span>
                                        </TableCell>
                                    </TableRow>
                                }
                            }).collect::<Vec<_>>()}
                        </TableBody>
                    </Table>
                }.into_any()
            }}
        </Card>
    }
}

/// Health details section
#[component]
fn HealthDetails(status: SystemStatusResponse) -> impl IntoView {
    // Clone checks to avoid lifetime issues
    let db_check = status.readiness.checks.db.clone();
    let migrations_check = status.readiness.checks.migrations.clone();
    let workers_check = status.readiness.checks.workers.clone();
    let models_check = status.readiness.checks.models.clone();

    view! {
        <Card title="Health Details".to_string() description="Readiness checks breakdown".to_string()>
            <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                <HealthCheckCard
                    name="Database".to_string()
                    check=db_check
                />
                <HealthCheckCard
                    name="Migrations".to_string()
                    check=migrations_check
                />
                <HealthCheckCard
                    name="Workers".to_string()
                    check=workers_check
                />
                <HealthCheckCard
                    name="Models".to_string()
                    check=models_check
                />
            </div>

            // Integrity status
            <div class="mt-6 pt-6 border-t">
                <h4 class="text-sm font-medium mb-4">"Integrity Status"</h4>
                <div class="grid gap-4 md:grid-cols-4">
                    <div class="flex items-center gap-2">
                        <StatusIndicator
                            color=if status.integrity.pf_deny_ok { StatusColor::Green } else { StatusColor::Red }
                            label="PF Deny".to_string()
                        />
                    </div>
                    <div class="flex items-center gap-2">
                        <StatusIndicator
                            color=if status.integrity.strict_mode { StatusColor::Green } else { StatusColor::Yellow }
                            label=format!("Mode: {}", status.integrity.mode)
                        />
                    </div>
                    <div class="flex items-center gap-2">
                        <StatusIndicator
                            color=if status.integrity.is_federated { StatusColor::Blue } else { StatusColor::Gray }
                            label=if status.integrity.is_federated { "Federated".to_string() } else { "Standalone".to_string() }
                        />
                    </div>
                    <div class="flex items-center gap-2">
                        {
                            let drift_color = match status.integrity.drift.level {
                                DriftLevel::Ok => StatusColor::Green,
                                DriftLevel::Warn => StatusColor::Yellow,
                                DriftLevel::Critical => StatusColor::Red,
                            };
                            let drift_label = format!("Drift: {:?}", status.integrity.drift.level);
                            view! {
                                <StatusIndicator color=drift_color label=drift_label/>
                            }
                        }
                    </div>
                </div>
            </div>
        </Card>
    }
}

/// Individual health check card
#[component]
fn HealthCheckCard(name: String, check: ComponentCheck) -> impl IntoView {
    let is_ready = matches!(check.status, ApiStatusIndicator::Ready);
    let is_critical = check.critical.unwrap_or(false);
    let latency = check.latency_ms;
    let reason = check.reason.clone();

    let color = if is_ready {
        StatusColor::Green
    } else if is_critical {
        StatusColor::Red
    } else {
        StatusColor::Yellow
    };

    view! {
        <div class="rounded-lg border p-4">
            <div class="flex items-center justify-between mb-2">
                <span class="text-sm font-medium">{name}</span>
                <StatusIndicator
                    color=color
                    pulsing=is_ready
                />
            </div>
            <div class="space-y-1">
                <p class="text-xs text-muted-foreground">
                    "Status: " {if is_ready { "Ready" } else { "Not Ready" }}
                </p>
                {latency.map(|l| view! {
                    <p class="text-xs text-muted-foreground">
                        "Latency: " {format!("{} ms", l)}
                    </p>
                })}
                {reason.clone().map(|r| {
                    let title = r.clone();
                    view! {
                        <p class="text-xs text-muted-foreground truncate" title=title>
                            {r}
                        </p>
                    }
                })}
            </div>
        </div>
    }
}

/// Metrics summary section
#[component]
fn MetricsSummary(
    metrics: Option<SystemMetricsResponse>,
    status: SystemStatusResponse,
) -> impl IntoView {
    view! {
        <Card title="Metrics Summary".to_string() description="System resource usage and performance".to_string()>
            {match metrics {
                Some(m) => view! {
                    <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                        // Requests per second
                        <MetricCard
                            label="Requests/sec".to_string()
                            value=format!("{:.1}", m.requests_per_second)
                            sub_label=None
                        />

                        // Average latency
                        <MetricCard
                            label="Avg Latency".to_string()
                            value=format!("{:.0} ms", m.avg_latency_ms)
                            sub_label=m.latency_p95_ms.map(|p95| format!("P95: {:.0} ms", p95))
                        />

                        // Active workers
                        <MetricCard
                            label="Active Workers".to_string()
                            value=m.active_workers.to_string()
                            sub_label=m.active_sessions.map(|s| format!("{} sessions", s))
                        />

                        // Uptime
                        <MetricCard
                            label="Uptime".to_string()
                            value=format_uptime(m.uptime_seconds)
                            sub_label=None
                        />

                        // CPU Usage
                        <MetricCard
                            label="CPU Usage".to_string()
                            value=format!("{:.1}%", m.cpu_usage_percent.unwrap_or(m.cpu_usage))
                            sub_label=Some(format!("Load: {:.2}", m.load_average.load_1min))
                        />

                        // Memory Usage
                        <MetricCard
                            label="Memory Usage".to_string()
                            value=format!("{:.1}%", m.memory_usage_percent.unwrap_or(m.memory_usage))
                            sub_label=None
                        />

                        // GPU Utilization
                        <MetricCard
                            label="GPU Utilization".to_string()
                            value=format!("{:.1}%", m.gpu_utilization)
                            sub_label=None
                        />

                        // Error rate
                        <MetricCard
                            label="Error Rate".to_string()
                            value=m.error_rate.map(|r| format!("{:.2}%", r * 100.0)).unwrap_or("-".to_string())
                            sub_label=None
                        />
                    </div>
                }.into_any(),
                None => {
                    // Fall back to kernel memory info from status
                    let memory_info = status.kernel.and_then(|k| k.memory);
                    view! {
                        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                            {memory_info.map(|mem| view! {
                                // UMA Memory
                                {mem.uma.map(|uma| view! {
                                    <MetricCard
                                        label="UMA Memory".to_string()
                                        value=uma.used_mb.map(|u| format!("{} MB", u)).unwrap_or("-".to_string())
                                        sub_label=uma.headroom_pct.map(|h| format!("{:.1}% headroom", h))
                                    />
                                })}
                                // ANE Memory
                                {mem.ane.map(|ane| view! {
                                    <MetricCard
                                        label="ANE Memory".to_string()
                                        value=ane.used_mb.map(|u| format!("{} MB", u)).unwrap_or("-".to_string())
                                        sub_label=ane.usage_pct.map(|u| format!("{:.1}% used", u))
                                    />
                                })}
                                // Memory Pressure
                                {mem.pressure.map(|p| view! {
                                    <MetricCard
                                        label="Memory Pressure".to_string()
                                        value=p.clone()
                                        sub_label=None
                                    />
                                })}
                            })}
                            <div class="text-center py-4 text-muted-foreground col-span-full">
                                <p class="text-sm">"Detailed metrics unavailable"</p>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Card>
    }
}

/// Individual metric card
#[component]
fn MetricCard(label: String, value: String, sub_label: Option<String>) -> impl IntoView {
    view! {
        <div class="rounded-lg border p-4">
            <p class="text-xs text-muted-foreground mb-1">{label}</p>
            <p class="text-2xl font-bold">{value}</p>
            {sub_label.map(|sub| view! {
                <p class="text-xs text-muted-foreground mt-1">{sub}</p>
            })}
        </div>
    }
}

/// Inference blockers / recent events section
#[component]
fn InferenceBlockersSection(blockers: Vec<InferenceBlocker>) -> impl IntoView {
    view! {
        <Card title="Inference Blockers".to_string() description="Issues preventing inference from running".to_string()>
            {if blockers.is_empty() {
                view! {
                    <div class="flex items-center gap-2 text-green-600">
                        <CheckCircleIcon/>
                        <span>"No blockers - system is ready for inference"</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-2">
                        {blockers.into_iter().map(|blocker| {
                            let (icon_color, message) = match blocker {
                                InferenceBlocker::DatabaseUnavailable => ("text-red-500", "Database is unavailable"),
                                InferenceBlocker::WorkerMissing => ("text-red-500", "No healthy workers available"),
                                InferenceBlocker::NoModelLoaded => ("text-yellow-500", "No model loaded"),
                                InferenceBlocker::ActiveModelMismatch => ("text-yellow-500", "Active model mismatch"),
                                InferenceBlocker::TelemetryDegraded => ("text-yellow-500", "Telemetry is degraded"),
                                InferenceBlocker::SystemBooting => ("text-blue-500", "System is still booting"),
                                InferenceBlocker::BootFailed => ("text-red-500", "Boot failed with critical error"),
                            };

                            view! {
                                <div class=format!("flex items-center gap-2 {}", icon_color)>
                                    <WarningIcon/>
                                    <span>{message}</span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </Card>
    }
}

/// Boot status section (optional, shown when boot info is available)
#[component]
fn BootStatusSection(boot: adapteros_api_types::BootStatus) -> impl IntoView {
    view! {
        <Card title="Boot Status".to_string() description="System boot lifecycle information".to_string()>
            <div class="space-y-4">
                // Phase
                <div class="flex items-center gap-4">
                    <span class="text-sm font-medium">"Current Phase:"</span>
                    <Badge variant=BadgeVariant::Secondary>
                        {boot.phase.clone()}
                    </Badge>
                </div>

                // Boot trace ID
                {boot.boot_trace_id.clone().map(|trace_id| view! {
                    <div class="flex items-center gap-2">
                        <span class="text-sm text-muted-foreground">"Trace ID:"</span>
                        <span class="font-mono text-sm">{trace_id}</span>
                    </div>
                })}

                // Timings
                {if !boot.timings.is_empty() {
                    view! {
                        <div class="space-y-2">
                            <span class="text-sm font-medium">"Phase Timings"</span>
                            <div class="grid gap-2 md:grid-cols-3">
                                {boot.timings.iter().map(|timing| {
                                    view! {
                                        <div class="flex items-center justify-between p-2 rounded border">
                                            <span class="text-sm">{timing.phase.clone()}</span>
                                            <span class="text-sm text-muted-foreground">{format!("{} ms", timing.elapsed_ms)}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}

                // Degraded components
                {if !boot.degraded.is_empty() {
                    view! {
                        <div class="space-y-2">
                            <span class="text-sm font-medium text-yellow-600">"Degraded Components"</span>
                            <div class="space-y-1">
                                {boot.degraded.iter().map(|d| {
                                    view! {
                                        <div class="flex items-center gap-2 text-yellow-600">
                                            <WarningIcon/>
                                            <span class="text-sm">{format!("{}: {}", d.component, d.reason)}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}

                // Boot failure
                {boot.failure.map(|failure| {
                    view! {
                        <div class="p-4 rounded-lg bg-destructive/10 border border-destructive">
                            <div class="flex items-center gap-2 text-destructive font-medium">
                                <WarningIcon/>
                                <span>"Boot Failure"</span>
                            </div>
                            <p class="text-sm text-destructive mt-2">
                                "Code: " <span class="font-mono">{failure.code}</span>
                            </p>
                            {failure.message.map(|msg| view! {
                                <p class="text-sm text-destructive/80 mt-1">{msg}</p>
                            })}
                        </div>
                    }
                })}
            </div>
        </Card>
    }
}

// --- Icon Components ---

#[component]
fn ChevronDownIcon() -> impl IntoView {
    view! {
        <svg class="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
        </svg>
    }
}

#[component]
fn ChevronUpIcon() -> impl IntoView {
    view! {
        <svg class="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7"/>
        </svg>
    }
}

#[component]
fn CheckCircleIcon() -> impl IntoView {
    view! {
        <svg class="h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </svg>
    }
}

#[component]
fn WarningIcon() -> impl IntoView {
    view! {
        <svg class="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
        </svg>
    }
}

// --- Helper Functions ---

/// Format a timestamp for display
fn format_timestamp(timestamp: &str) -> String {
    // Try to parse and format nicely, otherwise return as-is
    if timestamp == "-" || timestamp.is_empty() {
        return "-".to_string();
    }

    // If it looks like an ISO timestamp, try to make it more readable
    if timestamp.contains('T') {
        // Extract time portion
        if let Some(time_part) = timestamp.split('T').nth(1) {
            let time = time_part.split('.').next().unwrap_or(time_part);
            return time.to_string();
        }
    }

    timestamp.to_string()
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
