//! Training page
//!
//! Complete training jobs management with list view, detail panel, and job creation.
//!
//! ## Filter Architecture
//!
//! This page uses a hybrid filtering strategy:
//!
//! - **Server-side filtering**: Status filter is sent to the backend via `TrainingListParams`.
//!   This is efficient for the most common filter case (e.g., "show only running jobs").
//!
//! - **Client-side filtering**: CoreML filters (requested, exported, fallback) are applied
//!   after fetching jobs. This is intentional because CoreML state is derived from multiple
//!   fields (`requested_backend`, `backend`, `coreml_export_status`, etc.) and would require
//!   complex backend logic to filter. Client-side filtering also provides instant UI feedback
//!   when toggling CoreML checkboxes.

mod components;
pub mod config_presets;
pub mod dataset_wizard;
mod detail;
mod dialogs;
pub mod generate_wizard;
mod readiness;
mod state;
mod utils;
mod wizard;

use crate::api::ApiClient;
use crate::components::{AsyncBoundary, Button, ButtonVariant, SplitPanel};
use crate::hooks::{use_api_resource, use_conditional_polling, LoadingState};
use crate::signals::{try_use_route_context, SelectedEntity};
use adapteros_api_types::TrainingListParams;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use std::sync::Arc;

use components::{CoremlFilters, StatusFilter, TrainingJobList};
use detail::TrainingJobDetail;
use readiness::BackendReadinessPanel;
use state::{matches_coreml_filters, CoremlFilterState};
use wizard::CreateJobWizard;

