//! System page components
//!
//! UI components for the system overview page including status cards,
//! workers/nodes tables, system state, health details, metrics, and boot status.

use crate::api::types::{StorageModeResponse, StorageStatsResponse, TenantStorageUsageResponse};
use crate::api::{
    ApiClient, ApiError, ComponentStatus, ReadyzCheck, ReadyzChecks, ReadyzResponse, SseState,
    SystemHealthResponse, SystemReadyResponse,
};
use crate::components::{
    loaded_signal, Badge, BadgeVariant, Card, Column, DataTable, Spinner, StatusColor,
    StatusIndicator, StatusVariant, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{
    AdapterMemorySummary, AllModelsStatusResponse, BaseModelStatusResponse, ComponentCheck,
    DriftLevel, HealthResponse, InferenceBlocker, InferenceReadyState, MemoryPressureLevel,
    ModelLoadStatus, NodeResponse, RagStatus, ServiceHealthStatus, ServiceState,
    StatusIndicator as ApiStatusIndicator, SystemMetricsResponse, SystemStateResponse,
    SystemStatusResponse, TenantState, WorkerResponse,
};
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use super::utils::{format_timestamp, format_uptime, NODES_PAGE_SIZE, TENANTS_PAGE_SIZE};
use crate::components::{IconCheckCircle, IconWarning};

// ============================================================================
// SSE Indicator
// ============================================================================

/// SSE connection status indicator with detailed state display
#[component]
pub fn SseIndicator(state: RwSignal<SseState>) -> impl IntoView {
    view! {
        {move || {
            // Use try_get() to safely handle disposed signals during navigation
            let current_state = state.try_get().unwrap_or(SseState::Disconnected);
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
                        <span class="text-xs text-status-success font-medium">"(workers)"</span>
                    })}
                </div>
            }
        }}
    }
}

// ============================================================================
// System Content
// ============================================================================

/// Main system content with all sections
#[component]
pub fn SystemContent(
    status: SystemStatusResponse,
    workers: Vec<WorkerResponse>,
    nodes: Vec<NodeResponse>,
    metrics: Option<SystemMetricsResponse>,
    state: LoadingState<SystemStateResponse>,
    models_status: LoadingState<AllModelsStatusResponse>,
    healthz: LoadingState<(u16, HealthResponse)>,
    readyz: LoadingState<(u16, ReadyzResponse)>,
    healthz_all: LoadingState<SystemHealthResponse>,
    system_ready: LoadingState<(u16, SystemReadyResponse)>,
    /// Real-time worker status overrides from SSE (worker_id -> (status, timestamp))
    #[prop(default = HashMap::new())]
    worker_status_overrides: HashMap<String, (String, String)>,
) -> impl IntoView {
    let is_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let db_status = matches!(status.readiness.checks.db.status, ApiStatusIndicator::Ready);

    let inference_ready = matches!(status.inference_ready, InferenceReadyState::True);

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

    // Unified model status: prefer models_status API data for consistency
    let models_from_api = models_status.data().map(|data| {
        let loaded = data.models.iter().filter(|m| m.is_loaded).count() as i64;
        let total = data.models.len() as i64;
        let active_model = data.models.iter().find(|m| m.is_loaded);
        (loaded, total, active_model.cloned())
    });

    // Extract loaded/total counts - prioritize API data for consistency
    let (models_loaded, models_total, api_active_model) = match models_from_api {
        Some((loaded, total, active)) => (Some(loaded), Some(total), active),
        None => {
            // Fallback to kernel data only if API not available
            let kernel_models = status.kernel.as_ref().and_then(|k| k.models.as_ref());
            let loaded = kernel_models.and_then(|m| m.loaded).or_else(|| {
                status
                    .kernel
                    .as_ref()
                    .and_then(|k| k.adapters.as_ref())
                    .and_then(|a| a.loaded_models)
            });
            let total = kernel_models.and_then(|m| m.total);
            (loaded, total, None)
        }
    };

    // Active model detail - prefer API data, fall back to kernel
    let active_model_detail = if let Some(ref model) = api_active_model {
        let short_id = if model.model_name.len() > 8 {
            format!("{}...", &model.model_name[..8])
        } else {
            model.model_name.clone()
        };
        let status_label = format!("{:?}", model.status).to_lowercase();
        format!("Active: {} ({})", short_id, status_label)
    } else if let Some(summary) = status.kernel.as_ref().and_then(|k| k.model.as_ref()) {
        let status_label = summary.status.clone();
        if let Some(model_id) = summary.model_id.clone() {
            let short_id = adapteros_id::short_id(&model_id);
            format!("Active: {} ({})", short_id, status_label)
        } else {
            format!("Active: - ({})", status_label)
        }
    } else {
        "Active: -".to_string()
    };

    view! {
        // Section 1: Status Overview Cards
        <StatusOverview
            is_ready=is_ready
            db_status=db_status
            inference_ready=inference_ready
            healthy_workers=healthy_workers
            total_workers=total_workers
            models_loaded=models_loaded
            models_total=models_total
            active_model_detail=active_model_detail
        />

        // Section 2: Workers Table (with real-time SSE updates applied)
        <WorkersSection workers=workers_with_overrides.clone()/>

        // Section 3: Nodes Table
        <NodesSection nodes=nodes/>

        // Section 4: System State
        // Clone state before consuming it — the service control section needs node.services
        {
            let state_for_svc = state.clone();
            view! {
                <SystemStateSection state=state/>
                <ServiceControlSection state=state_for_svc/>
            }
        }

        // Section 5: Model Runtime
        <ModelRuntimeSection models_status=models_status/>

        // Section 6: Health Details
        <HealthDetails status=status.clone()/>

        // Section 7: Health Endpoints
        <HealthEndpointsSection
            healthz=healthz
            readyz=readyz
            healthz_all=healthz_all
            system_ready=system_ready
        />

        // Section 8: Metrics Summary
        <MetricsSummary metrics=metrics status=status.clone()/>

        // Section 9: Inference Blockers / Recent Events
        <InferenceBlockersSection blockers=status.inference_blockers.clone()/>

        // Section 10: Boot Status (if available)
        {status.boot.map(|boot| view! {
            <BootStatusSection boot=boot/>
        })}

        // Section 11: Admin Lifecycle (shutdown/maintenance/restart)
        <super::lifecycle::AdminLifecyclePanel/>

        // Section 12: Storage Visibility (admin-only, hides on 403)
        <StorageVisibilityPanel/>
    }
}

