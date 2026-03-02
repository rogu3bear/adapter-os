//! Contextual actions based on current page and selection state.
//!
//! Generates workflow-aware suggestions for the Command Palette based on
//! the current route and any selected entities. Trimmed to core workflow
//! routes per UI_OVERBUILT_AUDIT.

use crate::search::{SearchAction, SearchResult, SearchResultType};
use crate::signals::page_context::RouteContext;
use crate::utils::chat_path_with_adapter;
use leptos::prelude::GetUntracked;

/// Generate contextual actions based on current page context.
///
/// Returns a vector of search results representing actions relevant to
/// the current page and selection. These actions are scored higher than
/// regular results to appear at the top of the palette.
///
/// Core routes only: documents, adapters, update-center, training, chat, models, workers, datasets.
pub fn generate_contextual_actions(ctx: &RouteContext) -> Vec<SearchResult> {
    let mut actions = Vec::new();
    let route = ctx.current_route.get_untracked();
    let entity = ctx.selected_entity.get_untracked();

    // Documents — upload, train from doc
    if route.starts_with("/documents") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "document" && sel.entity_status.as_deref() == Some("indexed") {
                actions.push(SearchResult {
                    id: format!("ctx-train-doc-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Create adapter from this file".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!(
                        "/training?open_wizard=1&source=document&document_id={}",
                        sel.entity_id
                    )),
                    shortcut: None,
                });
            }
        } else {
            actions.push(SearchResult {
                id: "ctx-upload-doc".to_string(),
                result_type: SearchResultType::Action,
                title: "Add your files".to_string(),
                subtitle: Some("Upload files to start creating an adapter".to_string()),
                score: 1.5,
                action: SearchAction::Execute("upload-document".to_string()),
                shortcut: None,
            });
        }
    }

    // Adapters / Update Center — run command workflow from selected skill
    if route.starts_with("/adapters") || route.starts_with("/update-center") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "adapter" {
                actions.push(SearchResult {
                    id: format!("ctx-chat-adapter-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Test adapter in chat".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(chat_path_with_adapter(&sel.entity_id)),
                    shortcut: None,
                });
                actions.push(SearchResult {
                    id: format!("ctx-run-promote-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Run Promote".to_string(),
                    subtitle: Some(format!("{} · selected skill", sel.entity_name)),
                    score: 2.0,
                    action: SearchAction::Execute("run-promote-selected-adapter".to_string()),
                    shortcut: None,
                });
                actions.push(SearchResult {
                    id: format!("ctx-run-checkout-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Run Checkout".to_string(),
                    subtitle: Some(format!("{} · selected skill", sel.entity_name)),
                    score: 1.95,
                    action: SearchAction::Execute("run-checkout-selected-adapter".to_string()),
                    shortcut: None,
                });
                actions.push(SearchResult {
                    id: format!("ctx-feed-dataset-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Feed Dataset".to_string(),
                    subtitle: Some(format!("{} · continue with dataset", sel.entity_name)),
                    score: 1.9,
                    action: SearchAction::Execute("feed-dataset-selected-adapter".to_string()),
                    shortcut: None,
                });
            }
        } else if route.starts_with("/adapters") {
            actions.push(SearchResult {
                id: "ctx-train-new-adapter".to_string(),
                result_type: SearchResultType::Action,
                title: "Create Adapter".to_string(),
                subtitle: Some("Open the adapter creation wizard".to_string()),
                score: 1.5,
                action: SearchAction::Navigate("/training?open_wizard=1".to_string()),
                shortcut: None,
            });
        }
    }

    // Training — start job
    if route.starts_with("/training") {
        actions.push(SearchResult {
            id: "ctx-start-training".to_string(),
            result_type: SearchResultType::Action,
            title: "Create Adapter".to_string(),
            subtitle: Some("Open the adapter creation wizard".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/training?open_wizard=1".to_string()),
            shortcut: None,
        });
    }

    // Chat — new chat
    if route.starts_with("/chat") {
        actions.push(SearchResult {
            id: "ctx-new-chat".to_string(),
            result_type: SearchResultType::Action,
            title: "Start new chat".to_string(),
            subtitle: Some("Clear current conversation".to_string()),
            score: 1.5,
            action: SearchAction::Execute("clear-chat".to_string()),
            shortcut: None,
        });
    }

    // Models — chat with model if selected
    if route.starts_with("/models") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "model" {
                actions.push(SearchResult {
                    id: format!("ctx-chat-with-model-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Chat with this model".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!("/chat?model={}", sel.entity_id)),
                    shortcut: None,
                });
            }
        }
    }

    // Workers — view details if selected
    if route.starts_with("/workers") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "worker" {
                actions.push(SearchResult {
                    id: format!("ctx-view-worker-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View worker details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/workers/{}", sel.entity_id)),
                    shortcut: None,
                });
            }
        }
    }

    actions
}

/// Check if a result matches a query (case-insensitive substring match).
/// Used to filter contextual actions by the current search query.
pub fn contextual_result_matches(result: &SearchResult, query: &str) -> bool {
    let query_lower = query.to_lowercase();
    result.title.to_lowercase().contains(&query_lower)
        || result.id.to_lowercase().contains(&query_lower)
        || result
            .subtitle
            .as_ref()
            .map(|s| s.to_lowercase().contains(&query_lower))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contextual_result_matches() {
        let result = SearchResult {
            id: "ctx-train-doc-123".to_string(),
            result_type: SearchResultType::Action,
            title: "Train adapter from this document".to_string(),
            subtitle: Some("My Document".to_string()),
            score: 2.0,
            action: SearchAction::Navigate("/training".to_string()),
            shortcut: None,
        };

        assert!(contextual_result_matches(&result, "train"));
        assert!(contextual_result_matches(&result, "TRAIN"));
        assert!(contextual_result_matches(&result, "adapter"));
        assert!(contextual_result_matches(&result, "document"));
        assert!(contextual_result_matches(&result, "My"));
        assert!(!contextual_result_matches(&result, "foobar"));
    }
}
