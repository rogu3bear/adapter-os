//! KV storage operations for adapter stacks
//!
//! This module provides the KV storage implementation for adapter stacks,
//! complementing the SQL implementation with dual-write support during migration.
//!
//! Keys:
//! - `tenant/{tenant_id}/stack/{stack_id}` -> AdapterStackKv (JSON)
//! - `tenant/{tenant_id}/stacks` -> Vec<stack_id> (tenant listing)
//! - `tenant/{tenant_id}/stack-by-name/{name}` -> stack_id (name lookup)
//! - `tenant/{tenant_id}/stacks-by-state/{state}` -> Vec<stack_id> (state filter)
//! - `stack-lookup/{stack_id}` -> tenant_id (cross-tenant efficient lookup)

use adapteros_core::{AosError, Result};
use adapteros_storage::entities::stack::{AdapterStackKv, LifecycleState, WorkflowType};
use adapteros_storage::KvBackend;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::traits::{CreateStackRequest, StackRecord};

fn parse_routing_mode(mode: &Option<String>) -> Option<RoutingDeterminismMode> {
    mode.as_ref()
        .and_then(|m| RoutingDeterminismMode::from_str(m).ok())
}

fn routing_mode_to_string(mode: &Option<RoutingDeterminismMode>) -> Option<String> {
    mode.as_ref().map(|m| m.as_str().to_string())
}

/// Stack operations trait for KV backend
///
/// This trait defines all stack operations that can be performed
/// on a KV storage backend, mirroring the SQL operations.
#[async_trait]
pub trait StackKvOps: Send + Sync {
    /// Create a new stack
    async fn create_stack(&self, req: &CreateStackRequest) -> Result<String>;

    /// Get a stack by ID
    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<AdapterStackKv>>;

    /// Update an existing stack
    async fn update_stack(&self, id: &str, stack: &CreateStackRequest) -> Result<bool>;

    /// Delete a stack
    async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool>;

    /// List all stacks for a tenant
    async fn list_stacks_by_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterStackKv>>;

    /// List all stacks (across all tenants)
    async fn list_all_stacks(&self) -> Result<Vec<AdapterStackKv>>;

    /// Add an adapter to a stack
    async fn add_adapter_to_stack(&self, stack_id: &str, adapter_id: &str) -> Result<()>;

    /// Remove an adapter from a stack
    async fn remove_adapter_from_stack(&self, stack_id: &str, adapter_id: &str) -> Result<()>;

    /// Reorder adapters in a stack
    async fn reorder_adapters(&self, stack_id: &str, adapter_ids: Vec<String>) -> Result<()>;

    /// Activate a stack (set lifecycle state to Active)
    async fn activate_stack(&self, stack_id: &str) -> Result<()>;

    /// Deactivate a stack (set lifecycle state to Draft)
    async fn deactivate_stack(&self, stack_id: &str) -> Result<()>;

    /// Get stack by name (for tenant)
    async fn get_stack_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<AdapterStackKv>>;
}

/// KV backend implementation for stack operations
pub struct StackKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl StackKvRepository {
    /// Create a new stack KV backend
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    /// Generate primary key for a stack
    fn primary_key(tenant_id: &str, stack_id: &str) -> String {
        format!("tenant/{}/stack/{}", tenant_id, stack_id)
    }

