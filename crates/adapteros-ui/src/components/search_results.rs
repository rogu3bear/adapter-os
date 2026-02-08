//! Search results components for the command palette.

use crate::search::SearchResult;
use leptos::prelude::*;

/// Empty state for search.
#[component]
pub fn SearchEmptyState(#[prop(optional)] show_recent_hint: bool) -> impl IntoView {
    view! {
        <div class="search-empty">
            <div class="search-empty-icon">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="w-8 h-8"
                >
                    <circle cx="11" cy="11" r="7" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
            </div>
            <h3 class="search-empty-title">"Start typing to search"</h3>
            <p class="search-empty-description">
                "Search across pages, models, adapters, and actions."
            </p>
            {move || {
                if show_recent_hint {
                    view! {
                        <p class="search-empty-hint">"Tip: use the arrow keys to navigate."</p>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

/// List of search results.
#[component]
pub fn SearchResultsList(
    results: Signal<Vec<SearchResult>>,
    selected_index: Signal<usize>,
    on_select: Callback<SearchResult>,
) -> impl IntoView {
    view! {
        <div class="search-results">
            <For
                each=move || {
                    results
                        .get()
                        .into_iter()
                        .enumerate()
                        .collect::<Vec<(usize, SearchResult)>>()
                }
                key=|(idx, result)| format!("{}-{}", idx, result.id)
                children=move |(idx, result)| {
                    let on_select = on_select;
                    let result_clone = result.clone();
                    let is_selected = Signal::derive(move || selected_index.get() == idx);

                    view! {
                        <div
                            class=move || {
                                let base = "search-result";
                                if is_selected.get() {
                                    format!("{} search-result-selected", base)
                                } else {
                                    base.to_string()
                                }
                            }
                            on:click=move |_| on_select.run(result_clone.clone())
                        >
                            <div class="search-result-icon">
                                <svg
                                    xmlns="http://www.w3.org/2000/svg"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    class="w-4 h-4"
                                >
                                    <path d=result.result_type.icon_path() />
                                </svg>
                            </div>
                            <div class="search-result-content">
                                <div class="search-result-title">{result.title.clone()}</div>
                                {result.subtitle.clone().map(|subtitle| view! {
                                    <div class="search-result-subtitle">{subtitle}</div>
                                })}
                            </div>
                            {result.shortcut.clone().map(|shortcut| view! {
                                <div class="search-result-shortcut">{shortcut}</div>
                            })}
                        </div>
                    }
                }
            />
        </div>
    }
}
