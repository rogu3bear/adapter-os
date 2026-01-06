//! Search context for the command palette.

use crate::api::ApiClient;
use crate::search::{RecentItem, SearchAction, SearchResult, SearchResultType};
use leptos::prelude::*;
use std::sync::Arc;

const MAX_RECENT_ITEMS: usize = 6;

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
        self.recent.update(|items| {
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

        set_timeout_simple(
            move || {
                if search_version.get_untracked() != version {
                    return;
                }
                if query.is_empty() {
                    results.set(Vec::new());
                    selected_index.set(0);
                    return;
                }

                let mut matches: Vec<SearchResult> = static_results()
                    .into_iter()
                    .filter(|result| result.matches(&query))
                    .collect();

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
    let window = web_sys::window().expect("no window");
    let _ =
        window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}

fn static_results() -> Vec<SearchResult> {
    vec![
        SearchResult::new(
            "dashboard",
            "Dashboard",
            SearchResultType::Page,
            SearchAction::Navigate("/dashboard".to_string()),
        ),
        SearchResult::new(
            "adapters",
            "Adapters",
            SearchResultType::Page,
            SearchAction::Navigate("/adapters".to_string()),
        ),
        SearchResult::new(
            "chat",
            "Chat",
            SearchResultType::Page,
            SearchAction::Navigate("/chat".to_string()),
        ),
        SearchResult::new(
            "training",
            "Training",
            SearchResultType::Page,
            SearchAction::Navigate("/training".to_string()),
        ),
        SearchResult::new(
            "system",
            "System",
            SearchResultType::Page,
            SearchAction::Navigate("/system".to_string()),
        ),
        SearchResult::new(
            "models",
            "Models",
            SearchResultType::Page,
            SearchAction::Navigate("/models".to_string()),
        ),
        SearchResult::new(
            "policies",
            "Policies",
            SearchResultType::Page,
            SearchAction::Navigate("/policies".to_string()),
        ),
        SearchResult::new(
            "stacks",
            "Stacks",
            SearchResultType::Page,
            SearchAction::Navigate("/stacks".to_string()),
        ),
        SearchResult::new(
            "collections",
            "Collections",
            SearchResultType::Page,
            SearchAction::Navigate("/collections".to_string()),
        ),
        SearchResult::new(
            "documents",
            "Documents",
            SearchResultType::Page,
            SearchAction::Navigate("/documents".to_string()),
        ),
        SearchResult::new(
            "repositories",
            "Repositories",
            SearchResultType::Page,
            SearchAction::Navigate("/repositories".to_string()),
        ),
        SearchResult::new(
            "workers",
            "Workers",
            SearchResultType::Page,
            SearchAction::Navigate("/workers".to_string()),
        ),
        SearchResult::new(
            "admin",
            "Admin",
            SearchResultType::Page,
            SearchAction::Navigate("/admin".to_string()),
        ),
        SearchResult::new(
            "audit",
            "Audit",
            SearchResultType::Page,
            SearchAction::Navigate("/audit".to_string()),
        ),
        SearchResult::new(
            "settings",
            "Settings",
            SearchResultType::Page,
            SearchAction::Navigate("/settings".to_string()),
        ),
        SearchResult::new(
            "safe",
            "Safe Mode",
            SearchResultType::Page,
            SearchAction::Navigate("/safe".to_string()),
        ),
        SearchResult::new(
            "toggle-chat",
            "Toggle Chat Dock",
            SearchResultType::Action,
            SearchAction::Execute("toggle-chat".to_string()),
        ),
        SearchResult::new(
            "toggle-theme",
            "Toggle Theme",
            SearchResultType::Action,
            SearchAction::Execute("toggle-theme".to_string()),
        ),
    ]
}