    /// Idempotent upsert used by migration/repair paths.
    pub async fn put_stack(&self, stack: AdapterStackKv) -> Result<()> {
        let existing = self.get_stack(&stack.tenant_id, &stack.id).await?;
        let key = Self::primary_key(&stack.tenant_id, &stack.id);
        let payload = Self::serialize(&stack)?;
        self.backend
            .set(&key, payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store stack: {}", e)))?;
        self.update_indexes(&stack, existing.as_ref()).await
    }

    /// Generate secondary index key for stack by name
    fn name_index_key(tenant_id: &str, name: &str) -> String {
        format!("tenant/{}/stack-by-name/{}", tenant_id, name)
    }

    /// Generate secondary index key for stacks by lifecycle state
    fn state_index_key(tenant_id: &str, state: &str) -> String {
        format!("tenant/{}/stacks-by-state/{}", tenant_id, state)
    }

    /// Generate tenant index key (for listing all stacks in a tenant)
    fn tenant_index_key(tenant_id: &str) -> String {
        format!("tenant/{}/stacks", tenant_id)
    }

    /// Reverse lookup key for cross-tenant stack lookups by ID
    fn lookup_key(stack_id: &str) -> String {
        format!("stack-lookup/{}", stack_id)
    }

    /// Serialize a stack to bytes
    fn serialize(stack: &AdapterStackKv) -> Result<Vec<u8>> {
        serde_json::to_vec(stack).map_err(AosError::Serialization)
    }

    /// Deserialize a stack from bytes
    fn deserialize(bytes: &[u8]) -> Result<AdapterStackKv> {
        serde_json::from_slice(bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize stack: {}", e)))
    }

    /// Deterministic ordering: created_at DESC then id ASC.
    fn sort_stacks_deterministically(stacks: &mut [AdapterStackKv]) {
        stacks.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    /// Update secondary indexes for a stack
    async fn update_indexes(
        &self,
        stack: &AdapterStackKv,
        old_stack: Option<&AdapterStackKv>,
    ) -> Result<()> {
        // Name index (tenant/{tenant_id}/stack-by-name/{name} -> {id})
        let name_key = Self::name_index_key(&stack.tenant_id, &stack.name);
        self.backend
            .set(&name_key, stack.id.as_bytes().to_vec())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update name index: {}", e)))?;

        // State index (tenant/{tenant_id}/stacks-by-state/{state} -> Set<{id}>)
        // Remove from old state if it changed
        if let Some(old) = old_stack {
            if old.lifecycle_state != stack.lifecycle_state {
                let old_state_key =
                    Self::state_index_key(&stack.tenant_id, old.lifecycle_state.as_str());
                self.remove_from_set(&old_state_key, &stack.id).await?;
            }
        }
        // Add to new state
        let state_key = Self::state_index_key(&stack.tenant_id, stack.lifecycle_state.as_str());
        self.add_to_set(&state_key, &stack.id).await?;

        // Tenant index (tenant/{tenant_id}/stacks -> Set<{id}>)
        if old_stack.is_none() {
            let tenant_key = Self::tenant_index_key(&stack.tenant_id);
            self.add_to_set(&tenant_key, &stack.id).await?;
        }

        // Reverse lookup index (stack-lookup/{stack_id} -> tenant_id)
        self.backend
            .set(
                &Self::lookup_key(&stack.id),
                stack.tenant_id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update stack lookup index: {}", e))
            })?;

        Ok(())
    }

    /// Remove a stack from all indexes
    async fn remove_from_indexes(&self, stack: &AdapterStackKv) -> Result<()> {
        // Name index
        let name_key = Self::name_index_key(&stack.tenant_id, &stack.name);
        self.backend
            .delete(&name_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to remove from name index: {}", e)))?;

        // State index
        let state_key = Self::state_index_key(&stack.tenant_id, stack.lifecycle_state.as_str());
        self.remove_from_set(&state_key, &stack.id).await?;

        // Tenant index
        let tenant_key = Self::tenant_index_key(&stack.tenant_id);
        self.remove_from_set(&tenant_key, &stack.id).await?;

        // Reverse lookup index
        let _ = self.backend.delete(&Self::lookup_key(&stack.id)).await;

        Ok(())
    }

    /// Add an item to a set stored at a key
    async fn add_to_set(&self, key: &str, value: &str) -> Result<()> {
        let current = self
            .backend
            .get(key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read set: {}", e)))?;

        let mut set: Vec<String> = match current {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to deserialize set: {}", e)))?,
            None => Vec::new(),
        };

        if !set.contains(&value.to_string()) {
            set.push(value.to_string());
            let bytes = serde_json::to_vec(&set).map_err(|e| AosError::Serialization(e))?;
            self.backend
                .set(key, bytes)
                .await
                .map_err(|e| AosError::Database(format!("Failed to write set: {}", e)))?;
        }

        Ok(())
    }

    /// Remove an item from a set stored at a key
    async fn remove_from_set(&self, key: &str, value: &str) -> Result<()> {
        let current = self
            .backend
            .get(key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read set: {}", e)))?;

        if let Some(bytes) = current {
            let mut set: Vec<String> = serde_json::from_slice(&bytes)
                .map_err(|e| AosError::Database(format!("Failed to deserialize set: {}", e)))?;

            set.retain(|item| item != value);

            if set.is_empty() {
                self.backend.delete(key).await.map_err(|e| {
                    AosError::Database(format!("Failed to delete empty set: {}", e))
                })?;
            } else {
                let bytes = serde_json::to_vec(&set).map_err(|e| AosError::Serialization(e))?;
                self.backend
                    .set(key, bytes)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to write set: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Load multiple stacks by their IDs
    async fn load_stacks(
        &self,
        tenant_id: &str,
        stack_ids: &[String],
    ) -> Result<Vec<AdapterStackKv>> {
        let mut stacks = Vec::new();

        for id in stack_ids {
            let key = Self::primary_key(tenant_id, id);
            if let Some(bytes) =
                self.backend.get(&key).await.map_err(|e| {
                    AosError::Database(format!("Failed to load stack {}: {}", id, e))
                })?
            {
                match Self::deserialize(&bytes) {
                    Ok(stack) => stacks.push(stack),
                    Err(e) => {
                        error!(stack_id = %id, error = %e, "Failed to deserialize stack");
                    }
                }
            }
        }

        Ok(stacks)
    }

    /// Update stack lifecycle state
    pub async fn update_lifecycle_state(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_state: LifecycleState,
    ) -> Result<()> {
        // Get existing stack
        let key = Self::primary_key(tenant_id, stack_id);
        let bytes = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
        {
            Some(b) => b,
            None => return Err(AosError::NotFound(format!("Stack not found: {}", stack_id))),
        };

        let mut stack = Self::deserialize(&bytes)?;
        let old_state = stack.lifecycle_state;

        // Update lifecycle state
        stack.lifecycle_state = new_state;
        stack.updated_at = Utc::now();

        // Store updated stack
        let bytes = Self::serialize(&stack)?;
        self.backend.set(&key, bytes).await.map_err(|e| {
            AosError::Database(format!("Failed to update stack lifecycle state: {}", e))
        })?;

        // Update state index if state changed
        if old_state != new_state {
            let old_state_key = Self::state_index_key(tenant_id, old_state.as_str());
            self.remove_from_set(&old_state_key, stack_id).await?;

            let new_state_key = Self::state_index_key(tenant_id, new_state.as_str());
            self.add_to_set(&new_state_key, stack_id).await?;
        }

        debug!(
            stack_id = %stack_id,
            old_state = ?old_state,
            new_state = ?new_state,
            "Stack lifecycle state updated in KV store"
        );

        Ok(())
    }

    /// Get stack by ID using reverse lookup (cross-tenant efficient)
    ///
    /// Uses the stack-lookup index to quickly find the tenant_id and retrieve
    /// the stack without scanning all stacks.
    pub async fn get_stack_by_id(&self, stack_id: &str) -> Result<Option<AdapterStackKv>> {
        // First, look up the tenant_id from the reverse index
        let lookup_key = Self::lookup_key(stack_id);
        let Some(tenant_bytes) = self
            .backend
            .get(&lookup_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read stack lookup: {}", e)))?
        else {
            return Ok(None);
        };
        let tenant_id = String::from_utf8(tenant_bytes)
            .map_err(|e| AosError::Database(format!("Invalid tenant_id in stack lookup: {}", e)))?;

        // Now fetch the stack with the known tenant_id
        let key = Self::primary_key(&tenant_id, stack_id);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
        else {
            return Ok(None);
        };

        let stack = Self::deserialize(&bytes)?;
        Ok(Some(stack))
    }

    /// Update stack version
    pub async fn update_version(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_version: &str,
    ) -> Result<()> {
        // Get existing stack
        let key = Self::primary_key(tenant_id, stack_id);
        let bytes = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
        {
            Some(b) => b,
            None => return Err(AosError::NotFound(format!("Stack not found: {}", stack_id))),
        };

        let mut stack = Self::deserialize(&bytes)?;

        // Update version
        stack.version = new_version.to_string();
        stack.updated_at = Utc::now();

        // Store updated stack
        let bytes = Self::serialize(&stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update stack version: {}", e)))?;

        debug!(
            stack_id = %stack_id,
            new_version = %new_version,
            "Stack version updated in KV store"
        );

        Ok(())
    }
}

#[async_trait]
impl StackKvOps for StackKvRepository {
    async fn create_stack(&self, req: &CreateStackRequest) -> Result<String> {
        let id = crate::new_id(adapteros_id::IdPrefix::Stk);
        let now = Utc::now();

        let workflow_type = req
            .workflow_type
            .as_ref()
            .and_then(|wt| WorkflowType::parse_workflow(wt));
        let routing_determinism_mode = parse_routing_mode(&req.routing_determinism_mode);

        let stack = AdapterStackKv {
            id: id.clone(),
            tenant_id: req.tenant_id.clone(),
            name: req.name.clone(),
            description: req.description.clone(),
            version: "1".to_string(),
            lifecycle_state: LifecycleState::Active,
            adapter_ids: req.adapter_ids.clone(),
            workflow_type,
            determinism_mode: req.determinism_mode.clone(),
            routing_determinism_mode,
            created_by: None,
            created_at: now,
            updated_at: now,
        };

        // Check if name already exists
        let name_key = Self::name_index_key(&req.tenant_id, &req.name);
        if self
            .backend
            .exists(&name_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to check name index: {}", e)))?
        {
            return Err(AosError::Database(format!(
                "Stack with name '{}' already exists for tenant '{}'",
                req.name, req.tenant_id
            ))
            .into());
        }

        // Store stack
        let key = Self::primary_key(&req.tenant_id, &id);
        let bytes = Self::serialize(&stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store stack: {}", e)))?;

        // Update indexes
        self.update_indexes(&stack, None).await?;

        info!(stack_id = %id, tenant_id = %req.tenant_id, name = %req.name, "Stack created in KV store");
        Ok(id)
    }

    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<AdapterStackKv>> {
        let key = Self::primary_key(tenant_id, id);

        let bytes = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        let stack = Self::deserialize(&bytes)?;

        // Verify tenant ownership
        if stack.tenant_id != tenant_id {
            warn!(
                stack_id = %id,
                requested_tenant = %tenant_id,
                actual_tenant = %stack.tenant_id,
                "Tenant mismatch when retrieving stack"
            );
            return Ok(None);
        }

        Ok(Some(stack))
    }

    async fn update_stack(&self, id: &str, req: &CreateStackRequest) -> Result<bool> {
        // Get existing stack
        let key = Self::primary_key(&req.tenant_id, id);
        let old_bytes = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(false),
        };

        let old_stack = Self::deserialize(&old_bytes)?;

        // Create updated stack
        let workflow_type = req
            .workflow_type
            .as_ref()
            .and_then(|wt| WorkflowType::parse_workflow(wt));
        let routing_determinism_mode = parse_routing_mode(&req.routing_determinism_mode)
            .or(old_stack.routing_determinism_mode);

        let updated_stack = AdapterStackKv {
            id: id.to_string(),
            tenant_id: req.tenant_id.clone(),
            name: req.name.clone(),
            description: req.description.clone(),
            version: {
                let should_bump = old_stack.adapter_ids != req.adapter_ids
                    || old_stack.workflow_type != workflow_type;
                let current_version = old_stack.version.parse::<u64>().unwrap_or(1);
                let new_version = if should_bump {
                    current_version + 1
                } else {
                    current_version
                };
                new_version.to_string()
            },
            lifecycle_state: old_stack.lifecycle_state,
            adapter_ids: req.adapter_ids.clone(),
            workflow_type,
            determinism_mode: req
                .determinism_mode
                .clone()
                .or(old_stack.determinism_mode.clone()),
            routing_determinism_mode,
            created_by: old_stack.created_by.clone(),
            created_at: old_stack.created_at,
            updated_at: Utc::now(),
        };

        // Store updated stack
        let bytes = Self::serialize(&updated_stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?;

        // Update indexes (only if name changed)
        if old_stack.name != updated_stack.name {
            self.update_indexes(&updated_stack, Some(&old_stack))
                .await?;
        }

        info!(stack_id = %id, tenant_id = %req.tenant_id, "Stack updated in KV store");
        Ok(true)
    }

    async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool> {
        // Get stack to verify tenant and clean up indexes
        let stack = match self.get_stack(tenant_id, id).await? {
            Some(s) => s,
            None => return Ok(false),
        };

        // Delete from storage
        let key = Self::primary_key(tenant_id, id);
        let deleted = self
            .backend
            .delete(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;

        if deleted {
            // Remove from indexes
            self.remove_from_indexes(&stack).await?;
            info!(stack_id = %id, tenant_id = %tenant_id, "Stack deleted from KV store");
        }

        Ok(deleted)
    }

    async fn list_stacks_by_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterStackKv>> {
        let tenant_key = Self::tenant_index_key(tenant_id);

        let bytes = match self
            .backend
            .get(&tenant_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list stacks: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };

        let stack_ids: Vec<String> = serde_json::from_slice(&bytes)
            .map_err(|e| AosError::Database(format!("Failed to deserialize stack IDs: {}", e)))?;

        let mut stacks = self.load_stacks(tenant_id, &stack_ids).await?;
        Self::sort_stacks_deterministically(&mut stacks);
        Ok(stacks)
    }

    async fn list_all_stacks(&self) -> Result<Vec<AdapterStackKv>> {
        // Scan all tenant prefixes
        let prefix = "tenant/";
        let keys = self
            .backend
            .scan_prefix(prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan stacks: {}", e)))?;

        let mut stacks = Vec::new();
        for key in keys {
            // Only process primary keys (tenant/{id}/stack/{stack_id})
            if key.matches('/').count() == 3 && key.contains("/stack/") {
                if let Some(bytes) = self
                    .backend
                    .get(&key)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to get stack: {}", e)))?
                {
                    match Self::deserialize(&bytes) {
                        Ok(stack) => stacks.push(stack),
                        Err(e) => {
                            error!(key = %key, error = %e, "Failed to deserialize stack");
                        }
                    }
                }
            }
        }

        Self::sort_stacks_deterministically(&mut stacks);
        Ok(stacks)
    }

    async fn add_adapter_to_stack(&self, stack_id: &str, adapter_id: &str) -> Result<()> {
        // Use efficient reverse lookup to find the stack
        let stack = self
            .get_stack_by_id(stack_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Stack not found: {}", stack_id)))?;

        let mut updated_stack = stack.clone();
        if !updated_stack.adapter_ids.contains(&adapter_id.to_string()) {
            updated_stack.adapter_ids.push(adapter_id.to_string());
            updated_stack.updated_at = Utc::now();

            let key = Self::primary_key(&stack.tenant_id, stack_id);
            let bytes = Self::serialize(&updated_stack)?;
            self.backend.set(&key, bytes).await.map_err(|e| {
                AosError::Database(format!("Failed to add adapter to stack: {}", e))
            })?;

            debug!(stack_id = %stack_id, adapter_id = %adapter_id, "Adapter added to stack");
        }

        Ok(())
    }

    async fn remove_adapter_from_stack(&self, stack_id: &str, adapter_id: &str) -> Result<()> {
        // Use efficient reverse lookup to find the stack
        let stack = self
            .get_stack_by_id(stack_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Stack not found: {}", stack_id)))?;

        let mut updated_stack = stack.clone();
        updated_stack.adapter_ids.retain(|id| id != adapter_id);
        updated_stack.updated_at = Utc::now();

        let key = Self::primary_key(&stack.tenant_id, stack_id);
        let bytes = Self::serialize(&updated_stack)?;
        self.backend.set(&key, bytes).await.map_err(|e| {
            AosError::Database(format!("Failed to remove adapter from stack: {}", e))
        })?;

        debug!(stack_id = %stack_id, adapter_id = %adapter_id, "Adapter removed from stack");
        Ok(())
    }

    async fn reorder_adapters(&self, stack_id: &str, adapter_ids: Vec<String>) -> Result<()> {
        // Use efficient reverse lookup to find the stack
        let stack = self
            .get_stack_by_id(stack_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Stack not found: {}", stack_id)))?;

        let mut updated_stack = stack.clone();
        updated_stack.adapter_ids = adapter_ids;
        updated_stack.updated_at = Utc::now();

        let key = Self::primary_key(&stack.tenant_id, stack_id);
        let bytes = Self::serialize(&updated_stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to reorder adapters: {}", e)))?;

        debug!(stack_id = %stack_id, "Adapters reordered in stack");
        Ok(())
    }

    async fn activate_stack(&self, stack_id: &str) -> Result<()> {
        // Use efficient reverse lookup to find the stack
        let stack = self
            .get_stack_by_id(stack_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Stack not found: {}", stack_id)))?;

        let mut updated_stack = stack.clone();
        let old_state = updated_stack.lifecycle_state;
        updated_stack.lifecycle_state = LifecycleState::Active;
        updated_stack.updated_at = Utc::now();

        let key = Self::primary_key(&stack.tenant_id, stack_id);
        let bytes = Self::serialize(&updated_stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to activate stack: {}", e)))?;

        // Update state index
        if old_state != LifecycleState::Active {
            self.update_indexes(&updated_stack, Some(&stack)).await?;
        }

        info!(stack_id = %stack_id, "Stack activated");
        Ok(())
    }

    async fn deactivate_stack(&self, stack_id: &str) -> Result<()> {
        // Use efficient reverse lookup to find the stack
        let stack = self
            .get_stack_by_id(stack_id)
            .await?
            .ok_or_else(|| AosError::Database(format!("Stack not found: {}", stack_id)))?;

        let mut updated_stack = stack.clone();
        let old_state = updated_stack.lifecycle_state;
        updated_stack.lifecycle_state = LifecycleState::Draft;
        updated_stack.updated_at = Utc::now();

        let key = Self::primary_key(&stack.tenant_id, stack_id);
        let bytes = Self::serialize(&updated_stack)?;
        self.backend
            .set(&key, bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to deactivate stack: {}", e)))?;

        // Update state index
        if old_state != LifecycleState::Draft {
            self.update_indexes(&updated_stack, Some(&stack)).await?;
        }

        info!(stack_id = %stack_id, "Stack deactivated");
        Ok(())
    }

    async fn get_stack_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<AdapterStackKv>> {
        let name_key = Self::name_index_key(tenant_id, name);

        let id_bytes = match self
            .backend
            .get(&name_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get stack by name: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        let id = String::from_utf8(id_bytes)
            .map_err(|e| AosError::Database(format!("Invalid stack ID in name index: {}", e)))?;

        self.get_stack(tenant_id, &id).await
    }
}

// ============================================================================
// Conversion Functions between SQL and KV Types
// ============================================================================

/// Convert SQL StackRecord to KV AdapterStackKv
pub fn stack_record_to_kv(record: &StackRecord) -> Result<AdapterStackKv> {
    use chrono::NaiveDateTime;

    // Parse adapter IDs from JSON
    let adapter_ids: Vec<String> = serde_json::from_str(&record.adapter_ids_json)
        .map_err(|e| AosError::Database(format!("Failed to parse adapter_ids_json: {}", e)))?;

    // Parse workflow type
    let workflow_type = record
        .workflow_type
        .as_ref()
        .and_then(|wt| WorkflowType::parse_workflow(wt));

    // Parse lifecycle state
    let lifecycle_state =
        LifecycleState::parse_state(&record.lifecycle_state).ok_or_else(|| {
            AosError::Database(format!(
                "Invalid lifecycle state: {}",
                record.lifecycle_state
            ))
        })?;

    // Parse timestamps
    let created_at = NaiveDateTime::parse_from_str(&record.created_at, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        .unwrap_or_else(Utc::now);

    let updated_at = NaiveDateTime::parse_from_str(&record.updated_at, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        .unwrap_or_else(Utc::now);

    let routing_determinism_mode = parse_routing_mode(&record.routing_determinism_mode);

    Ok(AdapterStackKv {
        id: record.id.clone(),
        tenant_id: record.tenant_id.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        version: record.version.to_string(),
        lifecycle_state,
        adapter_ids,
        workflow_type,
        determinism_mode: record.determinism_mode.clone(),
        created_by: record.created_by.clone(),
        created_at,
        updated_at,
        routing_determinism_mode,
    })
}

/// Convert KV AdapterStackKv to SQL StackRecord
pub fn kv_to_stack_record(kv: &AdapterStackKv) -> Result<StackRecord> {
    let adapter_ids_json =
        serde_json::to_string(&kv.adapter_ids).map_err(|e| AosError::Serialization(e))?;

    let workflow_type = kv.workflow_type.as_ref().map(|wt| wt.to_string());

    let created_at = kv.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let updated_at = kv.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();

    let version = kv.version.clone();

    let routing_determinism_mode = routing_mode_to_string(&kv.routing_determinism_mode);

    Ok(StackRecord {
        id: kv.id.clone(),
        tenant_id: kv.tenant_id.clone(),
        name: kv.name.clone(),
        description: kv.description.clone(),
        adapter_ids_json,
        workflow_type,
        lifecycle_state: kv.lifecycle_state.as_str().to_string(),
        created_at,
        updated_at,
        created_by: kv.created_by.clone(),
        version,
        determinism_mode: kv.determinism_mode.clone(),
        routing_determinism_mode,
        metadata_json: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_record_to_kv_conversion() {
        let record = StackRecord {
            id: "stack-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            name: "Test Stack".to_string(),
            description: Some("A test stack".to_string()),
            adapter_ids_json: r#"["adapter-1", "adapter-2"]"#.to_string(),
            workflow_type: Some("Sequential".to_string()),
            lifecycle_state: "active".to_string(),
            created_at: "2025-11-29 12:00:00".to_string(),
            updated_at: "2025-11-29 12:00:00".to_string(),
            created_by: Some("user-1".to_string()),
            version: "1.0.0".to_string(),
            determinism_mode: None,
            routing_determinism_mode: None,
            metadata_json: None,
        };

        let kv = stack_record_to_kv(&record).unwrap();
        assert_eq!(kv.id, "stack-1");
        assert_eq!(kv.tenant_id, "tenant-1");
        assert_eq!(kv.adapter_ids.len(), 2);
        assert_eq!(kv.workflow_type, Some(WorkflowType::Sequential));
        assert_eq!(kv.lifecycle_state, LifecycleState::Active);
    }

    #[test]
    fn test_kv_to_stack_record_conversion() {
        let kv = AdapterStackKv {
            id: "stack-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            name: "Test Stack".to_string(),
            description: Some("A test stack".to_string()),
            version: "1".to_string(),
            lifecycle_state: LifecycleState::Active,
            adapter_ids: vec!["adapter-1".to_string(), "adapter-2".to_string()],
            workflow_type: Some(WorkflowType::Sequential),
            created_by: Some("user-1".to_string()),
            determinism_mode: None,
            routing_determinism_mode: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let record = kv_to_stack_record(&kv).unwrap();
        assert_eq!(record.id, "stack-1");
        assert_eq!(record.tenant_id, "tenant-1");
        assert!(record.adapter_ids_json.contains("adapter-1"));
        assert_eq!(record.workflow_type, Some("Sequential".to_string()));
        assert_eq!(record.lifecycle_state, "active");
    }

    #[test]
    fn test_primary_key_format() {
        let key = StackKvRepository::primary_key("tenant-1", "stack-1");
        assert_eq!(key, "tenant/tenant-1/stack/stack-1");
    }

    #[test]
    fn test_name_index_key_format() {
        let key = StackKvRepository::name_index_key("tenant-1", "my-stack");
        assert_eq!(key, "tenant/tenant-1/stack-by-name/my-stack");
    }

    #[test]
    fn test_state_index_key_format() {
        let key = StackKvRepository::state_index_key("tenant-1", "active");
        assert_eq!(key, "tenant/tenant-1/stacks-by-state/active");
    }

    #[test]
    fn test_deterministic_sorting() {
        let mut stacks = vec![
            AdapterStackKv {
                id: "b".to_string(),
                tenant_id: "t".to_string(),
                name: "two".to_string(),
                description: None,
                version: "1".to_string(),
                lifecycle_state: LifecycleState::Active,
                adapter_ids: vec![],
                workflow_type: None,
                created_by: None,
                determinism_mode: None,
                routing_determinism_mode: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            AdapterStackKv {
                id: "a".to_string(),
                tenant_id: "t".to_string(),
                name: "one".to_string(),
                description: None,
                version: "1".to_string(),
                lifecycle_state: LifecycleState::Active,
                adapter_ids: vec![],
                workflow_type: None,
                created_by: None,
                determinism_mode: None,
                routing_determinism_mode: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ];

        // Force same timestamp so tie-breaker uses id ASC
        let ts = Utc::now();
        stacks[0].created_at = ts;
        stacks[1].created_at = ts;

        StackKvRepository::sort_stacks_deterministically(&mut stacks);
        assert_eq!(stacks[0].id, "a");
        assert_eq!(stacks[1].id, "b");
    }
}
