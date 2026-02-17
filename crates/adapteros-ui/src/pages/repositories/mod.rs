//! Repositories management page
//!
//! Complete repository management with list view, detail panel, sync status, and publish workflow.

mod detail;
mod dialogs;
mod list;

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonVariant, ErrorDisplay, PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    Select, Spinner, SplitPanel, SplitRatio,
};
use crate::hooks::{use_api_resource, LoadingState};
use detail::{RepositoryDetailPanel, RepositoryDetailStandalone};
use dialogs::RegisterRepositoryDialog;
use leptos::prelude::*;
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
    let pending_repo_selection = RwSignal::new(None::<String>);

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

    // Keep list/detail state consistent across filtering and refresh.
    Effect::new(move || {
        let Some(selected_id) = selected_repo_id.get() else {
            return;
        };
        if pending_repo_selection.get().as_deref() == Some(selected_id.as_str()) {
            return;
        }

        let LoadingState::Loaded(data) = repos.get() else {
            return;
        };

        if !data.repos.iter().any(|repo| repo.repo_id == selected_id) {
            selected_repo_id.set(None);
        }
    });
    Effect::new(move || {
        let Some(pending_id) = pending_repo_selection.get() else {
            return;
        };
        let LoadingState::Loaded(data) = repos.get() else {
            return;
        };
        if data.repos.iter().any(|repo| repo.repo_id == pending_id) {
            selected_repo_id.set(Some(pending_id));
            pending_repo_selection.set(None);
        }
    });

    let on_repository_registered = Callback::new(move |repo_id: String| {
        selected_repo_id.set(Some(repo_id.clone()));
        pending_repo_selection.set(Some(repo_id));
        refetch_repos.run(());
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
                <Select
                    value=status_filter
                    options=vec![
                        ("".to_string(), "All Status".to_string()),
                        ("active".to_string(), "Active".to_string()),
                        ("scanning".to_string(), "Scanning".to_string()),
                        ("pending".to_string(), "Pending".to_string()),
                        ("error".to_string(), "Error".to_string()),
                    ]
                    class="w-40".to_string()
                />
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
                                        <ErrorDisplay error=e/>
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
            on_registered=on_repository_registered
        />
    }
}

/// Repository detail page (standalone route)
#[component]
pub fn RepositoryDetail() -> impl IntoView {
    view! {
        <crate::components::DetailPageShell
            title="Repository Details"
            section="Data"
            section_href="/repositories"
            entity_plural="Repositories"
            list_href="/repositories"
        >
            {move || {
                let id = expect_context::<crate::components::DetailEntityId>().get();
                view! { <RepositoryDetailStandalone repo_id=id/> }
            }}
        </crate::components::DetailPageShell>
    }
}
