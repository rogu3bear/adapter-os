//! Search index
//!
//! Search index builder using nav_registry as the single source of truth.
//! All page definitions come from nav_registry to avoid duplicate sources.

use super::fuzzy::fuzzy_score;
use super::types::SearchResult;
use crate::components::layout::nav_registry::{all_nav_items, NavItem};
use adapteros_api_types::UiProfile;

const RUNS_PAGE_NAME: &str = "Runs";
const RUNS_SEARCH_KEYWORDS: &[&str] = &[
    "runs",
    "run",
    "flight recorder",
    "flight",
    "recorder",
    "traces",
    "provenance",
    "receipts",
];

fn page_name_for_item(item: &'static NavItem) -> &'static str {
    if item.route == "/runs" {
        RUNS_PAGE_NAME
    } else {
        item.label
    }
}

fn page_keywords_for_item(item: &'static NavItem) -> &'static [&'static str] {
    if item.route == "/runs" {
        RUNS_SEARCH_KEYWORDS
    } else {
        item.keywords
    }
}

/// Definition of a searchable page
#[derive(Debug, Clone)]
pub struct PageDefinition {
    /// Unique identifier
    pub id: &'static str,
    /// Display name
    pub name: &'static str,
    /// Description for search matching
    pub description: &'static str,
    /// Navigation path
    pub path: &'static str,
    /// Search keywords (additional terms that should match)
    pub keywords: &'static [&'static str],
}

impl PageDefinition {
    /// Convert from NavItem (nav_registry is the canonical source)
    fn from_nav_item(item: &'static NavItem) -> Self {
        Self {
            id: item.id,
            name: page_name_for_item(item),
            // ASSUMPTION: Description is derived from label for now
            // since NavItem doesn't have a description field
            description: page_name_for_item(item),
            path: item.route,
            keywords: page_keywords_for_item(item),
        }
    }
}

/// Get all searchable pages from nav_registry (canonical source of truth)
pub fn get_pages(profile: UiProfile) -> Vec<PageDefinition> {
    all_nav_items(profile)
        .into_iter()
        .filter(|item| !item.hidden)
        .map(PageDefinition::from_nav_item)
        .collect()
}

/// Searchable action/command
#[derive(Debug, Clone)]
pub struct ActionDefinition {
    /// Unique identifier
    pub id: &'static str,
    /// Display name
    pub name: &'static str,
    /// Description
    pub description: &'static str,
    /// Command key for execution
    pub command: &'static str,
    /// Keyboard shortcut hint
    pub shortcut: Option<&'static str>,
    /// Search keywords
    pub keywords: &'static [&'static str],
}

impl ActionDefinition {
    const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        command: &'static str,
        shortcut: Option<&'static str>,
        keywords: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            name,
            description,
            command,
            shortcut,
            keywords,
        }
    }
}

/// All searchable actions/commands
pub static ACTIONS: &[ActionDefinition] = &[
    ActionDefinition::new(
        "toggle-chat",
        "Toggle Chat Panel",
        "Show or hide the chat dock",
        "toggle-chat",
        Some("⌘ B"),
        &["chat", "dock", "panel", "sidebar"],
    ),
    ActionDefinition::new(
        "toggle-theme",
        "Toggle Theme",
        "Switch between light and dark mode",
        "toggle-theme",
        None,
        &["dark", "light", "mode", "appearance"],
    ),
    ActionDefinition::new(
        "new-chat",
        "New Chat Session",
        "Start a new chat conversation",
        "new-chat",
        None,
        &["conversation", "fresh", "clear"],
    ),
    ActionDefinition::new(
        "refresh",
        "Refresh Data",
        "Reload current page data",
        "refresh",
        Some("⌘ R"),
        &["reload", "update", "sync"],
    ),
    ActionDefinition::new(
        "logout",
        "Sign Out",
        "Sign out of your account",
        "logout",
        None,
        &["signout", "exit", "leave"],
    ),
];

/// Search index for fast fuzzy matching
#[derive(Debug, Clone)]
pub struct SearchIndex {
    /// Minimum score threshold for results
    min_score: f32,
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchIndex {
    /// Create a new search index
    pub fn new() -> Self {
        Self { min_score: 0.3 }
    }

    /// Set minimum score threshold
    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }

