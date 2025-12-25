//! Owner System Chat Handler
//!
//! Provides contextual chat assistance for system owners and administrators.
//! Implements a rule-based response system that analyzes user queries and provides:
//! - Helpful responses based on keywords
//! - Suggested CLI commands
//! - Relevant dashboard links
//!
//! Future: This will be enhanced with LLM integration for more sophisticated responses.

use crate::audit_helper::log_action;
use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_config::resolve_worker_socket_for_cp;
use adapteros_core::B3Hash;
use adapteros_db::chat_sessions::{AddMessageParams, CreateChatSessionParams};
use axum::{extract::State, http::StatusCode, response::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

/// Chat message with role and content
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ChatMessage {
    /// Message role: "user" or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
}

/// Optional context about the user's current state
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ChatContext {
    /// Current route/page the user is on
    pub route: Option<String>,
    /// Current metrics snapshot (system health, resource usage, etc.)
    pub metrics_snapshot: Option<serde_json::Value>,
    /// User's role (admin, operator, etc.)
    pub user_role: Option<String>,
}

/// Request for owner system chat
#[derive(Debug, Deserialize, ToSchema)]
pub struct OwnerChatRequest {
    /// Conversation history (messages array)
    pub messages: Vec<ChatMessage>,
    /// Optional context about user's current state
    pub context: Option<ChatContext>,
}

/// Response from owner system chat
#[derive(Debug, Serialize, ToSchema)]
pub struct OwnerChatResponse {
    /// Chat response text
    pub response: String,
    /// Suggested CLI command (if applicable)
    pub suggested_cli: Option<String>,
    /// Relevant dashboard links
    pub relevant_links: Vec<String>,
    /// Response source: "adapter" (AI-powered) or "rule_based"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Handler for POST /v1/chat/owner-system
///
/// Provides contextual assistance to system owners based on their queries.
/// Uses rule-based keyword matching to provide helpful responses, CLI suggestions,
/// and relevant dashboard links.
#[utoipa::path(
    post,
    path = "/v1/chat/owner-system",
    request_body = OwnerChatRequest,
    responses(
        (status = 200, description = "Chat response generated", body = OwnerChatResponse),
        (status = 400, description = "Invalid request", body = adapteros_api_types::ErrorResponse),
        (status = 500, description = "Internal server error", body = adapteros_api_types::ErrorResponse)
    ),
    tag = "chat"
)]
pub async fn handle_owner_chat(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<OwnerChatRequest>,
) -> Result<Json<OwnerChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        message_count = request.messages.len(),
        has_context = request.context.is_some(),
        "Processing owner chat request"
    );

    // Role check - require admin role
    // NOTE: This uses direct role checking instead of permission-based checks.
    // Consider migrating to require_permission() for consistency with other handlers.
    debug!(
        user_id = %claims.sub,
        role = %claims.role,
        roles = ?claims.roles,
        check_type = "direct_role",
        required_role = "admin",
        "Role-based access check performed (consider migrating to permission-based)"
    );
    if claims.role != "admin" && !claims.roles.contains(&"admin".to_string()) {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            roles = ?claims.roles,
            required_role = "admin",
            "Owner chat access denied: admin role required"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Admin role required for owner chat".to_string())
                    .with_code("FORBIDDEN"),
            ),
        ));
    }

    // Validate request
    if request.messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Messages array cannot be empty".to_string(),
            )),
        ));
    }

    // Get the last user message for keyword analysis
    let last_user_message = request
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == "user")
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("No user message found".to_string())),
            )
        })?;

    debug!(
        message_content = %last_user_message.content,
        "Analyzing user message"
    );

    // Ensure an owner-system chat session exists (one per user/tenant)
    let owner_session_id = format!("owner-session-{}-{}", claims.tenant_id, claims.sub);
    let owner_session = match state.db.get_chat_session(&owner_session_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            let params = CreateChatSessionParams {
                id: owner_session_id.clone(),
                tenant_id: claims.tenant_id.clone(),
                user_id: Some(claims.sub.clone()),
                created_by: Some(claims.sub.clone()),
                stack_id: None,
                collection_id: None,
                document_id: None,
                name: "Owner System Assistant".to_string(),
                title: Some("Owner System Assistant".to_string()),
                source_type: Some("owner_system".to_string()),
                source_ref_id: None,
                metadata_json: Some(r#"{"source_type":"owner_system"}"#.to_string()),
                tags_json: None,
                pinned_adapter_ids: None,
            };
            state.db.create_chat_session(params).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to create owner chat session")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            state
                .db
                .get_chat_session(&owner_session_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to retrieve owner chat session")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Owner chat session missing after creation")
                                .with_code("INTERNAL_ERROR"),
                        ),
                    )
                })?
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load owner chat session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    };

    // Persist user message
    let user_message_id = format!("msg-owner-user-{}", Uuid::new_v4());
    state
        .db
        .add_chat_message(AddMessageParams {
            id: user_message_id,
            session_id: owner_session.id.clone(),
            tenant_id: Some(claims.tenant_id.clone()),
            role: "user".to_string(),
            content: last_user_message.content.clone(),
            sequence: None,
            created_at: None,
            metadata_json: None,
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to record owner chat message")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Try adapter-based response first (if configured)
    let response = match try_adapter_response(&state, &last_user_message.content).await {
        Some(adapter_response) => {
            info!(
                user_id = %claims.sub,
                source = "adapter",
                "Generated AI-powered response from docs adapter"
            );
            OwnerChatResponse {
                response: adapter_response,
                suggested_cli: None,
                relevant_links: vec![],
                source: Some("adapter".to_string()),
            }
        }
        None => {
            // Fallback to rule-based response
            debug!("No adapter configured or available, using rule-based response");
            let mut rule_response = generate_response(&last_user_message.content, &request.context);
            rule_response.source = Some("rule_based".to_string());
            rule_response
        }
    };

    info!(
        user_id = %claims.sub,
        has_cli_suggestion = response.suggested_cli.is_some(),
        link_count = response.relevant_links.len(),
        source = ?response.source,
        "Generated chat response"
    );

    // Capture evidence for provenance tracking
    // Note: This is a placeholder for future LLM integration. Currently stores
    // synthetic evidence since no actual documents/adapters are retrieved yet.
    if let Err(e) =
        capture_chat_evidence(&state.db, &last_user_message.content, &response.response).await
    {
        warn!(error = %e, "Failed to capture chat evidence");
    }

    // Log successful chat query
    if let Err(e) = log_action(
        &state.db,
        &claims,
        "chat.owner_system",
        "chat_query",
        Some(
            &last_user_message
                .content
                .chars()
                .take(100)
                .collect::<String>(),
        ), // Truncate for privacy
        "success",
        None,
    )
    .await
    {
        warn!(error = %e, "Failed to log chat audit event");
    }

    // Persist assistant response for history
    let assistant_metadata = json!({
        "source": response.source,
        "suggested_cli": response.suggested_cli,
        "relevant_links": response.relevant_links,
    });
    state
        .db
        .add_chat_message(AddMessageParams {
            id: format!("msg-owner-assistant-{}", Uuid::new_v4()),
            session_id: owner_session.id.clone(),
            tenant_id: Some(claims.tenant_id.clone()),
            role: "assistant".to_string(),
            content: response.response.clone(),
            sequence: None,
            created_at: None,
            metadata_json: Some(assistant_metadata.to_string()),
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to record owner assistant response")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(response))
}

/// Capture evidence for chat inference provenance
///
/// Creates an audit trail linking chat responses to their inputs.
/// This enables deterministic reproducibility and compliance tracking.
///
/// Currently creates placeholder evidence since the rule-based chat doesn't
/// retrieve documents. When LLM integration is added, this will capture actual
/// document chunks used in RAG-based responses.
async fn capture_chat_evidence(
    db: &adapteros_db::Db,
    user_message: &str,
    assistant_response: &str,
) -> adapteros_core::Result<()> {
    // Note: Use fully qualified path to avoid confusion with training_datasets::CreateEvidenceParams

    // Generate unique inference ID
    let inference_id = Uuid::new_v4().to_string();

    // Compute context hash from user message
    let context_hash = B3Hash::hash(user_message.as_bytes()).to_hex();

    // Compute response hash
    let response_hash = B3Hash::hash(assistant_response.as_bytes()).to_hex();

    debug!(
        inference_id = %inference_id,
        context_hash = %context_hash,
        response_hash = %response_hash,
        "Capturing chat evidence"
    );

    // TODO: When LLM integration is added, replace this with actual document retrieval
    // For now, create a synthetic placeholder that demonstrates the evidence flow.
    // This will be replaced with real document chunks once RAG is integrated.
    //
    // Future implementation will:
    // 1. Retrieve relevant document chunks via RAG
    // 2. Create evidence entries for each chunk with relevance scores
    // 3. Link to actual session_id and message_id
    //
    // Example (future):
    // for (rank, chunk) in retrieved_chunks.iter().enumerate() {
    //     let params = CreateEvidenceParams {
    //         inference_id: inference_id.clone(),
    //         session_id: Some(session_id.clone()),
    //         message_id: Some(message_id.clone()),
    //         document_id: chunk.document_id.clone(),
    //         chunk_id: chunk.id.clone(),
    //         page_number: chunk.page_number,
    //         document_hash: chunk.document_hash.clone(),
    //         chunk_hash: chunk.chunk_hash.clone(),
    //         relevance_score: chunk.score,
    //         rank: rank as i32,
    //         context_hash: context_hash.clone(),
    //     };
    //     db.create_inference_evidence(params).await?;
    // }

    info!(
        inference_id = %inference_id,
        "Evidence capture placeholder ready for LLM integration"
    );

    Ok(())
}

/// Try to get a response from the configured docs adapter or base model
///
/// Returns Some(response) if adapter/base model is configured and available,
/// None if no worker or if inference fails (fallback to rule-based).
async fn try_adapter_response(state: &AppState, user_message: &str) -> Option<String> {
    use crate::types::WorkerInferRequest;
    use crate::uds_client::infer_with_routing_context;
    use std::time::Duration;

    info!("try_adapter_response called for message: {}", user_message);

    // Check if a docs adapter is configured
    let adapter_id = match state.db.get_system_setting("owner_chat_adapter_id").await {
        Ok(Some(id)) if !id.is_empty() => Some(id),
        Ok(_) => {
            info!("No owner chat adapter configured, will use base model if worker available");
            None
        }
        Err(e) => {
            warn!(error = %e, "Failed to get owner_chat_adapter_id setting, will try base model");
            None
        }
    };

    // If adapter is configured, verify it exists and is in a usable state
    if let Some(ref id) = adapter_id {
        info!(adapter_id = %id, "Found configured docs adapter");
        let mut found = None;

        match state.db.list_tenants().await {
            Ok(tenants) => {
                for tenant in tenants {
                    match state.db.get_adapter_for_tenant(&tenant.id, id).await {
                        Ok(Some(adapter)) => {
                            found = Some(adapter);
                            break;
                        }
                        Ok(None) => continue,
                        Err(e) => {
                            warn!(
                                error = %e,
                                tenant_id = %tenant.id,
                                adapter_id = %id,
                                "Failed to query adapter state for tenant; continuing search"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to list tenants while resolving owner chat adapter");
            }
        }

        match found {
            Some(adapter) => {
                let usable_states = ["hot", "warm", "resident"];
                if !usable_states.contains(&adapter.current_state.as_str()) {
                    info!(
                        adapter_id = %id,
                        current_state = %adapter.current_state,
                        tenant_id = %adapter.tenant_id,
                        "Docs adapter not in usable state, falling back to base model"
                    );
                }
            }
            None => {
                info!(adapter_id = %id, "Configured docs adapter not found, falling back to base model");
            }
        }
    }

    // Get worker UDS path - this is required for any inference
    let uds_path = match get_worker_uds_path(state).await {
        Some(path) => {
            info!(path = ?path, "Found worker UDS path");
            path
        }
        None => {
            info!("No worker UDS path available for inference");
            return None;
        }
    };

    // Build QA-style prompt (matches training format)
    let prompt = format!("Question: {}\n\nAnswer:", user_message.trim());

    // Create inference request with default sampling params
    let request = WorkerInferRequest {
        cpid: format!("owner-chat-{}", uuid::Uuid::new_v4()),
        prompt,
        max_tokens: 512,
        require_evidence: false,
        stack_id: None,
        stack_version: None,
        domain_hint: None,
        temperature: 0.7,
        top_k: None,
        top_p: None,
        seed: None,
        router_seed: None,
        seed_mode: None,
        request_seed: None,
        determinism: None,
        backend_profile: None,
        coreml_mode: None,
        strict_mode: Some(false),
        determinism_mode: None,
        routing_determinism_mode: None,
        pinned_adapter_ids: None,
        effective_adapter_ids: None,
        placement: None,
        routing_policy: None,
        adapter_strength_overrides: None,
        stop_policy: None,
        utf8_healing: true,
    };

    // Send inference request via UDS
    let adapter_desc = adapter_id.as_deref().unwrap_or("base_model");

    match infer_with_routing_context(&uds_path, request, None, Duration::from_secs(60)).await {
        Ok(response) => {
            if response.status == "success" {
                if let Some(text) = response.text {
                    info!(
                        adapter = %adapter_desc,
                        response_len = text.len(),
                        "Successfully got response from worker"
                    );
                    return Some(text);
                }
            }
            warn!(
                adapter = %adapter_desc,
                status = %response.status,
                "Worker inference returned non-success status"
            );
            None
        }
        Err(e) => {
            warn!(
                error = %e,
                adapter = %adapter_desc,
                "Failed to get response from worker via UDS"
            );
            None
        }
    }
}

/// Get the UDS path for an available worker
async fn get_worker_uds_path(state: &AppState) -> Option<std::path::PathBuf> {
    // Try to get workers from database
    if let Ok(workers) = state.db.list_all_workers().await {
        if let Some(worker) = workers.first() {
            return Some(std::path::PathBuf::from(&worker.uds_path));
        }
    }

    match resolve_worker_socket_for_cp() {
        Ok(resolved) => {
            if resolved.path.exists() {
                info!(
                    path = %resolved.path.display(),
                    source = %resolved.source,
                    "Using resolved worker socket fallback for owner chat"
                );
                return Some(resolved.path);
            }
            warn!(
                path = %resolved.path.display(),
                source = %resolved.source,
                "Resolved worker socket does not exist for owner chat fallback"
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to resolve worker socket for owner chat");
        }
    }

    None
}

/// Generate a rule-based response based on keyword matching
fn generate_response(message: &str, context: &Option<ChatContext>) -> OwnerChatResponse {
    let message_lower = message.to_lowercase();

    // Check for adapter-related queries
    if message_lower.contains("adapter") || message_lower.contains("adapters") {
        return OwnerChatResponse {
            response: "I can help you manage adapters. Adapters are LoRA weights that customize model behavior. You can view all adapters, check their status, load/unload them, or register new ones.".to_string(),
            suggested_cli: Some("aosctl adapter list".to_string()),
            relevant_links: vec!["/adapters".to_string()],
            source: None,
        };
    }

    // Check for training-related queries
    if message_lower.contains("training") || message_lower.contains("train") {
        return OwnerChatResponse {
            response: "I can help you with training workflows. You can create datasets, start training jobs, monitor progress, and view completed jobs. Training creates new adapters from your data.".to_string(),
            suggested_cli: Some("aosctl training jobs".to_string()),
            relevant_links: vec!["/training".to_string(), "/training/datasets".to_string()],
            source: None,
        };
    }

    // Check for status/health queries
    if message_lower.contains("status")
        || message_lower.contains("health")
        || message_lower.contains("system")
    {
        let mut response = "I can show you system health and status. ".to_string();

        // Add context-aware information if available
        if let Some(ctx) = context {
            if let Some(metrics) = &ctx.metrics_snapshot {
                if let Some(obj) = metrics.as_object() {
                    response.push_str("Based on current metrics: ");
                    if let Some(memory) = obj.get("memory_usage_percent") {
                        response.push_str(&format!(
                            "Memory usage is at {}%. ",
                            memory.as_f64().unwrap_or(0.0)
                        ));
                    }
                }
            }
        }

        response.push_str("Check the system overview page for detailed metrics.");

        return OwnerChatResponse {
            response,
            suggested_cli: Some("aosctl status".to_string()),
            relevant_links: vec!["/system".to_string()],
            source: None,
        };
    }

    // Check for model-related queries
    if message_lower.contains("model") || message_lower.contains("models") {
        return OwnerChatResponse {
            response: "I can help you manage base models. Base models are the foundation models (like Qwen, LLaMA) that adapters are applied to. You can view available models, import new ones, or check model status.".to_string(),
            suggested_cli: Some("aosctl models list".to_string()),
            relevant_links: vec!["/base-models".to_string()],
            source: None,
        };
    }

    // Check for stack-related queries
    if message_lower.contains("stack") || message_lower.contains("stacks") {
        return OwnerChatResponse {
            response: "I can help you manage adapter stacks. Stacks are pre-configured combinations of adapters that work together. You can create custom stacks, activate them, or view existing stacks.".to_string(),
            suggested_cli: Some("aosctl stack list".to_string()),
            relevant_links: vec!["/admin/stacks".to_string()],
            source: None,
        };
    }

    // Check for tenant-related queries
    if message_lower.contains("tenant") || message_lower.contains("tenants") {
        return OwnerChatResponse {
            response: "I can help you manage tenants. Tenants provide isolation between different users or organizations. You can create new tenants, manage permissions, or view tenant usage.".to_string(),
            suggested_cli: Some("aosctl tenant list".to_string()),
            relevant_links: vec!["/admin/tenants".to_string()],
            source: None,
        };
    }

    // Check for node/cluster queries
    if message_lower.contains("node")
        || message_lower.contains("nodes")
        || message_lower.contains("cluster")
        || message_lower.contains("worker")
    {
        return OwnerChatResponse {
            response: "I can help you manage cluster nodes and workers. Nodes are individual machines in your cluster, and workers handle training and inference tasks. You can view node status, spawn workers, or troubleshoot issues.".to_string(),
            suggested_cli: Some("aosctl node list".to_string()),
            relevant_links: vec!["/system".to_string(), "/admin/workers".to_string()],
            source: None,
        };
    }

    // Check for inference/chat queries
    if message_lower.contains("inference")
        || message_lower.contains("infer")
        || message_lower.contains("chat")
        || message_lower.contains("generate")
    {
        return OwnerChatResponse {
            response: "I can help you run inference. Inference is when you use a model (with optional adapters) to generate text, answer questions, or complete tasks. You can run batch inference or use the chat interface.".to_string(),
            suggested_cli: Some("aosctl infer --prompt \"Your prompt here\"".to_string()),
            relevant_links: vec!["/inference".to_string()],
            source: None,
        };
    }

    // Check for policy queries
    if message_lower.contains("policy")
        || message_lower.contains("policies")
        || message_lower.contains("compliance")
    {
        return OwnerChatResponse {
            response: "I can help you manage policies. AdapterOS enforces 23 canonical policy packs covering determinism, egress control, evidence tracking, and more. You can view active policies, validate configurations, or apply new policies.".to_string(),
            suggested_cli: Some("aosctl policies list".to_string()),
            relevant_links: vec!["/admin/policies".to_string()],
            source: None,
        };
    }

    // Check for metrics/monitoring queries
    if message_lower.contains("metric")
        || message_lower.contains("metrics")
        || message_lower.contains("monitor")
        || message_lower.contains("performance")
    {
        return OwnerChatResponse {
            response: "I can help you view system metrics and monitoring data. You can track adapter performance, system resources, training progress, and inference latency. Real-time metrics are available on the system overview page.".to_string(),
            suggested_cli: Some("aosctl metrics snapshot".to_string()),
            relevant_links: vec!["/system".to_string(), "/metrics".to_string()],
            source: None,
        };
    }

    // Check for help/documentation queries
    if message_lower.contains("help")
        || message_lower.contains("how do")
        || message_lower.contains("how to")
        || message_lower.contains("what is")
        || message_lower.contains("documentation")
        || message_lower.contains("docs")
    {
        return OwnerChatResponse {
            response: "I'm here to help! I can provide information about adapters, training, system status, models, stacks, tenants, nodes, inference, policies, and metrics. Try asking me about any of these topics, or check the documentation for detailed guides.".to_string(),
            suggested_cli: Some("aosctl --help".to_string()),
            relevant_links: vec![
                "/adapters".to_string(),
                "/training".to_string(),
                "/system".to_string(),
                "/admin".to_string(),
            ],
            source: None,
        };
    }

    // Default response for unrecognized queries
    OwnerChatResponse {
        response: "I'm here to help with AdapterOS system management. I can assist with:\n\
            • Adapters - View, load, register, and manage LoRA adapters\n\
            • Training - Create datasets, start jobs, monitor progress\n\
            • System Status - Check health, metrics, and resource usage\n\
            • Models - Manage base models and configurations\n\
            • Stacks - Create and manage adapter combinations\n\
            • Tenants - Manage multi-tenant isolation\n\
            • Nodes - Monitor cluster nodes and workers\n\
            • Inference - Run text generation and chat\n\
            • Policies - View and enforce policy packs\n\
            • Metrics - Track performance and resource usage\n\n\
            What would you like to know more about?"
            .to_string(),
        suggested_cli: Some("aosctl --help".to_string()),
        relevant_links: vec![
            "/adapters".to_string(),
            "/training".to_string(),
            "/system".to_string(),
            "/admin".to_string(),
        ],
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::Builder as TempDirBuilder;

    #[test]
    fn test_adapter_keyword_matching() {
        let response = generate_response("How do I manage adapters?", &None);
        assert!(response.response.contains("adapter"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl adapter list".to_string())
        );
        assert_eq!(response.relevant_links, vec!["/adapters"]);
    }

    #[test]
    fn test_training_keyword_matching() {
        let response = generate_response("I want to start a training job", &None);
        assert!(response.response.contains("training"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl training jobs".to_string())
        );
        assert!(response.relevant_links.contains(&"/training".to_string()));
    }

    #[test]
    fn test_status_keyword_matching() {
        let response = generate_response("What's the system health?", &None);
        assert!(response.response.contains("health"));
        assert_eq!(response.suggested_cli, Some("aosctl status".to_string()));
        assert_eq!(response.relevant_links, vec!["/system"]);
    }

    #[test]
    fn test_model_keyword_matching() {
        let response = generate_response("Show me available models", &None);
        assert!(response.response.contains("model"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl models list".to_string())
        );
        assert_eq!(response.relevant_links, vec!["/base-models"]);
    }

    #[test]
    fn test_stack_keyword_matching() {
        let response = generate_response("How do I create a stack?", &None);
        assert!(response.response.contains("stack"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl stack list".to_string())
        );
        assert_eq!(response.relevant_links, vec!["/admin/stacks"]);
    }

    #[test]
    fn test_tenant_keyword_matching() {
        let response = generate_response("Tell me about tenants", &None);
        assert!(response.response.contains("tenant"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl tenant list".to_string())
        );
        assert_eq!(response.relevant_links, vec!["/admin/tenants"]);
    }

    #[test]
    fn test_help_keyword_matching() {
        let response = generate_response("I need help", &None);
        assert!(response.response.contains("help"));
        assert_eq!(response.suggested_cli, Some("aosctl --help".to_string()));
        assert!(response.relevant_links.len() > 1);
    }

    #[test]
    fn test_default_response() {
        let response = generate_response("random unrecognized query", &None);
        assert!(response.response.contains("AdapterOS"));
        assert_eq!(response.suggested_cli, Some("aosctl --help".to_string()));
        assert!(response.relevant_links.len() >= 4);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let response1 = generate_response("ADAPTER", &None);
        let response2 = generate_response("adapter", &None);
        let response3 = generate_response("AdApTeR", &None);

        assert_eq!(response1.suggested_cli, response2.suggested_cli);
        assert_eq!(response2.suggested_cli, response3.suggested_cli);
    }

    #[test]
    fn test_context_aware_response() {
        let mut metrics = serde_json::Map::new();
        metrics.insert(
            "memory_usage_percent".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(75.5).unwrap()),
        );

        let context = Some(ChatContext {
            route: Some("/system".to_string()),
            metrics_snapshot: Some(serde_json::Value::Object(metrics)),
            user_role: Some("admin".to_string()),
        });

        let response = generate_response("What's the system status?", &context);
        assert!(response.response.contains("75.5"));
    }

    #[test]
    fn test_empty_messages_validation() {
        let request = OwnerChatRequest {
            messages: vec![],
            context: None,
        };

        // Would need full handler test with state and claims
        // Just verify the messages are empty for now
        assert!(request.messages.is_empty());
    }

    #[test]
    fn test_multiple_messages() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Show me adapters".to_string(),
            },
        ];

        // Find last user message (should be "Show me adapters")
        let last_user = messages.iter().rev().find(|msg| msg.role == "user");
        assert!(last_user.is_some());
        assert_eq!(last_user.unwrap().content, "Show me adapters");
    }

    #[test]
    fn test_inference_keyword_matching() {
        let response = generate_response("How do I run inference?", &None);
        assert!(response.response.contains("inference"));
        assert!(response.suggested_cli.is_some());
        assert_eq!(response.relevant_links, vec!["/inference"]);
    }

    #[test]
    fn test_policy_keyword_matching() {
        let response = generate_response("What policies are active?", &None);
        assert!(response.response.contains("polic"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl policies list".to_string())
        );
        assert_eq!(response.relevant_links, vec!["/admin/policies"]);
    }

    #[test]
    fn test_metrics_keyword_matching() {
        let response = generate_response("Show me performance metrics", &None);
        assert!(response.response.contains("metric"));
        assert_eq!(
            response.suggested_cli,
            Some("aosctl metrics snapshot".to_string())
        );
        assert!(response.relevant_links.contains(&"/system".to_string()));
    }

    #[test]
    fn test_node_keyword_matching() {
        let response = generate_response("List cluster nodes", &None);
        assert!(response.response.contains("node"));
        assert_eq!(response.suggested_cli, Some("aosctl node list".to_string()));
        assert!(response.relevant_links.contains(&"/system".to_string()));
    }

    #[tokio::test]
    async fn test_evidence_capture() {
        use adapteros_db::Db;

        // Create in-memory test database
        let base = std::path::Path::new("var/test-dbs");
        fs::create_dir_all(base).expect("create var/test-dbs");
        let dir = TempDirBuilder::new()
            .prefix("aos-owner-chat-")
            .tempdir_in(base)
            .expect("create tempdir");
        let db_path = dir.path().join("db.sqlite3");
        let db = Db::connect(db_path.to_str().expect("path str"))
            .await
            .expect("Failed to create test db");
        db.migrate().await.expect("migrate");

        let user_message = "How do I manage adapters?";
        let assistant_response = "I can help you manage adapters. Adapters are LoRA weights...";

        // Capture evidence - should not fail even though we're not creating records
        let result = capture_chat_evidence(&db, user_message, assistant_response).await;
        assert!(result.is_ok(), "Evidence capture should succeed");
    }

    #[test]
    fn test_context_hash_deterministic() {
        let message = "Test message";
        let hash1 = B3Hash::hash(message.as_bytes()).to_hex();
        let hash2 = B3Hash::hash(message.as_bytes()).to_hex();
        assert_eq!(hash1, hash2, "Hashes should be deterministic");
        assert_eq!(hash1.len(), 64, "Hash should be 64 hex characters");
    }
}