// ============================================================================
// Status Overview
// ============================================================================

/// Status overview cards at the top
#[component]
fn StatusOverview(
    is_ready: bool,
    db_status: bool,
    inference_ready: bool,
    healthy_workers: usize,
    total_workers: usize,
    models_loaded: Option<i64>,
    models_total: Option<i64>,
    active_model_detail: String,
) -> impl IntoView {
    let models_label = match (models_loaded, models_total) {
        (Some(loaded), Some(total)) => format!("{} / {}", loaded, total),
        (Some(loaded), None) => loaded.to_string(),
        (None, Some(total)) => format!("- / {}", total),
        (None, None) => "-".to_string(),
    };
    let active_model_title = active_model_detail.clone();

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
                    {models_label}
                </div>
                <p class="text-xs text-muted-foreground">"Loaded / Total"</p>
                <p class="text-xs text-muted-foreground truncate" title=active_model_title>{active_model_detail}</p>
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

// ============================================================================
// Workers Summary Section
// ============================================================================

/// Compact workers summary with status counts and link to /workers
#[component]
fn WorkersSection(workers: Vec<WorkerResponse>) -> impl IntoView {
    let total = workers.len();
    let healthy = workers.iter().filter(|w| w.status == "healthy").count();
    let draining = workers.iter().filter(|w| w.status == "draining").count();
    let error = workers.iter().filter(|w| w.status == "error").count();
    let stopped = workers.iter().filter(|w| w.status == "stopped").count();
    let other = total - healthy - draining - error - stopped;
    let status_glossary = "healthy=ready and accepting requests; draining=rejecting new requests while finishing in-flight; stopped=clean shutdown complete; error=terminal failure; crashed/failed=legacy terminal failure labels.";
    let action_guidance =
        "Decision: choose Drain for graceful maintenance; choose Stop only for urgent termination.";

    view! {
        <Card title="Workers".to_string() description="Worker process overview".to_string()>
            {if total == 0 {
                view! {
                    <div class="text-center py-6">
                        <p class="text-muted-foreground">"No workers registered"</p>
                        <p class="text-sm text-muted-foreground mt-1">"Workers serve inference requests. Next: open Manage Workers and choose Spawn Worker."</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="flex items-center gap-6 flex-wrap">
                        <div class="text-center">
                            <div class="text-2xl font-bold">{total}</div>
                            <p class="text-xs text-muted-foreground">"Total"</p>
                        </div>
                        {(healthy > 0).then(|| view! {
                            <div class="flex items-center gap-1.5">
                                <Badge variant=BadgeVariant::Success>{format!("{} healthy", healthy)}</Badge>
                            </div>
                        })}
                        {(draining > 0).then(|| view! {
                            <div class="flex items-center gap-1.5">
                                <Badge variant=BadgeVariant::Warning>{format!("{} draining", draining)}</Badge>
                            </div>
                        })}
                        {(error > 0).then(|| view! {
                            <div class="flex items-center gap-1.5">
                                <Badge variant=BadgeVariant::Destructive>{format!("{} error", error)}</Badge>
                            </div>
                        })}
                        {(stopped > 0).then(|| view! {
                            <div class="flex items-center gap-1.5">
                                <Badge variant=BadgeVariant::Secondary>{format!("{} stopped", stopped)}</Badge>
                            </div>
                        })}
                        {(other > 0).then(|| view! {
                            <div class="flex items-center gap-1.5">
                                <Badge variant=BadgeVariant::Secondary>{format!("{} other", other)}</Badge>
                            </div>
                        })}
                    </div>
                }.into_any()
            }}

            <div class="mt-3 text-xs text-muted-foreground space-y-1">
                <p>{"Status guide: "}{status_glossary}</p>
                <p>{action_guidance}</p>
            </div>

            <div class="mt-4 pt-4 border-t">
                <a
                    href="/workers"
                    class="text-sm font-medium hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                >
                    "Manage Workers →"
                </a>
            </div>
        </Card>
    }
}

// ============================================================================
// Nodes Section
// ============================================================================

/// Nodes section (client-side pagination)
#[component]
fn NodesSection(nodes: Vec<NodeResponse>) -> impl IntoView {
    let total = nodes.len();
    let visible_count = RwSignal::new(NODES_PAGE_SIZE);

    let columns: Vec<Column<NodeResponse>> = vec![
        Column::custom("ID", |node: &NodeResponse| {
            let short_id = adapteros_id::short_id(&node.node.id);
            let full_id = node.node.id.clone();
            view! { <span class="font-mono text-sm" title=full_id>{short_id}</span> }
        }),
        Column::text("Hostname", |node: &NodeResponse| node.node.hostname.clone()),
        Column::custom("Status", |node: &NodeResponse| {
            let variant = StatusVariant::from_worker_status(&node.node.status).to_badge_variant();
            let status = node.node.status.clone();
            view! { <Badge variant=variant>{status}</Badge> }
        }),
        Column::custom("Endpoint", |node: &NodeResponse| {
            let endpoint = node.node.agent_endpoint.clone();
            view! { <span class="text-sm font-mono">{endpoint}</span> }
        }),
        Column::custom("Last Seen", |node: &NodeResponse| {
            let ts = node
                .node
                .last_seen_at
                .clone()
                .map(|t| format_timestamp(&t))
                .unwrap_or("-".to_string());
            view! { <span class="text-sm text-muted-foreground">{ts}</span> }
        }),
    ];

    let visible_nodes = Signal::derive(move || {
        let count = visible_count.get();
        nodes.iter().take(count).cloned().collect::<Vec<_>>()
    });

    view! {
        <Card title="Nodes".to_string() description="Cluster nodes and their connectivity status".to_string()>
            <DataTable
                data=loaded_signal(visible_nodes)
                columns=columns
                empty_title="No nodes registered"
                card=false
            />

            // Show more button
            {move || {
                let count = visible_count.get();
                if count < total {
                    let remaining = total - count;
                    Some(view! {
                        <div class="text-center py-4 border-t">
                            <button
                                class="text-sm text-muted-foreground hover:text-foreground underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    visible_count.update(|c| *c = (*c + NODES_PAGE_SIZE).min(total));
                                }
                            >
                                {format!("Show more ({} remaining)", remaining)}
                            </button>
                        </div>
                    })
                } else {
                    None
                }
            }}
        </Card>
    }
}

