//! Provenance handler for chat sessions
//!
//! Provides get_chat_provenance for tracing session lineage.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_provenance】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::{
    AdapterProvenance, BaseModelInfo, ChatProvenanceResponse, DatasetProvenance, ProvenanceEvent,
    ProvenanceEventType, SessionSummary, StackProvenance, TrainingJobProvenance,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::debug;

use super::types::{AdapterProvenanceRow, BaseModelRow};

/// Get provenance chain for a chat session
///
/// Returns the complete lineage: chat -> stack -> adapters -> training jobs -> datasets -> base model
///
/// GET /v1/chat/sessions/:session_id/provenance
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/provenance",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Provenance retrieved", body = ChatProvenanceResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_provenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<ChatProvenanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Get session
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Verify tenant access
    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    // Count messages for the session
    let messages = state
        .db
        .get_chat_messages(&session_id, None)
        .await
        .unwrap_or_default();
    let message_count = messages.len() as i64;

    // Load captured provenance entries if any
    let provenance_entries = state
        .db
        .list_chat_provenance_for_session(&session_id)
        .await
        .unwrap_or_default();
    let entries = if provenance_entries.is_empty() {
        None
    } else {
        Some(
            provenance_entries
                .into_iter()
                .map(|p| adapteros_api_types::ChatProvenanceEntry {
                    message_id: p.message_id,
                    inference_call_id: p.inference_call_id,
                    payload_snapshot: serde_json::from_str(&p.payload_snapshot)
                        .unwrap_or(serde_json::Value::String(p.payload_snapshot)),
                    created_at: p.created_at,
                })
                .collect(),
        )
    };

    // Build session summary
    let session_summary = SessionSummary {
        id: session.id.clone(),
        name: session.name.clone(),
        tenant_id: session.tenant_id.clone(),
        stack_id: session.stack_id.clone(),
        collection_id: session.collection_id.clone(),
        created_at: session.created_at.clone(),
        last_activity_at: session.last_activity_at.clone(),
        message_count,
    };

    // Load stack and adapters if session has a stack
    let mut stack_provenance = None;
    let mut adapter_provenances = Vec::new();
    let mut base_model_info = None;
    let mut timeline_events = Vec::new();

    if let Some(ref stack_id) = session.stack_id {
        // Get stack
        if let Ok(Some(stack)) = state.db.get_stack(&session.tenant_id, stack_id).await {
            let adapter_ids: Vec<String> =
                serde_json::from_str(&stack.adapter_ids_json).unwrap_or_default();

            stack_provenance = Some(StackProvenance {
                id: stack.id.clone(),
                name: stack.name.clone(),
                description: stack.description.clone(),
                workflow_type: stack.workflow_type.clone(),
                adapter_ids: adapter_ids.clone(),
                created_at: stack.created_at.clone(),
                created_by: stack.created_by.clone(),
            });

            // Add stack creation event
            timeline_events.push(ProvenanceEvent {
                event_type: ProvenanceEventType::StackCreated,
                entity_id: stack.id.clone(),
                entity_name: stack.name.clone(),
                timestamp: stack.created_at.clone(),
                description: format!("Stack '{}' created", stack.name),
            });

            // Load each adapter with extended provenance data
            for adapter_id in &adapter_ids {
                // Query adapter with training_job_id and base_model_id (direct SQL for fields not in struct)
                // SECURITY: Include tenant_id filter to prevent cross-tenant data leakage
                let adapter_row: Option<AdapterProvenanceRow> = sqlx::query_as(
                    "SELECT id, name, hash_b3, tier, training_job_id, base_model_id, created_at
                     FROM adapters WHERE id = ? AND tenant_id = ?",
                )
                .bind(adapter_id)
                .bind(&session.tenant_id)
                .fetch_optional(state.db.pool())
                .await
                .ok()
                .flatten();

                if let Some(adapter) = adapter_row {
                    let mut training_job_prov = None;

                    // Load training job if linked
                    if let Some(ref job_id) = adapter.training_job_id {
                        if let Ok(Some(job)) = state.db.get_training_job(job_id).await {
                            // SECURITY: Validate training job belongs to session tenant
                            // Skip if tenant mismatch to prevent cross-tenant data leakage
                            if job.tenant_id.as_ref() != Some(&session.tenant_id) {
                                continue;
                            }

                            let mut dataset_prov = None;

                            // Load dataset if linked
                            if let Some(ref dataset_id) = job.dataset_id {
                                if let Ok(Some(dataset)) =
                                    state.db.get_training_dataset(dataset_id).await
                                {
                                    // SECURITY: Validate dataset belongs to session tenant
                                    // Skip dataset provenance if tenant mismatch to prevent cross-tenant data leakage
                                    if dataset.tenant_id.as_ref() == Some(&session.tenant_id) {
                                        dataset_prov = Some(DatasetProvenance {
                                            id: dataset.id.clone(),
                                            name: dataset.name.clone(),
                                            description: dataset.description.clone(),
                                            format: dataset.format.clone(),
                                            file_count: dataset.file_count,
                                            total_size_bytes: dataset.total_size_bytes,
                                            hash_b3: dataset.hash_b3.clone(),
                                            validation_status: dataset.validation_status.clone(),
                                            created_at: dataset.created_at.clone(),
                                            created_by: dataset.created_by.clone(),
                                        });

                                        // Add dataset event
                                        timeline_events.push(ProvenanceEvent {
                                            event_type: ProvenanceEventType::DatasetCreated,
                                            entity_id: dataset.id.clone(),
                                            entity_name: dataset.name.clone(),
                                            timestamp: dataset.created_at.clone(),
                                            description: format!(
                                                "Dataset '{}' created",
                                                dataset.name
                                            ),
                                        });
                                    }
                                }
                            }

                            training_job_prov = Some(TrainingJobProvenance {
                                id: job.id.clone(),
                                status: job.status.clone(),
                                started_at: job.started_at.clone(),
                                completed_at: job.completed_at.clone(),
                                created_by: job.created_by.clone(),
                                dataset: dataset_prov,
                                base_model_id: job.base_model_id.clone(),
                                config_hash_b3: job.config_hash_b3.clone(),
                            });

                            // Add training job events
                            timeline_events.push(ProvenanceEvent {
                                event_type: ProvenanceEventType::TrainingJobStarted,
                                entity_id: job.id.clone(),
                                entity_name: job
                                    .adapter_name
                                    .clone()
                                    .unwrap_or_else(|| "training".to_string()),
                                timestamp: job.started_at.clone(),
                                description: "Training job started".to_string(),
                            });

                            if let Some(ref completed_at) = job.completed_at {
                                timeline_events.push(ProvenanceEvent {
                                    event_type: ProvenanceEventType::TrainingJobCompleted,
                                    entity_id: job.id.clone(),
                                    entity_name: job
                                        .adapter_name
                                        .clone()
                                        .unwrap_or_else(|| "training".to_string()),
                                    timestamp: completed_at.clone(),
                                    description: format!(
                                        "Training job completed with status: {}",
                                        job.status
                                    ),
                                });
                            }

                            // Use base_model_id from training job for overall base model
                            if base_model_info.is_none() {
                                if let Some(ref model_id) = job.base_model_id {
                                    // Query model info
                                    let model_row: Option<BaseModelRow> = sqlx::query_as(
                                        "SELECT id, name, hash_b3, created_at FROM models WHERE id = ?",
                                    )
                                    .bind(model_id)
                                    .fetch_optional(state.db.pool())
                                    .await
                                    .ok()
                                    .flatten();

                                    if let Some(model) = model_row {
                                        base_model_info = Some(BaseModelInfo {
                                            id: model.id,
                                            name: model.name,
                                            hash_b3: model.hash_b3,
                                            created_at: model.created_at,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    adapter_provenances.push(AdapterProvenance {
                        id: adapter.id.clone(),
                        name: adapter.name.clone(),
                        hash_b3: adapter.hash_b3.clone(),
                        tier: adapter.tier.clone(),
                        externally_created: adapter.training_job_id.is_none(),
                        training_job: training_job_prov,
                        created_at: adapter.created_at.clone(),
                    });

                    // Add adapter registration event
                    timeline_events.push(ProvenanceEvent {
                        event_type: ProvenanceEventType::AdapterRegistered,
                        entity_id: adapter.id.clone(),
                        entity_name: adapter.name.clone(),
                        timestamp: adapter.created_at.clone(),
                        description: format!("Adapter '{}' registered", adapter.name),
                    });
                }
            }
        }
    }

    // Add chat started event
    timeline_events.push(ProvenanceEvent {
        event_type: ProvenanceEventType::ChatStarted,
        entity_id: session.id.clone(),
        entity_name: session.name.clone(),
        timestamp: session.created_at.clone(),
        description: format!("Chat session '{}' started", session.name),
    });

    // Sort timeline by timestamp
    timeline_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Compute provenance hash (BLAKE3 of the provenance data for audit trail)
    let provenance_data = serde_json::json!({
        "session_id": session.id,
        "stack_id": session.stack_id,
        "adapter_ids": adapter_provenances.iter().map(|a| &a.id).collect::<Vec<_>>(),
        "message_count": message_count,
    });
    let provenance_hash = blake3::hash(provenance_data.to_string().as_bytes())
        .to_hex()
        .to_string();

    let computed_at = chrono::Utc::now().to_rfc3339();

    let response = ChatProvenanceResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        session: session_summary,
        stack: stack_provenance,
        adapters: adapter_provenances,
        base_model: base_model_info,
        timeline: Some(timeline_events),
        entries,
        provenance_hash,
        computed_at,
    };

    debug!(
        session_id = %session_id,
        adapters_count = response.adapters.len(),
        has_stack = response.stack.is_some(),
        "Retrieved chat provenance"
    );

    Ok(Json(response))
}
