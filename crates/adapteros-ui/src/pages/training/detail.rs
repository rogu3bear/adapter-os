//! Training job detail panel
//!
//! Components for displaying training job details and metrics.
//!
//! When a training job completes, this component triggers a global refetch of
//! adapters and stacks to ensure newly trained adapters appear in the UI.
//!
//! ## Layout Architecture
//!
//! The detail panel uses a tabbed layout for progressive disclosure:
//! - **Overview**: Always visible - job status, progress, quick links (sticky header)
//! - **Configuration**: Training parameters, epochs, learning rate
//! - **Backend**: Backend selection, device info, determinism settings
//! - **Export**: CoreML export status and artifacts
//! - **Metrics**: Live training metrics and loss curve (running jobs only)
//! - **Logs**: Live log viewer (running jobs only)

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity, DetailRow, ErrorDisplay,
    Link, Spinner, TabButton, TabPanel,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_notifications, use_refetch};
use crate::utils::chat_path_with_adapter;
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use std::sync::Arc;

use super::components::{CoremlBadges, JobStatusBadge, ProgressBar};
use super::state::CoremlState;
use super::utils::{format_backend_or, format_date, format_duration, format_number};

/// Tab identifiers for training job detail
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    Configuration,
    Backend,
    Export,
    Metrics,
}

