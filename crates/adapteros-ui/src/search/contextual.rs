//! Contextual actions based on current page and selection state.
//!
//! Generates workflow-aware suggestions for the Command Palette based on
//! the current route and any selected entities.

use crate::search::{SearchAction, SearchResult, SearchResultType};
use crate::signals::page_context::RouteContext;
use crate::utils::chat_path_with_adapter;
use leptos::prelude::GetUntracked;

/// Generate contextual actions based on current page context.
///
/// Returns a vector of search results representing actions relevant to
/// the current page and selection. These actions are scored higher than
/// regular results to appear at the top of the palette.
pub fn generate_contextual_actions(ctx: &RouteContext) -> Vec<SearchResult> {
    let mut actions = Vec::new();
    let route = ctx.current_route.get_untracked();
    let entity = ctx.selected_entity.get_untracked();

    // Document page contextual actions
    if route.starts_with("/documents") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "document" {
                // Train adapter action for indexed documents
                if sel.entity_status.as_deref() == Some("indexed") {
                    actions.push(SearchResult {
                        id: format!("ctx-train-doc-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Train adapter from this document".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 2.0, // High score to appear at top
                        action: SearchAction::Navigate(format!(
                            "/training?source=document&document_id={}",
                            sel.entity_id
                        )),
                        shortcut: None,
                    });
                }

                // Reprocess action for failed documents
                if sel.entity_status.as_deref() == Some("failed") {
                    actions.push(SearchResult {
                        id: format!("ctx-retry-doc-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Retry document processing".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 2.0,
                        action: SearchAction::Execute(format!("retry-document:{}", sel.entity_id)),
                        shortcut: None,
                    });
                }

                // View chunks action for indexed documents
                if sel.entity_status.as_deref() == Some("indexed") {
                    actions.push(SearchResult {
                        id: format!("ctx-view-chunks-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "View document chunks".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 1.8,
                        action: SearchAction::Navigate(format!("/documents/{}", sel.entity_id)),
                        shortcut: None,
                    });
                }
            }
        } else {
            // No selection - show general document actions
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

    // Adapters page contextual actions
    if route.starts_with("/adapters") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "adapter" {
                // Test in chat action
                actions.push(SearchResult {
                    id: format!("ctx-chat-adapter-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Test adapter in chat".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(chat_path_with_adapter(&sel.entity_id)),
                    shortcut: None,
                });

                // View adapter details
                actions.push(SearchResult {
                    id: format!("ctx-view-adapter-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View adapter details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/adapters/{}", sel.entity_id)),
                    shortcut: None,
                });

                // Add to stack action (only for active adapters)
                if sel.entity_status.as_deref() == Some("active") {
                    actions.push(SearchResult {
                        id: format!("ctx-add-to-stack-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Add adapter to stack".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 1.7,
                        action: SearchAction::Navigate(format!(
                            "/stacks?add_adapter={}",
                            sel.entity_id
                        )),
                        shortcut: None,
                    });
                }
            }
        } else {
            // No selection - show general adapter actions
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

    // Training page contextual actions
    if route.starts_with("/training") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "training_job" {
                // For completed jobs - test the trained adapter
                if sel.entity_status.as_deref() == Some("completed") {
                    actions.push(SearchResult {
                        id: format!("ctx-chat-trained-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Open chat to test training results".to_string(),
                        subtitle: Some("Continue validation in chat".to_string()),
                        score: 2.0,
                        action: SearchAction::Navigate("/chat".to_string()),
                        shortcut: None,
                    });

                    // View trained adapter
                    actions.push(SearchResult {
                        id: format!("ctx-view-trained-adapter-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "View trained adapter".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 1.8,
                        action: SearchAction::Navigate("/adapters".to_string()),
                        shortcut: None,
                    });
                }

                // For running jobs - show cancel action
                if sel.entity_status.as_deref() == Some("running") {
                    actions.push(SearchResult {
                        id: format!("ctx-cancel-training-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Cancel training job".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 1.9,
                        action: SearchAction::Execute(format!("cancel-training:{}", sel.entity_id)),
                        shortcut: None,
                    });
                }

                // For failed jobs - show retry action
                if sel.entity_status.as_deref() == Some("failed") {
                    actions.push(SearchResult {
                        id: format!("ctx-retry-training-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Retry training job".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 2.0,
                        action: SearchAction::Execute(format!("retry-training:{}", sel.entity_id)),
                        shortcut: None,
                    });
                }
            }
        } else {
            // No selection - show start training action
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
    }

    // Chat page contextual actions
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

        actions.push(SearchResult {
            id: "ctx-toggle-reasoning".to_string(),
            result_type: SearchResultType::Action,
            title: "Toggle reasoning mode".to_string(),
            subtitle: Some("Enable/disable extended reasoning".to_string()),
            score: 1.4,
            action: SearchAction::Execute("toggle-reasoning".to_string()),
            shortcut: None,
        });
    }

    // Collections page contextual actions
    if route.starts_with("/collections") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "collection" {
                actions.push(SearchResult {
                    id: format!("ctx-search-collection-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Search in collection".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!(
                        "/collections/{}?search=true",
                        sel.entity_id
                    )),
                    shortcut: None,
                });
            }
        }
    }

    // Models page contextual actions
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

    // Workers page contextual actions
    if route.starts_with("/workers") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "worker" {
                // View worker details
                actions.push(SearchResult {
                    id: format!("ctx-view-worker-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View worker details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/workers/{}", sel.entity_id)),
                    shortcut: None,
                });

                // Drain worker (if active)
                if sel.entity_status.as_deref() == Some("active") {
                    actions.push(SearchResult {
                        id: format!("ctx-drain-worker-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Drain worker".to_string(),
                        subtitle: Some(format!("Stop accepting new work on {}", sel.entity_name)),
                        score: 1.5,
                        action: SearchAction::Execute(format!("drain-worker:{}", sel.entity_id)),
                        shortcut: None,
                    });
                }
            }
        }
    }

    // Stacks page contextual actions
    if route.starts_with("/stacks") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "stack" {
                actions.push(SearchResult {
                    id: format!("ctx-chat-with-stack-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Chat with this stack".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Navigate(format!("/chat?stack={}", sel.entity_id)),
                    shortcut: None,
                });
            }
        } else {
            actions.push(SearchResult {
                id: "ctx-create-stack".to_string(),
                result_type: SearchResultType::Action,
                title: "Create new stack".to_string(),
                subtitle: Some("Combine adapters into a stack".to_string()),
                score: 1.5,
                action: SearchAction::Navigate("/stacks?create=true".to_string()),
                shortcut: None,
            });
        }
    }

    // Repositories page contextual actions
    if route.starts_with("/repositories") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "repository" {
                actions.push(SearchResult {
                    id: format!("ctx-view-repo-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View repository".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/repositories/{}", sel.entity_id)),
                    shortcut: None,
                });

                actions.push(SearchResult {
                    id: format!("ctx-sync-repo-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Sync repository".to_string(),
                    subtitle: Some(format!("Pull latest changes for {}", sel.entity_name)),
                    score: 1.7,
                    action: SearchAction::Execute(format!("sync-repository:{}", sel.entity_id)),
                    shortcut: None,
                });
            }
        }
    }

    // Datasets page contextual actions
    if route.starts_with("/datasets") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "dataset" {
                // Train adapter from this dataset
                if sel.entity_status.as_deref() == Some("ready")
                    || sel.entity_status.as_deref() == Some("indexed")
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

                // View dataset details
                actions.push(SearchResult {
                    id: format!("ctx-view-dataset-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View dataset details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/datasets/{}", sel.entity_id)),
                    shortcut: None,
                });

                // Delete dataset (for failed datasets)
                if sel.entity_status.as_deref() == Some("failed") {
                    actions.push(SearchResult {
                        id: format!("ctx-delete-dataset-{}", sel.entity_id),
                        result_type: SearchResultType::Action,
                        title: "Delete failed dataset".to_string(),
                        subtitle: Some(sel.entity_name.clone()),
                        score: 1.5,
                        action: SearchAction::Execute(format!("delete-dataset:{}", sel.entity_id)),
                        shortcut: None,
                    });
                }
            }
        } else {
            // No selection - show general dataset actions
            actions.push(SearchResult {
                id: "ctx-upload-dataset".to_string(),
                result_type: SearchResultType::Action,
                title: "Upload dataset".to_string(),
                subtitle: Some("Upload a new training dataset".to_string()),
                score: 1.5,
                action: SearchAction::Execute("open-dataset-upload".to_string()),
                shortcut: None,
            });

            actions.push(SearchResult {
                id: "ctx-generate-dataset".to_string(),
                result_type: SearchResultType::Action,
                title: "Generate from documents".to_string(),
                subtitle: Some("Create dataset from indexed documents".to_string()),
                score: 1.4,
                action: SearchAction::Navigate("/documents".to_string()),
                shortcut: None,
            });
        }
    }

    // Runs page contextual actions (training runs / diagnostic runs)
    if route.starts_with("/runs") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "run" || sel.entity_type == "diag_run" {
                // View run details
                actions.push(SearchResult {
                    id: format!("ctx-view-run-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View run details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Navigate(format!("/runs/{}", sel.entity_id)),
                    shortcut: None,
                });

                // Compare with another run
                actions.push(SearchResult {
                    id: format!("ctx-compare-run-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Compare with another run".to_string(),
                    subtitle: Some("Open diff view".to_string()),
                    score: 1.7,
                    action: SearchAction::Navigate(format!("/runs/{}?tab=diff", sel.entity_id)),
                    shortcut: None,
                });
            }
        } else {
            // No selection - show general runs actions
            actions.push(SearchResult {
                id: "ctx-view-latest-run".to_string(),
                result_type: SearchResultType::Action,
                title: "View latest run".to_string(),
                subtitle: Some("Open the most recent diagnostic run".to_string()),
                score: 1.5,
                action: SearchAction::Execute("view-latest-run".to_string()),
                shortcut: None,
            });

            actions.push(SearchResult {
                id: "ctx-export-runs".to_string(),
                result_type: SearchResultType::Action,
                title: "Export runs".to_string(),
                subtitle: Some("Export run data to JSON".to_string()),
                score: 1.4,
                action: SearchAction::Execute("export-runs".to_string()),
                shortcut: None,
            });
        }
    }

    // Reviews page contextual actions
    if route.starts_with("/reviews") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "review" {
                // Quick approve
                actions.push(SearchResult {
                    id: format!("ctx-approve-review-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Approve this review".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 2.0,
                    action: SearchAction::Execute(format!("approve-review:{}", sel.entity_id)),
                    shortcut: None,
                });

                // Quick reject
                actions.push(SearchResult {
                    id: format!("ctx-reject-review-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Reject this review".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Execute(format!("reject-review:{}", sel.entity_id)),
                    shortcut: None,
                });
            }
        } else {
            // No selection - show general reviews actions
            actions.push(SearchResult {
                id: "ctx-refresh-reviews".to_string(),
                result_type: SearchResultType::Action,
                title: "Refresh review queue".to_string(),
                subtitle: Some("Reload pending reviews".to_string()),
                score: 1.5,
                action: SearchAction::Execute("refresh-reviews".to_string()),
                shortcut: None,
            });
        }
    }

    // Audit page contextual actions
    if route.starts_with("/audit") {
        actions.push(SearchResult {
            id: "ctx-export-audit".to_string(),
            result_type: SearchResultType::Action,
            title: "Export audit logs".to_string(),
            subtitle: Some("Download logs as JSONL".to_string()),
            score: 1.5,
            action: SearchAction::Execute("export-audit-logs".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-verify-chain".to_string(),
            result_type: SearchResultType::Action,
            title: "Verify hash chain".to_string(),
            subtitle: Some("Validate audit log integrity".to_string()),
            score: 1.4,
            action: SearchAction::Execute("verify-audit-chain".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-view-compliance".to_string(),
            result_type: SearchResultType::Action,
            title: "View compliance report".to_string(),
            subtitle: Some("Open compliance audit tab".to_string()),
            score: 1.3,
            action: SearchAction::Execute("switch-audit-tab:compliance".to_string()),
            shortcut: None,
        });
    }

    // Policies page contextual actions
    if route.starts_with("/policies") {
        if let Some(ref sel) = entity {
            if sel.entity_type == "policy" {
                // View policy
                actions.push(SearchResult {
                    id: format!("ctx-view-policy-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "View policy details".to_string(),
                    subtitle: Some(sel.entity_name.clone()),
                    score: 1.8,
                    action: SearchAction::Execute(format!("select-policy:{}", sel.entity_id)),
                    shortcut: None,
                });

                // Validate policy
                actions.push(SearchResult {
                    id: format!("ctx-validate-policy-{}", sel.entity_id),
                    result_type: SearchResultType::Action,
                    title: "Validate policy".to_string(),
                    subtitle: Some("Check policy JSON syntax".to_string()),
                    score: 1.7,
                    action: SearchAction::Execute(format!("validate-policy:{}", sel.entity_id)),
                    shortcut: None,
                });
            }
        } else {
            // No selection - show general policy actions
            actions.push(SearchResult {
                id: "ctx-create-policy".to_string(),
                result_type: SearchResultType::Action,
                title: "Create policy pack".to_string(),
                subtitle: Some("Define new enforcement rules".to_string()),
                score: 1.5,
                action: SearchAction::Execute("open-create-policy".to_string()),
                shortcut: None,
            });

            actions.push(SearchResult {
                id: "ctx-import-policy".to_string(),
                result_type: SearchResultType::Action,
                title: "Import policy".to_string(),
                subtitle: Some("Load policy from JSON file".to_string()),
                score: 1.4,
                action: SearchAction::Execute("import-policy".to_string()),
                shortcut: None,
            });
        }
    }

    // Monitoring page contextual actions
    if route.starts_with("/monitoring") {
        actions.push(SearchResult {
            id: "ctx-acknowledge-alerts".to_string(),
            result_type: SearchResultType::Action,
            title: "Acknowledge all alerts".to_string(),
            subtitle: Some("Mark all active alerts as acknowledged".to_string()),
            score: 1.5,
            action: SearchAction::Execute("acknowledge-all-alerts".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-refresh-health".to_string(),
            result_type: SearchResultType::Action,
            title: "Refresh health status".to_string(),
            subtitle: Some("Reload all health metrics".to_string()),
            score: 1.4,
            action: SearchAction::Execute("refresh-monitoring".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-view-anomalies".to_string(),
            result_type: SearchResultType::Action,
            title: "View anomalies".to_string(),
            subtitle: Some("Switch to anomalies tab".to_string()),
            score: 1.3,
            action: SearchAction::Execute("switch-monitoring-tab:anomalies".to_string()),
            shortcut: None,
        });
    }

    // Errors page contextual actions
    if route.starts_with("/errors") {
        actions.push(SearchResult {
            id: "ctx-clear-error-feed".to_string(),
            result_type: SearchResultType::Action,
            title: "Clear live feed".to_string(),
            subtitle: Some("Clear buffered errors from live view".to_string()),
            score: 1.5,
            action: SearchAction::Execute("clear-error-feed".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-pause-error-feed".to_string(),
            result_type: SearchResultType::Action,
            title: "Pause live feed".to_string(),
            subtitle: Some("Stop receiving new errors".to_string()),
            score: 1.4,
            action: SearchAction::Execute("toggle-error-feed-pause".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-create-alert-rule".to_string(),
            result_type: SearchResultType::Action,
            title: "Create alert rule".to_string(),
            subtitle: Some("Set up threshold-based alerts".to_string()),
            score: 1.3,
            action: SearchAction::Execute("open-create-alert-rule".to_string()),
            shortcut: None,
        });
    }

    // Admin page contextual actions
    if route.starts_with("/admin") {
        actions.push(SearchResult {
            id: "ctx-create-user".to_string(),
            result_type: SearchResultType::Action,
            title: "Create user".to_string(),
            subtitle: Some("Add a new user to the system".to_string()),
            score: 1.5,
            action: SearchAction::Execute("open-create-user".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-create-api-key".to_string(),
            result_type: SearchResultType::Action,
            title: "Create API key".to_string(),
            subtitle: Some("Generate a new API key".to_string()),
            score: 1.4,
            action: SearchAction::Execute("open-create-api-key".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-manage-roles".to_string(),
            result_type: SearchResultType::Action,
            title: "Manage roles".to_string(),
            subtitle: Some("View and edit role definitions".to_string()),
            score: 1.3,
            action: SearchAction::Execute("switch-admin-tab:roles".to_string()),
            shortcut: None,
        });
    }

    // Agents page contextual actions
    if route.starts_with("/agents") {
        actions.push(SearchResult {
            id: "ctx-open-worker-health".to_string(),
            result_type: SearchResultType::Action,
            title: "Open worker health".to_string(),
            subtitle: Some("Inspect worker state and capacity".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/workers".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-open-agent-files".to_string(),
            result_type: SearchResultType::Action,
            title: "Open file browser".to_string(),
            subtitle: Some("Inspect repositories and workspace files".to_string()),
            score: 1.4,
            action: SearchAction::Navigate("/files".to_string()),
            shortcut: None,
        });
    }

    // Files page contextual actions
    if route.starts_with("/files") {
        actions.push(SearchResult {
            id: "ctx-open-repositories".to_string(),
            result_type: SearchResultType::Action,
            title: "Open repositories".to_string(),
            subtitle: Some("Review connected code sources".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/repositories".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-open-documents".to_string(),
            result_type: SearchResultType::Action,
            title: "Open documents".to_string(),
            subtitle: Some("Browse indexed docs for RAG".to_string()),
            score: 1.4,
            action: SearchAction::Navigate("/documents".to_string()),
            shortcut: None,
        });
    }

    // Settings page contextual actions
    if route.starts_with("/settings") {
        actions.push(SearchResult {
            id: "ctx-open-system-status".to_string(),
            result_type: SearchResultType::Action,
            title: "Open system status".to_string(),
            subtitle: Some("View runtime and component health".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/system".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-open-user-profile".to_string(),
            result_type: SearchResultType::Action,
            title: "Open user profile".to_string(),
            subtitle: Some("Manage account and preferences".to_string()),
            score: 1.4,
            action: SearchAction::Navigate("/user".to_string()),
            shortcut: None,
        });
    }

    // System page contextual actions
    if route.starts_with("/system") {
        actions.push(SearchResult {
            id: "ctx-open-monitoring".to_string(),
            result_type: SearchResultType::Action,
            title: "Open monitoring".to_string(),
            subtitle: Some("Inspect alerts and system metrics".to_string()),
            score: 1.5,
            action: SearchAction::Navigate("/monitoring".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-open-system-settings".to_string(),
            result_type: SearchResultType::Action,
            title: "Open settings".to_string(),
            subtitle: Some("Adjust runtime and UI preferences".to_string()),
            score: 1.4,
            action: SearchAction::Navigate("/settings".to_string()),
            shortcut: None,
        });
    }

    // Routing page contextual actions
    if route.starts_with("/routing") {
        actions.push(SearchResult {
            id: "ctx-create-routing-rule".to_string(),
            result_type: SearchResultType::Action,
            title: "Create routing rule".to_string(),
            subtitle: Some("Define new adapter routing logic".to_string()),
            score: 1.5,
            action: SearchAction::Execute("open-create-routing-rule".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-view-decisions".to_string(),
            result_type: SearchResultType::Action,
            title: "View recent decisions".to_string(),
            subtitle: Some("Inspect routing decision history".to_string()),
            score: 1.4,
            action: SearchAction::Execute("switch-routing-tab:decisions".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-refresh-routing".to_string(),
            result_type: SearchResultType::Action,
            title: "Refresh routing rules".to_string(),
            subtitle: Some("Reload rule configuration".to_string()),
            score: 1.3,
            action: SearchAction::Execute("refresh-routing-rules".to_string()),
            shortcut: None,
        });
    }

    // Diff page contextual actions
    if route.starts_with("/diff") {
        actions.push(SearchResult {
            id: "ctx-compare-runs".to_string(),
            result_type: SearchResultType::Action,
            title: "Compare selected runs".to_string(),
            subtitle: Some("Execute diff on selected runs".to_string()),
            score: 1.5,
            action: SearchAction::Execute("compare-selected-runs".to_string()),
            shortcut: None,
        });

        actions.push(SearchResult {
            id: "ctx-refresh-diff-runs".to_string(),
            result_type: SearchResultType::Action,
            title: "Refresh available runs".to_string(),
            subtitle: Some("Reload the run list".to_string()),
            score: 1.4,
            action: SearchAction::Execute("refresh-diff-runs".to_string()),
            shortcut: None,
        });
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
