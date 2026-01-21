//! Repository detail components

use super::dialogs::PublishAdapterDialog;
use super::helpers::{format_date, format_number};
use super::list::RepoStatusBadge;
use crate::api::{ApiClient, RepositoryAdapter, RepositoryVersion};
use crate::components::{
    Badge, BadgeVariant, Card, Spinner, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
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

    // Publish dialog state
    let publish_dialog_open = RwSignal::new(false);

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
                        let is_scanning = data.repository.status == "scanning" || data.repository.status == "syncing";
                        let repo_data = data.clone();
                        let repo_id_for_publish = data.repository.id.clone();

                        view! {
                            <RepositoryContent
                                repo_data=repo_data
                                is_scanning=is_scanning
                                syncing=syncing
                                repo_id_sync=repo_id_sync
                                publish_dialog_open=publish_dialog_open
                                repo_id_for_publish=repo_id_for_publish
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

    // Publish dialog state
    let publish_dialog_open = RwSignal::new(false);

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
                        let is_scanning = data.repository.status == "scanning" || data.repository.status == "syncing";
                        let repo_data = data.clone();
                        let repo_id_for_publish = data.repository.id.clone();

                        view! {
                            <RepositoryContent
                                repo_data=repo_data
                                is_scanning=is_scanning
                                syncing=syncing
                                repo_id_sync=repo_id_sync
                                publish_dialog_open=publish_dialog_open
                                repo_id_for_publish=repo_id_for_publish
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
    repo_data: crate::api::RepositoryDetailResponse,
    is_scanning: bool,
    syncing: RwSignal<bool>,
    repo_id_sync: String,
    publish_dialog_open: RwSignal<bool>,
    repo_id_for_publish: String,
) -> impl IntoView {
    view! {
        // Status and actions
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <RepoStatusBadge status=repo_data.repository.status.clone()/>
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
                                        let _ = client.sync_repository(&repo_id).await;
                                        syncing.set(false);
                                    });
                                }
                            }
                        >
                            {move || if syncing.get() || is_scanning { "Syncing..." } else { "Sync Now" }}
                        </button>
                        <button
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                            on:click=move |_| publish_dialog_open.set(true)
                        >
                            "Publish Adapter"
                        </button>
                    </div>
                </div>
            </div>
        </Card>

        // Basic info
        <Card title="Information".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow label="Repository ID" value=repo_data.repository.repo_id.clone()/>
                <DetailRow label="Path" value=repo_data.repository.path.clone()/>
                <DetailRow label="Default Branch" value=repo_data.repository.default_branch.clone()/>
                <DetailRow label="Created" value=format_date(&repo_data.repository.created_at)/>
                <DetailRow label="Updated" value=format_date(&repo_data.repository.updated_at)/>
            </div>
        </Card>

        // Languages and frameworks
        <Card title="Languages & Frameworks".to_string() class="mt-4".to_string()>
            <div class="space-y-3">
                <div>
                    <p class="text-sm text-muted-foreground mb-2">"Languages"</p>
                    <div class="flex flex-wrap gap-2">
                        {if repo_data.repository.languages.is_empty() {
                            view! { <span class="text-muted-foreground text-sm">"None detected"</span> }.into_any()
                        } else {
                            let langs = repo_data.repository.languages.clone();
                            view! {
                                <span class="text-sm">{langs.join(", ")}</span>
                            }.into_any()
                        }}
                    </div>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground mb-2">"Frameworks"</p>
                    <div class="flex flex-wrap gap-2">
                        {if repo_data.repository.frameworks.is_empty() {
                            view! { <span class="text-muted-foreground text-sm">"None detected"</span> }.into_any()
                        } else {
                            let fws = repo_data.repository.frameworks.clone();
                            view! {
                                <span class="text-sm">{fws.join(", ")}</span>
                            }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </Card>

        // Statistics
        <Card title="Statistics".to_string() class="mt-4".to_string()>
            <div class="grid gap-4 md:grid-cols-2">
                <div>
                    <p class="text-sm text-muted-foreground">"Files"</p>
                    <p class="text-2xl font-bold">
                        {repo_data.repository.file_count.map(|c| format_number(c as u64)).unwrap_or_else(|| "-".to_string())}
                    </p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Symbols"</p>
                    <p class="text-2xl font-bold">
                        {repo_data.repository.symbol_count.map(|c| format_number(c as u64)).unwrap_or_else(|| "-".to_string())}
                    </p>
                </div>
            </div>
        </Card>

        // Adapters
        <Card title="Adapters".to_string() class="mt-4".to_string()>
            <AdaptersList adapters=repo_data.adapters.clone()/>
        </Card>

        // Versions
        <Card title="Versions".to_string() class="mt-4".to_string()>
            <VersionsList versions=repo_data.versions.clone()/>
        </Card>

        // Publish dialog
        <PublishAdapterDialog
            open=publish_dialog_open
            repo_id=repo_id_for_publish
        />
    }
}