/// Training job detail panel
#[component]
pub fn TrainingJobDetail(
    job_id: String,
    on_close: impl Fn() + Copy + 'static,
    on_cancelled: impl Fn() + Copy + Send + Sync + 'static,
    /// Optional return-to path (e.g., "/chat") — renders a "Back to ..." link in the header
    #[prop(optional)]
    return_to: Option<String>,
) -> impl IntoView {
    let job_id_for_fetch = job_id.clone();

    // Global refetch context for triggering adapter/stack list refresh
    let refetch_action = use_refetch();

    // Track previous status to detect completion transition
    let prev_status = RwSignal::new(String::new());

    // Fetch job details
    let (job, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = job_id_for_fetch.clone();
        async move { client.get_training_job(&id).await }
    });

    // Notifications for status changes
    let notifications = use_notifications();

    // Detect when job transitions to completed and trigger adapter/stack refresh
    {
        let notifications = notifications.clone();
        Effect::new(move || {
            if let LoadingState::Loaded(ref data) = job.get() {
                let current_status = data.status.clone();
                let previous = prev_status.get_untracked();

                // Detect transition to completed from a non-completed state
                if current_status == "completed" && previous != "completed" && !previous.is_empty()
                {
                    // Job just finished! Trigger global refetch of adapters and stacks
                    tracing::info!(
                        job_id = %data.id,
                        adapter_id = ?data.adapter_id,
                        "Training completed - triggering adapter and stack refresh"
                    );
                    refetch_action.adapters();
                    refetch_action.stacks();

                    // Show success notification with "View Adapter" action
                    // This links directly to the adapter detail page so users can find what they created
                    let adapter_name = data.adapter_name.clone();
                    let adapter_url = data
                        .adapter_id
                        .as_ref()
                        .map(|id| format!("/adapters/{}", id))
                        .unwrap_or_else(|| "/adapters".to_string());

                    notifications.success_with_action(
                        "Adapter Ready!",
                        &format!("'{}' is now available for inference", adapter_name),
                        "View Adapter",
                        &adapter_url,
                    );
                }

                // Update previous status for next comparison
                prev_status.set(current_status);
            }
        });
    }

    // Poll for updates on running jobs
    // Return value (stop fn) intentionally ignored - polling runs until unmount
    let _ = use_polling(3000, move || async move {
        refetch.run(());
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
    let cancel_callback = {
        let notifications = notifications.clone();
        Callback::new(move |_| {
            let job_id = job_id_for_cancel.clone();
            let notifications = notifications.clone();
            cancelling.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::new();
                match client.cancel_training_job(&job_id).await {
                    Ok(_) => {
                        show_cancel_confirm.set(false);
                        notifications
                            .info("Training cancelled", "The training job has been stopped");
                        on_cancelled();
                    }
                    Err(_) => {
                        // On error, close dialog - user can retry via UI
                        show_cancel_confirm.set(false);
                    }
                }
                cancelling.set(false);
            });
        })
    };

    // Derive return button label from path
    let return_button = return_to.map(|path| {
        let label = if path == "/chat" || path.starts_with("/chat/") {
            "Back to Chat"
        } else if path == "/adapters" || path.starts_with("/adapters/") {
            "Back to Adapters"
        } else if path == "/datasets" || path.starts_with("/datasets/") {
            "Back to Datasets"
        } else {
            "Go Back"
        };
        (label, path)
    });

    view! {
        <div class="space-y-4 min-w-0">
            // Header with close button
            <div class="flex items-start justify-between gap-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Training job"</p>
                    <h2 class="heading-3 leading-tight">{job_id.clone()}</h2>
                </div>
                <div class="flex items-center gap-2">
                    {return_button.map(|(label, href)| view! {
                        <Link href=href class="btn btn-secondary btn-sm">
                            <svg
                                xmlns="http://www.w3.org/2000/svg"
                                width="14"
                                height="14"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                class="mr-1.5"
                            >
                                <path d="m15 18-6-6 6-6"/>
                            </svg>
                            {label}
                        </Link>
                    })}
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
                                on_retry=Callback::new(move |_| refetch.run(()))
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Job detail content with tabbed layout
#[component]
pub fn JobDetailContent(
    job: TrainingJobResponse,
    cancelling: RwSignal<bool>,
    show_cancel_confirm: RwSignal<bool>,
    on_cancel: Callback<()>,
    on_cancel_dismiss: Callback<()>,
) -> impl IntoView {
    // Active tab state - default to Overview
    let active_tab = RwSignal::new(DetailTab::Overview);

    // Clone values before view! to avoid move issues
    let status = job.status.clone();
    let status_for_badge = job.status.clone();
    let status_for_progress = job.status.clone();
    let job_id_for_detail = job.id.clone();
    let job_id_for_logs = job.id.clone();
    let job_id_for_metrics = job.id.clone();
    let adapter_id_for_detail = job.adapter_id.clone();
    let coreml_state = CoremlState::from_job(&job);
    let coreml_export_requested = job.coreml_export_requested.unwrap_or(false);
    let job_id_for_overview = job_id_for_detail.clone();
    let adapter_link = adapter_id_for_detail
        .clone()
        .map(|id| (format!("Adapter {}", id), format!("/adapters/{}", id)));
    let dataset_link = job
        .dataset_id
        .clone()
        .map(|ds| (format!("Dataset {}", ds), format!("/datasets/{}", ds)));

    let is_running = status == "running";
    let is_pending = status == "pending";
    let is_completed = status == "completed";
    let can_cancel = is_running || is_pending;
    let show_progress = is_running || is_completed;

    // Determine if we should show the Metrics tab (only for running/completed jobs)
    let show_metrics_tab = is_running || is_completed;

    // Clone job data for each tab section
    let job_for_config = job.clone();
    let job_for_backend = job.clone();
    let job_for_export = job.clone();
    let job_for_metrics = job.clone();
    let coreml_state_for_backend = coreml_state.clone();
    let coreml_state_for_export = coreml_state.clone();

    view! {
        <div class="space-y-4 min-w-0">
            // Sticky Overview Header - always visible
            <div class="overflow-x-hidden">
                <div class="sticky top-0 z-10 -mx-4 px-4 py-3 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 border-b">
                    <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                        <div class="flex items-center flex-wrap gap-3">
                            <JobStatusBadge status=status_for_badge/>
                            <span class="text-sm text-muted-foreground">
                                {format!("{} • {}", job_id_for_overview, status)}
                            </span>
                            {adapter_link.clone().map(|(label, href)| view! {
                                <Link href=href class="text-sm font-medium">
                                    {label}
                                </Link>
                            })}
                            {dataset_link.clone().map(|(label, href)| view! {
                                <Link href=href class="text-sm font-medium">
                                    {label}
                                </Link>
                            })}
                        </div>
                        {can_cancel.then(|| {
                            let is_cancelling = Signal::derive(move || cancelling.get());
                            view! {
                                <Button
                                    variant=ButtonVariant::Destructive
                                    on_click=Callback::new(move |_| show_cancel_confirm.set(true))
                                    loading=is_cancelling
                                    disabled=is_cancelling
                                >
                                    {move || if cancelling.get() { "Cancelling..." } else { "Cancel job" }}
                                </Button>
                            }
                        })}
                    </div>

                    {show_progress.then(|| view! {
                        <div class="space-y-2 mt-3">
                            <div class="flex justify-between text-sm">
                                <span>"Progress"</span>
                                <span class="font-medium">{format!("{:.1}%", job.progress_pct.unwrap_or(0.0))}</span>
                            </div>
                            <ProgressBar progress=job.progress_pct.unwrap_or(0.0) status=status_for_progress/>
                        </div>
                    })}

                    {job.error_message.clone().map(|err| view! {
                        <div class="rounded-lg border border-status-error bg-status-error/10 p-3 mt-3">
                            <p class="text-sm text-status-error">{err}</p>
                        </div>
                    })}

                    // Completion banner with handoff actions
                    {(is_completed && adapter_id_for_detail.is_some()).then(|| {
                        let adapter_id = adapter_id_for_detail.clone().unwrap();
                        let adapter_href = format!("/adapters/{}", adapter_id);
                        let chat_href = chat_path_with_adapter(&adapter_id);
                        view! {
                            <div class="rounded-lg border border-status-success bg-status-success/10 p-4 mt-4">
                                <div class="flex items-start gap-3">
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        width="20"
                                        height="20"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        class="text-status-success mt-0.5 shrink-0"
                                    >
                                        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
                                        <polyline points="22 4 12 14.01 9 11.01"/>
                                    </svg>
                                    <div class="space-y-3 flex-1">
                                        <div>
                                            <p class="font-medium text-sm">"Training complete"</p>
                                            <p class="text-xs text-muted-foreground">"Your adapter is ready for inference."</p>
                                        </div>
                                        <div class="flex flex-wrap items-center gap-2">
                                            <Link href=adapter_href class="btn btn-primary btn-sm">
                                                <svg
                                                    xmlns="http://www.w3.org/2000/svg"
                                                    width="14"
                                                    height="14"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    class="mr-1.5"
                                                >
                                                    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
                                                    <polyline points="3.29 7 12 12 20.71 7"/>
                                                    <line x1="12" y1="22" x2="12" y2="12"/>
                                                </svg>
                                                "View Adapter"
                                            </Link>
                                            <Link href=chat_href class="btn btn-secondary btn-sm">
                                                <svg
                                                    xmlns="http://www.w3.org/2000/svg"
                                                    width="14"
                                                    height="14"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                    class="mr-1.5"
                                                >
                                                    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
                                                </svg>
                                                "Try in Chat"
                                            </Link>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }
                    })}
                </div>
            </div>

            // Tab Navigation
            <div class="border-b">
                <nav class="-mb-px flex space-x-6 overflow-x-auto" role="tablist" aria-label="Training job details">
                    <TabButton
                        tab=DetailTab::Overview
                        label="Overview".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab=DetailTab::Configuration
                        label="Configuration".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab=DetailTab::Backend
                        label="Backend".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab=DetailTab::Export
                        label="Export".to_string()
                        active=active_tab
                    />
                    {show_metrics_tab.then(|| view! {
                        <TabButton
                            tab=DetailTab::Metrics
                            label=if is_running { "Live Metrics".to_string() } else { "Final Metrics".to_string() }
                            active=active_tab
                        />
                    })}
                </nav>
            </div>

            // Tab Panels
            <TabPanel tab=DetailTab::Overview active=active_tab>
                <OverviewTabContent
                    job=job.clone()
                    job_id=job_id_for_detail.clone()
                    adapter_id=adapter_id_for_detail.clone()
                />
            </TabPanel>

            <TabPanel tab=DetailTab::Configuration active=active_tab>
                <ConfigurationTabContent job=job_for_config/>
            </TabPanel>

            <TabPanel tab=DetailTab::Backend active=active_tab>
                <BackendTabContent job=job_for_backend coreml_state=coreml_state_for_backend/>
            </TabPanel>

            <TabPanel tab=DetailTab::Export active=active_tab>
                <ExportTabContent
                    job=job_for_export
                    coreml_state=coreml_state_for_export
                    coreml_export_requested=coreml_export_requested
                />
            </TabPanel>

            <TabPanel tab=DetailTab::Metrics active=active_tab>
                <MetricsTabContent
                    job=job_for_metrics
                    job_id_for_metrics=job_id_for_metrics
                    job_id_for_logs=job_id_for_logs
                    is_running=is_running
                    is_completed=is_completed
                />
            </TabPanel>
        </div>

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

// =============================================================================
// Tab Content Components
// =============================================================================

/// Overview tab - job metadata and timestamps
#[component]
fn OverviewTabContent(
    job: TrainingJobResponse,
    job_id: String,
    adapter_id: Option<String>,
) -> impl IntoView {
    view! {
        <Card title="Job Details".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Job ID" value=job_id mono=true/>
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

        // Artifacts section (for completed jobs with outputs)
        {job.aos_path.clone().map(|path| view! {
            <Card title="Artifacts".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm">
                    <DetailRow label="Adapter Path" value=path mono=true/>
                    {adapter_id.clone().map(|id| view! {
                        <DetailRow label="Adapter ID" value=id mono=true/>
                    })}
                    {job.package_hash_b3.clone().map(|hash| view! {
                        <div class="flex flex-col gap-1">
                            <span class="text-muted-foreground">"Package Hash"</span>
                            <span class="font-mono text-xs break-all bg-muted/50 p-2 rounded">{hash}</span>
                        </div>
                    })}
                </div>
            </Card>
        })}
    }
}

/// Configuration tab - training parameters
#[component]
fn ConfigurationTabContent(job: TrainingJobResponse) -> impl IntoView {
    view! {
        <Card title="Training Parameters".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Total Epochs" value=job.total_epochs.to_string()/>
                <DetailRow label="Current Epoch" value=job.current_epoch.unwrap_or(0).to_string()/>
                <DetailRow label="Learning Rate" value=format!("{:.6}", job.learning_rate)/>
                {job.current_loss.map(|loss| view! {
                    <DetailRow label="Current Loss" value=format!("{:.4}", loss)/>
                })}
                {job.tokens_per_second.map(|tps| view! {
                    <DetailRow label="Tokens/sec" value=format!("{:.1}", tps)/>
                })}
            </div>
        </Card>
    }
}

/// Backend tab - backend selection and device info
#[component]
fn BackendTabContent(job: TrainingJobResponse, coreml_state: CoremlState) -> impl IntoView {
    view! {
        <Card title="Backend Selection".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow
                    label="Requested Backend"
                    value=format_backend_or(job.requested_backend.as_deref(), "Not specified")
                />
                <DetailRow
                    label="Selected Backend"
                    value=format_backend_or(job.backend.as_deref(), "Pending")
                />
                {job.backend_reason.clone().map(|reason| view! {
                    <DetailRow label="Selection Reason" value=reason/>
                })}
                {job.backend_device.clone().map(|device| view! {
                    <DetailRow label="Device" value=device/>
                })}
            </div>

            {coreml_state.coreml_fallback.then(|| view! {
                <div class="mt-3 rounded-lg border border-status-error bg-status-error/10 p-3">
                    <p class="text-sm text-status-error">
                        {"CoreML was requested, but the job ran on "}
                        {format_backend_or(job.backend.as_deref(), "a different backend")}
                        {"."}
                    </p>
                    {coreml_state.fallback_reason.clone().map(|reason| view! {
                        <p class="text-xs text-status-error mt-1">{"Reason: "}{reason}</p>
                    })}
                </div>
            })}
        </Card>

        // Determinism settings (collapsible section)
        {(job.determinism_mode.is_some() || job.training_seed.is_some()).then(|| view! {
            <Card title="Determinism Settings".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm md:grid-cols-2">
                    {job.determinism_mode.clone().map(|mode| view! {
                        <DetailRow label="Determinism Mode" value=mode/>
                    })}
                    {job.training_seed.map(|seed| view! {
                        <DetailRow label="Training Seed" value=seed.to_string() mono=true/>
                    })}
                </div>
            </Card>
        })}

        // CoreML training fallback info
        {job.coreml_training_fallback.clone().map(|reason| view! {
            <Card title="CoreML Training Fallback".to_string() class="mt-4".to_string()>
                <p class="text-sm text-muted-foreground">{reason}</p>
            </Card>
        })}
    }
}

/// Export tab - CoreML export status and artifacts
#[component]
fn ExportTabContent(
    job: TrainingJobResponse,
    coreml_state: CoremlState,
    coreml_export_requested: bool,
) -> impl IntoView {
    // Clone for use in different sections
    let coreml_state_for_artifacts = coreml_state.clone();
    let has_artifacts =
        coreml_state.package_path.is_some() || coreml_state.fused_package_hash.is_some();
    let has_verification = job.coreml_base_manifest_hash.is_some()
        || job.coreml_adapter_hash_b3.is_some()
        || job.coreml_fusion_verified.is_some();

    view! {
        <Card title="CoreML Export Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center gap-3">
                    <CoremlBadges state=coreml_state.clone()/>
                </div>

                <div class="grid gap-3 text-sm md:grid-cols-2">
                    <DetailRow
                        label="Export Requested"
                        value=if coreml_export_requested { "Yes".to_string() } else { "No".to_string() }
                    />
                    {coreml_state.export_status.clone().map(|status| view! {
                        <DetailRow label="Export Status" value=status/>
                    })}
                    {coreml_state.export_reason.clone().map(|reason| view! {
                        <DetailRow label="Export Reason" value=reason/>
                    })}
                </div>
            </div>
        </Card>

        // Export artifacts (when available)
        {has_artifacts.then(|| {
            let state = coreml_state_for_artifacts.clone();
            view! {
                <Card title="Export Artifacts".to_string() class="mt-4".to_string()>
                    <div class="space-y-3 text-sm">
                        {state.package_path.clone().map(|path| view! {
                            <DetailRow label="Package Path" value=path mono=true/>
                        })}
                        {state.metadata_path.clone().map(|path| view! {
                            <DetailRow label="Metadata Path" value=path mono=true/>
                        })}
                        {state.fused_package_hash.clone().map(|hash| view! {
                            <div class="flex flex-col gap-1">
                                <span class="text-muted-foreground">"Fused Package Hash"</span>
                                <span class="font-mono text-xs break-all bg-muted/50 p-2 rounded">{hash}</span>
                            </div>
                        })}
                    </div>
                </Card>
            }
        })}

        // Verification hashes
        {has_verification.then(|| view! {
            <Card title="Verification".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm md:grid-cols-2">
                    {job.coreml_base_manifest_hash.clone().map(|hash| view! {
                        <DetailRow label="Base Manifest Hash" value=hash mono=true/>
                    })}
                    {job.coreml_adapter_hash_b3.clone().map(|hash| view! {
                        <DetailRow label="Adapter Hash (B3)" value=hash mono=true/>
                    })}
                    {job.coreml_fusion_verified.map(|verified| view! {
                        <DetailRow
                            label="Fusion Verified"
                            value=if verified { "Yes".to_string() } else { "No".to_string() }
                        />
                    })}
                </div>
            </Card>
        })}
    }
}

/// Metrics tab - live or final metrics display
#[component]
fn MetricsTabContent(
    job: TrainingJobResponse,
    job_id_for_metrics: String,
    job_id_for_logs: String,
    is_running: bool,
    is_completed: bool,
) -> impl IntoView {
    view! {
        // Final metrics (for completed jobs)
        {is_completed.then(|| view! {
            <Card title="Final Metrics".to_string()>
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

        // Live metrics chart (for running jobs)
        {is_running.then(|| {
            view! {
                <Card title="Training Metrics".to_string()>
                    <MetricsChart job_id=job_id_for_metrics.clone()/>
                </Card>
            }
        })}

        // Live logs (for running jobs)
        {is_running.then(|| view! {
            <Card title="Live Logs".to_string() class="mt-4".to_string()>
                <LogViewer job_id=job_id_for_logs/>
            </Card>
        })}
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
        <div class="h-48 overflow-auto bg-muted rounded-md p-3 font-mono text-xs text-status-success">
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

/// Metrics chart component - displays training loss curve
#[component]
pub fn MetricsChart(job_id: String) -> impl IntoView {
    use crate::api::ApiClient;
    use crate::hooks::use_polling;
    use adapteros_api_types::TrainingMetricEntry;

    let metrics: RwSignal<Vec<TrainingMetricEntry>> = RwSignal::new(vec![]);
    let loading = RwSignal::new(true);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    // Initial fetch
    let job_id_clone = job_id.clone();
    Effect::new(move || {
        let job_id = job_id_clone.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.get_training_metrics(&job_id).await {
                Ok(response) => {
                    metrics.set(response.metrics);
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
            if let Ok(response) = client.get_training_metrics(&job_id).await {
                metrics.set(response.metrics);
            }
        }
    });

    view! {
        <div class="space-y-4">
            {move || {
                if loading.get() {
                    view! {
                        <div class="h-32 flex items-center justify-center text-muted-foreground">
                            "Loading metrics..."
                        </div>
                    }.into_any()
                } else if let Some(err) = error.get() {
                    view! {
                        <div class="h-32 flex items-center justify-center text-status-error text-sm">
                            "Metrics unavailable: "{err}
                        </div>
                    }.into_any()
                } else if metrics.get().is_empty() {
                    view! {
                        <div class="h-32 flex items-center justify-center text-muted-foreground">
                            "No metrics data yet..."
                        </div>
                    }.into_any()
                } else {
                    let data = metrics.get();
                    let latest = data.last();

                    // Calculate min/max loss for scaling
                    let losses: Vec<f64> = data.iter().map(|m| m.loss).collect();
                    let min_loss = losses.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max_loss = losses.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let range = (max_loss - min_loss).max(0.001); // Prevent division by zero

                    // Build SVG path for loss curve
                    let points: Vec<String> = data.iter().enumerate().map(|(i, m)| {
                        let x = if data.len() > 1 {
                            (i as f64 / (data.len() - 1) as f64) * 100.0
                        } else {
                            50.0
                        };
                        let y = 100.0 - ((m.loss - min_loss) / range * 80.0 + 10.0); // 10% padding
                        format!("{:.1},{:.1}", x, y)
                    }).collect();

                    let path_data = if points.len() > 1 {
                        format!("M {} L {}", points[0], points[1..].join(" L "))
                    } else if !points.is_empty() {
                        format!("M {} L {}", points[0], points[0])
                    } else {
                        String::new()
                    };

                    view! {
                        <div>
                            // Summary stats
                            <div class="grid grid-cols-4 gap-4 mb-4">
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Steps"</p>
                                    <p class="text-lg font-semibold">{data.len()}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Current Epoch"</p>
                                    <p class="text-lg font-semibold">{latest.map(|m| m.epoch).unwrap_or(0)}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Latest Loss"</p>
                                    <p class="text-lg font-semibold">{format!("{:.4}", latest.map(|m| m.loss).unwrap_or(0.0))}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Min Loss"</p>
                                    <p class="text-lg font-semibold text-status-success">{format!("{:.4}", min_loss)}</p>
                                </div>
                            </div>

                            // Loss curve visualization
                            <div class="relative h-32 bg-muted/30 rounded-md p-2">
                                <svg class="w-full h-full" viewBox="0 0 100 100" preserveAspectRatio="none">
                                    // Grid lines
                                    <line x1="0" y1="25" x2="100" y2="25" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>
                                    <line x1="0" y1="50" x2="100" y2="50" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>
                                    <line x1="0" y1="75" x2="100" y2="75" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>

                                    // Loss curve
                                    <path
                                        d=path_data
                                        fill="none"
                                        stroke="hsl(var(--primary))"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        vector-effect="non-scaling-stroke"
                                    />
                                </svg>

                                // Y-axis labels
                                <div class="absolute left-0 top-0 h-full flex flex-col justify-between text-2xs text-muted-foreground py-1">
                                    <span>{format!("{:.2}", max_loss)}</span>
                                    <span>{format!("{:.2}", min_loss)}</span>
                                </div>
                            </div>

                            <p class="text-xs text-muted-foreground text-center mt-2">
                                "Loss over training steps"
                            </p>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
