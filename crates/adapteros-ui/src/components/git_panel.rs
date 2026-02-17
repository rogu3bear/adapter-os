use crate::api::types::{
    UiCommitDiffResponse, UiCommitResponse, UiGitBranchInfo, UiGitStatusResponse,
};
use leptos::prelude::*;

#[component]
pub fn GitPanel(
    status: Option<UiGitStatusResponse>,
    branches: Vec<UiGitBranchInfo>,
    recent_commits: Vec<UiCommitResponse>,
    diff_preview: Option<UiCommitDiffResponse>,
    is_busy: bool,
    panel_message: Option<String>,
    on_refresh: Callback<()>,
    on_stage: Callback<String>,
    on_unstage: Callback<String>,
    on_commit: Callback<String>,
    on_select_commit: Callback<String>,
) -> impl IntoView {
    let (commit_message, set_commit_message) = signal(String::new());
    let status = status.unwrap_or_default();
    let changed_total =
        status.modified_files.len() + status.staged_files.len() + status.untracked_files.len();
    let diff = diff_preview;

    view! {
        <div class="files-git-panel">
            <div class="files-git-header">"Git"</div>
            <div class="files-git-summary">
                <span class="files-git-branch">{if status.branch.is_empty() { "-".to_string() } else { status.branch }}</span>
                <span class="files-git-count">{format!("{} changed", changed_total)}</span>
            </div>
            <div class="files-git-toolbar">
                <button class="btn btn-ghost btn-sm" disabled=is_busy on:click=move |_| on_refresh.run(())>
                    "Refresh"
                </button>
            </div>
            {if let Some(message) = panel_message {
                view! { <div class="files-git-note">{message}</div> }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            <div class="files-git-section">
                <div class="files-git-section-title">"Staged"</div>
                {if status.staged_files.is_empty() {
                    view! { <div class="files-git-empty">"No staged files"</div> }.into_any()
                } else {
                    view! {
                        <ul class="files-git-list">
                            {status
                                .staged_files
                                .into_iter()
                                .map(|path| {
                                    let unstage_path = path.clone();
                                    view! {
                                        <li class="files-git-list-item">
                                            <span>{path}</span>
                                            <button
                                                class="btn btn-ghost btn-sm"
                                                disabled=is_busy
                                                on:click=move |_| on_unstage.run(unstage_path.clone())
                                            >
                                                "Unstage"
                                            </button>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                    .into_any()
                }}
            </div>

            <div class="files-git-section">
                <div class="files-git-section-title">"Modified"</div>
                {if status.modified_files.is_empty() {
                    view! { <div class="files-git-empty">"No modified files"</div> }.into_any()
                } else {
                    view! {
                        <ul class="files-git-list">
                            {status
                                .modified_files
                                .into_iter()
                                .map(|path| {
                                    let stage_path = path.clone();
                                    view! {
                                        <li class="files-git-list-item">
                                            <span>{path}</span>
                                            <button
                                                class="btn btn-secondary btn-sm"
                                                disabled=is_busy
                                                on:click=move |_| on_stage.run(stage_path.clone())
                                            >
                                                "Stage"
                                            </button>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                    .into_any()
                }}
            </div>
            <div class="files-git-section">
                <div class="files-git-section-title">"Untracked"</div>
                {if status.untracked_files.is_empty() {
                    view! { <div class="files-git-empty">"No untracked files"</div> }.into_any()
                } else {
                    view! {
                        <ul class="files-git-list">
                            {status
                                .untracked_files
                                .into_iter()
                                .map(|path| {
                                    let stage_path = path.clone();
                                    view! {
                                        <li class="files-git-list-item">
                                            <span>{path}</span>
                                            <button
                                                class="btn btn-secondary btn-sm"
                                                disabled=is_busy
                                                on:click=move |_| on_stage.run(stage_path.clone())
                                            >
                                                "Stage"
                                            </button>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                    .into_any()
                }}
            </div>
            <div class="files-git-section">
                <div class="files-git-section-title">"Commit"</div>
                <textarea
                    class="files-git-commit-input"
                    prop:value=move || commit_message.get()
                    on:input=move |ev| set_commit_message.set(event_target_value(&ev))
                    placeholder="Commit message"
                />
                <button
                    class="btn btn-primary btn-sm files-git-commit-btn"
                    disabled=move || is_busy || commit_message.get().trim().is_empty()
                    on:click=move |_| on_commit.run(commit_message.get().trim().to_string())
                >
                    "Commit"
                </button>
            </div>
            <div class="files-git-section">
                <div class="files-git-section-title">"Recent commits"</div>
                {if recent_commits.is_empty() {
                    view! { <div class="files-git-empty">"No commits available"</div> }.into_any()
                } else {
                    view! {
                        <ul class="files-git-list">
                            {recent_commits
                                .into_iter()
                                .map(|commit| {
                                    let sha = commit.sha.clone();
                                    view! {
                                        <li class="files-git-commit-row">
                                            <button
                                                class="btn btn-link btn-sm files-git-commit-link"
                                                disabled=is_busy
                                                on:click=move |_| on_select_commit.run(sha.clone())
                                            >
                                                {commit.message}
                                            </button>
                                            <span class="files-git-meta">{commit.sha.chars().take(8).collect::<String>()}</span>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                    .into_any()
                }}
            </div>
            {if let Some(preview) = diff {
                view! {
                    <div class="files-git-section">
                        <div class="files-git-section-title">"Diff preview"</div>
                        <div class="files-git-meta">
                            {format!(
                                "{} files, +{}, -{}",
                                preview.stats.files_changed, preview.stats.insertions, preview.stats.deletions
                            )}
                        </div>
                        <pre class="files-git-diff">{preview.diff}</pre>
                    </div>
                }
                    .into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            <div class="files-git-section">
                <div class="files-git-section-title">"Branches"</div>
                {if branches.is_empty() {
                    view! { <div class="files-git-empty">"No active branches"</div> }.into_any()
                } else {
                    view! {
                        <ul class="files-git-list files-git-branches">
                            {branches
                                .into_iter()
                                .map(|branch| {
                                    view! {
                                        <li>
                                            <span>{branch.branch_name}</span>
                                            <span class="files-git-meta">{branch.adapter_id}</span>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                    .into_any()
                }}
            </div>
        </div>
    }
}
