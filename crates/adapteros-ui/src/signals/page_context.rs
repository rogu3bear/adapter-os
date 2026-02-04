//! Global page context for tracking current route and selection state.
//!
//! This module provides a global context for tracking the current route and
//! any selected entities on the current page. This enables workflow-aware
//! suggestions in the Command Palette (Ctrl+K) based on the current context.

use leptos::prelude::*;

/// Entity selected on the current page
#[derive(Debug, Clone, Default)]
pub struct SelectedEntity {
    /// Entity type (e.g., "adapter", "document", "training_job")
    pub entity_type: String,
    /// Entity ID
    pub entity_id: String,
    /// Human-readable name for the entity
    pub entity_name: String,
    /// Optional status (e.g., "indexed", "completed", "active")
    pub entity_status: Option<String>,
}

impl SelectedEntity {
    /// Create a new selected entity
    pub fn new(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        entity_name: impl Into<String>,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            entity_name: entity_name.into(),
            entity_status: None,
        }
    }

    /// Create a new selected entity with status
    pub fn with_status(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        entity_name: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            entity_name: entity_name.into(),
            entity_status: Some(status.into()),
        }
    }
}

/// Global page context tracking current location and selection
#[derive(Clone)]
pub struct RouteContext {
    /// Current route path
    pub current_route: RwSignal<String>,
    /// Currently selected entity (if any)
    pub selected_entity: RwSignal<Option<SelectedEntity>>,
}

impl RouteContext {
    /// Create a new route context
    pub fn new() -> Self {
        Self {
            current_route: RwSignal::new(String::new()),
            selected_entity: RwSignal::new(None),
        }
    }

    /// Update the current route
    pub fn set_route(&self, route: &str) {
        self.current_route.set(route.to_string());
    }

    /// Get the current route (untracked)
    pub fn get_route(&self) -> String {
        self.current_route.get_untracked()
    }

    /// Set the selected entity
    pub fn set_selected(&self, entity: SelectedEntity) {
        self.selected_entity.set(Some(entity));
    }

    /// Clear the selected entity
    pub fn clear_selected(&self) {
        self.selected_entity.set(None);
    }

    /// Get the selected entity (untracked)
    pub fn get_selected(&self) -> Option<SelectedEntity> {
        self.selected_entity.get_untracked()
    }
}

impl Default for RouteContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide the route context to the application
pub fn provide_route_context() {
    let context = RouteContext::new();
    provide_context(context);
}

/// Get the route context from Leptos context.
/// Returns None if the context is not available.
pub fn try_use_route_context() -> Option<RouteContext> {
    use_context::<RouteContext>()
}

/// Get the route context from Leptos context.
/// Panics if the context is not available (use in components where context is guaranteed).
pub fn use_route_context() -> RouteContext {
    expect_context::<RouteContext>()
}
