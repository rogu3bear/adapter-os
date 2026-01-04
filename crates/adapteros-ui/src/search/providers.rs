//! Entity search providers
//!
//! Provides search across cached API entities (adapters, models, workers).

use super::fuzzy::fuzzy_score;
use super::types::SearchResult;
use crate::api::ApiClient;
use adapteros_api_types::adapters::AdapterResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Minimum score for entity search results
const MIN_ENTITY_SCORE: f32 = 0.3;

/// Cache for entity data with lazy loading
#[derive(Clone)]
pub struct EntityCache {
    /// Cached adapters
    adapters: RwSignal<Option<Vec<AdapterResponse>>>,
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
            adapters: RwSignal::new(None),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
            client,
        }
    }

    /// Check if adapters are cached
    pub fn has_adapters(&self) -> bool {
        self.adapters.get_untracked().is_some()
    }

    /// Get loading state
    pub fn is_loading(&self) -> bool {
        self.loading.get_untracked()
    }

    /// Get last error
    pub fn last_error(&self) -> Option<String> {
        self.error.get_untracked()
    }

    /// Fetch adapters if not cached
    pub async fn ensure_adapters(&self) {
        if self.adapters.get_untracked().is_some() || self.loading.get_untracked() {
            return;
        }

        self.loading.set(true);
        self.error.set(None);

        match self.client.list_adapters().await {
            Ok(adapters) => {
                self.adapters.set(Some(adapters));
            }
            Err(e) => {
                self.error.set(Some(e.to_string()));
            }
        }

        self.loading.set(false);
    }

    /// Force refresh adapters
    pub async fn refresh_adapters(&self) {
        self.adapters.set(None);
        self.ensure_adapters().await;
    }

    /// Search cached adapters
    pub fn search_adapters(&self, query: &str) -> Vec<SearchResult> {
        let adapters = match self.adapters.get_untracked() {
            Some(list) => list,
            None => return Vec::new(),
        };

        if query.is_empty() {
            // Return top adapters when query is empty
            return adapters
                .iter()
                .take(5)
                .map(|a| SearchResult::adapter(&a.id, &a.name, &a.adapter_id, 1.0))
                .collect();
        }

        let mut results: Vec<SearchResult> = adapters
            .iter()
            .filter_map(|adapter| {
                // Score against name and adapter_id
                let name_score = fuzzy_score(&adapter.name, query);
                let id_score = fuzzy_score(&adapter.adapter_id, query) * 0.9;
                let intent_score = adapter
                    .intent
                    .as_ref()
                    .map(|i| fuzzy_score(i, query) * 0.7)
                    .unwrap_or(0.0);

                let best_score = name_score.max(id_score).max(intent_score);

                if best_score >= MIN_ENTITY_SCORE {
                    Some(SearchResult::adapter(
                        &adapter.id,
                        &adapter.name,
                        &adapter.adapter_id,
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
        results.truncate(10); // Limit entity results
        results
    }

    /// Clear all cached data
    pub fn clear(&self) {
        self.adapters.set(None);
        self.error.set(None);
    }
}
