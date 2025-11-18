//! Telemetry extension helpers for stack versioning (PRD-03)

use crate::state::AppState;
use adapteros_telemetry::events::{InferenceEvent, RouterDecisionEvent};

/// Extension trait for attaching stack metadata from AppState
pub trait StackMetadataExt {
    /// Attach stack metadata from the active stack for the given tenant
    ///
    /// Returns self for method chaining.
    async fn with_active_stack(self, state: &AppState, tenant_id: &str) -> Self;
}

impl StackMetadataExt for InferenceEvent {
    async fn with_active_stack(self, state: &AppState, tenant_id: &str) -> Self {
        if let Some((stack_id, stack_version)) = state.get_active_stack_metadata(tenant_id).await {
            self.with_stack_metadata(Some(stack_id), Some(stack_version))
        } else {
            // No active stack, leave fields as None
            self
        }
    }
}

impl StackMetadataExt for RouterDecisionEvent {
    async fn with_active_stack(mut self, state: &AppState, tenant_id: &str) -> Self {
        if let Some((stack_id, stack_version)) = state.get_active_stack_metadata(tenant_id).await {
            self.stack_id = Some(stack_id);
            self.stack_version = Some(stack_version);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::events::InferenceEvent;

    #[tokio::test]
    async fn test_stack_metadata_ext_no_active_stack() {
        // This would require mock AppState setup
        // For now, documenting the pattern
    }
}
