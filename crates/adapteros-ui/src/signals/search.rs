//! Search context for the command palette.

use crate::api::ApiClient;
use crate::search::{
    contextual_result_matches, generate_contextual_actions, RecentItem, SearchResult,
};
use crate::signals::page_context::RouteContext;
use leptos::prelude::*;
use std::sync::Arc;

const MAX_RECENT_ITEMS: usize = 6;

/// Check if a result matches a query (case-insensitive substring match)
fn result_matches(result: &SearchResult, query: &str) -> bool {
    let query_lower = query.to_lowercase();
    result.title.to_lowercase().contains(&query_lower)
        || result.id.to_lowercase().contains(&query_lower)
        || result
            .subtitle
            .as_ref()
            .map(|s| s.to_lowercase().contains(&query_lower))
            .unwrap_or(false)
}

#[derive(Clone)]
pub struct SearchContext {
    pub query: RwSignal<String>,
    pub results: RwSignal<Vec<SearchResult>>,
    pub selected_index: RwSignal<usize>,
    pub command_palette_open: RwSignal<bool>,
    recent: RwSignal<Vec<RecentItem>>,
    client: Arc<ApiClient>,
    search_version: RwSignal<u64>,
}

impl SearchContext {
    pub fn open(&self) {
        self.command_palette_open.set(true);
    }

    pub fn close(&self) {
        self.command_palette_open.set(false);
        self.query.set(String::new());
        self.results.set(Vec::new());
        self.selected_index.set(0);
    }

    pub fn toggle(&self) {
        if self.command_palette_open.get_untracked() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn select_next(&self) {
        let len = self.results.get_untracked().len();
        if len == 0 {
            return;
        }
        self.selected_index.update(|idx| {
            *idx = (*idx + 1) % len;
        });
    }

    pub fn select_prev(&self) {
        let len = self.results.get_untracked().len();
        if len == 0 {
            return;
        }
        self.selected_index.update(|idx| {
            if *idx == 0 {
                *idx = len - 1;
            } else {
                *idx -= 1;
            }
        });
    }

    pub fn selected_result(&self) -> Option<SearchResult> {
        let results = self.results.get_untracked();
        let idx = self.selected_index.get_untracked();
        results.get(idx).cloned()
    }

    pub fn record_recent(&self, item: RecentItem) {
        self.recent.update(|items: &mut Vec<RecentItem>| {
            items.retain(|existing| existing.id != item.id);
            items.insert(0, item);
            if items.len() > MAX_RECENT_ITEMS {
                items.truncate(MAX_RECENT_ITEMS);
            }
        });
    }

    pub fn recent_items(&self) -> Vec<RecentItem> {
        self.recent.get_untracked()
    }

    pub fn prefetch_entities(&self) {
        let _ = self.client.clone();
    }

    pub fn search_debounced(&self, value: String) {
        self.query.set(value.clone());
        let query = value.trim().to_string();

        self.search_version.update(|v| *v += 1);
        let version = self.search_version.get_untracked();
        let results = self.results;
        let selected_index = self.selected_index;
        let search_version = self.search_version;

        // Get contextual actions if RouteContext is available
        let contextual_actions = use_context::<RouteContext>()
            .map(|ctx| generate_contextual_actions(&ctx))
            .unwrap_or_default();

        set_timeout_simple(
            move || {
                if search_version.get_untracked() != version {
                    return;
                }

                // Start with contextual actions (filtered by query if not empty)
                let mut matches: Vec<SearchResult> = if query.is_empty() {
                    // Show all contextual actions when query is empty
                    contextual_actions
                } else {
                    // Filter contextual actions by query
                    contextual_actions
                        .into_iter()
                        .filter(|result| contextual_result_matches(result, &query))
                        .collect()
                };

                // If query is not empty, also search static results
                if !query.is_empty() {
                    let static_matches: Vec<SearchResult> = static_results()
                        .into_iter()
                        .filter(|result| result_matches(result, &query))
                        .collect();
                    matches.extend(static_matches);
                }

                // Sort by score (contextual actions have higher scores)
                matches.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                results.set(matches);
                selected_index.set(0);
            },
            120,
        );
    }

    pub fn search_immediate(&self, value: String) {
        self.query.set(value.clone());
        let query = value.trim().to_string();
        self.search_version.update(|v| *v += 1);

        // Get contextual actions if RouteContext is available
        let contextual_actions = use_context::<RouteContext>()
            .map(|ctx| generate_contextual_actions(&ctx))
            .unwrap_or_default();

        // Start with contextual actions (filtered by query if not empty)
        let mut matches: Vec<SearchResult> = if query.is_empty() {
            // Show all contextual actions when query is empty
            contextual_actions
        } else {
            // Filter contextual actions by query
            contextual_actions
                .into_iter()
                .filter(|result| contextual_result_matches(result, &query))
                .collect()
        };

        // If query is not empty, also search static results
        if !query.is_empty() {
            let static_matches: Vec<SearchResult> = static_results()
                .into_iter()
                .filter(|result| result_matches(result, &query))
                .collect();
            matches.extend(static_matches);
        }

        // Sort by score (contextual actions have higher scores)
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.results.set(matches);
        self.selected_index.set(0);
    }
}

/// Provide search context.
pub fn provide_search_context(client: Arc<ApiClient>) {
    let context = SearchContext {
        query: RwSignal::new(String::new()),
        results: RwSignal::new(Vec::new()),
        selected_index: RwSignal::new(0),
        command_palette_open: RwSignal::new(false),
        recent: RwSignal::new(Vec::new()),
        client,
        search_version: RwSignal::new(0),
    };

    provide_context(context);
}

/// Use search context.
pub fn use_search() -> SearchContext {
    expect_context::<SearchContext>()
}

#[cfg(target_arch = "wasm32")]
fn set_timeout_simple<F: FnOnce() + 'static>(f: F, ms: i32) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let closure = Closure::once_into_js(f);
    let Some(window) = web_sys::window() else {
        tracing::error!("set_timeout_simple: no window object available");
        return;
    };
    let _ =
        window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}

