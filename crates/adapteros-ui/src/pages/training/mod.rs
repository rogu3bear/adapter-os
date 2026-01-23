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
use crate::components::{Button, ButtonVariant, ErrorDisplay, Spinner, SplitPanel};
use crate::hooks::{use_api_resource, use_conditional_polling, LoadingState};
use adapteros_api_types::TrainingListParams;
use leptos::prelude::*;
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

    let on_job_created = move || {
        create_dialog_open.set(false);
        refetch_jobs.run(());
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_job_id.get().is_some());

    view! {
        <div class="p-6 space-y-6">
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

                            // Job list
                            {move || {
                                match jobs.get() {
                                    LoadingState::Idle | LoadingState::Loading => {
                                        view! {
                                            <div class="flex items-center justify-center py-12">
                                                <Spinner/>
                                            </div>
                                        }.into_any()
                                    }
                                    LoadingState::Loaded(data) => {
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
                                        }.into_any()
                                    }
                                    LoadingState::Error(e) => {
                                        view! {
                                            <ErrorDisplay
                                                error=e
                                                on_retry=Callback::new(move |_| refetch_jobs.run(()))
                                            />
                                        }.into_any()
                                    }
                                }
                            }}
                        </div>
                    }
                }
                detail_panel=move || {
                    // Detail panel content - job_id comes from selected_job_id
                    let job_id = selected_job_id.get().unwrap_or_default();
                    view! {
                        <TrainingJobDetail
                            job_id=job_id
                            on_close=on_close_detail
                            on_cancelled=move || refetch_jobs.run(())
                        />
                    }
                }
            />

            // Create job wizard (outside SplitPanel, it's a modal)
            <CreateJobWizard
                open=create_dialog_open
                on_created=on_job_created
            />
        </div>
    }
}