// ============================================================================
// System State Section
// ============================================================================

#[component]
fn SystemStateSection(state: LoadingState<SystemStateResponse>) -> impl IntoView {
    match state {
        LoadingState::Loaded(state) => {
            let SystemStateResponse {
                tenants,
                node,
                memory,
                rag_status,
                ..
            } = state;
            let tenant_count = tenants.len();
            let stack_total: usize = tenants.iter().map(|t| t.stacks.len()).sum();
            let active_stack_count: usize = tenants
                .iter()
                .flat_map(|t| t.stacks.iter())
                .filter(|s| s.is_active)
                .count();
            let adapter_total: usize = tenants.iter().map(|t| t.adapter_count).sum();
            let headroom_percent = memory.headroom_percent;
            let pressure_level = memory.pressure_level;
            let top_adapters = memory.top_adapters;

            view! {
                <div class="space-y-6">
                    <StateSummary
                        tenant_count=tenant_count
                        stack_total=stack_total
                        active_stack_count=active_stack_count
                        adapter_total=adapter_total
                        headroom_percent=headroom_percent
                        pressure_level=pressure_level
                        rag_status=rag_status
                    />
                    <TenantsSection tenants=tenants/>
                    <div class="grid gap-6 lg:grid-cols-2">
                        <NodeServicesSection services=node.services/>
                        <TopAdaptersSection adapters=top_adapters/>
                    </div>
                </div>
            }
            .into_any()
        }
        LoadingState::Idle | LoadingState::Loading => view! {
            <Card title="System State".to_string() description="Tenant and service inventory".to_string()>
                <div class="flex items-center justify-center gap-2 py-6 text-muted-foreground">
                    <Spinner/>
                    <span class="text-sm">"Loading system state..."</span>
                </div>
            </Card>
        }
        .into_any(),
        LoadingState::Error(e) => view! {
            <Card title="System State".to_string() description="Tenant and service inventory".to_string()>
                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                    <p class="text-sm text-destructive">{format!("Failed to load: {}", e)}</p>
                </div>
            </Card>
        }
        .into_any(),
    }
}

#[component]
fn StateSummary(
    tenant_count: usize,
    stack_total: usize,
    active_stack_count: usize,
    adapter_total: usize,
    headroom_percent: f32,
    pressure_level: MemoryPressureLevel,
    rag_status: Option<RagStatus>,
) -> impl IntoView {
    view! {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-5">
            <Card title="Tenants".to_string()>
                <div class="text-2xl font-bold">{tenant_count}</div>
                <p class="text-xs text-muted-foreground">"Total tenants"</p>
            </Card>
            <Card title="Stacks".to_string()>
                <div class="text-2xl font-bold">
                    {format!("{} / {}", active_stack_count, stack_total)}
                </div>
                <p class="text-xs text-muted-foreground">"Active / total"</p>
            </Card>
            <Card title="Adapters".to_string()>
                <div class="text-2xl font-bold">{adapter_total}</div>
                <p class="text-xs text-muted-foreground">"Active adapters"</p>
            </Card>
            <Card title="Headroom".to_string()>
                <div class="text-2xl font-bold">{format!("{:.1}%", headroom_percent)}</div>
                <p class="text-xs text-muted-foreground">"Memory headroom"</p>
            </Card>
            <Card title="Pressure".to_string()>
                <div class="text-2xl font-bold">{pressure_level.to_string()}</div>
                <p class="text-xs text-muted-foreground">"Memory pressure"</p>
            </Card>
            {rag_status.map(|rag| {
                let (label, detail, color) = match rag {
                    RagStatus::Enabled { model_hash, dimension } => {
                        let short_hash = adapteros_id::format_hash_short(&model_hash);
                        (
                            "Enabled".to_string(),
                            format!("Model: {} ({}d)", short_hash, dimension),
                            StatusColor::Green,
                        )
                    }
                    RagStatus::Disabled { reason } => (
                        "Disabled".to_string(),
                        format!("Reason: {}", reason),
                        StatusColor::Yellow,
                    ),
                };

                view! {
                    <Card title="RAG".to_string()>
                        <div class="flex items-center gap-2">
                            <StatusIndicator color=color label=label/>
                        </div>
                        <p class="text-xs text-muted-foreground">{detail}</p>
                    </Card>
                }
            })}
        </div>
    }
}

