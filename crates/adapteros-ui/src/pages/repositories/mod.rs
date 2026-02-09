//! Repositories management page
//!
//! Complete repository management with list view, detail panel, sync status, and publish workflow.

mod detail;
mod dialogs;
mod list;

use crate::api::ApiClient;
use crate::components::{
    BreadcrumbItem, BreadcrumbTrail, Button, ButtonVariant, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, Select, Spinner, SplitPanel, SplitRatio,
};
use crate::hooks::{use_api_resource, LoadingState};
use detail::{RepositoryDetailPanel, RepositoryDetailStandalone};
use dialogs::RegisterRepositoryDialog;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use list::RepositoryList;
use std::sync::Arc;

// Re-export helpers for sub-modules
pub(crate) mod helpers {
    pub use crate::utils::format_datetime;
}

/// Repositories list page
#[component]
pub fn Repositories() -> impl IntoView {
    // Selected repository ID for detail panel
    let selected_repo_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());

    // Dialog states
    let register_dialog_open = RwSignal::new(false);

    // Fetch repositories with server-side filtering
    let (repos, refetch_repos) = use_api_resource(move |client: Arc<ApiClient>| {
        let filter = status_filter.get();
        async move {
            let status = if filter.is_empty() {
                None
            } else {
                Some(filter.as_str())
            };
            client.list_repositories(status).await
        }
    });

    view! {
        <PageScaffold
            title="Repositories"
            subtitle="Register and scan codebases to power code intelligence."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Data", "/repositories"),
                PageBreadcrumbItem::current("Repositories"),
            ]
        >
            <PageScaffoldActions slot>
                <StatusFilter filter=status_filter/>
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| register_dialog_open.set(true))
                >
                    "Register Repository"
                </Button>
                <Button
                    variant=ButtonVariant::Secondary
                    on_click=Callback::new(move |_| refetch_repos.run(()))
                >
                    "Refresh"
                </Button>
            </PageScaffoldActions>

            <SplitPanel
                has_selection=Signal::derive(move || selected_repo_id.get().is_some())
                on_close=Callback::new(move |_| selected_repo_id.set(None))
                back_label="Back to Repositories"
                ratio=SplitRatio::Half
                list_panel=move || {
                    view! {
                        {move || {
                            match repos.get() {
                                LoadingState::Idle | LoadingState::Loading => {
                                    view! {
                                        <div class="flex items-center justify-center py-12">
                                            <Spinner/>
                                        </div>
                                    }.into_any()
                                }
                                LoadingState::Loaded(data) => {
                                    view! {
                                        <RepositoryList
                                            repos=data.repos.clone()
                                            selected_id=selected_repo_id
                                        />
                                    }.into_any()
                                }
                                LoadingState::Error(e) => {
                                    view! {
                                        <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                            <p class="text-destructive">{e.to_string()}</p>
                                        </div>
                                    }.into_any()
                                }
                            }
                        }}
                    }
                }
                detail_panel=move || {
                    view! {
                        {move || {
                            selected_repo_id.get().map(|repo_id| {
                                view! {
                                    <RepositoryDetailPanel
                                        repo_id=repo_id.clone()
                                        selected_repo_id=selected_repo_id
                                    />
                                }
                            })
                        }}
                    }
                }
            />
        </PageScaffold>

        // Register repository dialog
        <RegisterRepositoryDialog
            open=register_dialog_open
        />
    }
}

/// Repository detail page (standalone route)
#[component]
pub fn RepositoryDetail() -> impl IntoView {
    let params = use_params_map();

    // Get repository ID from URL
    let repo_id = Memo::new(move |_| params.get().get("id").unwrap_or_default());

    view! {
        <div class="space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Repositories", "/repositories"),
                BreadcrumbItem::current(repo_id.get()),
            ]/>

            <h1 class="heading-1">"Repository Details"</h1>

            <RepositoryDetailStandalone repo_id=repo_id.get()/>
        </div>
    }
}

/// Status filter dropdown
#[component]
fn StatusFilter(filter: RwSignal<String>) -> impl IntoView {
    view! {
        <Select
            value=filter
            options=vec![
                ("".to_string(), "All Status".to_string()),
                ("active".to_string(), "Active".to_string()),
                ("scanning".to_string(), "Scanning".to_string()),
                ("pending".to_string(), "Pending".to_string()),
                ("error".to_string(), "Error".to_string()),
            ]
            class="w-40".to_string()
        />
    }
}
