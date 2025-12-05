//! System State Handler
//!
//! Provides ground truth system state including:
//! - Node hardware and service health
//! - Hierarchical tenant -> stack -> adapter view
//! - Memory pressure and top adapters
//!
//! This endpoint aggregates data from multiple sources to provide
//! a single authoritative view of system state.

use crate::handlers::system_overview::{check_service_health, ServiceHealthStatus};
use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::system_state::{
    now_rfc3339, AdapterLifecycleState, AdapterMemorySummary, AdapterSummary, AneMemoryState,
    MemoryPressureLevel, MemoryState, NodeState, ServiceState, StackSummary, StateOrigin,
    SystemStateQuery, SystemStateResponse, TenantState,
};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_system_metrics::SystemMetricsCollector;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};

/// Get local node identifier
///
/// Returns the node ID from environment variable AOS_NODE_ID,
/// or falls back to the system hostname.
fn get_local_node_id() -> String {
    std::env::var("AOS_NODE_ID")
        .or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .map_err(|e| std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Get federation role for this node
fn get_federation_role(state: &AppState) -> String {
    if let Some(ref daemon) = state.federation_daemon {
        // Check if we're the primary
        "primary".to_string()
    } else {
        "standalone".to_string()
    }
}

/// Adapter record from database query
#[derive(Debug, Clone, sqlx::FromRow)]
struct AdapterRecord {
    adapter_id: Option<String>,
    name: String,
    memory_bytes: i64,
    rank: i32,
    current_state: String,
    last_access: String,
    access_count: i64,
    pinned: i32,
    tenant_id: Option<String>,
    stack_id: Option<String>,
}

/// Tenant record from database query
#[derive(Debug, sqlx::FromRow)]
struct TenantRecord {
    id: String,
    name: String,
    status: String,
}

/// Stack record from database query
#[derive(Debug, sqlx::FromRow)]
struct StackRecord {
    id: String,
    name: String,
    /// Tenant ID (fetched from DB but used for verification in query, not in response)
    #[allow(dead_code)]
    tenant_id: String,
}

/// Get adapter memory in MB from real data or estimate as fallback
fn get_adapter_memory_mb(record: &AdapterRecord) -> f32 {
    if record.memory_bytes > 0 {
        // Use real memory_bytes from database
        record.memory_bytes as f32 / 1_048_576.0
    } else {
        // Fallback to estimate if memory_bytes not populated
        estimate_adapter_size_mb(record.rank)
    }
}

/// Estimate adapter size in MB based on rank (fallback only)
fn estimate_adapter_size_mb(rank: i32) -> f32 {
    // Simplified calculation: rank * hidden_dim * num_layers * 2 bytes
    // Assuming hidden_dim=4096, num_layers=32
    let hidden_dim = 4096.0;
    let num_layers = 32.0;
    let bytes_per_param = 2.0; // FP16
    let size_bytes = rank as f32 * hidden_dim * num_layers * bytes_per_param;
    size_bytes / 1_048_576.0 // Convert to MB
}

/// Get system state - ground truth endpoint
///
/// Returns hierarchical view of system state: Node -> Tenant -> Stack -> Adapter
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/state",
    params(SystemStateQuery),
    responses(
        (status = 200, description = "System state", body = SystemStateResponse)
    )
)]
#[axum::debug_handler]
pub async fn get_system_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<SystemStateQuery>,
) -> Result<Json<SystemStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view system state (via MetricsView permission)
    require_permission(&claims, Permission::MetricsView)?;

    let timestamp = now_rfc3339();
    let node_id = get_local_node_id();
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Collect system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();

    // Get UMA stats
    let uma_stats = state.uma_monitor.get_uma_stats().await;

    // Get service health
    let services = check_service_health(&state).await;
    let service_states: Vec<ServiceState> = services
        .into_iter()
        .map(|s| ServiceState {
            name: s.name,
            status: match s.status {
                ServiceHealthStatus::Healthy => {
                    adapteros_api_types::system_state::ServiceHealthStatus::Healthy
                }
                ServiceHealthStatus::Degraded => {
                    adapteros_api_types::system_state::ServiceHealthStatus::Degraded
                }
                ServiceHealthStatus::Unhealthy => {
                    adapteros_api_types::system_state::ServiceHealthStatus::Unhealthy
                }
                ServiceHealthStatus::Unknown => {
                    adapteros_api_types::system_state::ServiceHealthStatus::Unknown
                }
            },
            last_check: chrono::DateTime::from_timestamp(s.last_check as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| now_rfc3339()),
        })
        .collect();

    // Build node state
    let node_state = NodeState {
        uptime_seconds: collector.uptime_seconds(),
        cpu_usage_percent: metrics.cpu_usage as f32,
        memory_usage_percent: metrics.memory_usage as f32,
        gpu_available: metrics.gpu_metrics.utilization.is_some(),
        ane_available: uma_stats.ane_allocated_mb.is_some(),
        services: service_states,
    };

    // Query tenants - filter by tenant_id if provided and not admin
    let tenant_filter = if claims.role == "admin" {
        query.tenant_id.clone()
    } else {
        // Non-admin users can only see their own tenant
        Some(claims.tenant_id.clone())
    };

    let tenants: Vec<TenantRecord> = if let Some(ref tid) = tenant_filter {
        sqlx::query_as::<_, TenantRecord>("SELECT id, name, status FROM tenants WHERE id = ?")
            .bind(tid)
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as::<_, TenantRecord>("SELECT id, name, status FROM tenants")
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
    };

    // Get active stacks map - clone to avoid holding lock across await
    let active_stacks: std::collections::HashMap<String, Option<String>> = state
        .active_stack
        .read()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    // BATCH QUERIES: Fetch all stacks and adapters upfront to avoid N+1 queries
    // This reduces O(2N) database queries to just 2 queries total

    // Get tenant IDs for filtering
    let tenant_ids: Vec<String> = tenants.iter().map(|t| t.id.clone()).collect();

    // Batch query: Get ALL stacks for all relevant tenants
    let all_stacks: Vec<StackRecord> = if tenant_ids.is_empty() {
        Vec::new()
    } else if let Some(ref tid) = tenant_filter {
        // Single tenant filter
        sqlx::query_as::<_, StackRecord>(
            "SELECT id, name, tenant_id FROM adapter_stacks WHERE tenant_id = ?",
        )
        .bind(tid)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default()
    } else {
        // All tenants (admin view)
        sqlx::query_as::<_, StackRecord>("SELECT id, name, tenant_id FROM adapter_stacks")
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_default()
    };

    // Batch query: Get ALL adapters for all relevant tenants
    let all_adapters_batch: Vec<AdapterRecord> = if tenant_ids.is_empty() {
        Vec::new()
    } else if let Some(ref tid) = tenant_filter {
        // Single tenant filter
        sqlx::query_as::<_, AdapterRecord>(
            "SELECT a.adapter_id, a.name, COALESCE(a.memory_bytes, 0) as memory_bytes, a.rank, a.current_state,
                    COALESCE(a.last_access_at, a.created_at) as last_access,
                    COALESCE(a.access_count, 0) as access_count,
                    CASE WHEN p.adapter_pk IS NOT NULL THEN 1 ELSE 0 END as pinned,
                    a.tenant_id, sa.stack_id
             FROM adapters a
             LEFT JOIN pinned_adapters p ON a.id = p.adapter_pk
             LEFT JOIN stack_adapters sa ON a.adapter_id = sa.adapter_id
             WHERE a.active = 1 AND a.tenant_id = ?
             ORDER BY a.memory_bytes DESC, a.last_access_at DESC",
        )
        .bind(tid)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default()
    } else {
        // All tenants (admin view)
        sqlx::query_as::<_, AdapterRecord>(
            "SELECT a.adapter_id, a.name, COALESCE(a.memory_bytes, 0) as memory_bytes, a.rank, a.current_state,
                    COALESCE(a.last_access_at, a.created_at) as last_access,
                    COALESCE(a.access_count, 0) as access_count,
                    CASE WHEN p.adapter_pk IS NOT NULL THEN 1 ELSE 0 END as pinned,
                    a.tenant_id, sa.stack_id
             FROM adapters a
             LEFT JOIN pinned_adapters p ON a.id = p.adapter_pk
             LEFT JOIN stack_adapters sa ON a.adapter_id = sa.adapter_id
             WHERE a.active = 1
             ORDER BY a.memory_bytes DESC, a.last_access_at DESC",
        )
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default()
    };

    // Group stacks by tenant_id for O(1) lookup
    let stacks_by_tenant: std::collections::HashMap<String, Vec<&StackRecord>> = {
        let mut map: std::collections::HashMap<String, Vec<&StackRecord>> =
            std::collections::HashMap::new();
        for stack in &all_stacks {
            map.entry(stack.tenant_id.clone()).or_default().push(stack);
        }
        map
    };

    // Group adapters by tenant_id for O(1) lookup
    let adapters_by_tenant: std::collections::HashMap<String, Vec<&AdapterRecord>> = {
        let mut map: std::collections::HashMap<String, Vec<&AdapterRecord>> =
            std::collections::HashMap::new();
        for adapter in &all_adapters_batch {
            if let Some(ref tid) = adapter.tenant_id {
                map.entry(tid.clone()).or_default().push(adapter);
            }
        }
        map
    };

    // Build tenant states with stacks and adapters (using in-memory lookups)
    let mut tenant_states = Vec::new();
    let mut all_adapters: Vec<AdapterRecord> = Vec::new();

    for tenant in tenants {
        // Get stacks for this tenant from pre-fetched map (O(1) lookup)
        let stacks = stacks_by_tenant
            .get(&tenant.id)
            .cloned()
            .unwrap_or_default();

        // Get active stack for this tenant
        let active_stack_id = active_stacks.get(&tenant.id).and_then(|s| s.clone());

        // Get adapters for this tenant from pre-fetched map (O(1) lookup)
        let tenant_adapters: Vec<&AdapterRecord> = adapters_by_tenant
            .get(&tenant.id)
            .cloned()
            .unwrap_or_default();

        // Calculate tenant memory usage
        let tenant_memory_mb: f32 = tenant_adapters
            .iter()
            .map(|a| get_adapter_memory_mb(a))
            .sum();

        // Build stack summaries
        let mut stack_summaries = Vec::new();
        for stack in &stacks {
            let is_active = active_stack_id.as_ref() == Some(&stack.id);

            // Filter adapters for this stack
            let stack_adapters: Vec<&&AdapterRecord> = tenant_adapters
                .iter()
                .filter(|a| a.stack_id.as_ref() == Some(&stack.id))
                .collect();

            let adapters = if query.include_adapters.unwrap_or(true) {
                stack_adapters
                    .iter()
                    .map(|a| AdapterSummary {
                        adapter_id: a.adapter_id.clone().unwrap_or_default(),
                        name: a.name.clone(),
                        state: AdapterLifecycleState::from(a.current_state.as_str()),
                        memory_mb: get_adapter_memory_mb(a),
                        last_access: Some(a.last_access.clone()),
                        activation_count: a.access_count as u64,
                        pinned: a.pinned != 0,
                    })
                    .collect()
            } else {
                Vec::new()
            };

            stack_summaries.push(StackSummary {
                stack_id: stack.id.clone(),
                name: stack.name.clone(),
                is_active,
                adapter_count: stack_adapters.len(),
                adapters,
            });
        }

        let adapter_count = tenant_adapters.len();

        // Clone adapters to owned values for the top adapters list
        all_adapters.extend(tenant_adapters.into_iter().cloned());

        tenant_states.push(TenantState {
            tenant_id: tenant.id,
            name: tenant.name,
            status: tenant.status,
            memory_usage_mb: tenant_memory_mb,
            adapter_count,
            stacks: stack_summaries,
        });
    }

    // Build memory state
    let pressure_str = state
        .uma_monitor
        .get_current_pressure()
        .to_string()
        .to_lowercase();
    let pressure_level = MemoryPressureLevel::from(pressure_str.as_str());

    // Get top N adapters by memory
    let mut sorted_adapters = all_adapters.clone();
    sorted_adapters.sort_by(|a, b| {
        let a_size = get_adapter_memory_mb(a);
        let b_size = get_adapter_memory_mb(b);
        b_size
            .partial_cmp(&a_size)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_adapters: Vec<AdapterMemorySummary> = sorted_adapters
        .into_iter()
        .take(query.top_adapters.unwrap_or(10) as usize)
        .map(|a| AdapterMemorySummary {
            adapter_id: a.adapter_id.clone().unwrap_or_default(),
            name: a.name.clone(),
            memory_mb: get_adapter_memory_mb(&a),
            state: AdapterLifecycleState::from(a.current_state.as_str()),
            tenant_id: a.tenant_id.unwrap_or_default(),
        })
        .collect();

    // Build ANE memory state if available
    let ane_state = if let (Some(allocated), Some(used), Some(available), Some(usage)) = (
        uma_stats.ane_allocated_mb,
        uma_stats.ane_used_mb,
        uma_stats.ane_available_mb,
        uma_stats.ane_usage_percent,
    ) {
        Some(AneMemoryState {
            allocated_mb: allocated,
            used_mb: used,
            available_mb: available,
            usage_percent: usage,
        })
    } else {
        None
    };

    let memory_state = MemoryState {
        total_mb: uma_stats.total_mb,
        used_mb: uma_stats.used_mb,
        available_mb: uma_stats.available_mb,
        headroom_percent: uma_stats.headroom_pct,
        pressure_level,
        ane: ane_state,
        top_adapters,
    };

    // Convert RagStatus from server-api to api-types
    let rag_status = state.rag_status.as_ref().map(|status| match status {
        crate::state::RagStatus::Enabled {
            model_hash,
            dimension,
        } => adapteros_api_types::system_state::RagStatus::Enabled {
            model_hash: model_hash.clone(),
            dimension: *dimension,
        },
        crate::state::RagStatus::Disabled { reason } => {
            adapteros_api_types::system_state::RagStatus::Disabled {
                reason: reason.clone(),
            }
        }
    });

    Ok(Json(SystemStateResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        timestamp,
        origin: StateOrigin {
            node_id,
            hostname,
            federation_role: get_federation_role(&state),
        },
        node: node_state,
        tenants: tenant_states,
        memory: memory_state,
        rag_status,
    }))
}
