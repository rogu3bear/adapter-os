//! Workers page view components
//!
//! Subcomponents for displaying worker lists, details, and metrics.

use crate::api::{report_error_with_toast, ApiClient, ApiError, WorkerMetricsResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ConfirmationDialog,
    ConfirmationSeverity, CopyableId, EmptyState, EmptyStateVariant, Spinner, StatusColor,
    StatusIndicator, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api, use_api_resource, use_navigate, use_scope_alive, LoadingState};
use crate::signals::use_notifications;
use adapteros_api_types::WorkerResponse;
use leptos::prelude::*;
use std::sync::Arc;

use super::utils::{
    format_timestamp, format_uptime, health_badge_variant, is_terminal_worker_status,
    status_badge_variant, worker_display_name, WorkerHealthRecord, WorkerHealthSummary,
    WORKERS_PAGE_SIZE,
};
use crate::components::{IconPause, IconRefresh, IconStop, IconTrash, IconX};

// ============================================================================
// Summary Cards
// ============================================================================

#[component]
pub fn WorkersSummary(
    workers: Vec<WorkerResponse>,
    health_summary: Option<WorkerHealthSummary>,
) -> impl IntoView {
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

    let health_card = health_summary.map(|summary| {
        let counts = summary.summary;
        let updated = format_timestamp(&summary.timestamp);
        let show_updated = updated != "-";
        view! {
            <Card title="Health Summary".to_string()>
                <div class="space-y-2 text-xs">
                    <div class="flex items-center justify-between">
                        <span class="text-muted-foreground">"Total"</span>
                        <span class="font-semibold">{counts.total}</span>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-muted-foreground">"Healthy"</span>
                        <Badge variant=health_badge_variant("healthy")>{counts.healthy}</Badge>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-muted-foreground">"Degraded"</span>
                        <Badge variant=health_badge_variant("degraded")>{counts.degraded}</Badge>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-muted-foreground">"Crashed"</span>
                        <Badge variant=health_badge_variant("crashed")>{counts.crashed}</Badge>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-muted-foreground">"Unknown"</span>
                        <Badge variant=health_badge_variant("unknown")>{counts.unknown}</Badge>
                    </div>
                </div>
                {show_updated.then(|| {
                    view! {
                        <p class="mt-2 text-2xs text-muted-foreground">
                            {"Updated: "}{updated}
                        </p>
                    }
                })}
            </Card>
        }
    });

    view! {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-5">
            <Card title="Total Workers".to_string()>
                <div class="text-2xl font-bold">{total}</div>
                <p class="text-xs text-muted-foreground">"Registered workers"</p>
            </Card>

            <Card title="Healthy".to_string()>
                <div class="flex items-center gap-2">
                    <StatusIndicator color=StatusColor::Green/>
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

            {health_card}
        </div>
    }
}

// ============================================================================
// Workers List
// ============================================================================

#[derive(Clone, Copy)]
struct WorkerLifecycleActions {
    can_drain: bool,
    can_stop: bool,
    can_restart: bool,
    can_remove: bool,
}

fn worker_lifecycle_actions(status: &str) -> WorkerLifecycleActions {
    let can_drain =
        status.eq_ignore_ascii_case("healthy") || status.eq_ignore_ascii_case("serving");
    let can_stop = can_drain || status.eq_ignore_ascii_case("draining");
    let is_terminal = is_terminal_worker_status(status);

    WorkerLifecycleActions {
        can_drain,
        can_stop,
        can_restart: can_stop,
        can_remove: is_terminal,
    }
}

fn worker_no_action_reason(status: &str) -> String {
    if status.eq_ignore_ascii_case("pending")
        || status.eq_ignore_ascii_case("created")
        || status.eq_ignore_ascii_case("registered")
    {
        "Worker is still starting. Lifecycle actions appear when it is healthy.".to_string()
    } else {
        format!(
            "No lifecycle actions available while status is '{}'.",
            status
        )
    }
}

