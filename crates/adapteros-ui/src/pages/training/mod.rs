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
pub mod generate_wizard;
mod readiness;
mod state;
mod utils;
mod wizard;

use crate::api::ApiClient;
use crate::components::{
    AsyncBoundary, Button, ButtonVariant, Link, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, SplitPanel,
};
use crate::hooks::{use_api_resource, use_conditional_polling, LoadingState};
use crate::signals::{try_use_route_context, SelectedEntity};
use adapteros_api_types::TrainingListParams;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
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
    let initial_base_model_id = RwSignal::new(None::<String>);
    let initial_preferred_backend = RwSignal::new(None::<String>);
    let initial_backend_policy = RwSignal::new(None::<String>);
    let initial_epochs = RwSignal::new(None::<String>);
    let initial_learning_rate = RwSignal::new(None::<String>);
    let initial_batch_size = RwSignal::new(None::<String>);
    let initial_rank = RwSignal::new(None::<String>);
    let initial_alpha = RwSignal::new(None::<String>);

    // Adapter name filter (from adapter detail provenance link)
    let adapter_name_filter = RwSignal::new(None::<String>);

    // Return-to path after wizard completion (from cross-page CTAs)
    let return_to = RwSignal::new(None::<String>);

    // Handle query parameters for document-to-training and dataset-to-training workflows
    let query = use_query_map();
    let params_consumed = RwSignal::new(false);
    Effect::new(move || {
        let Some(params) = query.try_get() else {
            return;
        };
        // Guard: only consume params once to prevent re-triggering on browser back
        if params_consumed.get_untracked() {
            return;
        }
        let has_params = params.get("source").is_some()
            || params.get("dataset_id").is_some()
            || params.get("open_wizard").is_some()
            || params.get("job_id").is_some()
            || params.get("adapter_name").is_some()
            || params.get("return_to").is_some()
            || params.get("base_model_id").is_some();
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
        if let Some(base_model_id) = params.get("base_model_id") {
            initial_base_model_id.set(Some(base_model_id.clone()));
        }
        if let Some(preferred_backend) = params.get("preferred_backend") {
            initial_preferred_backend.set(Some(preferred_backend.clone()));
        }
        if let Some(backend_policy) = params.get("backend_policy") {
            initial_backend_policy.set(Some(backend_policy.clone()));
        }
        if let Some(epochs) = params.get("epochs") {
            initial_epochs.set(Some(epochs.clone()));
        }
        if let Some(learning_rate) = params.get("learning_rate") {
            initial_learning_rate.set(Some(learning_rate.clone()));
        }
        if let Some(batch_size) = params.get("batch_size") {
            initial_batch_size.set(Some(batch_size.clone()));
        }
        if let Some(rank) = params.get("rank") {
            initial_rank.set(Some(rank.clone()));
        }
        if let Some(alpha) = params.get("alpha") {
            initial_alpha.set(Some(alpha.clone()));
        }
        // Generic wizard auto-open (from chat/adapters CTAs)
        if params.get("open_wizard").as_deref() == Some("1") {
            create_dialog_open.set(true);
        }
        // Job deep-link workflow (from datasets draft page after training starts)
        if let Some(job_id) = params.get("job_id") {
            selected_job_id.set(Some(job_id.clone()));
        }
        // Adapter provenance workflow (from adapter detail page)
        if let Some(name) = params.get("adapter_name") {
            adapter_name_filter.set(Some(name.clone()));
        }
        // Return-to path for post-wizard navigation
        if let Some(path) = params.get("return_to") {
            return_to.set(Some(path.clone()));
        }
        // Mark consumed and clean URL without route navigation/remount.
        params_consumed.set(true);
        if let Some(window) = web_sys::window() {
            if let Ok(history) = window.history() {
                let _ = history.replace_state_with_url(
                    &wasm_bindgen::JsValue::NULL,
                    "",
                    Some("/training"),
                );
            }
        }
    });

    // Fetch training jobs with server-side filtering
    let (jobs, refetch_jobs) = use_api_resource(move |client: Arc<ApiClient>| {
        let filter = status_filter.get_untracked();
        let adapter_name = adapter_name_filter.get_untracked();
        async move {
            let has_filter = !filter.is_empty() || adapter_name.is_some();
            let params = if has_filter {
                Some(TrainingListParams {
                    status: if filter.is_empty() {
                        None
                    } else {
                        Some(filter)
                    },
                    adapter_name,
                    ..Default::default()
                })
            } else {
                None
            };
            client.list_training_jobs(params.as_ref()).await
        }
    });

    // Derive whether we need to poll (only when there are active jobs)
    let should_poll = Signal::derive(move || {
        matches!(jobs.get(), LoadingState::Loaded(ref data) if data.jobs.iter().any(|job| {
            matches!(job.status.as_str(), "running" | "pending")
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
        // If this scope was disposed (e.g., route changed), ignore late async completion.
        if create_dialog_open.try_set(false).is_none() {
            return;
        }
        refetch_jobs.run(());
        let _ = selected_job_id.try_set(Some(job_id));
    };

    // Derive selection state for SplitPanel
    let has_selection = Signal::derive(move || selected_job_id.get().is_some());

    // Publish selection to RouteContext for contextual actions in Command Palette
    {
        Effect::new(move || {
            if let Some(route_ctx) = try_use_route_context() {
                if let Some(job_id) = selected_job_id.try_get().flatten() {
                    // Find the job name and status from loaded data
                    if let LoadingState::Loaded(data) = jobs.try_get().unwrap_or(LoadingState::Idle)
                    {
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
        <PageScaffold
            title="Training Jobs"
            subtitle="Launch, monitor, and validate training runs for adapter builds."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Train", "/training"),
                PageBreadcrumbItem::current("Training Jobs"),
            ]
        >
            <PageScaffoldActions slot>
                <Link href="/datasets" class="btn btn-secondary btn-sm">
                    "Datasets"
                </Link>
                <StatusFilter filter=status_filter/>
                <CoremlFilters filter=coreml_filter/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| create_dialog_open.set(true))
                >
                    "New Training Job"
                </Button>
            </PageScaffoldActions>

            <BackendReadinessPanel/>
            <SplitPanel
                has_selection=has_selection
                on_close=Callback::new(move |_| on_close_detail())
                back_label="Back to Training Jobs"
                list_panel=move || {
                    view! {
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
                    }
                }
                detail_panel=move || {
                    // Detail panel content - job_id comes from selected_job_id
                    let job_id = selected_job_id.get().unwrap_or_default();
                    let ret = return_to.get_untracked();
                    view! {
                        <TrainingJobDetail
                            job_id=job_id
                            on_close=on_close_detail
                            on_cancelled=move || refetch_jobs.run(())
                            return_to=ret
                        />
                    }
                }
            />

            // Create job wizard (outside SplitPanel, it's a modal)
            <CreateJobWizard
                open=create_dialog_open
                on_created=on_job_created
                initial_dataset_id=initial_dataset_id
                source_document_id=source_document_id
                initial_base_model_id=initial_base_model_id
                initial_preferred_backend=initial_preferred_backend
                initial_backend_policy=initial_backend_policy
                initial_epochs=initial_epochs
                initial_learning_rate=initial_learning_rate
                initial_batch_size=initial_batch_size
                initial_rank=initial_rank
                initial_alpha=initial_alpha
            />
        </PageScaffold>
    }
}
