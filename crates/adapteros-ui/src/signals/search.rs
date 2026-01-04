//! Search state management
//!
//! Global search state for Command Palette and Global Search.

use crate::api::ApiClient;
use crate::search::{EntityCache, RecentItem, RecentItemsManager, SearchIndex, SearchResult};
use leptos::prelude::*;
use std::sync::Arc;

/// Global search context
#[derive(Clone)]
pub struct SearchContext {
    /// Current search query
    pub query: RwSignal<String>,
    /// Search results
    pub results: RwSignal<Vec<SearchResult>>,
    /// Currently selected result index
    pub selected_index: RwSignal<usize>,
    /// Whether the command palette is open
    pub command_palette_open: RwSignal<bool>,
    /// Recent items manager (for persistence)
    recent_items: StoredValue<RecentItemsManager>,
    /// Recent items signal (for reactivity)
    pub recent_items_signal: RwSignal<Vec<RecentItem>>,
    /// Entity cache for adapters, models, workers
    pub entity_cache: StoredValue<EntityCache>,
    /// Search index for pages and actions
    search_index: StoredValue<SearchIndex>,
    /// Search debounce timer ID
    debounce_timer: RwSignal<Option<i32>>,
}

impl SearchContext {
    /// Create a new search context
    pub fn new(client: Arc<ApiClient>) -> Self {
        let recent_manager = RecentItemsManager::new();
        let recent_items = recent_manager.items().to_vec();

        Self {
            query: RwSignal::new(String::new()),
            results: RwSignal::new(Vec::new()),
            selected_index: RwSignal::new(0),
            command_palette_open: RwSignal::new(false),
            recent_items: StoredValue::new(recent_manager),
            recent_items_signal: RwSignal::new(recent_items),
            entity_cache: StoredValue::new(EntityCache::new(client)),
            search_index: StoredValue::new(SearchIndex::new()),
            debounce_timer: RwSignal::new(None),
        }
    }

    /// Open the command palette
    pub fn open(&self) {
        self.command_palette_open.set(true);
        self.query.set(String::new());
        self.results.set(Vec::new());
        self.selected_index.set(0);
    }

    /// Close the command palette
    pub fn close(&self) {
        self.command_palette_open.set(false);
        self.query.set(String::new());
        self.results.set(Vec::new());
        self.selected_index.set(0);

        // Cancel any pending debounce
        if let Some(timer_id) = self.debounce_timer.get_untracked() {
            cancel_timeout(timer_id);
            self.debounce_timer.set(None);
        }
    }

    /// Toggle the command palette
    pub fn toggle(&self) {
        if self.command_palette_open.get_untracked() {
            self.close();
        } else {
            self.open();
        }
    }

    /// Perform search with debouncing
    pub fn search_debounced(&self, query: String) {
        // Cancel existing timer
        if let Some(timer_id) = self.debounce_timer.get_untracked() {
            cancel_timeout(timer_id);
        }

        self.query.set(query.clone());

        // If query is empty, show recent items instead
        if query.is_empty() {
            self.results.set(Vec::new());
            self.selected_index.set(0);
            return;
        }

        // Set up debounced search
        let ctx = self.clone();
        let timer_id = set_timeout(
            move || {
                ctx.execute_search(&query);
            },
            150, // 150ms debounce
        );
        self.debounce_timer.set(Some(timer_id));
    }

    /// Execute the actual search
    fn execute_search(&self, query: &str) {
        let mut all_results = Vec::new();

        // Search pages and actions
        self.search_index.with_value(|index| {
            all_results.extend(index.search(query));
        });

        // Search cached entities
        self.entity_cache.with_value(|cache| {
            all_results.extend(cache.search_adapters(query));
        });

        // Sort by score descending
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit total results
        all_results.truncate(20);

        self.results.set(all_results);
        self.selected_index.set(0);
    }

    /// Move selection up
    pub fn select_prev(&self) {
        let current = self.selected_index.get_untracked();
        if current > 0 {
            self.selected_index.set(current - 1);
        }
    }

    /// Move selection down
    pub fn select_next(&self) {
        let current = self.selected_index.get_untracked();
        let len = self.results.get_untracked().len();
        if len > 0 && current < len - 1 {
            self.selected_index.set(current + 1);
        }
    }

    /// Get the currently selected result
    pub fn selected_result(&self) -> Option<SearchResult> {
        let idx = self.selected_index.get_untracked();
        self.results.get_untracked().get(idx).cloned()
    }

    /// Record a navigation to recent items
    pub fn record_recent(&self, item: RecentItem) {
        self.recent_items.update_value(|manager| {
            manager.add(item);
        });
        // Update reactive signal
        self.recent_items.with_value(|manager| {
            self.recent_items_signal.set(manager.items().to_vec());
        });
    }

    /// Get recent items
    pub fn recent_items(&self) -> Vec<RecentItem> {
        self.recent_items_signal.get()
    }

    /// Clear recent items
    pub fn clear_recent(&self) {
        self.recent_items.update_value(|manager| {
            manager.clear();
        });
        self.recent_items_signal.set(Vec::new());
    }

    /// Fetch entities for search (call when palette opens)
    pub fn prefetch_entities(&self) {
        self.entity_cache.with_value(|cache| {
            let cache = cache.clone();
            wasm_bindgen_futures::spawn_local(async move {
                cache.ensure_adapters().await;
            });
        });
    }
}

/// Provide search context to the app
pub fn provide_search_context(client: Arc<ApiClient>) {
    let context = SearchContext::new(client);
    provide_context(context);
}

/// Use the search context
pub fn use_search() -> SearchContext {
    expect_context::<SearchContext>()
}

// Timer helpers for WASM
#[cfg(target_arch = "wasm32")]
fn set_timeout<F: FnOnce() + 'static>(f: F, ms: i32) -> i32 {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let closure = Closure::once_into_js(f);
    let window = web_sys::window().expect("no window");
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms)
        .expect("set_timeout failed")
}

#[cfg(target_arch = "wasm32")]
fn cancel_timeout(id: i32) {
    if let Some(window) = web_sys::window() {
        window.clear_timeout_with_handle(id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout<F: FnOnce() + 'static>(_f: F, _ms: i32) -> i32 {
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn cancel_timeout(_id: i32) {}