    /// Search pages by query
    pub fn search_pages(&self, query: &str) -> Vec<SearchResult> {
        let pages = get_pages(UiProfile::Full);

        if query.is_empty() {
            // Return all pages when query is empty (for browsing)
            return pages
                .iter()
                .map(|page| {
                    SearchResult::page(page.id, page.name, Some(page.description), page.path, 1.0)
                })
                .collect();
        }

        let mut results: Vec<SearchResult> = pages
            .iter()
            .filter_map(|page| {
                // Score against name, description, and keywords
                let name_score = fuzzy_score(page.name, query);
                let desc_score = fuzzy_score(page.description, query) * 0.8;
                let keyword_score = page
                    .keywords
                    .iter()
                    .map(|kw| fuzzy_score(kw, query) * 0.9)
                    .fold(0.0_f32, |a, b| a.max(b));

                let best_score = name_score.max(desc_score).max(keyword_score);

                if best_score >= self.min_score {
                    Some(SearchResult::page(
                        page.id,
                        page.name,
                        Some(page.description),
                        page.path,
                        best_score,
                    ))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Search actions by query
    pub fn search_actions(&self, query: &str) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new(); // Don't show actions when query is empty
        }

        let mut results: Vec<SearchResult> = ACTIONS
            .iter()
            .filter_map(|action| {
                let name_score = fuzzy_score(action.name, query);
                let desc_score = fuzzy_score(action.description, query) * 0.8;
                let keyword_score = action
                    .keywords
                    .iter()
                    .map(|kw| fuzzy_score(kw, query) * 0.9)
                    .fold(0.0_f32, |a, b| a.max(b));

                let best_score = name_score.max(desc_score).max(keyword_score);

                if best_score >= self.min_score {
                    Some(SearchResult::action(
                        action.id,
                        action.name,
                        Some(action.description),
                        action.command,
                        action.shortcut,
                        best_score,
                    ))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Perform combined search across all sources
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let mut all_results = Vec::new();

        all_results.extend(self.search_pages(query));
        all_results.extend(self.search_actions(query));

        // Sort by score descending, then by type priority
        all_results.sort_by(|a, b| match b.score.partial_cmp(&a.score) {
            Some(std::cmp::Ordering::Equal) | None => a
                .result_type
                .sort_priority()
                .cmp(&b.result_type.sort_priority()),
            Some(ord) => ord,
        });

        all_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_pages() {
        let index = SearchIndex::new();
        let results = index.search_pages("home");
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|r| r.title == "Home" || r.title == "Dashboard"),
            "Expected Home/Dashboard result, got: {:?}",
            results.iter().map(|r| &r.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_search_by_keyword() {
        let index = SearchIndex::new();
        let results = index.search_pages("lora");
        assert!(!results.is_empty());
        // Both "Adapters" and "Training Jobs" have "lora" as a keyword
        // and score identically — assert presence, not position.
        assert!(
            results.iter().any(|r| r.title == "Adapters"),
            "Expected 'Adapters' in results for 'lora', got: {:?}",
            results.iter().map(|r| &r.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_empty_query() {
        let index = SearchIndex::new();
        let results = index.search_pages("");
        // Returns all non-hidden pages from nav_registry
        assert!(!results.is_empty());
    }

    #[test]
    fn test_runs_uses_canonical_name() {
        let index = SearchIndex::new();
        let results = index.search_pages("runs");
        let runs_result = results
            .iter()
            .find(|r| r.path() == Some("/runs"))
            .expect("Expected /runs result");
        assert_eq!(runs_result.title, "Runs");
    }

    #[test]
    fn test_flight_recorder_alias_matches_runs() {
        let index = SearchIndex::new();
        let results = index.search_pages("flight recorder");
        assert!(
            results.iter().any(|r| r.path() == Some("/runs")),
            "Expected /runs alias match for 'flight recorder', got: {:?}",
            results.iter().map(|r| &r.title).collect::<Vec<_>>()
        );
    }
}
