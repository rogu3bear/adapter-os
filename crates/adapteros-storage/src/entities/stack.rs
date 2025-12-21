//! Adapter stack entity KV schema
//!
//! This module defines the canonical adapter stack entity for key-value storage,
//! replacing the SQL `adapter_stacks` table.

use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Workflow type for adapter stacks
///
/// Defines how adapters in a stack are executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkflowType {
    /// Adapters execute in parallel
    Parallel,
    /// Adapters execute in upstream-downstream order
    UpstreamDownstream,
    /// Adapters execute sequentially
    Sequential,
}

impl WorkflowType {
    /// Convert workflow type to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowType::Parallel => "Parallel",
            WorkflowType::UpstreamDownstream => "UpstreamDownstream",
            WorkflowType::Sequential => "Sequential",
        }
    }

    /// Parse workflow type from string
    pub fn parse_workflow(s: &str) -> Option<Self> {
        match s {
            "Parallel" => Some(WorkflowType::Parallel),
            "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
            "Sequential" => Some(WorkflowType::Sequential),
            _ => None,
        }
    }
}

impl std::fmt::Display for WorkflowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Lifecycle state for adapter stacks
///
/// Mirrors the lifecycle state used for adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "deprecated")]
    Deprecated,
    #[serde(rename = "retired")]
    Retired,
}

impl LifecycleState {
    /// Convert lifecycle state to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleState::Draft => "draft",
            LifecycleState::Active => "active",
            LifecycleState::Deprecated => "deprecated",
            LifecycleState::Retired => "retired",
        }
    }

    /// Parse lifecycle state from string
    pub fn parse_state(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Some(LifecycleState::Draft),
            "active" => Some(LifecycleState::Active),
            "deprecated" => Some(LifecycleState::Deprecated),
            "retired" => Some(LifecycleState::Retired),
            _ => None,
        }
    }
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Canonical adapter stack entity for KV storage
///
/// This struct represents the authoritative schema for adapter stack entities in the
/// key-value storage backend. It includes all fields from the SQL `adapter_stacks` table
/// with proper type conversions.
///
/// **Key Design:**
/// - Primary key: `tenant/{tenant_id}/stack/{id}`
/// - Secondary indexes:
///   - `tenant/{tenant_id}/stack-by-name/{name}` -> `{id}`
///   - `tenant/{tenant_id}/stacks-by-state/{lifecycle_state}` -> Set<{id}>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterStackKv {
    // Core identity
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,

    // Versioning
    pub version: String,
    pub lifecycle_state: LifecycleState,

    // Configuration (ordered list of adapter IDs)
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub determinism_mode: Option<String>,
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,

    // Audit
    pub created_by: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AdapterStackKv {
    /// Check if the stack is active
    pub fn is_active(&self) -> bool {
        self.lifecycle_state == LifecycleState::Active
    }

    /// Check if the stack is retired
    pub fn is_retired(&self) -> bool {
        self.lifecycle_state == LifecycleState::Retired
    }

    /// Get the number of adapters in the stack
    pub fn adapter_count(&self) -> usize {
        self.adapter_ids.len()
    }

    /// Check if the stack contains a specific adapter
    pub fn contains_adapter(&self, adapter_id: &str) -> bool {
        self.adapter_ids.iter().any(|id| id == adapter_id)
    }

    /// Get the position of an adapter in the stack (0-indexed)
    pub fn adapter_position(&self, adapter_id: &str) -> Option<usize> {
        self.adapter_ids.iter().position(|id| id == adapter_id)
    }
}

