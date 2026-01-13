//! Training job detail panel
//!
//! Components for displaying training job details and metrics.

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity, ErrorDisplay, Spinner,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use std::sync::Arc;

use super::components::{CoremlBadges, JobStatusBadge, ProgressBar};
use super::state::CoremlState;
use super::utils::{format_backend_or, format_date, format_duration, format_number};

/// Training job detail panel
#[component]
pub fn TrainingJobDetail(
    job_id: String,
    on_close: impl Fn() + Copy + 'static,
    on_cancelled: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let job_id_for_fetch = job_id.clone();

    // Fetch job details
    let (job, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = job_id_for_fetch.clone();
        async move { client.get_training_job(&id).await }
    });

    // Store refetch in a signal for sharing
    let refetch_signal = StoredValue::new(refetch);

    // Poll for updates on running jobs
    // Return value (stop fn) intentionally ignored - polling runs until unmount
    let _ = use_polling(3000, move || async move {
        refetch_signal.with_value(|f| f());
    });

    // Cancel job handler
    let job_id_for_cancel = job_id.clone();
    let cancelling = RwSignal::new(false);
    let show_cancel_confirm = RwSignal::new(false);

    // Handle cancel dialog dismiss
    let on_cancel_dismiss = Callback::new(move |_| {
        // Reset cancelling state if user dismisses dialog during loading
        cancelling.set(false);
    });

    // Create the cancel callback outside the reactive closure
    let cancel_callback = Callback::new(move |_| {
        let job_id = job_id_for_cancel.clone();
        cancelling.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.cancel_training_job(&job_id).await {
                Ok(_) => {
                    show_cancel_confirm.set(false);
                    on_cancelled();
                }
                Err(_) => {
                    // On error, close dialog - user can retry via UI
                    show_cancel_confirm.set(false);
                }
            }
            cancelling.set(false);
        });
    });

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="text-xl font-semibold">"Job Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                    aria-label="Close"
                    type="button"
                    on:click=move |_| on_close()
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            {move || {
                match job.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <JobDetailContent
                                job=data
                                cancelling=cancelling
                                show_cancel_confirm=show_cancel_confirm
                                on_cancel=cancel_callback
                                on_cancel_dismiss=on_cancel_dismiss
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Job detail content
#[component]
pub fn JobDetailContent(
    job: TrainingJobResponse,
    cancelling: RwSignal<bool>,
    show_cancel_confirm: RwSignal<bool>,
    on_cancel: Callback<()>,
    on_cancel_dismiss: Callback<()>,
) -> impl IntoView {
    // Clone values before view! to avoid move issues
    let status = job.status.clone();
    let status_for_badge = job.status.clone();
    let status_for_progress = job.status.clone();
    let job_id = job.id.clone();
    let job_id_for_logs = job.id.clone();
    let coreml_state_for_warning = CoremlState::from_job(&job);
    let coreml_state_for_export = coreml_state_for_warning.clone();
    let coreml_state_for_badges = coreml_state_for_warning.clone();
    let coreml_export_requested = job.coreml_export_requested.unwrap_or(false);

    let is_running = status == "running";
    let is_pending = status == "pending";
    let is_completed = status == "completed";
    let can_cancel = is_running || is_pending;
    let show_progress = is_running || is_completed;

    view! {
        // Status and progress
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <JobStatusBadge status=status_for_badge/>
                    {can_cancel.then(|| {
                        let is_cancelling = Signal::derive(move || cancelling.get());
                        view! {
                            <Button
                                variant=ButtonVariant::Destructive
                                on_click=Callback::new(move |_| show_cancel_confirm.set(true))
                                loading=is_cancelling
                                disabled=is_cancelling
                            >
                                {move || if cancelling.get() { "Cancelling..." } else { "Cancel Job" }}
                            </Button>
                        }
                    })}
                </div>

                {show_progress.then(|| view! {
                    <div class="space-y-2">
                        <div class="flex justify-between text-sm">
                            <span>"Progress"</span>
                            <span class="font-medium">{format!("{:.1}%", job.progress_pct.unwrap_or(0.0))}</span>
                        </div>
                        <ProgressBar progress=job.progress_pct.unwrap_or(0.0) status=status_for_progress/>
                    </div>
                })}

                {job.error_message.clone().map(|err| view! {
                    <div class="rounded-lg border border-status-error bg-status-error/10 p-3">
                        <p class="text-sm text-status-error">{err}</p>
                    </div>
                })}
            </div>
        </Card>

        // Job metadata
        <Card title="Details".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow label="Job ID" value=job_id/>
                <DetailRow label="Adapter" value=job.adapter_name.clone()/>
                {job.category.clone().map(|cat| view! {
                    <DetailRow label="Category" value=cat/>
                })}
                {job.dataset_id.clone().map(|ds| view! {
                    <DetailRow label="Dataset" value=ds/>
                })}
                <DetailRow label="Created" value=format_date(&job.created_at)/>
                {job.started_at.clone().map(|ts| view! {
                    <DetailRow label="Started" value=format_date(&ts)/>
                })}
                {job.completed_at.clone().map(|ts| view! {
                    <DetailRow label="Completed" value=format_date(&ts)/>
                })}
            </div>
        </Card>

        // Training configuration
        <Card title="Configuration".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Epochs" value=format!("{} / {}", job.current_epoch.unwrap_or(0), job.total_epochs)/>
                <DetailRow label="Learning Rate" value=format!("{:.6}", job.learning_rate)/>
                {job.current_loss.map(|loss| view! {
                    <DetailRow label="Current Loss" value=format!("{:.4}", loss)/>
                })}
                {job.tokens_per_second.map(|tps| view! {
                    <DetailRow label="Tokens/sec" value=format!("{:.1}", tps)/>
                })}
            </div>
        </Card>

        // Backend information
        <Card title="Backend".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow
                    label="Requested Backend"
                    value=format_backend_or(job.requested_backend.as_deref(), "Not specified")
                />
                <DetailRow
                    label="Selected Backend"
                    value=format_backend_or(job.backend.as_deref(), "Pending")
                />
                {job.backend_reason.clone().map(|reason| view! {
                    <DetailRow label="Backend Reason" value=reason/>
                })}
                {job.coreml_training_fallback.clone().map(|reason| view! {
                    <DetailRow label="CoreML Fallback" value=reason/>
                })}
                {job.backend_device.clone().map(|device| view! {
                    <DetailRow label="Device" value=device/>
                })}
                {job.determinism_mode.clone().map(|mode| view! {
                    <DetailRow label="Determinism" value=mode/>
                })}
                {job.training_seed.map(|seed| view! {
                    <DetailRow label="Seed" value=seed.to_string()/>
                })}
            </div>

            {coreml_state_for_warning.coreml_fallback.then(|| view! {
                <div class="mt-3 rounded-lg border border-status-error bg-status-error/10 p-3">
                    <p class="text-sm text-status-error">
                        {"CoreML was requested, but the job ran on "}
                        {format_backend_or(job.backend.as_deref(), "a different backend")}
                        {"."}
                    </p>
                    {coreml_state_for_warning.fallback_reason.clone().map(|reason| view! {
                        <p class="text-xs text-status-error mt-1">{"Reason: "}{reason}</p>
                    })}
                </div>
            })}
        </Card>

        // CoreML export details
        <Card title="CoreML Export".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <CoremlBadges state=coreml_state_for_badges/>
                <DetailRow
                    label="Export Requested"
                    value=if coreml_export_requested { "Yes".to_string() } else { "No".to_string() }
                />
                {coreml_state_for_export.export_status.clone().map(|status| view! {
                    <DetailRow label="Export Status" value=status/>
                })}
                {coreml_state_for_export.export_reason.clone().map(|reason| view! {
                    <DetailRow label="Export Reason" value=reason/>
                })}
                {coreml_state_for_export.fused_package_hash.clone().map(|hash| view! {
                    <div>
                        <span class="text-muted-foreground">"Fused Package Hash: "</span>
                        <span class="font-mono text-xs break-all">{hash}</span>
                    </div>
                })}
                {coreml_state_for_export.package_path.clone().map(|path| view! {
                    <DetailRow label="Package Path" value=path/>
                })}
                {coreml_state_for_export.metadata_path.clone().map(|path| view! {
                    <DetailRow label="Metadata Path" value=path/>
                })}
                {job.coreml_base_manifest_hash.clone().map(|hash| view! {
                    <DetailRow label="Base Manifest Hash" value=hash/>
                })}
                {job.coreml_adapter_hash_b3.clone().map(|hash| view! {
                    <DetailRow label="Adapter Hash" value=hash/>
                })}
                {job.coreml_fusion_verified.map(|verified| view! {
                    <DetailRow
                        label="Fusion Verified"
                        value=if verified { "Yes".to_string() } else { "No".to_string() }
                    />
                })}
            </div>
        </Card>

        // Metrics (for completed jobs)
        {is_completed.then(|| view! {
            <Card title="Final Metrics".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm md:grid-cols-2">
                    {job.tokens_processed.map(|tokens| view! {
                        <DetailRow label="Tokens Processed" value=format_number(tokens)/>
                    })}
                    {job.examples_processed.map(|examples| view! {
                        <DetailRow label="Examples Processed" value=format_number(examples)/>
                    })}
                    {job.training_time_ms.map(|ms| view! {
                        <DetailRow label="Training Time" value=format_duration(ms)/>
                    })}
                    {job.peak_gpu_memory_mb.map(|mem| view! {
                        <DetailRow label="Peak GPU Memory" value=format!("{:.1} MB", mem)/>
                    })}
                </div>
            </Card>
        })}

        // Artifacts (for completed jobs)
        {job.aos_path.clone().map(|path| view! {
            <Card title="Artifacts".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm">
                    <DetailRow label="Adapter Path" value=path/>
                    {job.adapter_id.clone().map(|id| view! {
                        <DetailRow label="Adapter ID" value=id/>
                    })}
                    {job.package_hash_b3.clone().map(|hash| view! {
                        <div>
                            <span class="text-muted-foreground">"Package Hash: "</span>
                            <span class="font-mono text-xs break-all">{hash}</span>
                        </div>
                    })}
                </div>
            </Card>
        })}

        // Live logs placeholder (for running jobs)
        {is_running.then(|| view! {
            <Card title="Live Logs".to_string() class="mt-4".to_string()>
                <LogViewer job_id=job_id_for_logs/>
            </Card>
        })}

        // Cancel confirmation dialog
        <ConfirmationDialog
            open=show_cancel_confirm
            title="Cancel Training Job"
            description="Are you sure you want to cancel this training job? Progress will be lost, but you can start a new job with the same configuration."
            severity=ConfirmationSeverity::Warning
            confirm_text="Cancel Job"
            on_confirm=on_cancel
            on_cancel=on_cancel_dismiss
            loading=Signal::derive(move || cancelling.get())
        />
    }
}

/// Detail row component
#[component]
pub fn DetailRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between">
            <span class="text-muted-foreground">{label}</span>
            <span class="font-medium">{value}</span>
        </div>
    }
}

