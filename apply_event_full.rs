async fn apply_event<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    tenant_id: &str,
    event: &Value,
) -> adapteros_core::Result<()> {
    let event_type = event
        .get("event_type")
        .and_then(|v| v.as_str())
        .ok_or(AosError::Validation("Missing event_type".to_string()))?;

    let meta = event
        .get("metadata")
        .ok_or(AosError::Validation("Missing metadata".to_string()))?;

    match event_type {
        "adapter.registered" => {
            let id = meta
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing adapter id".to_string()))?
                .to_string();
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let rank = meta
                .get("rank")
                .and_then(|v| v.as_i64())
                .ok_or(AosError::Validation("Missing rank".to_string()))?
                as i32;
            let version = meta
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0")
                .to_string();
            let hash_b3 = meta
                .get("hash_b3")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing hash_b3".to_string()))?
                .to_string();

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO adapters 
                (tenant_id, adapter_id, name, rank, version, hash_b3, current_state, tier, created_at, updated_at) 
                VALUES (?, ?, ?, ?, ?, ?, 'unloaded', 'cold', datetime('now'), datetime('now'))
                "#
            )
            .bind(tenant_id)
            .bind(&id)
            .bind(&name)
            .bind(rank)
            .bind(&version)
            .bind(&hash_b3)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to register adapter {}: {}", id, e)))?;
        }
        "stack.created" => {
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing stack name".to_string()))?
                .to_string();
            let adapter_ids: Vec<String> = meta
                .get("adapter_ids")
                .and_then(|v| v.as_array())
                .ok_or(AosError::Validation("Missing adapter_ids".to_string()))?
                .iter()
                .filter_map(|vi| vi.as_str().map(|s| s.to_string()))
                .collect();
            let adapter_ids_json =
                serde_json::to_string(&adapter_ids).map_err(|e| AosError::Serialization(e))?;
            let workflow_type = meta
                .get("workflow_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let id = uuid::Uuid::now_v7().to_string(); // or use name as id if unique

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO adapter_stacks 
                (id, name, adapter_ids_json, workflow_type, created_at, updated_at) 
                VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
                "#,
            )
            .bind(&id)
            .bind(&name)
            .bind(&adapter_ids_json)
            .bind(&workflow_type)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to create stack {}: {}", name, e)))?;
        }
        "policy.updated" => {
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing policy name".to_string()))?
                .to_string();
            let rules: Vec<String> = meta
                .get("rules")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|vi| vi.as_str().map(|s| s.to_string()))
                .collect();
            let rules_json =
                serde_json::to_string(&rules).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO router_policies 
                (tenant_id, name, rules_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&name)
            .bind(&rules_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update policy {}: {}", name, e)))?;
        }
        "config.updated" => {
            let key = meta
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing config key".to_string()))?
                .to_string();
            let value = meta
                .get("value")
                .ok_or(AosError::Validation("Missing config value".to_string()))?
                .clone();

            let value_json =
                serde_json::to_string(&value).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update config {}: {}", key, e)))?;
        }
        "plugin.config.updated" => {
            let plugin = meta
                .get("plugin")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing plugin".to_string()))?;
            let config_key = meta
                .get("config_key")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing config_key".to_string()))?;
            let value = meta
                .get("value")
                .ok_or(AosError::Validation("Missing value".to_string()))?
                .clone();

            let key = format!("plugin.{}.{}", plugin, config_key);
            let value_json =
                serde_json::to_string(&value).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update plugin config {}: {}", key, e))
            })?;
        }
        "feature.flag.toggled" => {
            let flag = meta
                .get("flag")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing flag".to_string()))?;
            let enabled = meta
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or(AosError::Validation("Missing enabled".to_string()))?;

            let key = format!("flag.{}", flag);
            let value_json =
                serde_json::to_string(&enabled).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to toggle flag {}: {}", flag, e)))?;
        }
        _ => {
            tracing::debug!(
                "Ignored unknown event type: {} for tenant {}",
                event_type,
                tenant_id
            );
        }
    }

    Ok(())
}

// Update Request to have expected_state_hash: Option<String>
#[derive(Deserialize, ToSchema)]
pub struct HydrateTenantRequest {
    pub bundle_id: String,
    pub tenant_id: String,
    pub dry_run: bool,
    pub expected_state_hash: Option<String>,
}

// Update utoipa path to match

/// Assign policies to tenant
pub async fn assign_tenant_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignPoliciesRequest>,
) -> Result<Json<AssignPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance])?;

    // Create tenant-policy associations using Db trait method
    for policy_id in &req.policy_ids {
        state
