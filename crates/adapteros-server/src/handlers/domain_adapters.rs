use crate::services::adapter_loader::{load_adapter_to_executor, unload_adapter_from_executor, test_adapter_determinism, ExecutorLoadConfig};

/// Load a domain adapter into the executor with specified configuration.
pub async fn load_domain_adapter(State(state): State<AppState>, Json(req): Json<LoadDomainAdapterRequest>) -> Result<Json<DomainAdapterResponse>, AosError> {
    let config = ExecutorLoadConfig {
        tenant_id: req.tenant_id,
        adapter_id: req.adapter_id,
        rank: req.rank.unwrap_or(16),
        alpha: req.alpha.unwrap_or(1.0),
    };
    load_adapter_to_executor(&config, &state.executor).await?;
    Ok(Json(DomainAdapterResponse { status: "loaded".to_string() }))
}

/// Unload a domain adapter from the executor by adapter ID.
pub async fn unload_domain_adapter(State(state): State<AppState>, Path(adapter_id): Path<String>) -> Result<Json<DomainAdapterResponse>, AosError> {
    unload_adapter_from_executor(&adapter_id, &state.executor).await?;
    Ok(Json(DomainAdapterResponse { status: "unloaded".to_string() }))
}

/// Test a domain adapter for determinism compliance.
pub async fn test_domain_adapter(State(state): State<AppState>, Json(req): Json<TestDomainAdapterRequest>) -> Result<Json<TestDomainAdapterResponse>, AosError> {
    let is_deterministic = test_adapter_determinism(&req.adapter_id, &state.executor).await?;
    if !is_deterministic {
        return Err(AosError::PolicyViolation("Adapter not deterministic".to_string()));
    }
    Ok(Json(TestDomainAdapterResponse { passed: true, epsilon: 0.0 }))
}