#[component]
fn TenantsSection(tenants: Vec<TenantState>) -> impl IntoView {
    let total = tenants.len();
    let visible_count = RwSignal::new(TENANTS_PAGE_SIZE);

    let columns: Vec<Column<TenantState>> = vec![
        Column::custom("Tenant", |tenant: &TenantState| {
            let name = tenant.name.clone();
            let full_id = tenant.tenant_id.clone();
            let short_id = adapteros_id::short_id(&tenant.tenant_id);
            view! {
                <div>
                    <p class="text-sm font-medium">{name}</p>
                    <p class="text-xs text-muted-foreground font-mono" title=full_id>
                        {short_id}
                    </p>
                </div>
            }
        }),
        Column::custom("Status", |tenant: &TenantState| {
            let variant = match tenant.status.as_str() {
                "active" => BadgeVariant::Success,
                "paused" => BadgeVariant::Warning,
                "archived" => BadgeVariant::Secondary,
                "error" => BadgeVariant::Destructive,
                _ => BadgeVariant::Secondary,
            };
            let status = tenant.status.clone();
            view! { <Badge variant=variant>{status}</Badge> }
        }),
        Column::custom("Active Stack", |tenant: &TenantState| {
            let active_stack = tenant
                .stacks
                .iter()
                .find(|s| s.is_active)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "-".to_string());
            let title = active_stack.clone();
            view! { <span class="text-sm truncate" title=title>{active_stack}</span> }
        }),
        Column::custom("Stacks", |tenant: &TenantState| {
            let count = tenant.stacks.len();
            view! { <span class="text-sm">{count}</span> }
        }),
        Column::custom("Adapters", |tenant: &TenantState| {
            let count = tenant.adapter_count;
            view! { <span class="text-sm">{count}</span> }
        }),
        Column::custom("Memory", |tenant: &TenantState| {
            let mem = format!("{:.1} MB", tenant.memory_usage_mb);
            view! { <span class="text-sm">{mem}</span> }
        }),
    ];

    let visible_tenants = Signal::derive(move || {
        let count = visible_count.get();
        tenants.iter().take(count).cloned().collect::<Vec<_>>()
    });

    view! {
        <Card title="Tenants".to_string() description="Tenant status and active stacks".to_string()>
            <DataTable
                data=loaded_signal(visible_tenants)
                columns=columns
                empty_title="No tenants available"
                card=false
            />

            {move || {
                let count = visible_count.get();
                if count < total {
                    let remaining = total - count;
                    Some(view! {
                        <div class="text-center py-4 border-t">
                            <button
                                class="text-sm text-muted-foreground hover:text-foreground underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                on:click=move |_| {
                                    visible_count.update(|c| *c = (*c + TENANTS_PAGE_SIZE).min(total));
                                }
                            >
                                {format!("Show more ({} remaining)", remaining)}
                            </button>
                        </div>
                    })
                } else {
                    None
                }
            }}
        </Card>
    }
}

#[component]
fn NodeServicesSection(services: Vec<ServiceState>) -> impl IntoView {
    let columns: Vec<Column<ServiceState>> = vec![
        Column::custom("Service", |svc: &ServiceState| {
            let name = svc.name.clone();
            view! { <span class="text-sm font-medium">{name}</span> }
        }),
        Column::custom("Status", |svc: &ServiceState| {
            let (variant, label) = match svc.status {
                ServiceHealthStatus::Healthy => (BadgeVariant::Success, "Healthy"),
                ServiceHealthStatus::Degraded => (BadgeVariant::Warning, "Degraded"),
                ServiceHealthStatus::Unhealthy => (BadgeVariant::Destructive, "Unhealthy"),
                ServiceHealthStatus::Unknown => (BadgeVariant::Secondary, "Unknown"),
            };
            view! { <Badge variant=variant>{label}</Badge> }
        }),
        Column::custom("Last Check", |svc: &ServiceState| {
            let ts = format_timestamp(&svc.last_check);
            view! { <span class="text-sm text-muted-foreground">{ts}</span> }
        }),
    ];

    let items = Signal::derive(move || services.clone());

    view! {
        <Card title="Node Services".to_string() description="Service health checks".to_string()>
            <DataTable
                data=loaded_signal(items)
                columns=columns
                empty_title="No service data reported"
                card=false
            />
        </Card>
    }
}

/// Wrapper that extracts node services from system state for the ServiceControlPanel
#[component]
fn ServiceControlSection(state: LoadingState<SystemStateResponse>) -> impl IntoView {
    match state {
        LoadingState::Loaded(state) => view! {
            <super::services::ServiceControlPanel services=state.node.services/>
        }
        .into_any(),
        _ => view! {}.into_any(),
    }
}

