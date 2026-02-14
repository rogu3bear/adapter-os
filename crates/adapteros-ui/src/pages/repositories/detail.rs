//! Repository detail components

use super::helpers::format_datetime;
use super::list::RepoStatusBadge;
use crate::api::{
    report_error_with_toast, ApiClient, RepositoryDetailResponse, ScanRepositoryRequest,
};
use crate::components::{
    Button, ButtonSize, ButtonVariant, Card, DetailRow, ErrorDisplay, Spinner,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::use_auth;
use leptos::prelude::*;
use std::sync::Arc;

/// Repository detail panel (embedded in split view)
#[component]
pub fn RepositoryDetailPanel(
    repo_id: String,
    selected_repo_id: RwSignal<Option<String>>,
) -> impl IntoView {
    let repo_id_for_fetch = repo_id.clone();
    let repo_id_for_sync = repo_id.clone();

    // Fetch repository details
    let (repo, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = repo_id_for_fetch.clone();
        async move { client.get_repository(&id).await }
    });

    // Sync button state
    let syncing = RwSignal::new(false);

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"Repository Details"</h2>
                <Button
                    variant=ButtonVariant::Ghost
                    size=ButtonSize::IconSm
                    aria_label="Close details".to_string()
                    on_click=Callback::new(move |_| selected_repo_id.set(None))
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
                </Button>
            </div>

            {move || {
                let repo_id_sync = repo_id_for_sync.clone();
                match repo.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let is_scanning = data.status == "scanning" || data.status == "syncing";
                        let repo_data = data.clone();

                        view! {
                            <RepositoryContent
                                repo_data=repo_data
                                is_scanning=is_scanning
                                syncing=syncing
                                repo_id_sync=repo_id_sync
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) if e.is_not_found() => {
                        view! {
                            <div class="flex flex-col items-center justify-center py-12 px-4">
                                <div class="text-center">
                                    <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                    <h2 class="heading-3 mb-2">"Repository not found"</h2>
                                    <p class="text-muted-foreground mb-4">
                                        "This repository may have been deleted or doesn\u{2019}t exist."
                                    </p>
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        size=ButtonSize::Sm
                                        on_click=Callback::new(move |_| selected_repo_id.set(None))
                                    >
                                        "Back to list"
                                    </Button>
                                </div>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay error=e/>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Repository detail standalone (for route)
#[component]
pub fn RepositoryDetailStandalone(repo_id: String) -> impl IntoView {
    let repo_id_for_fetch = repo_id.clone();
    let repo_id_for_sync = repo_id.clone();

    // Fetch repository details
    let (repo, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = repo_id_for_fetch.clone();
        async move { client.get_repository(&id).await }
    });

    // Sync button state
    let syncing = RwSignal::new(false);

    view! {
        <div class="space-y-4">
            {move || {
                let repo_id_sync = repo_id_for_sync.clone();
                match repo.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let is_scanning = data.status == "scanning" || data.status == "syncing";
                        let repo_data = data.clone();

                        view! {
                            <RepositoryContent
                                repo_data=repo_data
                                is_scanning=is_scanning
                                syncing=syncing
                                repo_id_sync=repo_id_sync
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) if e.is_not_found() => {
                        view! {
                            <div class="flex min-h-[40vh] flex-col items-center justify-center px-4">
                                <Card class="p-8 max-w-md w-full text-center">
                                    <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                    <h2 class="heading-3 mb-2">"Repository not found"</h2>
                                    <p class="text-muted-foreground mb-6">
                                        "This repository may have been deleted or doesn\u{2019}t exist."
                                    </p>
                                    <a href="/repositories" class="btn btn-primary btn-md">
                                        "View all repositories"
                                    </a>
                                </Card>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay error=e/>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Repository content (shared between panel and standalone)
#[component]
fn RepositoryContent(
    repo_data: RepositoryDetailResponse,
    is_scanning: bool,
    syncing: RwSignal<bool>,
    repo_id_sync: String,
) -> impl IntoView {
    let (auth_state, _) = use_auth();
    let tenant_id = move || {
        auth_state
            .try_get()
            .and_then(|s| s.user().map(|u| u.tenant_id.clone()))
    };

    view! {
        // Status and actions
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <RepoStatusBadge status=repo_data.status.clone()/>
                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Secondary
                            size=ButtonSize::Sm
                            disabled=Signal::derive(move || syncing.try_get().unwrap_or(false) || is_scanning)
                            on_click=Callback::new({
                                let repo_id = repo_id_sync.clone();
                                move |_| {
                                    let repo_id = repo_id.clone();
                                    syncing.set(true);
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let client = ApiClient::new();
                                        let Some(tenant_id) = tenant_id() else {
                                            syncing.set(false);
                                            return;
                                        };
                                        let request = ScanRepositoryRequest {
                                            tenant_id,
                                            repo_id,
                                            // Use HEAD to trigger a full rescan against the default branch.
                                            commit: "HEAD".to_string(),
                                            full_scan: true,
                                        };
                                        if let Err(e) = client.scan_repository(&request).await {
                                            report_error_with_toast(&e, "Failed to scan repository", Some("/repositories"), true);
                                        }
                                        syncing.set(false);
                                    });
                                }
                            })
                        >
                            {move || if syncing.try_get().unwrap_or(false) || is_scanning { "Syncing..." } else { "Sync Now" }}
                        </Button>
                    </div>
                </div>
            </div>
        </Card>

        // Basic info
        <Card title="Information".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow label="Repository ID" value=repo_data.repo_id.clone()/>
                <DetailRow label="Path" value=repo_data.path.clone()/>
                <DetailRow label="Default Branch" value=repo_data.default_branch.clone()/>
                <DetailRow label="Created" value=format_datetime(&repo_data.created_at)/>
                <DetailRow label="Updated" value=format_datetime(&repo_data.updated_at)/>
                <DetailRow
                    label="Latest Scan"
                    value=repo_data
                        .latest_scan_at
                        .as_deref()
                        .map(format_datetime)
                        .unwrap_or_else(|| "Never".to_string())
                />
            </div>
        </Card>

        // Languages
        <Card title="Languages".to_string() class="mt-4".to_string()>
            <div class="flex flex-wrap gap-2">
                {if repo_data.languages.is_empty() {
                    view! { <span class="text-muted-foreground text-sm">"None detected"</span> }.into_any()
                } else {
                    let langs = repo_data.languages.clone();
                    view! {
                        <span class="text-sm">{langs.join(", ")}</span>
                    }.into_any()
                }}
            </div>
        </Card>

        // Scan details
        <Card title="Scan Details".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow
                    label="Latest Commit"
                    value=repo_data
                        .latest_scan_commit
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string())
                />
                <DetailRow
                    label="Graph Hash"
                    value=repo_data
                        .latest_graph_hash
                        .clone()
                        .unwrap_or_else(|| "Unavailable".to_string())
                />
            </div>
        </Card>
    }
}
