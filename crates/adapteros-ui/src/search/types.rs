//! Core search types
//!
//! Types for search results, actions, and result grouping.

use serde::{Deserialize, Serialize};

/// Type of search result for grouping and display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchResultType {
    /// Navigation page
    Page,
    /// LoRA adapter entity
    Adapter,
    /// Base model entity
    Model,
    /// Inference worker entity
    Worker,
    /// Adapter stack entity
    Stack,
    /// Executable action/command
    Action,
}

impl SearchResultType {
    /// Display name for the result type
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Page => "Pages",
            Self::Adapter => "Adapters",
            Self::Model => "Models",
            Self::Worker => "Workers",
            Self::Stack => "Stacks",
            Self::Action => "Actions",
        }
    }

    /// Icon for the result type (SVG path)
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Page => "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6",
            Self::Adapter => "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10",
            Self::Model => "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z",
            Self::Worker => "M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01",
            Self::Stack => "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10",
            Self::Action => "M13 10V3L4 14h7v7l9-11h-7z",
        }
    }

    /// Sort priority (lower = higher in results)
    pub fn sort_priority(&self) -> u8 {
        match self {
            Self::Action => 0,
            Self::Page => 1,
            Self::Adapter => 2,
            Self::Model => 3,
            Self::Worker => 4,
            Self::Stack => 5,
        }
    }
}

/// Action to execute when a search result is selected
#[derive(Debug, Clone)]
pub enum SearchAction {
    /// Navigate to a path
    Navigate(String),
    /// Execute a command (identified by key)
    Execute(String),
}

/// A single search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Unique identifier for this result
    pub id: String,
    /// Type of result for grouping
    pub result_type: SearchResultType,
    /// Primary display text
    pub title: String,
    /// Secondary display text
    pub subtitle: Option<String>,
    /// Fuzzy match score (0.0 to 1.0)
    pub score: f32,
    /// Action to execute when selected
    pub action: SearchAction,
    /// Optional keyboard shortcut hint
    pub shortcut: Option<String>,
}

impl SearchResult {
    /// Create a page navigation result
    pub fn page(id: &str, title: &str, subtitle: Option<&str>, path: &str, score: f32) -> Self {
        Self {
            id: id.to_string(),
            result_type: SearchResultType::Page,
            title: title.to_string(),
            subtitle: subtitle.map(|s| s.to_string()),
            score,
            action: SearchAction::Navigate(path.to_string()),
            shortcut: None,
        }
    }

    /// Create an adapter result
    pub fn adapter(id: &str, name: &str, adapter_id: &str, score: f32) -> Self {
        Self {
            id: id.to_string(),
            result_type: SearchResultType::Adapter,
            title: name.to_string(),
            subtitle: Some(adapter_id.to_string()),
            score,
            action: SearchAction::Navigate(format!("/adapters/{}", id)),
            shortcut: None,
        }
    }

    /// Create a model result
    pub fn model(id: &str, name: &str, score: f32) -> Self {
        Self {
            id: id.to_string(),
            result_type: SearchResultType::Model,
            title: name.to_string(),
            subtitle: None,
            score,
            action: SearchAction::Navigate(format!("/models/{}", id)),
            shortcut: None,
        }
    }

    /// Create a worker result
    pub fn worker(id: &str, status: &str, score: f32) -> Self {
        Self {
            id: id.to_string(),
            result_type: SearchResultType::Worker,
            title: format!("Worker {}", &id[..8.min(id.len())]),
            subtitle: Some(status.to_string()),
            score,
            action: SearchAction::Navigate(format!("/workers/{}", id)),
            shortcut: None,
        }
    }

    /// Create an action result
    pub fn action(
        id: &str,
        title: &str,
        subtitle: Option<&str>,
        command: &str,
        shortcut: Option<&str>,
        score: f32,
    ) -> Self {
        Self {
            id: id.to_string(),
            result_type: SearchResultType::Action,
            title: title.to_string(),
            subtitle: subtitle.map(|s| s.to_string()),
            score,
            action: SearchAction::Execute(command.to_string()),
            shortcut: shortcut.map(|s| s.to_string()),
        }
    }

    /// Get the navigation path if this is a Navigate action
    pub fn path(&self) -> Option<&str> {
        match &self.action {
            SearchAction::Navigate(path) => Some(path),
            SearchAction::Execute(_) => None,
        }
    }
}

/// Group search results by type
pub fn group_results(results: &[SearchResult]) -> Vec<(SearchResultType, Vec<&SearchResult>)> {
    use std::collections::BTreeMap;

    // Group by type
    let mut groups: BTreeMap<u8, (SearchResultType, Vec<&SearchResult>)> = BTreeMap::new();

    for result in results {
        let priority = result.result_type.sort_priority();
        groups
            .entry(priority)
            .or_insert_with(|| (result.result_type, Vec::new()))
            .1
            .push(result);
    }

    groups.into_values().collect()
}
