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

use crate::api::{
    use_sse_json_events, ApiClient, ReadyzResponse, SseState, SystemHealthResponse,
    SystemReadyResponse,
};
use crate::components::{
    Button, ButtonVariant, ErrorDisplay, PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    Spinner,
};
use crate::hooks::{use_api_resource, use_polling, use_sse_notifications, LoadingState};
use adapteros_api_types::{
    workers::WorkerStatusUpdate, HealthResponse, SystemStateResponse, WorkerResponse,
};
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
    let (status, refetch_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

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

    // Fetch health endpoints (return status even on non-2xx)
    let (healthz, refetch_healthz) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get_with_status::<HealthResponse>("/healthz").await
    });
    let (readyz, refetch_readyz) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get_with_status::<ReadyzResponse>("/readyz").await
    });
    let (healthz_all, refetch_healthz_all) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client.get::<SystemHealthResponse>("/healthz/all").await
        });
    let (system_ready, refetch_system_ready) =
        use_api_resource(|client: Arc<ApiClient>| async move {
            client
                .get_with_status::<SystemReadyResponse>("/system/ready")
                .await
        });

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
    let (sse_status, _reconnect) = use_sse_json_events::<WorkerStreamEvent, _>(
        "/v1/stream/workers",
        &["workers"],
        move |event| {
            match event {
                WorkerStreamEvent::FullList { workers: _ } => {
                    // When we receive a full list, clear overrides and trigger refetch
                    // to get the complete worker data
                    worker_status_overrides.set(HashMap::new());
                    last_sse_update.set(Some(crate::utils::now_utc().to_rfc3339()));
                    refetch_workers.run(());
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

    // Bridge SSE connection state to user notifications
    use_sse_notifications(sse_status.read_only());

    // Fallback: if SSE is disconnected for 30s, trigger a refetch to keep data warm
    Effect::new(move || {
        if sse_status.try_get() == Some(SseState::Disconnected) {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::callback::Timeout::new(30_000, move || {
                    refetch_workers.run(());
                })
                .forget();
            }
        }
    });

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref w)) = workers.try_get() {
            web_sys::console::log_1(&format!("[list] system/workers: {} items", w.len()).into());
        }
    });

    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let Some(LoadingState::Loaded(ref n)) = nodes.try_get() {
            web_sys::console::log_1(&format!("[list] system/nodes: {} items", n.len()).into());
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
        refetch_healthz.run(());
        refetch_readyz.run(());
        refetch_healthz_all.run(());
        refetch_system_ready.run(());
        // Only refetch workers if SSE is not connected
        // Use get_untracked since we're in an async context outside reactive tracking
        if sse_status.get_untracked() != SseState::Connected {
            refetch_workers.run(());
        }
    });

    view! {
        <PageScaffold
            title="Infrastructure"
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Org"),
                PageBreadcrumbItem::current("Infrastructure"),
            ]
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
                        refetch_healthz.run(());
                        refetch_readyz.run(());
                        refetch_healthz_all.run(());
                        refetch_system_ready.run(());
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
                            let state_data = system_state.get();
                            let models_status_data = models_status.get();
                            let healthz_data = healthz.get();
                            let readyz_data = readyz.get();
                            let healthz_all_data = healthz_all.get();
                            let system_ready_data = system_ready.get();
                            view! {
                                <SystemContent
                                    status=status_data
                                    workers=workers_data
                                    nodes=nodes_data
                                    metrics=metrics_data
                                    state=state_data
                                    models_status=models_status_data
                                    healthz=healthz_data
                                    readyz=readyz_data
                                    healthz_all=healthz_all_data
                                    system_ready=system_ready_data
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
