//! System page
//!
//! Comprehensive system overview with status, workers, nodes, health details,
//! metrics summary, and recent events.
//!
//! Uses SSE for real-time worker status updates via `/v1/stream/workers`.

mod components;
pub(crate) mod lifecycle;
pub(crate) mod services;
mod utils;

use crate::api::{use_sse_json_events, ApiClient, SseState};
use crate::components::{
    Button, ButtonVariant, ErrorDisplay, PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    SkeletonCard, SkeletonStatsGrid,
};
use crate::hooks::{
    use_api_resource, use_health_endpoints, use_polling, use_sse_notifications, use_system_status,
    LoadingState,
};
use crate::signals::{use_refetch_signal, RefetchTopic};
use adapteros_api_types::{workers::WorkerStatusUpdate, SystemStateResponse, WorkerResponse};
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::IconRefresh;
use components::{SseIndicator, SystemContent};

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
    let (status, refetch_status) = use_system_status();

    // Fetch workers list (initial load, then updated via SSE)
    let (workers, refetch_workers) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_workers().await });

    // Fetch nodes list
    let (nodes, refetch_nodes) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_nodes().await });

    // Fetch system metrics
    let (metrics, refetch_metrics) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_metrics().await });

    // Fetch models status (admin-only endpoint; handled gracefully on error)
    let (models_status, refetch_models_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.list_models_status().await });

    let (health_endpoints, refetch_health_endpoints) = use_health_endpoints();

    // Fetch system state (tenants, stacks, services)
    let (system_state, refetch_state) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .get::<SystemStateResponse>("/v1/system/state?include_adapters=false&top_adapters=10")
            .await
    });

    // Real-time worker status updates via SSE
    // Maps worker_id -> (status, timestamp) for incremental updates
    let worker_status_overrides: RwSignal<HashMap<String, (String, String)>> =
        RwSignal::new(HashMap::new());

    // Track when we last received a full worker list via SSE
    let last_sse_update = RwSignal::new(Option::<String>::None);

    // SSE connection for worker status stream
    // Use try_ variants to avoid panic when signals are disposed during navigation
    let (sse_status, _reconnect) = use_sse_json_events::<WorkerStreamEvent, _>(
        "/v1/stream/workers",
        &["workers"],
        move |event| {
            match event {
                WorkerStreamEvent::FullList { workers: _ } => {
                    // When we receive a full list, clear overrides and trigger refetch
                    // to get the complete worker data
                    let _ = worker_status_overrides.try_set(HashMap::new());
                    let _ = last_sse_update.try_set(Some(crate::utils::now_utc().to_rfc3339()));
                    refetch_workers.run(());
                }
                WorkerStreamEvent::StatusUpdate(update) => {
                    // Apply incremental status update
                    let _ = worker_status_overrides.try_update(|overrides| {
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

    // Bridge SSE connection state to user notifications
    use_sse_notifications(sse_status.read_only());

    // SSE-driven refresh from global lifecycle streams wired in Shell.
    let health_refetch_counter = use_refetch_signal(RefetchTopic::Health);
    Effect::new(move || {
        let Some(counter) = health_refetch_counter.try_get() else {
            return;
        };
        if counter > 0 {
            refetch_status.run(());
            refetch_workers.run(());
            refetch_nodes.run(());
            refetch_metrics.run(());
            refetch_state.run(());
            refetch_models_status.run(());
            refetch_health_endpoints.run(());
        }
    });

    // Debug logging for list sizes
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref w)) = workers.try_get() {
            crate::debug_log!("[list] system/workers: {} items", w.len());
        }
    });

    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref n)) = nodes.try_get() {
            crate::debug_log!("[list] system/nodes: {} items", n.len());
        }
    });

    // Set up polling interval for non-worker data (every 30 seconds)
    // Worker data is now primarily updated via SSE
    // Using use_polling hook which properly cleans up on unmount
    let _ = use_polling(30_000, move || async move {
        refetch_status.run(());
        refetch_nodes.run(());
        refetch_metrics.run(());
        refetch_state.run(());
        refetch_models_status.run(());
        refetch_health_endpoints.run(());
        // Worker polling is fallback-only while the worker stream is not connected.
        // Use try_get_untracked to avoid panic if signal is disposed during navigation.
        let sse_disposed_or_fallback = sse_status
            .try_get_untracked()
            .is_none_or(is_polling_fallback_active);
        if sse_disposed_or_fallback {
            refetch_workers.run(());
        }
    });

    view! {
        <PageScaffold
            title="System"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("System", "/system"),
                PageBreadcrumbItem::current("System"),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
                <SseIndicator state=sse_status/>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| {
                        refetch_status.run(());
                        refetch_workers.run(());
                        refetch_nodes.run(());
                        refetch_metrics.run(());
                        refetch_state.run(());
                        refetch_models_status.run(());
                        refetch_health_endpoints.run(());
                    })
                >
                    <IconRefresh/>
                    "Refresh"
                </Button>
            </PageScaffoldActions>

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
                                <div class="space-y-6">
                                    <SkeletonStatsGrid count=5 />
                                    <SkeletonCard has_header=true />
                                    <SkeletonCard has_header=true />
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
                            let state_data = system_state.get();
                            let models_status_data = models_status.get();
                            let health_endpoints_data = health_endpoints.get();
                            view! {
                                <SystemContent
                                    status=status_data
                                    workers=workers_data
                                    nodes=nodes_data
                                    metrics=metrics_data
                                    state=state_data
                                    models_status=models_status_data
                                    healthz=health_endpoints_data.healthz
                                    readyz=health_endpoints_data.readyz
                                    healthz_all=health_endpoints_data.healthz_all
                                    system_ready=health_endpoints_data.system_ready
                                    worker_status_overrides=overrides
                                />
                            }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| {
                                        refetch_status.run(());
                                        refetch_workers.run(());
                                        refetch_nodes.run(());
                                        refetch_metrics.run(());
                                    })
                                />
                            }.into_any()
                        }
                    }
                }}

        </PageScaffold>
    }
}

fn is_polling_fallback_active(state: SseState) -> bool {
    matches!(
        state,
        SseState::Disconnected | SseState::Connecting | SseState::Error | SseState::CircuitOpen
    )
}
