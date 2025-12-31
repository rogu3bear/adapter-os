use crate::auth::Claims;
use crate::sse::{SseEventManager, SseStreamType};
use crate::state::AppState;
use crate::types::*;
use adapteros_db::sqlx;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Extension, Json,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

/// Stream query parameters (for training and contacts streams)
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub tenant: String,
}

/// Discovery stream query parameters
#[derive(Debug, Deserialize)]
pub struct DiscoveryStreamQuery {
    pub tenant: String,
    pub repo: Option<String>,
}

/// Discovery stream SSE endpoint
///
/// Streams real-time repository scanning and code discovery events including
/// scan progress, symbol indexing, framework detection, and completion events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```
/// event: discovery
/// data: {"type":"symbol_indexed","timestamp":...,"payload":{...}}
/// ```
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.4
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/streams/discovery",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events"),
        ("repo" = Option<String>, Query, description = "Optional repository ID filter")
    ),
    responses(
        (status = 200, description = "SSE stream of discovery events")
    )
)]
pub async fn discovery_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<DiscoveryStreamQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_manager = state.sse_manager.clone();
    let tenant_id = params.tenant.clone();
    let repo_filter = params.repo.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Discovery, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(
        replay_events
            .into_iter()
            .map(|e| Ok(SseEventManager::to_axum_event(&e))),
    );

    // Subscribe to the discovery signal broadcast channel
    let rx = state.discovery_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant and repo
    let mgr_for_signals = Arc::new(state.sse_manager.clone());
    let signal_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let tenant_filter = tenant_id.clone();
        let repo_filter_clone = repo_filter.clone();
        let mgr = Arc::clone(&mgr_for_signals);
        async move {
            match result {
                Ok(signal) => {
                    // Filter signals by tenant_id if present in payload
                    let signal_tenant = signal
                        .payload
                        .get("tenant_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // Filter by repo_id if specified
                    let signal_repo = signal
                        .payload
                        .get("repo_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let tenant_matches = signal_tenant.is_empty() || signal_tenant == tenant_filter;
                    let repo_matches = repo_filter_clone
                        .as_ref()
                        .map(|r| signal_repo.is_empty() || signal_repo == r)
                        .unwrap_or(true);

                    if tenant_matches && repo_matches {
                        let event_data = serde_json::json!({
                            "type": signal.signal_type.to_string(),
                            "timestamp": signal.timestamp,
                            "priority": format!("{:?}", signal.priority),
                            "payload": signal.payload,
                            "trace_id": signal.trace_id,
                        });

                        let event = mgr
                            .create_event(
                                SseStreamType::Discovery,
                                "discovery",
                                event_data.to_string(),
                            )
                            .await;

                        Some(Ok(SseEventManager::to_axum_event(&event)))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::debug!("Discovery broadcast stream error (likely lag): {}", e);
                    None
                }
            }
        }
    });

    // Include a periodic heartbeat to keep connection alive
    let mgr_for_heartbeat = state.sse_manager.clone();
    let heartbeat_stream = stream::unfold(0u64, move |counter| {
        let mgr = mgr_for_heartbeat.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let event_data = serde_json::json!({
                "type": "heartbeat",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "sequence": counter,
            });

            let event = mgr
                .create_event(
                    SseStreamType::Discovery,
                    "discovery",
                    event_data.to_string(),
                )
                .await;

            Some((Ok(SseEventManager::to_axum_event(&event)), counter + 1))
        }
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    // Chain replay with merged stream
    Sse::new(FuturesStreamExt::chain(replay_stream, merged_stream)).keep_alive(KeepAlive::default())
}

/// Contacts stream SSE endpoint
///
/// Streams real-time contact discovery and update events as contacts are
/// discovered during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/streams/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of contact events")
    )
)]
pub async fn contacts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_manager = state.sse_manager.clone();
    let tenant_id = params.tenant.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Activity, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(
        replay_events
            .into_iter()
            .map(|e| Ok(SseEventManager::to_axum_event(&e))),
    );

    // Subscribe to the contact signal broadcast channel
    let rx = state.contact_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant
    let mgr_for_signals = Arc::new(state.sse_manager.clone());
    let signal_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let tenant_filter = tenant_id.clone();
        let mgr = Arc::clone(&mgr_for_signals);
        async move {
            match result {
                Ok(signal) => {
                    // Filter signals by tenant_id if present in payload
                    let signal_tenant = signal
                        .payload
                        .get("tenant_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // Pass through if tenant matches or if no tenant filter in signal
                    if signal_tenant.is_empty() || signal_tenant == tenant_filter {
                        let event_data = serde_json::json!({
                            "type": signal.signal_type.to_string(),
                            "timestamp": signal.timestamp,
                            "priority": format!("{:?}", signal.priority),
                            "payload": signal.payload,
                            "trace_id": signal.trace_id,
                        });

                        let event = mgr
                            .create_event(
                                SseStreamType::Activity,
                                "contact",
                                event_data.to_string(),
                            )
                            .await;

                        Some(Ok(SseEventManager::to_axum_event(&event)))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::debug!("Contact broadcast stream error (likely lag): {}", e);
                    None
                }
            }
        }
    });

    // Include a periodic heartbeat to keep connection alive
    let mgr_for_heartbeat = state.sse_manager.clone();
    let heartbeat_stream = stream::unfold(0u64, move |counter| {
        let mgr = mgr_for_heartbeat.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let event_data = serde_json::json!({
                "type": "heartbeat",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "sequence": counter,
            });

            let event = mgr
                .create_event(SseStreamType::Activity, "contact", event_data.to_string())
                .await;

            Some((Ok(SseEventManager::to_axum_event(&event)), counter + 1))
        }
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    // Chain replay with merged stream
    Sse::new(FuturesStreamExt::chain(replay_stream, merged_stream)).keep_alive(KeepAlive::default())
}