/// Training jobs page with list and detail panels
#[component]
pub fn Training() -> impl IntoView {
    // Selected job ID for detail panel
    let selected_job_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());
    let coreml_filter = RwSignal::new(CoremlFilterState::default());

    // Dialog open state
    let create_dialog_open = RwSignal::new(false);

    // Track source document for training (from query params)
    let source_document_id = RwSignal::new(None::<String>);
    // Track initial dataset for training (from query params)
    let initial_dataset_id = RwSignal::new(None::<String>);

    // Return-to path after wizard completion (from cross-page CTAs)
    let return_to = RwSignal::new(None::<String>);

    // Handle query parameters for document-to-training and dataset-to-training workflows
    let query = use_query_map();
    let params_consumed = RwSignal::new(false);
    let navigate = use_navigate();
    Effect::new(move || {
        let params = query.get();
        // Guard: only consume params once to prevent re-triggering on browser back
        if params_consumed.get_untracked() {
            return;
        }
        let has_params = params.get("source").is_some()
            || params.get("dataset_id").is_some()
            || params.get("open_wizard").is_some()
            || params.get("job_id").is_some()
            || params.get("return_to").is_some();
        if !has_params {
            return;
        }
        // Document-to-training workflow
        if params.get("source").as_deref() == Some("document") {
            if let Some(doc_id) = params.get("document_id") {
                source_document_id.set(Some(doc_id.clone()));
                create_dialog_open.set(true);
            }
        }
        // Dataset-to-training workflow (from dataset detail page)
        if let Some(ds_id) = params.get("dataset_id") {
            initial_dataset_id.set(Some(ds_id.clone()));
            create_dialog_open.set(true);
        }
        // Generic wizard auto-open (from chat/adapters CTAs)
        if params.get("open_wizard").as_deref() == Some("1") {
            create_dialog_open.set(true);
        }
        // Job deep-link workflow (from datasets draft page after training starts)
        if let Some(job_id) = params.get("job_id") {
            selected_job_id.set(Some(job_id.clone()));
        }
        // Return-to path for post-wizard navigation
        if let Some(path) = params.get("return_to") {
            return_to.set(Some(path.clone()));
        }
        // Mark consumed and clean URL to prevent re-triggering on browser back
        params_consumed.set(true);
        navigate(
            "/training",
            leptos_router::NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });

    // Fetch training jobs with server-side filtering
    let (jobs, refetch_jobs) = use_api_resource(move |client: Arc<ApiClient>| {
        let filter = status_filter.get_untracked();
        async move {
            let params = if filter.is_empty() {
                None
            } else {
                Some(TrainingListParams {
                    status: Some(filter),
                    ..Default::default()
                })
            };
            client.list_training_jobs(params.as_ref()).await
        }
    });

    // Derive whether we need to poll (only when there are running or pending jobs)
    let should_poll = Signal::derive(move || {
        matches!(jobs.get(), LoadingState::Loaded(ref data) if data.jobs.iter().any(|job| {
            matches!(job.status.as_str(), "running" | "pending" | "queued")
        }))
    });

    // Conditional polling for live updates (every 5 seconds when jobs are active)
    // Return value (stop fn) intentionally ignored - polling runs until unmount or no active jobs
    let _ = use_conditional_polling(5000, should_poll, move || async move {
        refetch_jobs.run(());
    });

    let on_job_select = move |job_id: String| {
        selected_job_id.set(Some(job_id));
    };

    let on_close_detail = move || {
        selected_job_id.set(None);
    };

    let on_job_created = move |job_id: String| {
        create_dialog_open.set(false);
        refetch_jobs.run(());
        selected_job_id.set(Some(job_id));
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_job_id.get().is_some());

    // Publish selection to RouteContext for contextual actions in Command Palette
    {
        let jobs = jobs.clone();
        Effect::new(move || {
            if let Some(route_ctx) = try_use_route_context() {
                if let Some(job_id) = selected_job_id.get() {
                    // Find the job name and status from loaded data
                    if let LoadingState::Loaded(data) = jobs.get() {
                        if let Some(job) = data.jobs.iter().find(|j| j.id == job_id) {
                            route_ctx.set_selected(SelectedEntity::with_status(
                                "training_job",
                                job_id.clone(),
                                job.adapter_name.clone(),
                                job.status.clone(),
                            ));
                        } else {
                            route_ctx.set_selected(SelectedEntity::new(
                                "training_job",
                                job_id.clone(),
                                job_id,
                            ));
                        }
                    } else {
                        route_ctx.set_selected(SelectedEntity::new(
                            "training_job",
                            job_id.clone(),
                            job_id,
                        ));
                    }
                } else {
                    route_ctx.clear_selected();
                }
            }
        });
    }

    view! {
        <div class="shell-page space-y-6">
            <BackendReadinessPanel/>
            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Training Jobs"
                list_panel=move || {
                    view! {
                        <div class="space-y-6">
                            // Header
                            <div class="flex items-center justify-between">
                                <h1 class="text-3xl font-bold tracking-tight">"Training Jobs"</h1>
                                <div class="flex items-center gap-2">
                                    <StatusFilter filter=status_filter/>
                                    <CoremlFilters filter=coreml_filter/>
                                    <Button
                                        variant=ButtonVariant::Primary
                                        on_click=Callback::new(move |_| create_dialog_open.set(true))
                                    >
                                        "New Training Job"
                                    </Button>
                                </div>
                            </div>
                            <p class="text-sm text-muted-foreground">
                                "Launch, monitor, and validate training runs for adapter builds."
                            </p>

                            // Job list
                            <AsyncBoundary
                                state=jobs
                                on_retry=Callback::new(move |_| refetch_jobs.run(()))
                                render=move |data| {
                                    // Apply client-side CoreML filters
                                    let filter_state = coreml_filter.get();
                                    let filtered_jobs = data
                                        .jobs
                                        .clone()
                                        .into_iter()
                                        .filter(|job| matches_coreml_filters(job, &filter_state))
                                        .collect::<Vec<_>>();
                                    let on_create = Callback::new(move |_| create_dialog_open.set(true));
                                    view! {
                                        <TrainingJobList
                                            jobs=filtered_jobs
                                            selected_id=selected_job_id
                                            on_select=on_job_select
                                            on_create=on_create
                                        />
                                    }
                                }
                            />
                        </div>
                    }
                }
                detail_panel=move || {
                    // Detail panel content - job_id comes from selected_job_id
                    let job_id = selected_job_id.get().unwrap_or_default();
                    match return_to.get_untracked() {
                        Some(ret) => view! {
                            <TrainingJobDetail
                                job_id=job_id
                                on_close=on_close_detail
                                on_cancelled=move || refetch_jobs.run(())
                                return_to=ret
                            />
                        }.into_any(),
                        None => view! {
                            <TrainingJobDetail
                                job_id=job_id
                                on_close=on_close_detail
                                on_cancelled=move || refetch_jobs.run(())
                            />
                        }.into_any(),
                    }
                }
            />

            // Create job wizard (outside SplitPanel, it's a modal)
            <CreateJobWizard
                open=create_dialog_open
                on_created=on_job_created
                initial_dataset_id=initial_dataset_id
            />
        </div>
    }
}
