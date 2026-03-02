//! Filesystem browser page

use crate::api::report_error_with_toast;
use crate::api::types::{
    FilesystemWriteFileRequest, UiCommitDiffResponse, UiCommitResponse, UiGitBranchInfo,
    UiGitStatusResponse,
};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, CodeEditor, EmptyState,
    EmptyStateVariant, ErrorDisplay, GitPanel, LoadingDisplay, PageBreadcrumbItem, PageScaffold,
    TreeView,
};
use crate::hooks::{use_api, LoadingState};
use adapteros_api_types::filesystem::{EntryType, FileBrowseResponse};
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn FileBrowser() -> impl IntoView {
    let api = use_api();
    let (current_path, set_current_path) = signal(String::new());
    let (last_requested_path, set_last_requested_path) = signal(String::new());
    let (show_hidden, set_show_hidden) = signal(false);
    let (browse_state, set_browse_state) =
        signal::<LoadingState<FileBrowseResponse>>(LoadingState::Loading);

    let (selected_file_path, set_selected_file_path) = signal::<Option<String>>(None);
    let (editor_content, set_editor_content) = signal(String::new());
    let (editor_loading, set_editor_loading) = signal(false);

    let (git_status, set_git_status) = signal::<Option<UiGitStatusResponse>>(None);
    let (git_branches, set_git_branches) = signal(Vec::<UiGitBranchInfo>::new());
    let (git_commits, set_git_commits) = signal(Vec::<UiCommitResponse>::new());
    let (git_diff_preview, set_git_diff_preview) = signal::<Option<UiCommitDiffResponse>>(None);
    let (git_busy, set_git_busy) = signal(false);
    let (git_panel_message, set_git_panel_message) = signal::<Option<String>>(None);

    let do_browse = {
        let api = Arc::clone(&api);
        move |path: String| {
            let api = Arc::clone(&api);
            let set_state = set_browse_state;
            let set_path = set_current_path;
            let set_last_path = set_last_requested_path;
            let hidden = show_hidden.get_untracked();
            set_last_path.set(path.clone());
            set_state.set(LoadingState::Loading);
            wasm_bindgen_futures::spawn_local(async move {
                match api.browse_filesystem(&path, hidden).await {
                    Ok(resp) => {
                        set_path.set(resp.path.clone());
                        set_state.set(LoadingState::Loaded(resp));
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to browse directory",
                            Some("/files"),
                            true,
                        );
                        set_state.set(LoadingState::Error(e));
                    }
                }
            });
        }
    };

    let refresh_git = {
        let api = Arc::clone(&api);
        move || {
            let api = Arc::clone(&api);
            let set_git_status = set_git_status;
            let set_git_branches = set_git_branches;
            let set_git_commits = set_git_commits;
            wasm_bindgen_futures::spawn_local(async move {
                match api.get_git_status().await {
                    Ok(status) => set_git_status.set(Some(status)),
                    Err(_) => set_git_status.set(None),
                }
                match api.list_git_branches_ui().await {
                    Ok(branches) => set_git_branches.set(branches),
                    Err(_) => set_git_branches.set(vec![]),
                }
                match api.list_recent_commits(10).await {
                    Ok(commits) => set_git_commits.set(commits),
                    Err(_) => set_git_commits.set(vec![]),
                }
            });
        }
    };

    let stage_file = {
        let api = Arc::clone(&api);
        let refresh_git = refresh_git.clone();
        move |path: String| {
            let api = Arc::clone(&api);
            let refresh_git = refresh_git.clone();
            set_git_busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.stage_git_file(&path).await {
                    Ok(_) => {
                        set_git_panel_message.set(Some(format!("Staged {}", path)));
                        refresh_git();
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to stage file", Some("/files"), true);
                        set_git_panel_message.set(Some("Unable to stage file".to_string()));
                    }
                }
                set_git_busy.set(false);
            });
        }
    };

    let unstage_file = {
        let api = Arc::clone(&api);
        let refresh_git = refresh_git.clone();
        move |path: String| {
            let api = Arc::clone(&api);
            let refresh_git = refresh_git.clone();
            set_git_busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.unstage_git_file(&path).await {
                    Ok(_) => {
                        set_git_panel_message.set(Some(format!("Unstaged {}", path)));
                        refresh_git();
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to unstage file", Some("/files"), true);
                        set_git_panel_message.set(Some("Unable to unstage file".to_string()));
                    }
                }
                set_git_busy.set(false);
            });
        }
    };

    let commit_changes = {
        let api = Arc::clone(&api);
        let refresh_git = refresh_git.clone();
        move |message: String| {
            if message.trim().is_empty() {
                return;
            }
            let api = Arc::clone(&api);
            let refresh_git = refresh_git.clone();
            set_git_busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.commit_git_changes(&message).await {
                    Ok(resp) => {
                        set_git_panel_message.set(Some(format!(
                            "Committed {}",
                            resp.commit_sha.chars().take(8).collect::<String>()
                        )));
                        refresh_git();
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Commit action failed", Some("/files"), true);
                        set_git_panel_message
                            .set(Some("Commit endpoint unavailable or failed".to_string()));
                    }
                }
                set_git_busy.set(false);
            });
        }
    };

    let select_commit_for_diff = {
        let api = Arc::clone(&api);
        move |sha: String| {
            let api = Arc::clone(&api);
            set_git_busy.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.get_commit_diff_ui(&sha).await {
                    Ok(diff) => {
                        set_git_diff_preview.set(Some(diff));
                        set_git_panel_message.set(Some(format!(
                            "Loaded diff for {}",
                            sha.chars().take(8).collect::<String>()
                        )));
                    }
                    Err(_) => {
                        set_git_diff_preview.set(None);
                        set_git_panel_message.set(Some(
                            "Diff preview not available on this backend".to_string(),
                        ));
                    }
                }
                set_git_busy.set(false);
            });
        }
    };

    {
        let browse = do_browse.clone();
        let refresh_git = refresh_git.clone();
        Effect::new(move || {
            browse(String::new());
            refresh_git();
        });
    }

    let navigate_to = {
        let browse = do_browse.clone();
        move |path: String| {
            set_selected_file_path.set(None);
            set_editor_content.set(String::new());
            browse(path);
        }
    };

    let go_up = {
        let browse = do_browse.clone();
        move || {
            if let LoadingState::Loaded(ref resp) = browse_state.get() {
                if let Some(ref parent) = resp.parent_path {
                    browse(parent.clone());
                }
            }
        }
    };

    let open_file = {
        let api = Arc::clone(&api);
        move |path: String| {
            let api = Arc::clone(&api);
            let set_selected = set_selected_file_path;
            let set_content = set_editor_content;
            let set_editor_loading = set_editor_loading;
            set_editor_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                match api.read_filesystem_file(&path).await {
                    Ok(resp) => {
                        set_selected.set(Some(resp.path));
                        set_content.set(resp.content);
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to load file", Some("/files"), true);
                    }
                }
                set_editor_loading.set(false);
            });
        }
    };

    let save_file = {
        let api = Arc::clone(&api);
        let refresh_git = refresh_git.clone();
        move |content: String| {
            let Some(path) = selected_file_path.get_untracked() else {
                return;
            };
            let api = Arc::clone(&api);
            let set_editor_loading = set_editor_loading;
            let refresh_git = refresh_git.clone();
            set_editor_loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let request = FilesystemWriteFileRequest { path, content };
                match api.write_filesystem_file(&request).await {
                    Ok(_) => {
                        refresh_git();
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to save file", Some("/files"), true);
                    }
                }
                set_editor_loading.set(false);
            });
        }
    };
    let save_file_cb = Callback::new(move |content: String| save_file(content));

    let toggle_hidden = {
        let browse = do_browse.clone();
        Callback::new(move |_: ()| {
            set_show_hidden.update(|h| *h = !*h);
            let path = current_path.get_untracked();
            browse(path);
        })
    };

    view! {
        <PageScaffold
            title="Files"
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Org"),
                PageBreadcrumbItem::current("Files"),
            ]
        >
            <div class="filesystem-browser filesystem-workspace">
                {move || {
                    let retry_browse = do_browse.clone();
                    let retry_path = last_requested_path;
                    match browse_state.get() {
                        LoadingState::Loading => {
                            view! { <LoadingDisplay message="Loading directory..."/> }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| {
                                        retry_browse(retry_path.get_untracked())
                                    })
                                />
                            }
                            .into_any()
                        }
                        LoadingState::Loaded(resp) => {
                            let nav = navigate_to.clone();
                            let open_file_cb = open_file.clone();
                            let go_up_click = go_up.clone();
                            let entries = resp.entries.clone();
                            let file_count = entries
                                .iter()
                                .filter(|entry| entry.entry_type == EntryType::File)
                                .count();
                            let dir_count = entries
                                .iter()
                                .filter(|entry| entry.entry_type == EntryType::Directory)
                                .count();
                            let on_refresh_cb = {
                                let refresh_git = refresh_git.clone();
                                Callback::new(move |_| refresh_git())
                            };
                            let on_stage_cb = {
                                let stage_file = stage_file.clone();
                                Callback::new(move |path| stage_file(path))
                            };
                            let on_unstage_cb = {
                                let unstage_file = unstage_file.clone();
                                Callback::new(move |path| unstage_file(path))
                            };
                            let on_commit_cb = {
                                let commit_changes = commit_changes.clone();
                                Callback::new(move |message| commit_changes(message))
                            };
                            let on_select_commit_cb = {
                                let select_commit_for_diff = select_commit_for_diff.clone();
                                Callback::new(move |sha| select_commit_for_diff(sha))
                            };

                            view! {
                                <div class="filesystem-toolbar">
                                    <div class="filesystem-path">
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            disabled=resp.parent_path.is_none()
                                            on_click=Callback::new(move |_| go_up_click())
                                        >
                                            "Up"
                                        </Button>
                                        <code class="filesystem-current-path">{resp.path.clone()}</code>
                                    </div>
                                    <div class="filesystem-actions">
                                        <Badge variant=BadgeVariant::Secondary>{format!("{} dirs", dir_count)}</Badge>
                                        <Badge variant=BadgeVariant::Default>{format!("{} files", file_count)}</Badge>
                                        <Button
                                            variant=if show_hidden.get() { ButtonVariant::Secondary } else { ButtonVariant::Ghost }
                                            size=ButtonSize::Sm
                                            on_click=toggle_hidden
                                        >
                                            {move || if show_hidden.get() { "Hide Hidden" } else { "Show Hidden" }}
                                        </Button>
                                    </div>
                                </div>

                                <div class="filesystem-roots">
                                    {resp
                                        .allowed_roots
                                        .iter()
                                        .map(|root| {
                                            let root_path = root.clone();
                                            let root_display = root.rsplit('/').next().unwrap_or(root).to_string();
                                            let nav = nav.clone();
                                            view! {
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new(move |_| nav(root_path.clone()))
                                                >
                                                    {root_display}
                                                </Button>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </div>

                                {if entries.is_empty() {
                                    view! {
                                        <EmptyState
                                            variant=EmptyStateVariant::Empty
                                            title="Empty directory"
                                            description="This directory has no entries."
                                        />
                                    }
                                    .into_any()
                                } else {
                                    view! {
                                        <div class="filesystem-grid">
                                            <TreeView
                                                entries=entries
                                                selected_path=selected_file_path.into()
                                                on_open_dir=Callback::new(move |path| nav(path))
                                                on_open_file=Callback::new(move |path| open_file_cb(path))
                                            />
                                            <CodeEditor
                                                active_path=selected_file_path.into()
                                                content=editor_content.into()
                                                is_loading=editor_loading.into()
                                                on_content_change=Callback::new(move |value| set_editor_content.set(value))
                                                on_save=save_file_cb.clone()
                                            />
                                            <GitPanel
                                                status=git_status.get()
                                                branches=git_branches.get()
                                                recent_commits=git_commits.get()
                                                diff_preview=git_diff_preview.get()
                                                is_busy=git_busy.get()
                                                panel_message=git_panel_message.get()
                                                on_refresh=on_refresh_cb
                                                on_stage=on_stage_cb
                                                on_unstage=on_unstage_cb
                                                on_commit=on_commit_cb
                                                on_select_commit=on_select_commit_cb
                                            />
                                        </div>
                                    }
                                    .into_any()
                                }}
                            }
                            .into_any()
                        }
                        _ => view! { <LoadingDisplay message="Loading directory..."/> }.into_any(),
                    }
                }}
            </div>
        </PageScaffold>
    }
}