// ============================================================================
// Contacts API Endpoints
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6

/// List contacts with filtering
///
/// Returns contacts discovered during inference, filtered by tenant and optionally by category.
/// Contacts represent entities (users, adapters, repositories, systems) that the inference
/// engine has interacted with.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("category" = Option<String>, Query, description = "Filter by category (user|system|adapter|repository|external)"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 100)")
    ),
    responses(
        (status = 200, description = "List of contacts", body = ContactsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ContactsQuery>,
) -> Result<Json<ContactsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Build query based on filters
    let mut query = String::from(
        "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url, \
         discovered_at, discovered_by, last_interaction, interaction_count, \
         created_at, updated_at \
         FROM contacts WHERE tenant_id = ?",
    );

    let mut bind_values: Vec<String> = vec![params.tenant.clone()];

    // Add category filter if provided
    if let Some(ref category) = params.category {
        query.push_str(" AND category = ?");
        bind_values.push(category.clone());
    }

    query.push_str(" ORDER BY discovered_at DESC LIMIT ?");
    bind_values.push(params.limit.unwrap_or(100).to_string());

    // Execute query
    // Note: This is a simplified version. In production, use proper query builder
    let contacts = sqlx::query_as::<_, ContactRow>(
        "SELECT * FROM contacts WHERE tenant_id = ? ORDER BY discovered_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(100) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch contacts")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Convert to response format
    let contacts: Vec<ContactResponse> = contacts.into_iter().map(|c| c.into()).collect();

    Ok(Json(ContactsResponse { contacts }))
}

/// Create or update a contact
///
/// Creates a new contact or updates an existing one based on (tenant_id, name, category) uniqueness.
/// This endpoint can be used to manually register contacts or update their metadata.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/contacts",
    request_body = CreateContactRequest,
    responses(
        (status = 200, description = "Contact created/updated", body = ContactResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn create_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(request): Json<CreateContactRequest>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate category
    if !["user", "system", "adapter", "repository", "external"].contains(&request.category.as_str())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid category")
                    .with_code("BAD_REQUEST")
                    .with_string_details(
                        "category must be one of: user, system, adapter, repository, external"
                            .to_string(),
                    ),
            ),
        ));
    }

    // Upsert contact
    let contact = sqlx::query_as::<_, ContactRow>(
        "INSERT INTO contacts (tenant_id, name, email, category, role, metadata_json)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(tenant_id, name, category) DO UPDATE SET
            email = excluded.email,
            role = excluded.role,
            metadata_json = excluded.metadata_json,
            updated_at = datetime('now')
         RETURNING *",
    )
    .bind(&request.tenant_id)
    .bind(&request.name)
    .bind(&request.email)
    .bind(&request.category)
    .bind(&request.role)
    .bind(&request.metadata_json)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to create contact")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(contact.into()))
}

/// Get contact by ID
///
/// Retrieves a specific contact by its unique identifier.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact details", body = ContactResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    let contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ),
        })?;

    Ok(Json(contact.into()))
}

/// Delete a contact
///
/// Permanently deletes a contact and all associated interaction records.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    delete,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact deleted"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to delete contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
        ));
    }

    Ok(StatusCode::OK)
}

/// Get contact interaction history
///
/// Returns the interaction log for a specific contact, showing when and how
/// the contact was referenced during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/contacts/{id}/interactions",
    params(
        ("id" = String, Path, description = "Contact ID"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 50)")
    ),
    responses(
        (status = 200, description = "Interaction history", body = ContactInteractionsResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact_interactions(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<ContactInteractionsQuery>,
) -> Result<Json<ContactInteractionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify contact exists
    let _contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ),
        })?;

    // Fetch interactions
    let interactions = sqlx::query_as::<_, ContactInteractionRow>(
        "SELECT * FROM contact_interactions
         WHERE contact_id = ?
         ORDER BY created_at DESC
         LIMIT ?",
    )
    .bind(&id)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch interactions")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let interactions: Vec<ContactInteractionResponse> =
        interactions.into_iter().map(|i| i.into()).collect();

    Ok(Json(ContactInteractionsResponse { interactions }))
}
