//! Adapter-related routes.
//!
//! This module contains all routes for:
//! - `/v1/adapters/*` - Adapter CRUD, lifecycle, health
//! - `/v1/adapter-repositories/*` - Adapter repository management
//! - `/v1/adapter-versions/*` - Adapter version management
//! - `/v1/adapter-stacks/*` - Stack composition and lifecycle

use crate::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

/// Build the adapter routes subrouter.
///
/// These routes require authentication and are merged into the protected routes.
pub fn adapter_routes() -> Router<AppState> {
    Router::new()
        // Adapter routes
        .route("/v1/adapters", get(handlers::adapters::list_adapters))
        .route(
            "/v1/adapters/{adapter_id}",
            get(handlers::adapters::get_adapter),
        )
        .route(
            "/v1/adapters/register",
            post(handlers::adapters_lifecycle::register_adapter),
        )
        .route(
            "/v1/adapters/import",
            post(handlers::adapters::import_adapter),
        )
        .route(
            "/v1/adapter-repositories",
            get(handlers::adapters::list_adapter_repositories)
                .post(handlers::create_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}",
            get(handlers::adapters::get_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/policy",
            get(handlers::adapters::get_adapter_repository_policy)
                .put(handlers::upsert_adapter_repository_policy),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/archive",
            post(handlers::archive_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/versions",
            get(handlers::adapters::list_adapter_versions),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/versions/rollback",
            post(handlers::rollback_adapter_version_handler),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/resolve-version",
            post(handlers::resolve_adapter_version_handler),
        )
        .route(
            "/v1/adapter-versions/draft",
            post(handlers::create_draft_version),
        )
        .route(
            "/v1/adapter-versions/{version_id}",
            get(handlers::adapters::get_adapter_version),
        )
        .route(
            "/v1/adapter-versions/{version_id}/promote",
            post(handlers::promote_adapter_version_handler),
        )
        .route(
            "/v1/adapter-versions/{version_id}/tag",
            post(handlers::tag_adapter_version_handler),
        )
        .route(
            "/v1/adapters/{adapter_id}",
            delete(handlers::adapters_lifecycle::delete_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/load",
            post(handlers::adapters_lifecycle::load_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/unload",
            post(handlers::adapters_lifecycle::unload_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/activate",
            post(handlers::adapters::activate_adapter),
        )
        .route(
            "/v1/adapters/verify-gpu",
            get(handlers::adapters::verify_gpu_integrity),
        )
        .route(
            "/v1/adapters/{adapter_id}/activations",
            get(handlers::adapters::get_adapter_activations),
        )
        .route(
            "/v1/adapters/{adapter_id}/usage",
            get(handlers::routing_decisions::get_adapter_usage),
        )
        // PRD-07: Lifecycle promotion/demotion (distinct from tier-based promotion)
        .route(
            "/v1/adapters/{adapter_id}/lifecycle/promote",
            post(handlers::promote_adapter_lifecycle),
        )
        .route(
            "/v1/adapters/{adapter_id}/lifecycle/demote",
            post(handlers::demote_adapter_lifecycle),
        )
        // PRD-08: Lineage and detail views
        .route(
            "/v1/adapters/{adapter_id}/lineage",
            get(handlers::adapters::get_adapter_lineage),
        )
        .route(
            "/v1/adapters/{adapter_id}/detail",
            get(handlers::adapters::get_adapter_detail),
        )
        .route(
            "/v1/adapters/{adapter_id}/manifest",
            get(handlers::download_adapter_manifest),
        )
        .route(
            "/v1/adapters/{adapter_id}/training-snapshot",
            get(handlers::adapters::get_adapter_training_snapshot),
        )
        .route(
            "/v1/adapters/{adapter_id}/training-export",
            get(handlers::adapters::export_training_provenance),
        )
        // PRD-ART-01: Adapter export as .aos file
        .route(
            "/v1/adapters/{adapter_id}/export",
            get(handlers::adapters::export_adapter),
        )
        .route(
            "/v1/adapters/directory/upsert",
            post(handlers::upsert_directory_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/health",
            get(handlers::adapters::get_adapter_health),
        )
        // Adapter pinning routes
        .route(
            "/v1/adapters/{adapter_id}/pin",
            get(handlers::get_pin_status)
                .post(handlers::pin_adapter)
                .delete(handlers::unpin_adapter),
        )
        // Adapter archive routes
        .route(
            "/v1/adapters/{adapter_id}/archive",
            get(handlers::adapters::get_archive_status)
                .post(handlers::adapters::archive_adapter)
                .delete(handlers::adapters::unarchive_adapter),
        )
        // Adapter duplicate route
        .route(
            "/v1/adapters/{adapter_id}/duplicate",
            post(handlers::adapters::duplicate_adapter),
        )
        // Tier-based state promotion (distinct from lifecycle promotion)
        .route(
            "/v1/adapters/{adapter_id}/state/promote",
            post(handlers::adapters::promote_adapter_state),
        )
        // Adapter hot-swap route
        .route("/v1/adapters/swap", post(handlers::adapters::swap_adapters))
        // Adapter statistics route
        .route(
            "/v1/adapters/{adapter_id}/stats",
            get(handlers::adapters::get_adapter_stats),
        )
        // Category policies routes
        .route(
            "/v1/adapters/category-policies",
            get(handlers::adapters::list_category_policies),
        )
        .route(
            "/v1/adapters/category-policies/{category}",
            get(handlers::adapters::get_category_policy)
                .put(handlers::adapters::update_category_policy),
        )
        // Semantic name validation routes
        .route(
            "/v1/adapters/validate-name",
            post(handlers::validate_adapter_name),
        )
        .route(
            "/v1/stacks/validate-name",
            post(handlers::validate_stack_name),
        )
        .route(
            "/v1/adapters/next-revision/{tenant}/{domain}/{purpose}",
            get(handlers::get_next_revision),
        )
        // Adapter stacks routes
        .route(
            "/v1/adapter-stacks",
            get(handlers::adapter_stacks::list_stacks),
        )
        .route(
            "/v1/adapter-stacks",
            post(handlers::adapter_stacks::create_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}",
            get(handlers::adapter_stacks::get_stack)
                .put(handlers::adapter_stacks::update_stack)
                .delete(handlers::adapter_stacks::delete_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}/history",
            get(handlers::adapter_stacks::get_stack_history),
        )
        .route(
            "/v1/adapter-stacks/{id}/policies",
            get(handlers::adapter_stacks::get_stack_policies),
        )
        .route(
            "/v1/adapter-stacks/{id}/activate",
            post(handlers::adapter_stacks::activate_stack),
        )
        .route(
            "/v1/adapter-stacks/deactivate",
            post(handlers::adapter_stacks::deactivate_stack),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_routes_builds() {
        // Verify routes compile and build without panic
        let _router: Router<AppState> = adapter_routes();
    }
}
