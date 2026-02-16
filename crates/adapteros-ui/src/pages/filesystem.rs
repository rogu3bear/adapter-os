//! Filesystem browser page

use crate::api::report_error_with_toast;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, EmptyState, EmptyStateVariant,
    ErrorDisplay, LoadingDisplay, PageBreadcrumbItem, PageScaffold, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api, LoadingState};
use crate::utils::format_bytes;
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

    {
        let browse = do_browse.clone();
        Effect::new(move || {
            browse(String::new());
        });
    }

    let navigate_to = {
        let browse = do_browse.clone();
        move |path: String| {
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
            <div class="filesystem-browser">
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
                        }.into_any()
                    }
                    LoadingState::Loaded(resp) => {
                        let resp_for_nav = resp.clone();
                        let resp_for_table = resp.clone();
                        let resp_for_roots = resp.clone();
                        let nav = navigate_to.clone();
                        let nav2 = navigate_to.clone();
                        let go_up = go_up.clone();
                        view! {
                            <div class="filesystem-toolbar">
                                <div class="filesystem-path">
                                    {move || {
                                        let go_up = go_up.clone();
                                        let has_parent = resp_for_nav.parent_path.is_some();
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                size=ButtonSize::Sm
                                                disabled=!has_parent
                                                on_click=Callback::new(move |_| go_up())
                                            >
                                                "Up"
                                            </Button>
                                            <code class="filesystem-current-path">
                                                {resp_for_nav.path.clone()}
                                            </code>
                                        }
                                    }}
                                </div>
                                <div class="filesystem-actions">
                                    <Button
                                        variant=if show_hidden.get() {
                                            ButtonVariant::Secondary
                                        } else {
                                            ButtonVariant::Ghost
                                        }
                                        size=ButtonSize::Sm
                                        on_click=toggle_hidden
                                    >
                                        {move || {
                                            if show_hidden.get() {
                                                "Hide Hidden"
                                            } else {
                                                "Show Hidden"
                                            }
                                        }}
                                    </Button>
                                </div>
                            </div>
                            <div class="filesystem-roots">
                                {resp_for_roots
                                    .allowed_roots
                                    .iter()
                                    .map(|root| {
                                        let root_path = root.clone();
                                        let root_display = root
                                            .rsplit('/')
                                            .next()
                                            .unwrap_or(root)
                                            .to_string();
                                        let nav = nav.clone();
                                        view! {
                                            <Button
                                                variant=ButtonVariant::Outline
                                                size=ButtonSize::Sm
                                                on_click=Callback::new(move |_| {
                                                    nav(root_path.clone())
                                                })
                                            >
                                                {root_display.clone()}
                                            </Button>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </div>
                            {if resp_for_table.entries.is_empty() {
                                view! {
                                    <EmptyState
                                        variant=EmptyStateVariant::Empty
                                        title="Empty directory"
                                        description="This directory has no entries."
                                    />
                                }
                                    .into_any()
                            } else {
                                let nav3 = nav2.clone();
                                view! {
                                    <Card>
                                        <Table>
                                            <TableHead>
                                                <TableRow>
                                                    <TableHeader>"Name"</TableHeader>
                                                    <TableHeader>"Type"</TableHeader>
                                                    <TableHeader>"Size"</TableHeader>
                                                    <TableHeader>"Modified"</TableHeader>
                                                </TableRow>
                                            </TableHead>
                                            <TableBody>
                                                {resp_for_table
                                                    .entries
                                                    .iter()
                                                    .map(|entry| {
                                                        let entry_path = entry.path.clone();
                                                        let name = entry.name.clone();
                                                        let is_dir = entry.entry_type
                                                            == EntryType::Directory;
                                                        let type_label = match entry.entry_type {
                                                            EntryType::Directory => "dir",
                                                            EntryType::File => "file",
                                                            EntryType::Symlink => "link",
                                                        };
                                                        let badge_variant = match entry.entry_type {
                                                            EntryType::Directory => {
                                                                BadgeVariant::Default
                                                            }
                                                            EntryType::File => {
                                                                BadgeVariant::Secondary
                                                            }
                                                            EntryType::Symlink => {
                                                                BadgeVariant::Warning
                                                            }
                                                        };
                                                        let size_str = entry
                                                            .size_bytes
                                                            .map(|b| format_bytes(b as i64))
                                                            .unwrap_or_default();
                                                        let modified_str = entry
                                                            .modified_at
                                                            .clone()
                                                            .unwrap_or_default();
                                                        let nav_click = nav3.clone();
                                                        view! {
                                                            <TableRow>
                                                                <TableCell>
                                                                    {if is_dir {
                                                                        let p = entry_path.clone();
                                                                        view! {
                                                                            <button
                                                                                type="button"
                                                                                class="link link-default filesystem-dir-link"
                                                                                style="background: transparent; border: 0; padding: 0; cursor: pointer;"
                                                                                on:click=move |_| {
                                                                                    nav_click(p.clone());
                                                                                }
                                                                            >
                                                                                {name.clone()}
                                                                            </button>
                                                                        }
                                                                            .into_any()
                                                                    } else {
                                                                        view! {
                                                                            <span>{name.clone()}</span>
                                                                        }
                                                                            .into_any()
                                                                    }}
                                                                </TableCell>
                                                                <TableCell>
                                                                    <Badge variant=badge_variant>
                                                                        {type_label}
                                                                    </Badge>
                                                                </TableCell>
                                                                <TableCell>{size_str}</TableCell>
                                                                <TableCell>
                                                                    <span class="filesystem-modified">
                                                                        {modified_str}
                                                                    </span>
                                                                </TableCell>
                                                            </TableRow>
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()}
                                            </TableBody>
                                        </Table>
                                    </Card>
                                }
                                    .into_any()
                            }}
                        }
                            .into_any()
                    }
                    _ => view! { <LoadingDisplay message="Loading directory..."/> }.into_any(),
                }}}
            </div>
        </PageScaffold>
    }
}
