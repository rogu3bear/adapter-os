//! Search results components
//!
//! Shared components for rendering search results in Command Palette and Global Search.

use crate::search::{SearchResult, SearchResultType};
use leptos::prelude::*;
use std::collections::BTreeMap;

/// List of search results with grouping
#[component]
pub fn SearchResultsList(
    /// Search results to display
    #[prop(into)]
    results: Signal<Vec<SearchResult>>,
    /// Currently selected index
    #[prop(into)]
    selected_index: Signal<usize>,
    /// Callback when a result is selected
    #[prop(into)]
    on_select: Callback<SearchResult>,
) -> impl IntoView {
    view! {
        <div class="max-h-80 overflow-y-auto">
            {move || {
                let results_vec = results.get();
                if results_vec.is_empty() {
                    return view! {
                        <div class="px-4 py-8 text-center text-sm text-muted-foreground">
                            "No results found"
                        </div>
                    }.into_any();
                }

                // Group results by type
                let grouped = group_by_type(&results_vec);
                let selected_idx = selected_index.get();

                view! {
                    <div class="py-2">
                        <GroupedResults
                            groups=grouped
                            selected_index=selected_idx
                            on_select=on_select.clone()
                        />
                    </div>
                }.into_any()
            }}
        </div>
    }
}

/// Group results by type, preserving order
fn group_by_type(results: &[SearchResult]) -> Vec<(SearchResultType, Vec<SearchResult>)> {
    let mut groups: BTreeMap<u8, (SearchResultType, Vec<SearchResult>)> = BTreeMap::new();

    for result in results {
        let priority = result.result_type.sort_priority();
        groups
            .entry(priority)
            .or_insert_with(|| (result.result_type, Vec::new()))
            .1
            .push(result.clone());
    }

    groups.into_values().collect()
}

/// Render grouped results
#[component]
fn GroupedResults(
    groups: Vec<(SearchResultType, Vec<SearchResult>)>,
    selected_index: usize,
    on_select: Callback<SearchResult>,
) -> impl IntoView {
    let mut global_idx = 0usize;

    groups
        .into_iter()
        .map(|(result_type, items)| {
            let start_idx = global_idx;
            global_idx += items.len();
            let on_select = on_select.clone();

            view! {
                <div class="mb-2">
                    // Group header
                    <div class="flex items-center gap-2 px-3 py-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="h-3.5 w-3.5"
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" d=result_type.icon_path()/>
                        </svg>
                        <span>{result_type.display_name()}</span>
                    </div>

                    // Group items
                    <div class="space-y-0.5">
                        {items.into_iter().enumerate().map(|(local_idx, result)| {
                            let item_idx = start_idx + local_idx;
                            let is_selected = item_idx == selected_index;
                            let result_clone = result.clone();
                            let on_select = on_select.clone();

                            view! {
                                <ResultItem
                                    result=result
                                    is_selected=is_selected
                                    on_click=move || on_select.run(result_clone.clone())
                                />
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            }
        })
        .collect::<Vec<_>>()
}

/// A single search result item
#[component]
fn ResultItem(
    result: SearchResult,
    is_selected: bool,
    #[prop(into)] on_click: Callback<()>,
) -> impl IntoView {
    let base_class = if is_selected {
        "flex items-center gap-3 px-3 py-2 mx-1 rounded-md bg-accent text-accent-foreground cursor-pointer"
    } else {
        "flex items-center gap-3 px-3 py-2 mx-1 rounded-md hover:bg-muted/50 cursor-pointer"
    };

    view! {
        <div
            class=base_class
            on:click=move |_| on_click.run(())
        >
            // Icon
            <div class="flex h-8 w-8 items-center justify-center rounded-md bg-muted shrink-0">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-4 w-4 text-muted-foreground"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d=result.result_type.icon_path()/>
                </svg>
            </div>

            // Title and subtitle
            <div class="flex-1 min-w-0">
                <p class="text-sm font-medium truncate">{result.title.clone()}</p>
                {result.subtitle.as_ref().map(|subtitle| {
                    view! {
                        <p class="text-xs text-muted-foreground truncate">{subtitle.clone()}</p>
                    }
                })}
            </div>

            // Shortcut hint (if any)
            {result.shortcut.as_ref().map(|shortcut| {
                view! {
                    <kbd class="hidden sm:flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-muted border border-border text-xs font-mono text-muted-foreground">
                        {shortcut.clone()}
                    </kbd>
                }
            })}

            // Enter hint when selected
            {if is_selected {
                Some(view! {
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-4 w-4 text-muted-foreground"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M13 7l5 5m0 0l-5 5m5-5H6"/>
                    </svg>
                })
            } else {
                None
            }}
        </div>
    }
}

/// Empty state when no results and no query
#[component]
pub fn SearchEmptyState(
    /// Whether to show recent items prompt
    #[prop(optional)]
    show_recent_hint: bool,
) -> impl IntoView {
    view! {
        <div class="px-4 py-8 text-center">
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-10 w-10 mx-auto mb-3 text-muted-foreground/50"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="1.5"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
            </svg>
            <p class="text-sm text-muted-foreground">
                "Type to search pages, adapters, and actions"
            </p>
            {if show_recent_hint {
                Some(view! {
                    <p class="text-xs text-muted-foreground/70 mt-1">
                        "or browse recent items below"
                    </p>
                })
            } else {
                None
            }}
        </div>
    }
}
