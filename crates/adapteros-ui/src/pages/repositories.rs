//! Repositories management page
//!
//! Complete repository management with list view, detail panel, sync status, and publish workflow.

use crate::api::{
    ApiClient, PublishAdapterRequest, RegisterRepositoryRequest, RepositoryAdapter,
    RepositoryResponse, RepositoryVersion,
};
use crate::components::{Badge, BadgeVariant, Card, Input, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::sync::Arc;

/// Repositories list page
#[component]
pub fn Repositories() -> impl IntoView {
    // Selected repository ID for detail panel
    let selected_repo_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());

    // Dialog states
    let register_dialog_open = RwSignal::new(false);

    // Fetch repositories
    let (repos, refetch_repos) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_repositories().await
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
                    <h1 class="text-3xl font-bold tracking-tight">"Repositories"</h1>
                    <div class="flex items-center gap-2">
                        <StatusFilter filter=status_filter/>
                        <button
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                            on:click=move |_| register_dialog_open.set(true)
                        >
                            "Register Repository"
                        </button>
                        <button
                            class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                            on:click=move |_| refetch_repos()
                        >
                            "Refresh"
                        </button>
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
                            let filter = status_filter.get();
                            let filtered_repos: Vec<_> = data.repositories.iter()
                                .filter(|r| filter.is_empty() || r.status == filter)
                                .cloned()
                                .collect();
                            view! {
                                <RepositoryList
                                    repos=filtered_repos
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

/// Repository list table
#[component]
fn RepositoryList(
    repos: Vec<RepositoryResponse>,
    selected_id: RwSignal<Option<String>>,
) -> impl IntoView {
    if repos.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <p class="text-muted-foreground">"No repositories found. Register one to get started."</p>
                </div>
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Repository"</TableHead>
                        <TableHead>"Languages"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Files"</TableHead>
                        <TableHead>"Updated"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {repos
                        .into_iter()
                        .map(|repo| {
                            let repo_id = repo.id.clone();
                            let repo_id_for_click = repo_id.clone();
                            let languages_display = if repo.languages.len() > 3 {
                                format!("{} +{}", repo.languages[..3].join(", "), repo.languages.len() - 3)
                            } else {
                                repo.languages.join(", ")
                            };

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.get().as_ref() == Some(&repo_id)
                                    on:click=move |_| selected_id.set(Some(repo_id_for_click.clone()))
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{repo.repo_id.clone()}</p>
                                            <p class="text-xs text-muted-foreground truncate max-w-xs">
                                                {repo.path.clone()}
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm">{languages_display}</span>
                                    </TableCell>
                                    <TableCell>
                                        <RepoStatusBadge status=repo.status.clone()/>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {repo.file_count.map(|c| format_number(c as u64)).unwrap_or_else(|| "-".to_string())}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_date(&repo.updated_at)}
                                        </span>
                                    </TableCell>
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Repository status badge
#[component]
fn RepoStatusBadge(status: String) -> impl IntoView {
    let (variant, label) = match status.as_str() {
        "active" => (BadgeVariant::Success, "Active"),
        "scanning" => (BadgeVariant::Default, "Scanning"),
        "pending" => (BadgeVariant::Secondary, "Pending"),
        "error" => (BadgeVariant::Destructive, "Error"),
        "syncing" => (BadgeVariant::Default, "Syncing"),
        _ => (BadgeVariant::Secondary, "Unknown"),
    };

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}

/// Repository detail panel (embedded in split view)
#[component]
fn RepositoryDetailPanel(
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
                    class="text-muted-foreground hover:text-foreground"
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
fn RepositoryDetailStandalone(repo_id: String) -> impl IntoView {
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
                            class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-3 py-1.5 text-sm font-medium hover:bg-accent disabled:opacity-50"
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
                            class="inline-flex items-center gap-2 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90"
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
fn DetailRow(
    label: &'static str,
    #[prop(into)] value: String,
) -> impl IntoView {
    view! {
        <div class="flex justify-between">
            <span class="text-muted-foreground">{label}</span>
            <span class="font-medium truncate max-w-xs">{value}</span>
        </div>
    }
}

/// Register repository dialog
#[component]
fn RegisterRepositoryDialog(open: RwSignal<bool>) -> impl IntoView {
    // Form state
    let repo_id = RwSignal::new(String::new());
    let path = RwSignal::new(String::new());
    let languages = RwSignal::new(String::new());
    let default_branch = RwSignal::new("main".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    view! {
        <Show when=move || open.get() fallback=|| view! {}>
            // Backdrop
            <div
                class="fixed inset-0 z-50 bg-black/80"
                on:click=move |_| {
                    open.set(false);
                    error.set(None);
                }
            />

            // Dialog
            <div class="fixed left-[50%] top-[50%] z-50 w-full max-w-lg translate-x-[-50%] translate-y-[-50%] border bg-background p-6 shadow-lg sm:rounded-lg">
                // Header
                <div class="flex items-center justify-between mb-4">
                    <div>
                        <h2 class="text-lg font-semibold">"Register Repository"</h2>
                        <p class="text-sm text-muted-foreground">"Add a codebase for adapter training"</p>
                    </div>
                    <button
                        class="rounded-sm opacity-70 hover:opacity-100"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M18 6 6 18"/>
                            <path d="m6 6 12 12"/>
                        </svg>
                    </button>
                </div>

                // Error message
                {move || error.get().map(|e| view! {
                    <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Form
                <div class="space-y-4">
                    <Input
                        value=repo_id
                        label="Repository ID".to_string()
                        placeholder="my-project".to_string()
                    />
                    <Input
                        value=path
                        label="Path".to_string()
                        placeholder="/path/to/repository".to_string()
                    />
                    <Input
                        value=languages
                        label="Languages (comma-separated)".to_string()
                        placeholder="rust, python, typescript".to_string()
                    />
                    <Input
                        value=default_branch
                        label="Default Branch".to_string()
                        placeholder="main".to_string()
                    />
                </div>

                // Footer
                <div class="flex justify-end gap-2 mt-6">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                        disabled=submitting.get()
                        on:click=move |_| {
                            // Validate
                            let rid = repo_id.get();
                            let p = path.get();

                            if rid.is_empty() {
                                error.set(Some("Repository ID is required".to_string()));
                                return;
                            }
                            if p.is_empty() {
                                error.set(Some("Path is required".to_string()));
                                return;
                            }

                            error.set(None);
                            submitting.set(true);

                            let langs: Vec<String> = languages
                                .get()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let branch = default_branch.get();

                            wasm_bindgen_futures::spawn_local(async move {
                                let client = ApiClient::new();

                                let request = RegisterRepositoryRequest {
                                    repo_id: rid,
                                    path: p,
                                    languages: langs,
                                    default_branch: branch,
                                };

                                match client.register_repository(&request).await {
                                    Ok(_) => {
                                        submitting.set(false);
                                        // Reset form
                                        repo_id.set(String::new());
                                        path.set(String::new());
                                        languages.set(String::new());
                                        default_branch.set("main".to_string());
                                        open.set(false);
                                    }
                                    Err(e) => {
                                        error.set(Some(e.to_string()));
                                        submitting.set(false);
                                    }
                                }
                            });
                        }
                    >
                        {move || if submitting.get() { "Registering..." } else { "Register" }}
                    </button>
                </div>
            </div>
        </Show>
    }
}

/// Publish adapter dialog
#[component]
fn PublishAdapterDialog(
    open: RwSignal<bool>,
    #[prop(into)] repo_id: String,
) -> impl IntoView {
    // Form state
    let adapter_name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let version = RwSignal::new("1.0.0".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let repo_id_for_submit = repo_id.clone();

    view! {
        <Show when=move || open.get() fallback=|| view! {}>
            // Backdrop
            <div
                class="fixed inset-0 z-50 bg-black/80"
                on:click=move |_| {
                    open.set(false);
                    error.set(None);
                }
            />

            // Dialog
            <div class="fixed left-[50%] top-[50%] z-50 w-full max-w-lg translate-x-[-50%] translate-y-[-50%] border bg-background p-6 shadow-lg sm:rounded-lg">
                // Header
                <div class="flex items-center justify-between mb-4">
                    <div>
                        <h2 class="text-lg font-semibold">"Publish Adapter"</h2>
                        <p class="text-sm text-muted-foreground">"Create an adapter from this repository"</p>
                    </div>
                    <button
                        class="rounded-sm opacity-70 hover:opacity-100"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M18 6 6 18"/>
                            <path d="m6 6 12 12"/>
                        </svg>
                    </button>
                </div>

                // Error message
                {move || error.get().map(|e| view! {
                    <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{e}</p>
                    </div>
                })}

                // Form
                <div class="space-y-4">
                    <Input
                        value=adapter_name
                        label="Adapter Name".to_string()
                        placeholder="my-project-adapter".to_string()
                    />
                    <Input
                        value=description
                        label="Description (optional)".to_string()
                        placeholder="Adapter for my project codebase".to_string()
                    />
                    <Input
                        value=version
                        label="Version".to_string()
                        placeholder="1.0.0".to_string()
                    />
                </div>

                // Footer
                <div class="flex justify-end gap-2 mt-6">
                    <button
                        class="inline-flex items-center gap-2 rounded-md border border-input bg-background px-4 py-2 text-sm font-medium hover:bg-accent"
                        on:click=move |_| {
                            open.set(false);
                            error.set(None);
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        class="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                        disabled=submitting.get()
                        on:click={
                            let rid = repo_id_for_submit.clone();
                            move |_| {
                                // Validate
                                let name = adapter_name.get();

                                if name.is_empty() {
                                    error.set(Some("Adapter name is required".to_string()));
                                    return;
                                }

                                error.set(None);
                                submitting.set(true);

                                let rid_inner = rid.clone();
                                let desc = description.get();
                                let ver = version.get();

                                wasm_bindgen_futures::spawn_local(async move {
                                    let client = ApiClient::new();

                                    let request = PublishAdapterRequest {
                                        repo_id: rid_inner.clone(),
                                        adapter_name: name,
                                        description: if desc.is_empty() { None } else { Some(desc) },
                                        version: if ver.is_empty() { None } else { Some(ver) },
                                    };

                                    match client.publish_repository_adapter(&rid_inner, &request).await {
                                        Ok(_) => {
                                            submitting.set(false);
                                            // Reset form
                                            adapter_name.set(String::new());
                                            description.set(String::new());
                                            version.set("1.0.0".to_string());
                                            open.set(false);
                                        }
                                        Err(e) => {
                                            error.set(Some(e.to_string()));
                                            submitting.set(false);
                                        }
                                    }
                                });
                            }
                        }
                    >
                        {move || if submitting.get() { "Publishing..." } else { "Publish" }}
                    </button>
                </div>
            </div>
        </Show>
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Format a date string for display
fn format_date(date_str: &str) -> String {
    if date_str.len() >= 16 {
        format!("{} {}", &date_str[0..10], &date_str[11..16])
    } else {
        date_str.to_string()
    }
}

/// Format a large number with commas
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