#[component]
fn TopAdaptersSection(adapters: Vec<AdapterMemorySummary>) -> impl IntoView {
    let columns: Vec<Column<AdapterMemorySummary>> = vec![
        Column::custom("Adapter", |a: &AdapterMemorySummary| {
            let name = a.name.clone();
            let full_id = a.adapter_id.clone();
            let short_id = adapteros_id::short_id(&a.adapter_id);
            view! {
                <div>
                    <p class="text-sm font-medium">{name}</p>
                    <p class="text-xs text-muted-foreground font-mono" title=full_id>
                        {short_id}
                    </p>
                </div>
            }
        }),
        Column::text("State", |a: &AdapterMemorySummary| a.state.to_string()),
        Column::custom("Tenant", |a: &AdapterMemorySummary| {
            let full_id = a.tenant_id.clone();
            let short_id = adapteros_id::short_id(&a.tenant_id);
            view! {
                <span class="text-sm font-mono" title=full_id>{short_id}</span>
            }
        }),
        Column::text("Memory", |a: &AdapterMemorySummary| {
            format!("{:.1} MB", a.memory_mb)
        }),
    ];

    let items = Signal::derive(move || adapters.clone());

    view! {
        <Card title="Top Adapters".to_string() description="Highest memory adapters".to_string()>
            <DataTable
                data=loaded_signal(items)
                columns=columns
                empty_title="No adapter memory data available"
                card=false
            />
        </Card>
    }
}

// ============================================================================
// Model Runtime Section
// ============================================================================