fn static_results() -> Vec<SearchResult> {
    vec![
        SearchResult::page("dashboard", "Dashboard", None, "/dashboard", 1.0),
        SearchResult::page("adapters", "Adapters", None, "/adapters", 1.0),
        SearchResult::page("chat", "Chat", None, "/chat", 1.0),
        SearchResult::page("training", "Training", None, "/training", 1.0),
        SearchResult::page("system", "System", None, "/system", 1.0),
        SearchResult::page("models", "Models", None, "/models", 1.0),
        SearchResult::page("policies", "Policies", None, "/policies", 1.0),
        SearchResult::page("stacks", "Stacks", None, "/stacks", 1.0),
        SearchResult::page("collections", "Collections", None, "/collections", 1.0),
        SearchResult::page("documents", "Documents", None, "/documents", 1.0),
        SearchResult::page("repositories", "Repositories", None, "/repositories", 1.0),
        SearchResult::page("workers", "Workers", None, "/workers", 1.0),
        SearchResult::page("admin", "Admin", None, "/admin", 1.0),
        SearchResult::page("audit", "Audit", None, "/audit", 1.0),
        SearchResult::page("settings", "Settings", None, "/settings", 1.0),
        SearchResult::page("user", "User", None, "/user", 1.0),
        SearchResult::page("safe", "Safe Mode", None, "/safe", 1.0),
        SearchResult::action(
            "toggle-chat",
            "Toggle Chat Dock",
            None,
            "toggle-chat",
            None,
            1.0,
        ),
        SearchResult::action(
            "toggle-theme",
            "Toggle Theme",
            None,
            "toggle-theme",
            None,
            1.0,
        ),
    ]
}