fn restart_remove_reason(status: &str) -> Option<String> {
    if is_terminal_worker_status(status) {
        return None;
    }

    if status.eq_ignore_ascii_case("pending")
        || status.eq_ignore_ascii_case("created")
        || status.eq_ignore_ascii_case("registered")
    {
        return Some(worker_no_action_reason(status));
    }

    if status.eq_ignore_ascii_case("draining") {
        return Some(
            "Worker is draining. Restart is available now; Remove becomes available after stopped/error."
                .to_string(),
        );
    }

    if status.eq_ignore_ascii_case("healthy") || status.eq_ignore_ascii_case("serving") {
        return Some(
            "Restart is available now. Remove becomes available after stopped/error.".to_string(),
        );
    }

    Some("Remove is available once status is stopped/error/crashed/failed.".to_string())
}

const WORKER_STATUS_GLOSSARY: &str = "healthy=ready and accepting requests; draining=rejecting new requests while finishing in-flight; stopped=clean shutdown complete; error=terminal failure; crashed/failed=legacy terminal failure labels.";
const DRAIN_STOP_GUIDANCE: &str =
    "Decision: choose Drain for graceful maintenance; choose Stop only for urgent termination.";

#[component]
pub fn WorkersList(
    workers: Vec<WorkerResponse>,
    selected_worker: RwSignal<Option<String>>,
    health_map: std::collections::HashMap<String, WorkerHealthRecord>,
    on_drain: Callback<String>,
    on_stop: Callback<String>,
    on_restart: Callback<String>,
    on_remove: Callback<String>,
    on_spawn: Callback<()>,
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
                    <div data-testid="workers-empty-state">
                        <EmptyState
                            variant=EmptyStateVariant::Empty
                            title="No workers yet"
                            description="Workers serve inference requests. Next: choose Spawn Worker and wait for status to become healthy."
                            action_label="Spawn Worker"
                            on_action=on_spawn
                        />
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="mb-3 space-y-1">
                        <p class="text-xs text-muted-foreground">
                            {"Status guide: "}{WORKER_STATUS_GLOSSARY}
                        </p>
                        <p class="text-xs text-muted-foreground">{DRAIN_STOP_GUIDANCE}</p>
                    </div>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"ID"</TableHead>
                                <TableHead>"Health"</TableHead>
                                <TableHead>"Backend"</TableHead>
                                <TableHead>"Model"</TableHead>
                                <TableHead>"Cache"</TableHead>
                                <TableHead>"Errors"</TableHead>
                                <TableHead>"Last Seen"</TableHead>
                                <TableHead class="text-right min-w-[260px]">"Actions"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {move || {
                                let count = visible_count.get();
                                let health_map = health_map.clone();
                                workers.iter().take(count).map(|worker| {
                                    let worker_id = worker.id.clone();
                                    let worker_id_drain = worker.id.clone();
                                    let worker_id_stop = worker.id.clone();
                                    let worker_id_restart = worker.id.clone();
                                    let worker_id_remove = worker.id.clone();
                                    let lifecycle_actions = worker_lifecycle_actions(&worker.status);
                                    let action_reason = if lifecycle_actions.can_drain
                                        || lifecycle_actions.can_stop
                                        || lifecycle_actions.can_restart
                                        || lifecycle_actions.can_remove
                                    {
                                        None
                                    } else {
                                        Some(worker_no_action_reason(&worker.status))
                                    };
                                    let health = health_map.get(&worker.id).cloned();

                                    view! {
                                        <WorkerRow
                                            worker=worker.clone()
                                            health=health
                                            on_select=Callback::new(move |_| {
                                                selected_worker.set(Some(worker_id.clone()));
                                            })
                                            on_drain=Callback::new(move |_| {
                                                on_drain.run(worker_id_drain.clone());
                                            })
                                            on_stop=Callback::new(move |_| {
                                                on_stop.run(worker_id_stop.clone());
                                            })
                                            on_restart=Callback::new(move |_| {
                                                on_restart.run(worker_id_restart.clone());
                                            })
                                            on_remove=Callback::new(move |_| {
                                                on_remove.run(worker_id_remove.clone());
                                            })
                                            show_drain=lifecycle_actions.can_drain
                                            show_stop=lifecycle_actions.can_stop
                                            show_restart=lifecycle_actions.can_restart
                                            show_remove=lifecycle_actions.can_remove
                                            action_reason=action_reason
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
    health: Option<WorkerHealthRecord>,
    on_select: Callback<()>,
    on_drain: Callback<()>,
    on_stop: Callback<()>,
    on_restart: Callback<()>,
    on_remove: Callback<()>,
    show_drain: bool,
    show_stop: bool,
    show_restart: bool,
    show_remove: bool,
    action_reason: Option<String>,
) -> impl IntoView {
    let health_status = health
        .as_ref()
        .map(|h| h.health_status.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let health_variant = health_badge_variant(health_status.as_str());
    let errors_24h = health.as_ref().map(|h| h.recent_incidents_24h).unwrap_or(0);

    let short_worker_id = worker_display_name(&worker.id, worker.display_name.as_deref());
    let short_tenant_id = adapteros_id::short_id(&worker.tenant_id);

    let backend = worker
        .backend
        .clone()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());
    let capabilities = worker.capabilities.clone();
    let has_capabilities = !capabilities.is_empty();
    let visible_caps: Vec<String> = capabilities.iter().take(3).cloned().collect();
    let extra_caps = capabilities.len().saturating_sub(visible_caps.len());
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
    let has_actions = show_drain || show_stop || show_restart || show_remove;
    let action_reason = action_reason
        .unwrap_or_else(|| "No lifecycle actions available for this status.".to_string());

    view! {
        <tr
            class="table-row cursor-pointer hover:bg-muted/50"
            on:click=move |_| on_select.run(())
            on:keydown=move |e: web_sys::KeyboardEvent| {
                if e.key() == "Enter" || e.key() == " " {
                    e.prevent_default();
                    on_select.run(());
                }
            }
            role="button"
            tabindex=0
        >
            <TableCell>
                <a
                    href=format!("/workers/{}", worker.id)
                    class="font-mono text-sm text-primary hover:underline"
                    title=worker.id.clone()
                    data-testid="workers-seeded-link"
                    on:click=move |e: web_sys::MouseEvent| e.stop_propagation()
                >
                    {short_worker_id.clone()}
                </a>
                <div class="text-xs text-muted-foreground font-mono mt-1" title=worker.tenant_id.clone()>
                    {"tenant: "}{short_tenant_id}
                </div>
            </TableCell>
            <TableCell>
                <div class="flex flex-col gap-1">
                    <Badge variant=health_variant>
                        {health_status.clone()}
                    </Badge>
                    <span class="text-xs text-muted-foreground">{worker.status.clone()}</span>
                </div>
            </TableCell>
            <TableCell>
                <div class="space-y-1">
                    <span class="text-sm">{backend}</span>
                    {has_capabilities.then(move || {
                        let visible_caps = visible_caps.clone();
                        view! {
                            <div class="flex flex-wrap gap-1">
                                {visible_caps.into_iter().map(|cap| view! {
                                    <Badge variant=BadgeVariant::Secondary>
                                        {cap}
                                    </Badge>
                                }).collect::<Vec<_>>()}
                                {if extra_caps > 0 {
                                    Some(view! {
                                        <Badge variant=BadgeVariant::Outline>
                                            {format!("+{}", extra_caps)}
                                        </Badge>
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        }
                    })}
                    {(!has_capabilities).then(|| view! {
                        <span class="text-xs text-muted-foreground">"No capabilities"</span>
                    })}
                </div>
            </TableCell>
            <TableCell>
                <span class="text-sm font-mono" title=model>{short_model}</span>
            </TableCell>
            <TableCell>
                <span class="text-sm">{cache_display}</span>
            </TableCell>
            <TableCell>
                {if errors_24h > 0 {
                    view! {
                        <Badge variant=BadgeVariant::Destructive>
                            {format!("{} in 24h", errors_24h)}
                        </Badge>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-sm text-muted-foreground">"-"</span>
                    }.into_any()
                }}
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{format_timestamp(&last_seen)}</span>
            </TableCell>
            <TableCell class="text-right">
                <div
                    class="flex items-center justify-end gap-1"
                    on:click=move |e: web_sys::MouseEvent| e.stop_propagation()
                >
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
                    {show_restart.then(|| view! {
                        <Button
                            variant=ButtonVariant::Secondary
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| on_restart.run(()))
                        >
                            <IconRefresh/>
                            "Restart"
                        </Button>
                    })}
                    {show_remove.then(|| view! {
                        <Button
                            variant=ButtonVariant::Destructive
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| on_remove.run(()))
                        >
                            <IconTrash/>
                            "Remove"
                        </Button>
                    })}
                    {(!has_actions).then(|| view! {
                        <span class="text-xs text-muted-foreground">{action_reason.clone()}</span>
                    })}
                </div>
            </TableCell>
        </tr>
    }
}

// ============================================================================
// Worker Detail Panel (slide-out)
// ============================================================================

#[component]
pub fn WorkerDetailPanel(
    worker: WorkerResponse,
    health: Option<WorkerHealthRecord>,
    on_close: Callback<()>,
) -> impl IntoView {
    let navigate = use_navigate();
    let worker_id = worker.id.clone();
    let health_status = health
        .as_ref()
        .map(|h| h.health_status.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let recent_incidents = health
        .as_ref()
        .map(|h| h.recent_incidents_24h.to_string())
        .unwrap_or_else(|| "-".to_string());

    // Fetch metrics for this worker
    let (metrics, _refetch_metrics) = use_api_resource({
        let worker_id = worker.id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id.clone();
            async move { client.get_worker_metrics(&id).await }
        }
    });

    // Fetch recent error logs for this worker
    let (error_logs, _refetch_error_logs) = use_api_resource({
        let worker_id = worker.id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id.clone();
            async move { client.get_worker_logs(&id, Some("error"), Some(5)).await }
        }
    });

    view! {
        <Card title="Worker Details".to_string()>
            <div class="space-y-6">
                // Header
                <div class="flex items-center justify-between" data-testid="worker-panel-header">
                    <div class="flex items-center gap-3">
                        <span class="font-mono text-lg">{worker_display_name(&worker.id, worker.display_name.as_deref())}</span>
                        <Badge variant=status_badge_variant(&worker.status)>
                            {worker.status.clone()}
                        </Badge>
                    </div>
                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Secondary
                            size=ButtonSize::Sm
                            data_testid="worker-panel-view-full".to_string()
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
                            data_testid="worker-panel-close".to_string()
                            on_click=Callback::new(move |_| on_close.run(()))
                        >
                            <IconX/>
                        </Button>
                    </div>
                </div>

                // Info grid
                <div class="grid grid-cols-2 gap-4" data-testid="worker-panel-info-grid">
                    <DetailItem label="Worker ID" value=worker.id.clone()/>
                    <DetailItem label="Node ID" value=worker.node_id.clone()/>
                    <DetailItem label="Tenant ID" value=worker.tenant_id.clone()/>
                    <DetailItem label="Plan ID" value=worker.plan_id.clone()/>
                    <DetailItem label="Health" value=health_status/>
                    <DetailItem label="Incidents (24h)" value=recent_incidents/>
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
                            <div data-testid="worker-panel-capabilities">
                                <h4 class="text-sm font-medium mb-2">"Capabilities"</h4>
                                <div class="flex flex-wrap gap-2">
                                    {cap_views}
                                </div>
                            </div>
                        }
                    })
                }

                // Cache info
                <div data-testid="worker-panel-cache-card">
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
                            <div data-testid="worker-panel-metrics-loaded">
                                <WorkerMetricsPanel metrics=m/>
                            </div>
                        }.into_any(),
                        LoadingState::Loading => view! {
                            <div
                                class="flex items-center gap-2 text-muted-foreground"
                                data-testid="worker-panel-metrics-loading"
                            >
                                <Spinner/>
                                <span>"Loading metrics..."</span>
                            </div>
                        }.into_any(),
                        _ => view! {}.into_any(),
                    }
                }}

                // Recent errors
                <div data-testid="worker-panel-errors-card">
                    <h4 class="text-sm font-medium mb-2">"Recent Errors"</h4>
                    {match error_logs.get() {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <div
                                class="flex items-center gap-2 text-muted-foreground"
                                data-testid="worker-panel-errors-loading"
                            >
                                <Spinner/>
                                <span>"Loading errors..."</span>
                            </div>
                        }.into_any(),
                        LoadingState::Loaded(logs) => {
                            if logs.is_empty() {
                                view! {
                                    <p
                                        class="text-sm text-muted-foreground"
                                        data-testid="worker-panel-errors-empty"
                                    >
                                        "No recent errors"
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-2">
                                        {logs.into_iter().map(|log| view! {
                                            <div class="rounded-md border p-2">
                                                <p class="text-xs text-muted-foreground">{format_timestamp(&log.timestamp)}</p>
                                                <p class="text-sm">{log.message}</p>
                                            </div>
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            if matches!(&e, ApiError::Forbidden(_)) {
                                view! {
                                    <p
                                        class="text-sm text-muted-foreground"
                                        data-testid="worker-panel-errors-error"
                                    >
                                        "Recent errors require Operator or Admin permissions."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <p
                                        class="text-sm text-destructive"
                                        data-testid="worker-panel-errors-error"
                                    >
                                        {format!("Failed to load errors: {}", e)}
                                    </p>
                                }.into_any()
                            }
                        }
                    }}
                </div>
            </div>
        </Card>
    }
}

#[component]
pub fn DetailItem(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div>
            <p class="text-xs text-muted-foreground">{label}</p>
            {if label.contains("ID") {
                view! { <CopyableId id=value.clone() truncate=24 /> }.into_any()
            } else {
                view! { <p class="text-sm font-mono truncate" title=value.clone()>{value.clone()}</p> }.into_any()
            }}
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
    let alive = use_scope_alive();
    let navigate = leptos_router::hooks::use_navigate();
    let notifications = use_notifications();
    let api_client = use_api();
    let action_loading = RwSignal::new(false);
    let action_error = RwSignal::new(Option::<String>::None);
    let show_stop_confirm = RwSignal::new(false);
    let show_drain_confirm = RwSignal::new(false);
    let show_restart_confirm = RwSignal::new(false);
    let show_remove_confirm = RwSignal::new(false);
    let lifecycle_actions = worker_lifecycle_actions(&worker.status);
    let restart_remove_hint = restart_remove_reason(&worker.status);
    let worker_id_for_health = worker.id.clone();

    // Fetch worker health summary for health status + incidents
    let (health_summary, refetch_health) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .get::<WorkerHealthSummary>("/v1/workers/health/summary")
            .await
    });

    // Fetch recent error logs for this worker
    let (error_logs, refetch_error_logs) = use_api_resource({
        let worker_id = worker.id.clone();
        move |client: Arc<ApiClient>| {
            let id = worker_id.clone();
            async move { client.get_worker_logs(&id, Some("error"), Some(10)).await }
        }
    });

    view! {
        <div class="space-y-6">
            // Header
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <h2 class="heading-2 font-mono" data-testid="worker-detail-heading">
                        {worker_display_name(&worker.id, worker.display_name.as_deref())}
                    </h2>
                    <span data-testid="worker-detail-status-badge">
                        <Badge variant=status_badge_variant(&worker.status)>
                            {worker.status.clone()}
                        </Badge>
                    </span>
                </div>
                <div class="flex flex-col items-end gap-2">
                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Secondary
                            data_testid="worker-detail-refresh".to_string()
                            on_click=Callback::new(move |_| {
                                on_refresh.run(());
                                refetch_health.run(());
                                refetch_error_logs.run(());
                            })
                        >
                            <IconRefresh/>
                            "Refresh"
                        </Button>
                        {lifecycle_actions.can_drain.then(|| {
                            view! {
                                <Button
                                    variant=ButtonVariant::Secondary
                                    disabled=Signal::from(action_loading)
                                    data_testid="worker-detail-drain".to_string()
                                    on_click=Callback::new(move |_| {
                                        show_drain_confirm.set(true);
                                    })
                                >
                                    <IconPause/>
                                    "Drain"
                                </Button>
                            }
                        })}
                        {lifecycle_actions.can_stop.then(|| {
                            view! {
                                <Button
                                    variant=ButtonVariant::Destructive
                                    disabled=Signal::from(action_loading)
                                    data_testid="worker-detail-stop".to_string()
                                    on_click=Callback::new(move |_| {
                                        show_stop_confirm.set(true);
                                    })
                                >
                                    <IconStop/>
                                    "Stop"
                                </Button>
                            }
                        })}
                        {lifecycle_actions.can_restart.then(|| {
                            view! {
                                <Button
                                    variant=ButtonVariant::Secondary
                                    disabled=Signal::from(action_loading)
                                    data_testid="worker-detail-restart".to_string()
                                    on_click=Callback::new(move |_| {
                                        show_restart_confirm.set(true);
                                    })
                                >
                                    <IconRefresh/>
                                    "Restart"
                                </Button>
                            }
                        })}
                        {lifecycle_actions.can_remove.then(|| {
                            view! {
                                <Button
                                    variant=ButtonVariant::Destructive
                                    disabled=Signal::from(action_loading)
                                    data_testid="worker-detail-remove".to_string()
                                    on_click=Callback::new(move |_| {
                                        show_remove_confirm.set(true);
                                    })
                                >
                                    <IconTrash/>
                                    "Remove"
                                </Button>
                            }
                        })}
                    </div>
                    {(lifecycle_actions.can_drain || lifecycle_actions.can_stop).then(|| view! {
                        <p class="text-xs text-muted-foreground text-right max-w-[420px]">
                            {DRAIN_STOP_GUIDANCE}
                        </p>
                    })}
                    {restart_remove_hint.clone().map(|reason| view! {
                        <p class="text-xs text-muted-foreground text-right max-w-[420px]">{reason}</p>
                    })}
                </div>
            </div>

            // Drain confirmation dialog
            {
                let worker_id = worker.id.clone();
                let drain_desc = format!(
                    "Drain worker '{}'? Use drain for graceful maintenance: reject new requests while active requests complete.",
                    worker_display_name(&worker.id, worker.display_name.as_deref()),
                );
                view! {
                    <div data-testid="worker-detail-confirm-drain">
                        <ConfirmationDialog
                            open=show_drain_confirm
                            title="Drain Worker"
                            description=drain_desc
                            severity=ConfirmationSeverity::Warning
                            confirm_text="Drain"
                            on_confirm=Callback::new({
                                let alive = alive.clone();
                                let api_client = api_client.clone();
                                let notifications = notifications.clone();
                                move |_| {
                                    show_drain_confirm.set(false);
                                    action_loading.set(true);
                                    let client = api_client.clone();
                                    let worker_id = worker_id.clone();
                                    let alive = alive.clone();
                                    let notifications = notifications.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.drain_worker(&worker_id).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                notifications.success("Worker draining", "Worker is draining and will stop accepting new requests.");
                                                if alive.load(std::sync::atomic::Ordering::SeqCst) {
                                                    on_refresh.run(());
                                                }
                                            }
                                            Err(e) => {
                                                action_error.set(Some(e.user_message()));
                                                report_error_with_toast(&e, "Failed to drain worker", Some("/workers"), true);
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            })
                            on_cancel=Callback::new(move |_| {
                                show_drain_confirm.set(false);
                            })
                            loading=Signal::from(action_loading)
                        />
                    </div>
                }
            }

            // Stop confirmation dialog
            {
                let worker_id = worker.id.clone();
                let stop_desc = format!(
                    "Stop worker '{}'? Use stop for urgent shutdown: active inference requests terminate immediately.",
                    worker_display_name(&worker.id, worker.display_name.as_deref()),
                );
                view! {
                    <div data-testid="worker-detail-confirm-stop">
                        <ConfirmationDialog
                            open=show_stop_confirm
                            title="Stop Worker"
                            description=stop_desc
                            severity=ConfirmationSeverity::Warning
                            confirm_text="Stop"
                            on_confirm=Callback::new({
                                let api_client = api_client.clone();
                                let notifications = notifications.clone();
                                let navigate = navigate.clone();
                                move |_| {
                                    show_stop_confirm.set(false);
                                    action_loading.set(true);
                                    let client = api_client.clone();
                                    let worker_id = worker_id.clone();
                                    let notifications = notifications.clone();
                                    let navigate = navigate.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.stop_worker(&worker_id).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                let short = &worker_id[..8.min(worker_id.len())];
                                                notifications.success("Worker stopped", &format!("Worker {} has been stopped.", short));
                                                navigate("/workers", Default::default());
                                            }
                                            Err(e) => {
                                                action_error.set(Some(e.user_message()));
                                                report_error_with_toast(&e, "Failed to stop worker", Some("/workers"), true);
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            })
                            on_cancel=Callback::new(move |_| {
                                show_stop_confirm.set(false);
                            })
                            loading=Signal::from(action_loading)
                        />
                    </div>
                }
            }

            // Restart confirmation dialog
            {
                let worker_id = worker.id.clone();
                let restart_desc = format!(
                    "Restart worker '{}'? The process will be relaunched and in-flight requests may fail.",
                    worker_display_name(&worker.id, worker.display_name.as_deref()),
                );
                view! {
                    <div data-testid="worker-detail-confirm-restart">
                        <ConfirmationDialog
                            open=show_restart_confirm
                            title="Restart Worker"
                            description=restart_desc
                            severity=ConfirmationSeverity::Warning
                            confirm_text="Restart"
                            on_confirm=Callback::new({
                                let alive = alive.clone();
                                let api_client = api_client.clone();
                                let notifications = notifications.clone();
                                move |_| {
                                    show_restart_confirm.set(false);
                                    action_loading.set(true);
                                    let client = api_client.clone();
                                    let worker_id = worker_id.clone();
                                    let alive = alive.clone();
                                    let notifications = notifications.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.restart_worker(&worker_id).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                notifications.success(
                                                    "Worker restart requested",
                                                    "Worker restart has been initiated.",
                                                );
                                                if alive.load(std::sync::atomic::Ordering::SeqCst) {
                                                    on_refresh.run(());
                                                }
                                            }
                                            Err(e) => {
                                                action_error.set(Some(e.user_message()));
                                                report_error_with_toast(
                                                    &e,
                                                    "Failed to restart worker",
                                                    Some("/workers"),
                                                    true,
                                                );
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            })
                            on_cancel=Callback::new(move |_| {
                                show_restart_confirm.set(false);
                            })
                            loading=Signal::from(action_loading)
                        />
                    </div>
                }
            }

            // Remove confirmation dialog
            {
                let worker_id = worker.id.clone();
                let remove_desc = format!(
                    "Remove worker '{}'? This decommissions the worker record and cannot be undone.",
                    worker_display_name(&worker.id, worker.display_name.as_deref()),
                );
                view! {
                    <div data-testid="worker-detail-confirm-remove">
                        <ConfirmationDialog
                            open=show_remove_confirm
                            title="Remove Worker"
                            description=remove_desc
                            severity=ConfirmationSeverity::Destructive
                            confirm_text="Remove"
                            on_confirm=Callback::new({
                                let api_client = api_client.clone();
                                let notifications = notifications.clone();
                                let navigate = navigate.clone();
                                move |_| {
                                    show_remove_confirm.set(false);
                                    action_loading.set(true);
                                    let client = api_client.clone();
                                    let worker_id = worker_id.clone();
                                    let notifications = notifications.clone();
                                    let navigate = navigate.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.remove_worker(&worker_id).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                notifications.success(
                                                    "Worker removed",
                                                    "Worker has been decommissioned and removed.",
                                                );
                                                navigate("/workers", Default::default());
                                            }
                                            Err(e) => {
                                                action_error.set(Some(e.user_message()));
                                                report_error_with_toast(
                                                    &e,
                                                    "Failed to remove worker",
                                                    Some("/workers"),
                                                    true,
                                                );
                                            }
                                        }
                                        action_loading.set(false);
                                    });
                                }
                            })
                            on_cancel=Callback::new(move |_| {
                                show_remove_confirm.set(false);
                            })
                            loading=Signal::from(action_loading)
                        />
                    </div>
                }
            }

            // Error banner
            {move || action_error.get().map(|e| view! {
                <div
                    class="rounded-lg border border-destructive bg-destructive/10 p-4"
                    data-testid="worker-detail-action-error"
                >
                    <p class="text-destructive">{e}</p>
                </div>
            })}

            // Basic info card
            <div data-testid="worker-detail-info-card">
                <Card title="Worker Information".to_string()>
                    <div class="grid grid-cols-2 md:grid-cols-4 gap-6">
                        <DetailItem label="Worker ID" value=worker.id.clone()/>
                        <DetailItem label="Node ID" value=worker.node_id.clone()/>
                        <DetailItem label="Tenant ID" value=worker.tenant_id.clone()/>
                        <DetailItem label="Plan ID" value=worker.plan_id.clone()/>
                        {move || {
                            let (health_status, incidents) = match health_summary.get() {
                                LoadingState::Loaded(ref summary) => {
                                    let record = summary
                                        .workers
                                        .iter()
                                        .find(|r| r.worker_id == worker_id_for_health)
                                        .cloned();
                                    let status = record
                                        .as_ref()
                                        .map(|r| r.health_status.clone())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| "unknown".to_string());
                                    let incidents = record
                                        .as_ref()
                                        .map(|r| r.recent_incidents_24h.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    (status, incidents)
                                }
                                _ => ("unknown".to_string(), "-".to_string()),
                            };
                            view! {
                                <DetailItem label="Health" value=health_status/>
                                <DetailItem label="Incidents (24h)" value=incidents/>
                            }
                        }}
                        <DetailItem label="Backend" value=worker.backend.clone().filter(|b| !b.is_empty()).unwrap_or_else(|| "Unknown".to_string())/>
                        <DetailItem label="Model ID" value=worker.model_id.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "Not assigned".to_string())/>
                        <DetailItem label="Model Hash" value=worker.model_hash.clone().map(|h| adapteros_id::format_hash_short(&h)).unwrap_or("-".to_string())/>
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
                                <div class="mt-6 pt-6 border-t" data-testid="worker-detail-capabilities">
                                    <h4 class="text-sm font-medium mb-3">"Capabilities"</h4>
                                    <div class="flex flex-wrap gap-2">
                                        {cap_views}
                                    </div>
                                </div>
                            }
                        })
                    }
                </Card>
            </div>

            // Cache card
            <div data-testid="worker-detail-cache-card">
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
            </div>

            // Metrics card
            {metrics.map(|m| view! {
                <div data-testid="worker-detail-metrics-card">
                    <WorkerMetricsCard metrics=m/>
                </div>
            })}

            // Recent errors card
            <div data-testid="worker-detail-errors-card">
                <Card title="Recent Errors".to_string()>
                    {match error_logs.get() {
                        LoadingState::Idle | LoadingState::Loading => view! {
                            <div
                                class="flex items-center gap-2 text-muted-foreground"
                                data-testid="worker-detail-errors-loading"
                            >
                                <Spinner/>
                                <span>"Loading errors..."</span>
                            </div>
                        }.into_any(),
                        LoadingState::Loaded(logs) => {
                            if logs.is_empty() {
                                view! {
                                    <p class="text-sm text-muted-foreground" data-testid="worker-detail-errors-empty">
                                        "No recent errors"
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-3">
                                        {logs.into_iter().map(|log| view! {
                                            <div class="rounded-md border p-3">
                                                <p class="text-xs text-muted-foreground">{format_timestamp(&log.timestamp)}</p>
                                                <p class="text-sm">{log.message}</p>
                                            </div>
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            if matches!(&e, ApiError::Forbidden(_)) {
                                view! {
                                    <p
                                        class="text-sm text-muted-foreground"
                                        data-testid="worker-detail-errors-error"
                                    >
                                        "Recent errors require Operator or Admin permissions."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <p class="text-sm text-destructive" data-testid="worker-detail-errors-error">
                                        {format!("Failed to load errors: {}", e)}
                                    </p>
                                }.into_any()
                            }
                        }
                    }}
                </Card>
            </div>
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
            <p class="text-2xl font-bold">{value}</p>
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
