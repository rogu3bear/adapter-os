//! Reusable list controls hook for client-side search and pagination.
//!
//! Provides a `use_list_controls` hook that manages search, filter, and
//! client-side pagination state for list pages. Keeps filtered/sorted
//! computation in a Memo to avoid recalculating on every render.

use leptos::prelude::*;

/// Page size for client-side pagination.
pub const DEFAULT_PAGE_SIZE: usize = 25;

fn always_changed<V>(_: Option<&V>, _: Option<&V>) -> bool {
    true
}

/// State returned by [`use_list_controls`].
#[derive(Clone)]
pub struct ListControls<T: Clone + Send + Sync + 'static> {
    /// Search query text (bind to an Input).
    pub search: RwSignal<String>,
    /// Current page (1-indexed).
    pub page: RwSignal<usize>,
    /// Total number of items after filtering (before pagination).
    pub filtered_count: Signal<usize>,
    /// Total number of items before any filtering.
    pub total_count: Signal<usize>,
    /// Total number of pages.
    pub total_pages: Signal<usize>,
    /// The current page of items to render.
    pub visible_items: Signal<Vec<T>>,
    /// Whether there are more pages after the current one.
    pub has_next: Signal<bool>,
    /// Whether there are pages before the current one.
    pub has_prev: Signal<bool>,
    /// Page size used.
    pub page_size: usize,
}

impl<T: Clone + Send + Sync + 'static> ListControls<T> {
    /// Go to the next page.
    pub fn next_page(&self) {
        let max = self.total_pages.get_untracked();
        self.page.update(|p| *p = (*p + 1).min(max));
    }

    /// Go to the previous page.
    pub fn prev_page(&self) {
        self.page.update(|p| *p = p.saturating_sub(1).max(1));
    }
}

/// Create list controls with search and pagination over a reactive item list.
///
/// - `items`: Signal containing the full list of items.
/// - `search_fn`: Given `(item, query_lowercase)`, return true if item matches the search.
/// - `page_size`: Number of items per page. Use [`DEFAULT_PAGE_SIZE`] for the default.
///
/// The search query is debounced implicitly by Leptos's reactive graph (signals
/// only trigger when the value actually changes).
pub fn use_list_controls<T, F>(
    items: Signal<Vec<T>>,
    search_fn: F,
    page_size: usize,
) -> ListControls<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn(&T, &str) -> bool + Send + Sync + Clone + 'static,
{
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(1usize);

    // Reset to page 1 when search changes
    Effect::new(move || {
        let _ = search.try_get();
        let _ = page.try_set(1);
    });

    let total_count = Signal::derive(move || items.get().len());

    // Filtered items (memoized)
    let filtered = Memo::new_with_compare(
        {
            let search_fn = search_fn.clone();
            move |_| {
                let all = items.get();
                let query = search.get().trim().to_lowercase();
                if query.is_empty() {
                    all
                } else {
                    all.into_iter()
                        .filter(|item| search_fn(item, &query))
                        .collect()
                }
            }
        },
        always_changed::<Vec<T>>,
    );

    let filtered_count = Signal::derive(move || filtered.get().len());
    let total_pages = Signal::derive(move || {
        let count = filtered_count.get();
        if count == 0 {
            1
        } else {
            count.div_ceil(page_size)
        }
    });

    // Clamp page if filtered count shrinks
    Effect::new(move || {
        let Some(max) = total_pages.try_get() else {
            return;
        };
        if page.get_untracked() > max {
            let _ = page.try_set(max);
        }
    });

    let visible_items = Signal::derive(move || {
        let all = filtered.get();
        let p = page.get();
        let start = (p - 1) * page_size;
        all.into_iter().skip(start).take(page_size).collect()
    });

    let has_next = Signal::derive(move || page.get() < total_pages.get());
    let has_prev = Signal::derive(move || page.get() > 1);

    ListControls {
        search,
        page,
        filtered_count,
        total_count,
        total_pages,
        visible_items,
        has_next,
        has_prev,
        page_size,
    }
}
