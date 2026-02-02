//! Search index
//!
//! Static page definitions and search index builder.

use super::fuzzy::fuzzy_score;
use super::types::SearchResult;

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
    const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        path: &'static str,
        keywords: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            name,
            description,
            path,
            keywords,
        }
    }
}

/// All searchable pages in the application
pub static PAGES: &[PageDefinition] = &[
    PageDefinition::new(
        "dashboard",
        "Dashboard",
        "Overview and metrics",
        "/dashboard",
        &["home", "overview", "main", "index"],
    ),
    PageDefinition::new(
        "adapters",
        "Adapters",
        "Manage LoRA adapters",
        "/adapters",
        &["lora", "finetune", "weights", "models"],
    ),
    PageDefinition::new(
        "chat",
        "Chat",
        "Interactive inference",
        "/chat",
        &["inference", "generate", "prompt", "conversation"],
    ),
    PageDefinition::new(
        "training",
        "Training",
        "Training jobs and pipelines",
        "/training",
        &["train", "finetune", "jobs", "pipeline"],
    ),
    PageDefinition::new(
        "system",
        "System",
        "System status and health",
        "/system",
        &["health", "status", "diagnostics", "infrastructure"],
    ),
    PageDefinition::new(
        "settings",
        "Settings",
        "Configuration and preferences",
        "/settings",
        &["config", "preferences", "options", "configure"],
    ),
    PageDefinition::new(
        "user",
        "User",
        "Profile and personalization",
        "/user",
        &["profile", "preferences", "identity", "account"],
    ),
    PageDefinition::new(
        "models",
        "Models",
        "Base model management",
        "/models",
        &["llm", "foundation", "base", "weights"],
    ),
    PageDefinition::new(
        "policies",
        "Policies",
        "Execution and routing policies",
        "/policies",
        &["rules", "constraints", "enforcement", "determinism"],
    ),
    PageDefinition::new(
        "stacks",
        "Stacks",
        "Adapter stack configurations",
        "/stacks",
        &["combination", "ensemble", "routing"],
    ),
    PageDefinition::new(
        "collections",
        "Collections",
        "Document collections",
        "/collections",
        &["documents", "corpus", "dataset"],
    ),
    PageDefinition::new(
        "documents",
        "Documents",
        "Document management",
        "/documents",
        &["files", "upload", "corpus"],
    ),
    PageDefinition::new(
        "admin",
        "Admin",
        "Administrative controls",
        "/admin",
        &["administration", "manage", "users", "tenants"],
    ),
    PageDefinition::new(
        "audit",
        "Audit Log",
        "System audit trail",
        "/audit",
        &["logs", "history", "events", "compliance"],
    ),
    PageDefinition::new(
        "workers",
        "Workers",
        "Inference worker management",
        "/workers",
        &["runtime", "instances", "compute", "nodes"],
    ),
    PageDefinition::new(
        "repositories",
        "Repositories",
        "Code repository adapters",
        "/repositories",
        &["git", "code", "codebase", "repo"],
    ),
];

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
        if query.is_empty() {
            // Return all pages when query is empty (for browsing)
            return PAGES
                .iter()
                .map(|page| {
                    SearchResult::page(page.id, page.name, Some(page.description), page.path, 1.0)
                })
                .collect();
        }

        let mut results: Vec<SearchResult> = PAGES
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
        let results = index.search_pages("dash");
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Dashboard");
    }

    #[test]
    fn test_search_by_keyword() {
        let index = SearchIndex::new();
        let results = index.search_pages("lora");
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Adapters");
    }

    #[test]
    fn test_empty_query() {
        let index = SearchIndex::new();
        let results = index.search_pages("");
        assert_eq!(results.len(), PAGES.len());
    }
}
