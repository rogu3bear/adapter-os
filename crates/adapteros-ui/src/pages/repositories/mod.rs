//! Repositories management page
//!
//! Complete repository management with list view, detail panel, sync status, and publish workflow.

mod detail;
mod dialogs;
mod list;

use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Spinner};
use crate::hooks::{use_api_resource, LoadingState};
use detail::{RepositoryDetailPanel, RepositoryDetailStandalone};
use dialogs::RegisterRepositoryDialog;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use list::RepositoryList;
use std::sync::Arc;

// Re-export helpers for sub-modules
pub(crate) mod helpers {
    /// Format a date string for display
    pub fn format_date(date_str: &str) -> String {
        if date_str.len() >= 16 {
            format!("{} {}", &date_str[0..10], &date_str[11..16])
        } else {
            date_str.to_string()
        }
    }
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

    // Dynamic class for left panel width
    let left_panel_class = move || {
        if selected_repo_id.get().is_some() {
            "w-1/2 space-y-6 pr-4"
        } else {
            "flex-1 space-y-6"
        }
    };

    view! {
        <div class="flex h-full">
            // Left panel: Repository list
            <div class=left_panel_class>
                <div class="flex items-center justify-between">
                    <div>
                        <h1 class="text-3xl font-bold tracking-tight">"Repositories"</h1>
                        <p class="text-sm text-muted-foreground">
                            "Register and scan codebases to power code intelligence."
                        </p>
                    </div>
                    <div class="flex items-center gap-2">
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
                    </div>
                </div>

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
                            // Repositories are already filtered server-side
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
            </div>

            // Right panel: Repository detail (when selected)
            {move || {
                selected_repo_id.get().map(|repo_id| {
                    view! {
                        <div class="w-1/2 border-l pl-4">
                            <RepositoryDetailPanel
                                repo_id=repo_id.clone()
                                selected_repo_id=selected_repo_id
                            />
                        </div>
                    }
                })
            }}
        </div>

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
            <div class="flex items-center gap-4">
                <a href="/repositories" class="text-muted-foreground hover:text-foreground">
                    "<- Repositories"
                </a>
                <h1 class="text-3xl font-bold tracking-tight">"Repository Details"</h1>
            </div>

            <RepositoryDetailStandalone repo_id=repo_id.get()/>
        </div>
    }
}

/// Status filter dropdown
#[component]
fn StatusFilter(filter: RwSignal<String>) -> impl IntoView {
    view! {
        <select
            class="flex h-10 w-40 rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            on:change=move |ev| {
                filter.set(event_target_value(&ev));
            }
        >
            <option value="">"All Status"</option>
            <option value="active">"Active"</option>
            <option value="scanning">"Scanning"</option>
            <option value="pending">"Pending"</option>
            <option value="error">"Error"</option>
        </select>
    }
}