/// Log viewer component - fetches real training logs from API
#[component]
pub fn LogViewer(job_id: String) -> impl IntoView {
    use crate::api::ApiClient;
    use crate::hooks::use_polling;

    let logs: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let loading = RwSignal::new(true);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    // Initial fetch
    let job_id_clone = job_id.clone();
    Effect::new(move || {
        let job_id = job_id_clone.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.get_training_logs(&job_id).await {
                Ok(log_lines) => {
                    logs.set(log_lines);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }
            loading.set(false);
        });
    });

    // Poll for updates every 3 seconds
    let job_id_poll = job_id.clone();
    let _ = use_polling(3_000, move || {
        let job_id = job_id_poll.clone();
        async move {
            let client = ApiClient::new();
            if let Ok(log_lines) = client.get_training_logs(&job_id).await {
                logs.set(log_lines);
            }
        }
    });

    view! {
        <div class="h-48 overflow-auto bg-zinc-950 rounded-md p-3 font-mono text-xs text-status-success">
            {move || {
                if loading.get() {
                    view! {
                        <div class="text-muted-foreground">"Loading logs..."</div>
                    }.into_any()
                } else if let Some(err) = error.get() {
                    view! {
                        <div class="text-status-error">"Error: "{err}</div>
                    }.into_any()
                } else if logs.get().is_empty() {
                    view! {
                        <div class="text-muted-foreground">"No logs available yet..."</div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            {logs.get().into_iter().map(|line| {
                                view! { <div>{line}</div> }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
