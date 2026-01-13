//! System page
//!
//! Comprehensive system overview with status, workers, nodes, health details,
//! metrics summary, and recent events.
//!
//! Uses SSE for real-time worker status updates via `/v1/stream/workers`.

mod components;
mod icons;
mod utils;

use crate::api::{use_sse_json_events, ApiClient, SseState};
use crate::components::{ErrorDisplay, Spinner};
use crate::hooks::{use_api_resource, use_polling, use_sse_notifications, LoadingState};
use crate::pages::workers::dialogs::{PlanOption, SpawnWorkerDialog};
use adapteros_api_types::{workers::WorkerStatusUpdate, SpawnWorkerRequest, WorkerResponse};
use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use components::{SseIndicator, SystemContent};
use icons::{PlusIcon, RefreshIcon};

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

    // Fetch plans for spawn form
    let (plans, _refetch_plans) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.get::<Vec<PlanOption>>("/v1/plans").await
    });

    // Spawn dialog state
    let show_spawn_dialog = RwSignal::new(false);
    let spawn_error = RwSignal::new(Option::<String>::None);

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
    let (sse_status, _reconnect) = use_sse_json_events::<WorkerStreamEvent, _>(
        "/v1/stream/workers",
        &["workers"],
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

    // Bridge SSE connection state to user notifications
    use_sse_notifications(sse_status.read_only());

    // Fallback: if SSE is disconnected for 30s, trigger a refetch to keep data warm
    Effect::new(move || {
        if sse_status.get() == SseState::Disconnected {
            #[cfg(target_arch = "wasm32")]
            {
                let refetch = refetch_workers_signal.clone();
                gloo_timers::callback::Timeout::new(30_000, move || {
                    refetch.with_value(|f| f());
                })
                .forget();
            }
        }
    });

    // Debug logging for list sizes
    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let LoadingState::Loaded(ref w) = workers.get() {
            web_sys::console::log_1(&format!("[list] system/workers: {} items", w.len()).into());
        }
    });

    #[cfg(debug_assertions)]
    Effect::new(move |_| {
        if let LoadingState::Loaded(ref n) = nodes.get() {
            web_sys::console::log_1(&format!("[list] system/nodes: {} items", n.len()).into());
        }
    });

    // Set up polling interval for non-worker data (every 30 seconds)
    // Worker data is now primarily updated via SSE
    // Using use_polling hook which properly cleans up on unmount
    let _ = use_polling(30_000, move || async move {
        refetch_status_signal.with_value(|f| f());
        refetch_nodes_signal.with_value(|f| f());
        refetch_metrics_signal.with_value(|f| f());
        // Only refetch workers if SSE is not connected
        // Use get_untracked since we're in an async context outside reactive tracking
        if sse_status.get_untracked() != SseState::Connected {
            refetch_workers_signal.with_value(|f| f());
        }
    });

    view! {
        <div class="p-6 space-y-6">
            // Header with title and action buttons
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-4">
                        <h1 class="text-3xl font-bold tracking-tight">"System"</h1>
                        <SseIndicator state=sse_status/>
                    </div>
                    <div class="flex items-center gap-2">
                        <button
                            class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
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
                        <button
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                            on:click=move |_| show_spawn_dialog.set(true)
                        >
                            <PlusIcon/>
                            "Spawn Worker"
                        </button>
                    </div>
                </div>

                // Error banner for spawn errors
                {move || spawn_error.get().map(|e| view! {
                    <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                        <div class="flex items-center justify-between">
                            <p class="text-destructive font-medium">{e}</p>
                            <button
                                class="text-destructive hover:text-destructive/80"
                                on:click=move |_| spawn_error.set(None)
                            >
                                "×"
                            </button>
                        </div>
                    </div>
                })}

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
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| {
                                        refetch_status_signal.with_value(|f| f());
                                        refetch_workers_signal.with_value(|f| f());
                                        refetch_nodes_signal.with_value(|f| f());
                                        refetch_metrics_signal.with_value(|f| f());
                                    })
                                />
                            }.into_any()
                        }
                    }
                }}

                // Spawn worker dialog
                {move || {
                    let nodes_list = match nodes.get() {
                        LoadingState::Loaded(n) => n,
                        _ => Vec::new(),
                    };
                    let plans_list = match plans.get() {
                        LoadingState::Loaded(p) => p,
                        _ => Vec::new(),
                    };
                    view! {
                        <SpawnWorkerDialog
                            open=show_spawn_dialog
                            nodes=nodes_list
                            plans=plans_list
                            on_spawn=Callback::new({
                                let refetch = refetch_workers_signal;
                                move |request: SpawnWorkerRequest| {
                                    show_spawn_dialog.set(false);
                                    let client = ApiClient::new();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match client.spawn_worker(&request).await {
                                            Ok(_) => {
                                                spawn_error.set(None);
                                                refetch.with_value(|f| f());
                                            }
                                            Err(e) => {
                                                spawn_error.set(Some(format!("Failed to spawn worker: {}", e)));
                                            }
                                        }
                                    });
                                }
                            })
                        />
                    }
                }}
        </div>
    }
}