/// Conversion from SQL-like adapter stack representation
///
/// This implementation converts from a SQL-like adapter stack to the
/// new `AdapterStackKv` entity, handling type conversions and field mapping.
///
/// Note: Since there's no direct SQL struct exported from adapteros-db for adapter_stacks,
/// this conversion is provided for future migration compatibility.
impl AdapterStackKv {
    /// Convert from SQL row representation
    #[allow(clippy::too_many_arguments)]
    pub fn from_sql_row(
        id: String,
        tenant_id: String,
        name: String,
        description: Option<String>,
        adapter_ids_json: String,
        workflow_type: Option<String>,
        version: String,
        lifecycle_state: String,
        created_by: Option<String>,
        created_at: String,
        updated_at: String,
    ) -> Result<Self, String> {
        use chrono::NaiveDateTime;

        // Parse adapter IDs from JSON
        let adapter_ids: Vec<String> = serde_json::from_str(&adapter_ids_json)
            .map_err(|e| format!("Failed to parse adapter_ids_json: {}", e))?;

        // Parse workflow type
        let workflow_type = workflow_type.and_then(|wt| WorkflowType::parse_workflow(&wt));

        // Parse lifecycle state
        let lifecycle_state = LifecycleState::parse_state(&lifecycle_state)
            .ok_or_else(|| format!("Invalid lifecycle state: {}", lifecycle_state))?;

        // Parse timestamps
        let created_at = NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or_else(Utc::now);

        let updated_at = NaiveDateTime::parse_from_str(&updated_at, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or_else(Utc::now);

        let routing_determinism_mode = None;

        Ok(Self {
            id,
            tenant_id,
            name,
            description,
            version,
            lifecycle_state,
            adapter_ids,
            workflow_type,
            determinism_mode: None,
            routing_determinism_mode,
            created_by,
            created_at,
            updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_type_conversion() {
        assert_eq!(
            WorkflowType::parse_workflow("Parallel"),
            Some(WorkflowType::Parallel)
        );
        assert_eq!(
            WorkflowType::parse_workflow("UpstreamDownstream"),
            Some(WorkflowType::UpstreamDownstream)
        );
        assert_eq!(
            WorkflowType::parse_workflow("Sequential"),
            Some(WorkflowType::Sequential)
        );
        assert_eq!(WorkflowType::parse_workflow("Invalid"), None);
    }

    #[test]
    fn test_lifecycle_state_conversion() {
        assert_eq!(
            LifecycleState::parse_state("draft"),
            Some(LifecycleState::Draft)
        );
        assert_eq!(
            LifecycleState::parse_state("active"),
            Some(LifecycleState::Active)
        );
        assert_eq!(
            LifecycleState::parse_state("deprecated"),
            Some(LifecycleState::Deprecated)
        );
        assert_eq!(
            LifecycleState::parse_state("retired"),
            Some(LifecycleState::Retired)
        );
        assert_eq!(LifecycleState::parse_state("invalid"), None);
    }

    #[test]
    fn test_stack_adapter_methods() {
        let stack = AdapterStackKv {
            id: "stack-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            name: "Test Stack".to_string(),
            description: Some("A test stack".to_string()),
            version: "1.0.0".to_string(),
            lifecycle_state: LifecycleState::Active,
            adapter_ids: vec![
                "adapter-1".to_string(),
                "adapter-2".to_string(),
                "adapter-3".to_string(),
            ],
            workflow_type: Some(WorkflowType::Sequential),
            determinism_mode: None,
            routing_determinism_mode: None,
            created_by: Some("user-1".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(stack.is_active());
        assert!(!stack.is_retired());
        assert_eq!(stack.adapter_count(), 3);
        assert!(stack.contains_adapter("adapter-2"));
        assert!(!stack.contains_adapter("adapter-4"));
        assert_eq!(stack.adapter_position("adapter-2"), Some(1));
        assert_eq!(stack.adapter_position("adapter-4"), None);
    }

    #[test]
    fn test_stack_from_sql_row() {
        let result = AdapterStackKv::from_sql_row(
            "stack-1".to_string(),
            "tenant-1".to_string(),
            "Test Stack".to_string(),
            Some("A test stack".to_string()),
            r#"["adapter-1", "adapter-2", "adapter-3"]"#.to_string(),
            Some("Sequential".to_string()),
            "1.0.0".to_string(),
            "active".to_string(),
            Some("user-1".to_string()),
            "2025-11-29 12:00:00".to_string(),
            "2025-11-29 12:00:00".to_string(),
        );

        assert!(result.is_ok());
        let stack = result.unwrap();
        assert_eq!(stack.id, "stack-1");
        assert_eq!(stack.adapter_count(), 3);
        assert_eq!(stack.workflow_type, Some(WorkflowType::Sequential));
        assert_eq!(stack.lifecycle_state, LifecycleState::Active);
    }

    #[test]
    fn test_stack_from_sql_row_invalid_json() {
        let result = AdapterStackKv::from_sql_row(
            "stack-1".to_string(),
            "tenant-1".to_string(),
            "Test Stack".to_string(),
            None,
            "invalid json".to_string(),
            None,
            "1.0.0".to_string(),
            "active".to_string(),
            None,
            "2025-11-29 12:00:00".to_string(),
            "2025-11-29 12:00:00".to_string(),
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Failed to parse adapter_ids_json"));
    }
}
