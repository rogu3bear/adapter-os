//! Entity search providers
//!
//! Provides server-side search across adapters and pages via API.

use super::types::{SearchAction, SearchResult, SearchResultType};
use crate::api::ApiClient;
use leptos::prelude::*;
use std::sync::Arc;

fn normalize_entity_path(result_type: SearchResultType, id: &str, path: &str) -> String {
    match result_type {
        // Search results should deep-link to model detail when possible.
        SearchResultType::Model if path == "/models" || path == "/models/" || path.is_empty() => {
            format!("/models/{}", id)
        }
        SearchResultType::Worker
            if path == "/workers" || path == "/workers/" || path.is_empty() =>
        {
            format!("/workers/{}", id)
        }
        _ => path.to_string(),
    }
}

/// Search provider using server-side API
///
/// Instead of caching all entities client-side, this provider
/// calls the `/v1/search` endpoint for efficient server-side search.
#[derive(Clone)]
pub struct EntityCache {
    /// Loading state
    loading: RwSignal<bool>,
    /// Last error message
    error: RwSignal<Option<String>>,
    /// API client
    client: Arc<ApiClient>,
}

impl EntityCache {
    /// Create a new entity cache
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
            client,
        }
    }

    /// Get loading state
    pub fn is_loading(&self) -> bool {
        self.loading.get_untracked()
    }

    /// Get last error
    pub fn last_error(&self) -> Option<String> {
        self.error.get_untracked()
    }

    /// Search adapters and pages via server API
    ///
    /// This replaces client-side fuzzy matching with server-side search.
    pub async fn search(&self, query: &str, limit: Option<u32>) -> Vec<SearchResult> {
        // Require minimum query length
        if query.len() < 2 {
            return Vec::new();
        }

        self.loading.set(true);
        self.error.set(None);

        let results = match self.client.search(query, Some("all"), limit).await {
            Ok(response) => response
                .results
                .into_iter()
                .map(|r| {
                    let result_type = match r.result_type.as_str() {
                        "adapter" => SearchResultType::Adapter,
                        "page" => SearchResultType::Page,
                        "model" => SearchResultType::Model,
                        "worker" => SearchResultType::Worker,
                        "stack" => SearchResultType::Stack,
                        _ => SearchResultType::Action,
                    };
                    let path = normalize_entity_path(result_type, &r.id, &r.path);

                    SearchResult {
                        id: r.id,
                        result_type,
                        title: r.title,
                        subtitle: r.subtitle,
                        score: r.score,
                        action: SearchAction::Navigate(path),
                        shortcut: None,
                    }
                })
                .collect(),
            Err(e) => {
                self.error.set(Some(e.to_string()));
                Vec::new()
            }
        };

        self.loading.set(false);
        results
    }

    /// Clear error state
    pub fn clear(&self) {
        self.error.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_model_path_to_detail_when_generic() {
        assert_eq!(
            normalize_entity_path(SearchResultType::Model, "mdl_1", "/models"),
            "/models/mdl_1"
        );
        assert_eq!(
            normalize_entity_path(SearchResultType::Model, "mdl_1", ""),
            "/models/mdl_1"
        );
    }

    #[test]
    fn keeps_specific_path_unchanged() {
        assert_eq!(
            normalize_entity_path(SearchResultType::Model, "mdl_1", "/models/mdl_1"),
            "/models/mdl_1"
        );
        assert_eq!(
            normalize_entity_path(SearchResultType::Page, "dashboard", "/"),
            "/"
        );
    }
}
