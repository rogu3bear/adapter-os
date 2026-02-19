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
/// Core routes only: documents, adapters, training, chat, models, workers, datasets.
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
                    title: "Train adapter from this document".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!(
                        "/training?source=document&document_id={}",
                        sel.entity_id
                    )),
                    shortcut: None,
                });
            }
        } else {
            actions.push(SearchResult {
                id: "ctx-upload-doc".to_string(),
                result_type: SearchResultType::Action,
                title: "Upload new document".to_string(),
                subtitle: Some("Add a document for RAG indexing".to_string()),
                score: 1.5,
                action: SearchAction::Execute("upload-document".to_string()),
                shortcut: None,
            });
        }
    }

    // Adapters — train new, test in chat
    if route.starts_with("/adapters") {
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
            }
        } else {
            actions.push(SearchResult {
                id: "ctx-train-new-adapter".to_string(),
                result_type: SearchResultType::Action,
                title: "Train new adapter".to_string(),
                subtitle: Some("Start a new training job".to_string()),
                score: 1.5,
                action: SearchAction::Navigate("/training".to_string()),
                shortcut: None,
            });
        }
    }

    // Training — start job
    if route.starts_with("/training") {
        actions.push(SearchResult {
            id: "ctx-start-training".to_string(),
            result_type: SearchResultType::Action,
            title: "Start new training job".to_string(),
            subtitle: Some("Train a new adapter".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/training".to_string()),
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

    // Datasets — train from dataset, upload
    if route.starts_with("/datasets") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "dataset"
                && (sel.entity_status.as_deref() == Some("ready")
                    || sel.entity_status.as_deref() == Some("indexed"))
            {
                actions.push(SearchResult {
                    id: format!("ctx-train-from-dataset-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Train adapter from this dataset".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!(
                        "/training?dataset_id={}",
                        sel.entity_id
                    )),
                    shortcut: None,
                });
            }
        } else {
            actions.push(SearchResult {
                id: "ctx-upload-dataset".to_string(),
                result_type: SearchResultType::Action,
                title: "Upload dataset".to_string(),
                subtitle: Some("Upload a new training dataset".to_string()),
                score: 1.5,
                action: SearchAction::Execute("open-dataset-upload".to_string()),
                shortcut: None,
            });
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
