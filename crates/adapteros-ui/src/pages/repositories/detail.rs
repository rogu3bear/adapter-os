//! Repository detail components

use super::helpers::format_date;
use super::list::RepoStatusBadge;
use crate::api::{ApiClient, RepositoryDetailResponse, ScanRepositoryRequest};
use crate::components::{Card, Spinner};
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
                <h2 class="text-xl font-semibold">"Repository Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                    aria-label="Close details"
                    type="button"
                    on:click=move |_| selected_repo_id.set(None)
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
                let repo_id_sync = repo_id_for_sync.clone();
                match repo.get() {
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
                match repo.get() {
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
    let tenant_id = move || auth_state.get().user().map(|u| u.tenant_id.clone());

    view! {
        // Status and actions
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <RepoStatusBadge status=repo_data.status.clone()/>
                    <div class="flex items-center gap-2">
                        <button
                            class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-3 py-1.5 text-sm font-medium hover:bg-accent disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                            disabled=syncing.get() || is_scanning
                            on:click={
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
                                        let _ = client.scan_repository(&request).await;
                                        syncing.set(false);
                                    });
                                }
                            }
                        >
                            {move || if syncing.get() || is_scanning { "Syncing..." } else { "Sync Now" }}
                        </button>
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
                <DetailRow label="Created" value=format_date(&repo_data.created_at)/>
                <DetailRow label="Updated" value=format_date(&repo_data.updated_at)/>
                <DetailRow
                    label="Latest Scan"
                    value=repo_data
                        .latest_scan_at
                        .as_deref()
                        .map(format_date)
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

/// Detail row component
#[component]
fn DetailRow(label: &'static str, #[prop(into)] value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between">
            <span class="text-muted-foreground">{label}</span>
            <span class="font-medium truncate max-w-xs">{value}</span>
        </div>
    }
}
