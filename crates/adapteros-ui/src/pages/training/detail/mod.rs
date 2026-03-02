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

mod components;

use crate::api::{use_api_client, ApiClient};
use crate::components::{
    Button, ButtonLink, ButtonSize, ButtonVariant, ConfirmationDialog, ConfirmationSeverity,
    ErrorDisplay, Link, Spinner, TabButton, TabPanel,
};
use crate::hooks::{use_api_resource, use_conditional_polling, LoadingState};
use crate::signals::{use_notifications, use_refetch};
use crate::utils::chat_path_with_adapter;
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::components::{JobStatusBadge, ProgressBar};
use super::state::CoremlState;
use components::{
    BackendTabContent, ConfigurationTabContent, ExportTabContent, MetricsTabContent,
    OverviewTabContent,
};

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
    return_to: Option<String>,
) -> impl IntoView {
    let is_active = Arc::new(AtomicBool::new(true));
    {
        let is_active = Arc::clone(&is_active);
        on_cleanup(move || {
            is_active.store(false, Ordering::Relaxed);
        });
    }

    let job_id_for_fetch = job_id.clone();
    let client = use_api_client();

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
            if let Some(LoadingState::Loaded(ref data)) = job.try_get() {
                let current_status = data.status.clone();
                let previous = prev_status.try_get_untracked().unwrap_or_default();

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

                    // Show success notification with conversation handoff action
                    let adapter_name = data.adapter_name.clone();
                    let chat_url = data
                        .adapter_id
                        .as_ref()
                        .map(|id| chat_path_with_adapter(id))
                        .unwrap_or_else(|| "/chat".to_string());

                    notifications.success_with_action(
                        "Skill ready",
                        &format!("'{}' is ready for conversation", adapter_name),
                        "Start conversation",
                        &chat_url,
                    );
                }

                // Update previous status for next comparison
                let _ = prev_status.try_set(current_status);
            }
        });
    }

    // Only poll when job is still active (running/pending/queued)
    let should_poll = Signal::derive(move || {
        matches!(job.try_get(), Some(LoadingState::Loaded(ref data)) if {
            matches!(data.status.as_str(), "running" | "pending" | "queued")
        })
    });
    let _ = use_conditional_polling(3000, should_poll, move || async move {
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
        let client = client.clone();
        let is_active = Arc::clone(&is_active);
        Callback::new(move |_| {
            let job_id = job_id_for_cancel.clone();
            let notifications = notifications.clone();
            let client = client.clone();
            let is_active = Arc::clone(&is_active);
            let _ = cancelling.try_set(true);

            wasm_bindgen_futures::spawn_local(async move {
                if !is_active.load(Ordering::Relaxed) {
                    return;
                }
                match client.cancel_training_job(&job_id).await {
                    Ok(_) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = show_cancel_confirm.try_set(false);
                        notifications
                            .info("Skill build cancelled", "The skill build has been stopped.");
                        if is_active.load(Ordering::Relaxed) {
                            on_cancelled();
                        }
                    }
                    Err(e) => {
                        if !is_active.load(Ordering::Relaxed) {
                            return;
                        }
                        notifications.error("Cancel failed", &e.user_message());
                        let _ = show_cancel_confirm.try_set(false);
                    }
                }
                let _ = cancelling.try_set(false);
            });
        })
    };

    // Derive return button label and optional "training started" banner from path
    let return_banner = return_to
        .as_ref()
        .filter(|p| p == &"/chat" || p.starts_with("/chat/"))
        .cloned();
    let return_button = return_to.map(|path| {
        let (label, href) = if path == "/chat" || path.starts_with("/chat/") {
            ("Back to Conversation", path)
        } else if path == "/adapters" || path.starts_with("/adapters/") {
            ("Back to Adapters", path)
        } else {
            ("Go Back", path)
        };
        (label, href)
    });

    view! {
        <div class="space-y-4 min-w-0" data-testid="training-job-detail">
            // Header with close button
            <div class="flex items-start justify-between gap-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Adapter build"</p>
                    <h2 class="heading-3 leading-tight">{job_id.clone()}</h2>
                </div>
                <div class="flex items-center gap-2">
                    {return_button.map(|(label, href)| view! {
                        <ButtonLink href=href variant=ButtonVariant::Secondary size=ButtonSize::Sm>
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
                        </ButtonLink>
                    })}
                    <button
                        class="btn btn-ghost btn-icon-sm"
                        aria-label="Close"
                        type="button"
                        on:click=move |_| on_close()
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="18"
                            height="18"
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

            // "Training started" banner when navigating from chat
            {return_banner.map(|href| view! {
                <div class="training-return-banner">
                    <div class="flex items-center gap-3">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="w-5 h-5 text-status-success flex-shrink-0"
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                        </svg>
                        <div>
                            <p class="font-semibold text-sm">"Skill build started"</p>
                            <p class="text-xs text-muted-foreground">
                                "Your skill is being prepared now. You can monitor progress here or return to conversation."
                            </p>
                        </div>
                    </div>
                    <ButtonLink href=href variant=ButtonVariant::Primary size=ButtonSize::Sm>
                        "Back to Conversation"
                    </ButtonLink>
                </div>
            })}

            {move || {
                match job.try_get().unwrap_or(LoadingState::Loading) {
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
    let dataset_link = job.dataset_id.clone().map(|ds| {
        (
            format!("Dataset {}", ds),
            format!("/training?open_wizard=1&dataset_id={}", ds),
        )
    });

    let is_running = status == "running";
    let is_pending = status == "pending";
    let is_completed = status == "completed";
    let is_failed = status == "failed" || status == "cancelled";
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
                            let is_cancelling = Signal::derive(move || cancelling.try_get().unwrap_or(false));
                            view! {
                                <Button
                                    variant=ButtonVariant::Destructive
                                    on_click=Callback::new(move |_| show_cancel_confirm.set(true))
                                    loading=is_cancelling
                                    disabled=is_cancelling
                                >
                                    {move || if cancelling.try_get().unwrap_or(false) { "Cancelling..." } else { "Cancel build" }}
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

                    // Retry / resume banner for failed/cancelled jobs
                    {is_failed.then(|| {
                        let retry_href = if let Some(ref ds_id) = job.dataset_id {
                            format!("/training?dataset_id={}&open_wizard=1", ds_id)
                        } else {
                            "/training?open_wizard=1".to_string()
                        };
                        let job_id_for_resume = job.id.clone();
                        let checkpoint_epoch = job.current_epoch.unwrap_or(0);
                        view! {
                            <FailedJobActions
                                job_id=job_id_for_resume
                                checkpoint_epoch=checkpoint_epoch
                                retry_href=retry_href
                            />
                        }
                    })}

                    // Completion banner with handoff actions
                    {(is_completed && adapter_id_for_detail.is_some()).then(|| {
                        let adapter_id = adapter_id_for_detail.clone().unwrap_or_default();
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
                                            <p class="font-medium text-sm">"Skill ready"</p>
                                            <p class="text-xs text-muted-foreground">"Your new skill is now live and ready to use."</p>
                                        </div>
                                        <div class="flex flex-wrap items-center gap-2">
                                            <ButtonLink
                                                href=chat_href
                                                variant=ButtonVariant::Primary
                                                size=ButtonSize::Sm
                                            >
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
                                                "Start Conversation"
                                            </ButtonLink>
                                            <ButtonLink
                                                href=adapter_href
                                                variant=ButtonVariant::Secondary
                                                size=ButtonSize::Sm
                                            >
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
                                                "View Skill"
                                            </ButtonLink>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }
                    })}
                </div>
            </div>

            // Tab Navigation
            <nav class="tab-nav -mb-px" role="tablist" aria-label="Training job details">
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

            // Tab Panels
            <TabPanel tab=DetailTab::Overview active=active_tab>
                <OverviewTabContent
                    job=job.clone()
                    job_id=job_id_for_detail.clone()
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
            title="Cancel Skill Build"
            description="Are you sure you want to cancel this skill build? Progress will be lost, but you can start another build with the same setup."
            severity=ConfirmationSeverity::Warning
            confirm_text="Cancel Build"
            on_confirm=on_cancel
            on_cancel=on_cancel_dismiss
            loading=Signal::derive(move || cancelling.try_get().unwrap_or(false))
        />
    }
}

// =============================================================================
// Failed Job Actions (Retry / Resume from Checkpoint)
// =============================================================================

/// Actions banner for failed/cancelled jobs with checkpoint awareness.
///
/// Probes the checkpoint verify endpoint to determine if a checkpoint exists
/// for the given job. If found, shows "Resume from Checkpoint" alongside
/// the regular "Retry" link.
#[component]
fn FailedJobActions(job_id: String, checkpoint_epoch: u32, retry_href: String) -> impl IntoView {
    let has_checkpoint = RwSignal::new(false);
    let resuming = RwSignal::new(false);
    let notifications = use_notifications();
    let client = use_api_client();

    // Probe for checkpoint if the job made progress (epoch > 0)
    if checkpoint_epoch > 0 {
        let job_id_probe = job_id.clone();
        let probe_epoch = checkpoint_epoch.saturating_sub(1);
        let client = client.clone();
        Effect::new(move |_| {
            let job_id = job_id_probe.clone();
            let client = client.clone();
            gloo_timers::callback::Timeout::new(0, move || {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(Some(_)) = client.check_job_checkpoint(&job_id, probe_epoch).await {
                        let _ = has_checkpoint.try_set(true);
                    }
                });
            })
            .forget();
        });
    }

    // Resume handler: calls the retry endpoint (which reuses config + existing checkpoints)
    let job_id_for_resume = job_id.clone();
    let resume_callback = {
        let notifications = notifications.clone();
        let client = client.clone();
        Callback::new(move |_: ()| {
            let job_id = job_id_for_resume.clone();
            let notifications = notifications.clone();
            let client = client.clone();
            resuming.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                match client.retry_training_job(&job_id).await {
                    Ok(new_job) => {
                        notifications.success(
                            "Resuming training",
                            &format!("New job {} created from checkpoint", new_job.id),
                        );
                        // Navigate to the new job
                        let href = format!("/training/{}", new_job.id);
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href(&href);
                        }
                    }
                    Err(e) => {
                        notifications.error("Resume failed", &e.user_message());
                    }
                }
                let _ = resuming.try_set(false);
            });
        })
    };

    let is_resuming = Signal::derive(move || resuming.try_get().unwrap_or(false));

    view! {
        <div class="rounded-lg border border-muted bg-muted/30 p-4 mt-4">
            <div class="flex items-start justify-between gap-4">
                <div>
                    <p class="text-sm font-medium">"Retry this training?"</p>
                    <Show
                        when=move || has_checkpoint.try_get().unwrap_or(false)
                        fallback=|| view! {
                            <p class="text-xs text-muted-foreground">"Create a new job with the same dataset."</p>
                        }
                    >
                        <p class="text-xs text-muted-foreground">
                            {format!("Checkpoint available at epoch {}. Resume to continue from where training stopped.", checkpoint_epoch.saturating_sub(1))}
                        </p>
                    </Show>
                </div>
                <div class="flex items-center gap-2 shrink-0">
                    <Show
                        when=move || has_checkpoint.try_get().unwrap_or(false)
                        fallback=|| ()
                    >
                        <Button
                            variant=ButtonVariant::Primary
                            size=ButtonSize::Sm
                            on_click=Callback::new(move |_| resume_callback.run(()))
                            loading=is_resuming
                            disabled=is_resuming
                        >
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
                                <polygon points="5 3 19 12 5 21 5 3"/>
                            </svg>
                            {move || if resuming.try_get().unwrap_or(false) { "Resuming..." } else { "Resume from checkpoint" }}
                        </Button>
                    </Show>
                    <ButtonLink
                        href=retry_href.clone()
                        variant=ButtonVariant::Secondary
                        size=ButtonSize::Sm
                    >
                        "Retry from scratch"
                    </ButtonLink>
                </div>
            </div>
        </div>
    }
}