#[component]
fn ModelRuntimeSection(models_status: LoadingState<AllModelsStatusResponse>) -> impl IntoView {
    view! {
        <Card title="Model Runtime".to_string() description="Base model load status".to_string()>
            {match models_status {
                LoadingState::Idle | LoadingState::Loading => view! {
                    <div class="flex items-center justify-center gap-2 py-6 text-muted-foreground">
                        <Spinner/>
                        <span class="text-sm">"Loading model runtime..."</span>
                    </div>
                }.into_any(),
                LoadingState::Loaded(data) => {
                    if data.models.is_empty() {
                        view! {
                            <div class="text-center py-8">
                                <p class="text-muted-foreground">"No models available"</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>"Model"</TableHead>
                                        <TableHead>"Status"</TableHead>
                                        <TableHead>"Memory"</TableHead>
                                        <TableHead>"Loaded At"</TableHead>
                                        <TableHead>"Updated"</TableHead>
                                        <TableHead>"Error"</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {data.models.into_iter().map(|model| {
                                        view! { <ModelRuntimeRow model=model/> }
                                    }).collect::<Vec<_>>()}
                                </TableBody>
                            </Table>
                        }.into_any()
                    }
                }
                LoadingState::Error(e) => {
                    if matches!(&e, ApiError::Forbidden(_)) {
                        view! {
                            <div class="rounded-lg border p-4">
                                <p class="text-sm text-muted-foreground">
                                    "Model runtime status requires admin permissions."
                                </p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-sm text-destructive">{format!("Failed to load: {}", e)}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </Card>
    }
}

#[component]
fn ModelRuntimeRow(model: BaseModelStatusResponse) -> impl IntoView {
    let short_id = adapteros_id::short_id(&model.model_id);
    let name_title = model
        .model_path
        .clone()
        .unwrap_or_else(|| model.model_name.clone());
    let (status_variant, status_label) = model_status_badge(model.status);
    let memory_label = model
        .memory_usage_mb
        .map(|m| format!("{} MB", m))
        .unwrap_or_else(|| "-".to_string());
    let loaded_at = model.loaded_at.clone().unwrap_or_else(|| "-".to_string());
    let updated_at = format_timestamp(&model.updated_at);
    let has_error = model.error_message.is_some();
    let error_text = model
        .error_message
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let error_title = if has_error {
        error_text.clone()
    } else {
        String::new()
    };
    let error_class = if has_error {
        "text-sm text-destructive truncate max-w-60"
    } else {
        "text-sm text-muted-foreground"
    };

    view! {
        <TableRow>
            <TableCell>
                <div>
                    <p class="text-sm font-medium" title=name_title>{model.model_name.clone()}</p>
                    <p class="text-xs text-muted-foreground font-mono" title=model.model_id.clone()>
                        {short_id}
                    </p>
                </div>
            </TableCell>
            <TableCell>
                <Badge variant=status_variant>{status_label}</Badge>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{memory_label}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{format_timestamp(&loaded_at)}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{updated_at}</span>
            </TableCell>
            <TableCell>
                <span class=error_class title=error_title>{error_text}</span>
            </TableCell>
        </TableRow>
    }
}

fn model_status_badge(status: ModelLoadStatus) -> (BadgeVariant, &'static str) {
    match status {
        ModelLoadStatus::Ready => (BadgeVariant::Success, "Ready"),
        ModelLoadStatus::Loading => (BadgeVariant::Secondary, "Loading"),
        ModelLoadStatus::Unloading => (BadgeVariant::Secondary, "Unloading"),
        ModelLoadStatus::Checking => (BadgeVariant::Secondary, "Checking"),
        ModelLoadStatus::Error => (BadgeVariant::Destructive, "Error"),
        ModelLoadStatus::NoModel => (BadgeVariant::Secondary, "Unloaded"),
    }
}

// ============================================================================
// Health Details
// ============================================================================

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

// ============================================================================
// Health Endpoints
// ============================================================================

#[component]
fn HealthEndpointsSection(
    healthz: LoadingState<(u16, HealthResponse)>,
    readyz: LoadingState<(u16, ReadyzResponse)>,
    healthz_all: LoadingState<SystemHealthResponse>,
    system_ready: LoadingState<(u16, SystemReadyResponse)>,
) -> impl IntoView {
    let healthz_all_summary = healthz_all.clone();
    let healthz_all_components = healthz_all;

    view! {
        <Card title="Health Endpoints".to_string() description="Live status from /healthz, /readyz, and /system/ready".to_string()>
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
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
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
                                        <div class="text-sm text-muted-foreground">
                                            <span>{format!("HTTP {} | {}", status_code, summary)}</span>
                                        </div>
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
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}

                    {match healthz_all_summary {
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
                                <TableCell><span class="text-sm text-destructive">{e.user_message()}</span></TableCell>
                            </TableRow>
                        }.into_any(),
                    }}
                </TableBody>
            </Table>

            <div class="mt-4 pt-4 border-t">
                <h4 class="text-sm font-medium mb-2">"Component Checks"</h4>
                {match healthz_all_components {
                    LoadingState::Loaded(data) => {
                        if data.components.is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground">"No component checks reported."</p>
                            }.into_any()
                        } else {
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Component"</TableHead>
                                            <TableHead>"Status"</TableHead>
                                            <TableHead>"Message"</TableHead>
                                            <TableHead>"Endpoint"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {data.components.into_iter().map(|component| {
                                            let (variant, label) = component_status_badge(component.status);
                                            let endpoint = format!("/healthz/{}", component.component);
                                            view! {
                                                <TableRow>
                                                    <TableCell>
                                                        <span class="text-sm font-mono">{component.component}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <Badge variant=variant>{label}</Badge>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm text-muted-foreground">{component.message}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm font-mono text-muted-foreground">{endpoint}</span>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(e) => view! {
                        <p class="text-sm text-destructive">{format!("Failed to load component checks: {}", e)}</p>
                    }.into_any(),
                    _ => view! {
                        <div class="flex items-center gap-2 text-muted-foreground">
                            <Spinner/>
                            <span class="text-sm">"Loading component checks..."</span>
                        </div>
                    }.into_any(),
                }}
            </div>
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

// ============================================================================
// Metrics Summary
// ============================================================================

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

// ============================================================================
// Inference Blockers
// ============================================================================

/// Inference blockers / recent events section
#[component]
fn InferenceBlockersSection(blockers: Vec<InferenceBlocker>) -> impl IntoView {
    view! {
        <Card title="Inference Blockers".to_string() description="Issues preventing inference from running".to_string()>
            {if blockers.is_empty() {
                view! {
                    <div class="flex items-center gap-2 text-status-success">
                        <IconCheckCircle/>
                        <span>"No blockers - system is ready for inference"</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-2">
                        {blockers.into_iter().map(|blocker| {
                            let (icon_color, message) = match blocker {
                                InferenceBlocker::DatabaseUnavailable => ("text-status-error", "Database is unavailable"),
                                InferenceBlocker::WorkerMissing => ("text-status-error", "No healthy workers available"),
                                InferenceBlocker::NoModelLoaded => ("text-status-warning", "No model loaded"),
                                InferenceBlocker::ActiveModelMismatch => ("text-status-warning", "Active model mismatch"),
                                InferenceBlocker::TelemetryDegraded => ("text-status-warning", "Telemetry is degraded"),
                                InferenceBlocker::SystemBooting => ("text-status-info", "System is still booting"),
                                InferenceBlocker::BootFailed => ("text-status-error", "Boot failed with critical error"),
                            };

                            view! {
                                <div class=format!("flex items-center gap-2 {}", icon_color)>
                                    <IconWarning/>
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

// ============================================================================
// Boot Status
// ============================================================================

/// Boot status section (optional, shown when boot info is available)
/// Collapsible by default to avoid obscuring main content
#[component]
pub fn BootStatusSection(boot: adapteros_api_types::BootStatus) -> impl IntoView {
    // Default to collapsed unless there are issues
    let has_issues = !boot.degraded.is_empty() || boot.failure.is_some();
    let expanded = RwSignal::new(has_issues);
    let dismissed = RwSignal::new(false);

    // Store phase in a signal so it can be read reactively without ownership issues
    let phase_for_header = RwSignal::new(boot.phase.clone());

    view! {
        // Don't render if dismissed
        {move || {
            if dismissed.get() {
                return view! {}.into_any();
            }

            let boot = boot.clone();
            view! {
                <Card title="Boot Status".to_string() description="System boot lifecycle information".to_string()>
                    <div class="space-y-4">
                        // Header with phase, toggle, and dismiss buttons
                        <div class="flex items-center justify-between">
                            <div class="flex items-center gap-4">
                                <span class="text-sm font-medium">"Current Phase:"</span>
                                <Badge variant=BadgeVariant::Secondary>
                                    {move || phase_for_header.get()}
                                </Badge>
                            </div>
                            <div class="flex items-center gap-2">
                                // Toggle expand/collapse
                                <button
                                    class="p-1.5 rounded-md hover:bg-muted focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                    title=move || if expanded.get() { "Collapse boot details" } else { "Expand boot details" }
                                    aria-label=move || if expanded.get() { "Collapse boot details" } else { "Expand boot details" }
                                    aria-expanded=move || expanded.get().to_string()
                                    on:click=move |_| expanded.update(|v| *v = !*v)
                                >
                                    <svg
                                        class=move || format!("w-4 h-4 transition-transform {}", if expanded.get() { "rotate-180" } else { "" })
                                        fill="none"
                                        stroke="currentColor"
                                        viewBox="0 0 24 24"
                                        aria-hidden="true"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                                    </svg>
                                </button>
                                // Dismiss button
                                <button
                                    class="p-1.5 rounded-md hover:bg-muted focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                    title="Dismiss boot status"
                                    aria-label="Dismiss boot status panel"
                                    on:click=move |_| dismissed.set(true)
                                >
                                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                                    </svg>
                                </button>
                            </div>
                        </div>

                        // Collapsible details
                        {move || {
                            if !expanded.get() {
                                return view! {}.into_any();
                            }

                            let boot = boot.clone();
                            view! {
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
                                            <span class="text-sm font-medium text-status-warning">"Degraded Components"</span>
                                            <div class="space-y-1">
                                                {boot.degraded.iter().map(|d| {
                                                    view! {
                                                        <div class="flex items-center gap-2 text-status-warning">
                                                            <IconWarning/>
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
                                                <IconWarning/>
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
                            }.into_any()
                        }}
                    </div>
                </Card>
            }.into_any()
        }}
    }
}

// ============================================================================
// Storage Visibility (PASS 6)
// ============================================================================

fn format_bytes(bytes: u64) -> String {
    crate::utils::format_bytes(bytes as i64)
}

/// Storage visibility panel for system page.
///
/// Fetches storage mode, stats, and tenant usage independently.
/// Silently hides if all endpoints return errors (e.g. 403 for non-admins).
#[component]
pub fn StorageVisibilityPanel() -> impl IntoView {
    let (mode_data, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.get_storage_mode().await });
    let (stats_data, _) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.get_storage_stats().await });
    let (usage_data, _) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get_tenant_storage_usage().await
    });

    view! {
        {move || {
            let mode = mode_data.try_get().unwrap_or(LoadingState::Loading);
            let stats = stats_data.try_get().unwrap_or(LoadingState::Loading);
            let usage = usage_data.try_get().unwrap_or(LoadingState::Loading);

            // If all three failed, hide the entire panel
            let all_failed = matches!(&mode, LoadingState::Error(_))
                && matches!(&stats, LoadingState::Error(_))
                && matches!(&usage, LoadingState::Error(_));
            if all_failed {
                return view! {}.into_any();
            }

            // If all idle/loading, show spinner
            let all_loading = matches!(&mode, LoadingState::Idle | LoadingState::Loading)
                && matches!(&stats, LoadingState::Idle | LoadingState::Loading)
                && matches!(&usage, LoadingState::Idle | LoadingState::Loading);
            if all_loading {
                return view! {
                    <Card title="Storage".to_string() description="Storage backend visibility".to_string()>
                        <div class="flex items-center justify-center gap-2 py-6 text-muted-foreground">
                            <Spinner/>
                            <span class="text-sm">"Loading storage data..."</span>
                        </div>
                    </Card>
                }.into_any();
            }

            view! {
                <div class="space-y-4">
                    // Storage Mode card
                    {match mode {
                        LoadingState::Loaded(data) => view! {
                            <StorageModeCard data=data/>
                        }.into_any(),
                        _ => view! {}.into_any(),
                    }}

                    // Storage Stats card
                    {match stats {
                        LoadingState::Loaded(data) => view! {
                            <StorageStatsCard data=data/>
                        }.into_any(),
                        _ => view! {}.into_any(),
                    }}

                    // Tenant Usage card
                    {match usage {
                        LoadingState::Loaded(data) => view! {
                            <TenantUsageCard data=data/>
                        }.into_any(),
                        _ => view! {}.into_any(),
                    }}
                </div>
            }.into_any()
        }}
    }
}

#[component]
fn StorageModeCard(data: StorageModeResponse) -> impl IntoView {
    let mode_variant = match data.mode.as_str() {
        "dual" => BadgeVariant::Success,
        "kv" => BadgeVariant::Secondary,
        _ => BadgeVariant::Default,
    };

    view! {
        <Card title="Storage Mode".to_string() description="Current storage backend configuration".to_string()>
            <div class="space-y-3">
                <div class="flex items-center gap-3">
                    <span class="text-sm font-medium">"Mode:"</span>
                    <Badge variant=mode_variant>{data.mode.clone()}</Badge>
                </div>
                <p class="text-sm text-muted-foreground">{data.description.clone()}</p>
                <div class="grid gap-2 md:grid-cols-2">
                    <div class="flex items-center gap-2">
                        {if data.kv_available {
                            view! { <IconCheckCircle/> }.into_any()
                        } else {
                            view! { <IconWarning/> }.into_any()
                        }}
                        <span class="text-sm">
                            {if data.kv_available { "KV store available" } else { "KV store unavailable" }}
                        </span>
                    </div>
                    <div class="flex items-center gap-2">
                        {if data.dual_write_active {
                            view! { <IconCheckCircle/> }.into_any()
                        } else {
                            view! { <IconWarning/> }.into_any()
                        }}
                        <span class="text-sm">
                            {if data.dual_write_active { "Dual-write active" } else { "Dual-write inactive" }}
                        </span>
                    </div>
                </div>
            </div>
        </Card>
    }
}

#[component]
fn StorageStatsCard(data: StorageStatsResponse) -> impl IntoView {
    let sql_entries = json_object_to_entries(&data.sql_counts);
    let kv_entries = json_object_to_entries(&data.kv_counts);
    let collected = format_timestamp(&data.collected_at);

    view! {
        <Card title="Storage Statistics".to_string() description="Row and key counts by backend".to_string()>
            <div class="space-y-4">
                <div class="flex items-center gap-3">
                    <span class="text-sm font-medium">"Mode:"</span>
                    <Badge variant=BadgeVariant::Secondary>{data.mode.clone()}</Badge>
                </div>

                <div class="grid gap-4 md:grid-cols-2">
                    // SQL counts
                    <div>
                        <h4 class="text-sm font-medium mb-2">"SQL Counts"</h4>
                        {if sql_entries.is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground">"No SQL data"</p>
                            }.into_any()
                        } else {
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Table"</TableHead>
                                            <TableHead>"Count"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {sql_entries.into_iter().map(|(key, value)| view! {
                                            <TableRow>
                                                <TableCell>
                                                    <span class="text-sm font-mono">{key}</span>
                                                </TableCell>
                                                <TableCell>
                                                    <span class="text-sm">{value}</span>
                                                </TableCell>
                                            </TableRow>
                                        }).collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }.into_any()
                        }}
                    </div>

                    // KV counts
                    <div>
                        <h4 class="text-sm font-medium mb-2">"KV Counts"</h4>
                        {if kv_entries.is_empty() {
                            view! {
                                <p class="text-sm text-muted-foreground">"No KV data"</p>
                            }.into_any()
                        } else {
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Key"</TableHead>
                                            <TableHead>"Count"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {kv_entries.into_iter().map(|(key, value)| view! {
                                            <TableRow>
                                                <TableCell>
                                                    <span class="text-sm font-mono">{key}</span>
                                                </TableCell>
                                                <TableCell>
                                                    <span class="text-sm">{value}</span>
                                                </TableCell>
                                            </TableRow>
                                        }).collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }.into_any()
                        }}
                    </div>
                </div>

                <p class="text-xs text-muted-foreground">
                    "Collected: " {collected}
                </p>
            </div>
        </Card>
    }
}

/// Extract key-value pairs from a serde_json::Value object
fn json_object_to_entries(value: &serde_json::Value) -> Vec<(String, String)> {
    match value.as_object() {
        Some(map) => map
            .iter()
            .map(|(k, v)| {
                let display = match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), display)
            })
            .collect(),
        None => Vec::new(),
    }
}

#[component]
fn TenantUsageCard(data: TenantStorageUsageResponse) -> impl IntoView {
    let short_tenant = adapteros_id::short_id(&data.tenant_id);
    let full_tenant = data.tenant_id.clone();

    let dataset_label = format_bytes(data.dataset_bytes);
    let artifact_label = format_bytes(data.artifact_bytes);
    let total_bytes = data.dataset_bytes + data.artifact_bytes;
    let total_label = format_bytes(total_bytes);

    let soft_limit_label = format_bytes(data.soft_limit_bytes);
    let hard_limit_label = format_bytes(data.hard_limit_bytes);

    // Calculate usage percentages for progress display
    let soft_pct = if data.soft_limit_bytes > 0 {
        ((total_bytes as f64 / data.soft_limit_bytes as f64) * 100.0).min(100.0)
    } else {
        0.0
    };
    let hard_pct = if data.hard_limit_bytes > 0 {
        ((total_bytes as f64 / data.hard_limit_bytes as f64) * 100.0).min(100.0)
    } else {
        0.0
    };

    view! {
        <Card title="Tenant Storage Usage".to_string() description="Storage consumption and limits".to_string()>
            <div class="space-y-4">
                <div class="flex items-center gap-2">
                    <span class="text-sm font-medium">"Tenant:"</span>
                    <span class="text-sm font-mono" title=full_tenant>{short_tenant}</span>
                </div>

                <div class="grid gap-4 md:grid-cols-3">
                    <div class="rounded-lg border p-4">
                        <p class="text-xs text-muted-foreground mb-1">"Datasets"</p>
                        <p class="text-2xl font-bold">{dataset_label}</p>
                    </div>
                    <div class="rounded-lg border p-4">
                        <p class="text-xs text-muted-foreground mb-1">"Artifacts"</p>
                        <p class="text-2xl font-bold">{artifact_label}</p>
                    </div>
                    <div class="rounded-lg border p-4">
                        <p class="text-xs text-muted-foreground mb-1">"Total"</p>
                        <p class="text-2xl font-bold">{total_label}</p>
                    </div>
                </div>

                // Soft limit bar
                <div class="space-y-1">
                    <div class="flex items-center justify-between text-sm">
                        <span>"Soft limit"</span>
                        <div class="flex items-center gap-2">
                            <span class="text-muted-foreground">{soft_limit_label}</span>
                            {data.soft_exceeded.then(|| view! {
                                <Badge variant=BadgeVariant::Warning>"exceeded"</Badge>
                            })}
                        </div>
                    </div>
                    <div class="w-full h-2 rounded-full bg-muted overflow-hidden">
                        <div
                            class=if data.soft_exceeded { "h-full rounded-full bg-status-warning" } else { "h-full rounded-full bg-status-success" }
                            style=format!("width: {}%", soft_pct)
                        />
                    </div>
                </div>

                // Hard limit bar
                <div class="space-y-1">
                    <div class="flex items-center justify-between text-sm">
                        <span>"Hard limit"</span>
                        <div class="flex items-center gap-2">
                            <span class="text-muted-foreground">{hard_limit_label}</span>
                            {data.hard_exceeded.then(|| view! {
                                <Badge variant=BadgeVariant::Destructive>"exceeded"</Badge>
                            })}
                        </div>
                    </div>
                    <div class="w-full h-2 rounded-full bg-muted overflow-hidden">
                        <div
                            class=if data.hard_exceeded { "h-full rounded-full bg-status-error" } else { "h-full rounded-full bg-status-success" }
                            style=format!("width: {}%", hard_pct)
                        />
                    </div>
                </div>
            </div>
        </Card>
    }
}