/// Adapters list component
#[component]
fn AdaptersList(adapters: Vec<RepositoryAdapter>) -> impl IntoView {
    if adapters.is_empty() {
        return view! {
            <p class="text-muted-foreground">"No adapters published yet."</p>
        }
        .into_any();
    }

    view! {
        <Table>
            <TableHeader>
                <TableRow>
                    <TableHead>"Name"</TableHead>
                    <TableHead>"Version"</TableHead>
                    <TableHead>"Status"</TableHead>
                    <TableHead>"Created"</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {adapters
                    .into_iter()
                    .map(|adapter| {
                        let variant = match adapter.status.as_str() {
                            "active" => BadgeVariant::Success,
                            "pending" => BadgeVariant::Secondary,
                            _ => BadgeVariant::Secondary,
                        };
                        view! {
                            <TableRow>
                                <TableCell>
                                    <a
                                        href=format!("/adapters/{}", adapter.adapter_id)
                                        class="font-medium hover:underline"
                                    >
                                        {adapter.name}
                                    </a>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=BadgeVariant::Secondary>
                                        {adapter.version}
                                    </Badge>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=variant>
                                        {adapter.status}
                                    </Badge>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">
                                        {format_date(&adapter.created_at)}
                                    </span>
                                </TableCell>
                            </TableRow>
                        }
                    })
                    .collect::<Vec<_>>()}
            </TableBody>
        </Table>
    }
    .into_any()
}

/// Versions list component
#[component]
fn VersionsList(versions: Vec<RepositoryVersion>) -> impl IntoView {
    if versions.is_empty() {
        return view! {
            <p class="text-muted-foreground">"No versions tracked yet."</p>
        }
        .into_any();
    }

    view! {
        <Table>
            <TableHeader>
                <TableRow>
                    <TableHead>"Version"</TableHead>
                    <TableHead>"Commit"</TableHead>
                    <TableHead>"Adapter"</TableHead>
                    <TableHead>"Created"</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {versions
                    .into_iter()
                    .map(|version| {
                        let commit_short: String = version.commit_hash.chars().take(8).collect();
                        let adapter_link = version.adapter_id.clone();
                        view! {
                            <TableRow>
                                <TableCell>
                                    <span class="font-medium">{version.version}</span>
                                </TableCell>
                                <TableCell>
                                    <span class="font-mono text-xs">{commit_short}</span>
                                </TableCell>
                                <TableCell>
                                    {match adapter_link {
                                        Some(id) => view! {
                                            <a
                                                href=format!("/adapters/{}", id)
                                                class="text-primary hover:underline"
                                            >
                                                "View"
                                            </a>
                                        }.into_any(),
                                        None => view! {
                                            <span class="text-muted-foreground">"-"</span>
                                        }.into_any()
                                    }}
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">
                                        {format_date(&version.created_at)}
                                    </span>
                                </TableCell>
                            </TableRow>
                        }
                    })
                    .collect::<Vec<_>>()}
            </TableBody>
        </Table>
    }
    .into_any()
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
