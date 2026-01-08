# AdapterOS API Route Map

> **Auto-generated:** Do not edit manually.
> Run `./scripts/dev/generate_route_map.sh` to regenerate.
>
> Generated: 2026-01-07 07:13 UTC

## Overview

| Metric | Count |
|--------|-------|
| **Total Route Registrations** | 443 |
| **Extracted Handler Mappings** | 487 |

## Route Table

| Method | Path | Handler | Test File |
|--------|------|---------|-----------|
| `POST` | `/admin/lifecycle/request-maintenance` | `handlers::admin_lifecycle::request_maintenance` | `UNKNOWN` |
| `POST` | `/admin/lifecycle/request-shutdown` | `handlers::admin_lifecycle::request_shutdown` | `UNKNOWN` |
| `POST` | `/admin/lifecycle/safe-restart` | `handlers::admin_lifecycle::safe_restart` | `UNKNOWN` |
| `GET` | `/healthz` | `handlers::health` | `UNKNOWN` |
| `GET` | `/healthz/all` | `crate::health::check_all_health` | `UNKNOWN` |
| `GET` | `/healthz/{component}` | `crate::health::check_component_health` | `UNKNOWN` |
| `GET` | `/metrics` | `handlers::metrics_handler` | `UNKNOWN` |
| `GET` | `/readyz` | `handlers::ready` | `UNKNOWN` |
| `GET` | `/system/ready` | `system_ready` | `UNKNOWN` |
| `GET` | `/v1/activity/events` | `handlers::activity::list_activity_events` | `UNKNOWN` |
| `POST` | `/v1/activity/events` | `handlers::activity::create_activity_event` | `UNKNOWN` |
| `GET` | `/v1/activity/feed` | `handlers::activity::list_user_workspace_activity` | `UNKNOWN` |
| `GET` | `/v1/adapter-repositories` | `handlers::adapters::list_adapter_repositories` | `UNKNOWN` |
| `POST` | `/v1/adapter-repositories` | `handlers::create_adapter_repository` | `UNKNOWN` |
| `GET` | `/v1/adapter-repositories/{repo_id}` | `handlers::adapters::get_adapter_repository` | `UNKNOWN` |
| `POST` | `/v1/adapter-repositories/{repo_id}/archive` | `handlers::archive_adapter_repository` | `UNKNOWN` |
| `GET` | `/v1/adapter-repositories/{repo_id}/policy` | `handlers::adapters::get_adapter_repository_policy` | `UNKNOWN` |
| `PUT` | `/v1/adapter-repositories/{repo_id}/policy` | `handlers::upsert_adapter_repository_policy` | `UNKNOWN` |
| `POST` | `/v1/adapter-repositories/{repo_id}/resolve-version` | `handlers::resolve_adapter_version_handler` | `UNKNOWN` |
| `GET` | `/v1/adapter-repositories/{repo_id}/versions` | `handlers::adapters::list_adapter_versions` | `UNKNOWN` |
| `POST` | `/v1/adapter-repositories/{repo_id}/versions/rollback` | `handlers::rollback_adapter_version_handler` | `UNKNOWN` |
| `GET` | `/v1/adapter-stacks` | `handlers::adapter_stacks::list_stacks` | `UNKNOWN` |
| `POST` | `/v1/adapter-stacks` | `handlers::adapter_stacks::create_stack` | `UNKNOWN` |
| `POST` | `/v1/adapter-stacks/deactivate` | `handlers::adapter_stacks::deactivate_stack` | `UNKNOWN` |
| `DELETE` | `/v1/adapter-stacks/{id}` | `handlers::adapter_stacks::delete_stack` | `UNKNOWN` |
| `GET` | `/v1/adapter-stacks/{id}` | `handlers::adapter_stacks::get_stack` | `UNKNOWN` |
| `POST` | `/v1/adapter-stacks/{id}/activate` | `handlers::adapter_stacks::activate_stack` | `UNKNOWN` |
| `GET` | `/v1/adapter-stacks/{id}/history` | `handlers::adapter_stacks::get_stack_history` | `UNKNOWN` |
| `GET` | `/v1/adapter-stacks/{id}/policies` | `handlers::adapter_stacks::get_stack_policies` | `UNKNOWN` |
| `POST` | `/v1/adapter-versions/draft` | `handlers::create_draft_version` | `UNKNOWN` |
| `GET` | `/v1/adapter-versions/{version_id}` | `handlers::adapters::get_adapter_version` | `UNKNOWN` |
| `POST` | `/v1/adapter-versions/{version_id}/promote` | `handlers::promote_adapter_version_handler` | `UNKNOWN` |
| `POST` | `/v1/adapter-versions/{version_id}/tag` | `handlers::tag_adapter_version_handler` | `UNKNOWN` |
| `GET` | `/v1/adapters` | `handlers::adapters::list_adapters` | `UNKNOWN` |
| `GET` | `/v1/adapters/category-policies` | `handlers::adapters::list_category_policies` | `UNKNOWN` |
| `GET` | `/v1/adapters/category-policies/{category}` | `handlers::adapters::get_category_policy` | `UNKNOWN` |
| `PUT` | `/v1/adapters/category-policies/{category}` | `handlers::adapters::update_category_policy` | `UNKNOWN` |
| `POST` | `/v1/adapters/directory/upsert` | `handlers::upsert_directory_adapter` | `UNKNOWN` |
| `POST` | `/v1/adapters/import` | `handlers::adapters::import_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/next-revision/{tenant}/{domain}/{purpose}` | `handlers::get_next_revision` | `UNKNOWN` |
| `POST` | `/v1/adapters/register` | `handlers::adapters_lifecycle::register_adapter` | `UNKNOWN` |
| `POST` | `/v1/adapters/swap` | `handlers::adapters::swap_adapters` | `UNKNOWN` |
| `POST` | `/v1/adapters/validate-name` | `handlers::validate_adapter_name` | `UNKNOWN` |
| `GET` | `/v1/adapters/verify-gpu` | `handlers::adapters::verify_gpu_integrity` | `UNKNOWN` |
| `DELETE` | `/v1/adapters/{adapter_id}` | `handlers::adapters_lifecycle::delete_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}` | `handlers::adapters::get_adapter` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/activate` | `handlers::adapters::activate_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/activations` | `handlers::adapters::get_adapter_activations` | `UNKNOWN` |
| `DELETE` | `/v1/adapters/{adapter_id}/archive` | `handlers::adapters::unarchive_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/archive` | `handlers::adapters::get_archive_status` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/archive` | `handlers::adapters::archive_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/detail` | `handlers::adapters::get_adapter_detail` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/duplicate` | `handlers::adapters::duplicate_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/evidence` | `handlers::evidence::get_adapter_evidence` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/export` | `handlers::adapters::export_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/health` | `handlers::adapters::get_adapter_health` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/lifecycle/demote` | `handlers::demote_adapter_lifecycle` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/lifecycle/promote` | `handlers::promote_adapter_lifecycle` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/lineage` | `handlers::adapters::get_adapter_lineage` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/load` | `handlers::adapters_lifecycle::load_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/manifest` | `handlers::download_adapter_manifest` | `UNKNOWN` |
| `DELETE` | `/v1/adapters/{adapter_id}/pin` | `handlers::unpin_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/pin` | `handlers::get_pin_status` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/pin` | `handlers::pin_adapter` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/state/promote` | `handlers::adapters::promote_adapter_state` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/stats` | `handlers::adapters::get_adapter_stats` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/training-export` | `handlers::adapters::export_training_provenance` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/training-snapshot` | `handlers::adapters::get_adapter_training_snapshot` | `UNKNOWN` |
| `POST` | `/v1/adapters/{adapter_id}/unload` | `handlers::adapters_lifecycle::unload_adapter` | `UNKNOWN` |
| `GET` | `/v1/adapters/{adapter_id}/usage` | `handlers::routing_decisions::get_adapter_usage` | `UNKNOWN` |
| `GET` | `/v1/admin/users` | `handlers::admin::list_users` | `UNKNOWN` |
| `GET` | `/v1/api-keys` | `handlers::api_keys::list_api_keys` | `UNKNOWN` |
| `POST` | `/v1/api-keys` | `handlers::api_keys::create_api_key` | `UNKNOWN` |
| `DELETE` | `/v1/api-keys/{id}` | `handlers::api_keys::revoke_api_key` | `UNKNOWN` |
| `GET` | `/v1/audit/compliance` | `handlers::get_compliance_audit` | `UNKNOWN` |
| `GET` | `/v1/audit/federation` | `handlers::get_federation_audit` | `UNKNOWN` |
| `GET` | `/v1/audit/logs` | `handlers::admin::query_audit_logs` | `UNKNOWN` |
| `GET` | `/v1/audit/policy-decisions` | `handlers::query_policy_decisions` | `UNKNOWN` |
| `GET` | `/v1/audit/policy-decisions/verify-chain` | `handlers::verify_policy_audit_chain` | `UNKNOWN` |
| `GET` | `/v1/audits` | `handlers::list_audits_extended` | `UNKNOWN` |
| `POST` | `/v1/auth/bootstrap` | `handlers::auth_enhanced::bootstrap_admin_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/config` | `handlers::auth_enhanced::get_auth_config_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/dev-bypass` | `handlers::auth_enhanced::dev_bypass_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/health` | `handlers::auth_enhanced::auth_health_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/login` | `handlers::auth_enhanced::login_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/logout` | `handlers::auth_enhanced::logout_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/me` | `auth::auth_me` | `UNKNOWN` |
| `POST` | `/v1/auth/mfa/disable` | `handlers::auth_enhanced::mfa_disable_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/mfa/start` | `handlers::auth_enhanced::mfa_start_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/mfa/status` | `handlers::auth_enhanced::mfa_status_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/mfa/verify` | `handlers::auth_enhanced::mfa_verify_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/refresh` | `handlers::auth_enhanced::refresh_token_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/sessions` | `handlers::auth_enhanced::list_sessions_handler` | `UNKNOWN` |
| `DELETE` | `/v1/auth/sessions/{jti}` | `handlers::auth_enhanced::revoke_session_handler` | `UNKNOWN` |
| `GET` | `/v1/auth/tenants` | `handlers::auth_enhanced::list_user_tenants_handler` | `UNKNOWN` |
| `POST` | `/v1/auth/tenants/switch` | `handlers::auth_enhanced::switch_tenant_handler` | `UNKNOWN` |
| `POST` | `/v1/batches` | `handlers::batch::create_batch_job` | `UNKNOWN` |
| `GET` | `/v1/batches/{batch_id}` | `handlers::batch::get_batch_status` | `UNKNOWN` |
| `GET` | `/v1/batches/{batch_id}/items` | `handlers::batch::get_batch_items` | `UNKNOWN` |
| `GET` | `/v1/chat/categories` | `handlers::chat_sessions::list_chat_categories` | `UNKNOWN` |
| `POST` | `/v1/chat/categories` | `handlers::chat_sessions::create_chat_category` | `UNKNOWN` |
| `DELETE` | `/v1/chat/categories/{category_id}` | `handlers::chat_sessions::delete_chat_category` | `UNKNOWN` |
| `PUT` | `/v1/chat/categories/{category_id}` | `handlers::chat_sessions::update_chat_category` | `UNKNOWN` |
| `POST` | `/v1/chat/completions` | `handlers::openai_compat::chat_completions` | `UNKNOWN` |
| `GET` | `/v1/chat/messages/{message_id}/evidence` | `handlers::chat_sessions::get_message_evidence` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions` | `handlers::chat_sessions::list_chat_sessions` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions` | `handlers::chat_sessions::create_chat_session` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/archived` | `handlers::chat_sessions::list_archived_sessions` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/search` | `handlers::chat_sessions::search_chat_sessions` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/shared-with-me` | `handlers::chat_sessions::get_sessions_shared_with_me` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/trash` | `handlers::chat_sessions::list_deleted_sessions` | `UNKNOWN` |
| `DELETE` | `/v1/chat/sessions/{session_id}` | `handlers::chat_sessions::delete_chat_session` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}` | `handlers::chat_sessions::get_chat_session` | `UNKNOWN` |
| `PUT` | `/v1/chat/sessions/{session_id}` | `handlers::chat_sessions::update_chat_session` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/archive` | `handlers::chat_sessions::archive_session` | `UNKNOWN` |
| `PUT` | `/v1/chat/sessions/{session_id}/category` | `handlers::chat_sessions::set_session_category` | `UNKNOWN` |
| `PUT` | `/v1/chat/sessions/{session_id}/collection` | `handlers::chat_sessions::update_session_collection` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/fork` | `handlers::chat_sessions::fork_chat_session` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}/messages` | `handlers::chat_sessions::get_chat_messages` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/messages` | `handlers::chat_sessions::add_chat_message` | `UNKNOWN` |
| `DELETE` | `/v1/chat/sessions/{session_id}/permanent` | `handlers::chat_sessions::hard_delete_session` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}/provenance` | `handlers::chat_sessions::get_chat_provenance` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/restore` | `handlers::chat_sessions::restore_session` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}/shares` | `handlers::chat_sessions::get_session_shares` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/shares` | `handlers::chat_sessions::share_session` | `UNKNOWN` |
| `DELETE` | `/v1/chat/sessions/{session_id}/shares/{share_id}` | `handlers::chat_sessions::revoke_session_share` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}/summary` | `handlers::chat_sessions::get_session_summary` | `UNKNOWN` |
| `GET` | `/v1/chat/sessions/{session_id}/tags` | `handlers::chat_sessions::get_session_tags` | `UNKNOWN` |
| `POST` | `/v1/chat/sessions/{session_id}/tags` | `handlers::chat_sessions::assign_tags_to_session` | `UNKNOWN` |
| `DELETE` | `/v1/chat/sessions/{session_id}/tags/{tag_id}` | `handlers::chat_sessions::remove_tag_from_session` | `UNKNOWN` |
| `GET` | `/v1/chat/tags` | `handlers::chat_sessions::list_chat_tags` | `UNKNOWN` |
| `POST` | `/v1/chat/tags` | `handlers::chat_sessions::create_chat_tag` | `UNKNOWN` |
| `DELETE` | `/v1/chat/tags/{tag_id}` | `handlers::chat_sessions::delete_chat_tag` | `UNKNOWN` |
| `PUT` | `/v1/chat/tags/{tag_id}` | `handlers::chat_sessions::update_chat_tag` | `UNKNOWN` |
| `POST` | `/v1/chats/from_training_job` | `handlers::create_chat_from_training_job` | `UNKNOWN` |
| `POST` | `/v1/cli/owner-run` | `handlers::owner_cli::run_owner_cli_command` | `UNKNOWN` |
| `POST` | `/v1/code/commit-delta` | `handlers::code::create_commit_delta` | `UNKNOWN` |
| `POST` | `/v1/code/register-repo` | `handlers::code::register_repo` | `UNKNOWN` |
| `GET` | `/v1/code/repositories` | `handlers::code::list_repositories` | `UNKNOWN` |
| `GET` | `/v1/code/repositories/{repo_id}` | `handlers::code::get_repository` | `UNKNOWN` |
| `POST` | `/v1/code/scan` | `handlers::code::scan_repo` | `UNKNOWN` |
| `GET` | `/v1/code/scan/{job_id}` | `handlers::code::get_scan_status` | `UNKNOWN` |
| `GET` | `/v1/collections` | `handlers::collections::list_collections` | `UNKNOWN` |
| `POST` | `/v1/collections` | `handlers::collections::create_collection` | `UNKNOWN` |
| `DELETE` | `/v1/collections/{id}` | `handlers::collections::delete_collection` | `UNKNOWN` |
| `GET` | `/v1/collections/{id}` | `handlers::collections::get_collection` | `UNKNOWN` |
| `POST` | `/v1/collections/{id}/documents` | `handlers::collections::add_document_to_collection` | `UNKNOWN` |
| `DELETE` | `/v1/collections/{id}/documents/{doc_id}` | `handlers::collections::remove_document_from_collection` | `UNKNOWN` |
| `GET` | `/v1/commits` | `handlers::adapters::list_commits` | `UNKNOWN` |
| `GET` | `/v1/commits/{sha}` | `handlers::adapters::get_commit` | `UNKNOWN` |
| `GET` | `/v1/commits/{sha}/diff` | `handlers::adapters::get_commit_diff` | `UNKNOWN` |
| `GET` | `/v1/contacts` | `handlers::chat_sessions::list_contacts` | `UNKNOWN` |
| `POST` | `/v1/contacts` | `handlers::chat_sessions::create_contact` | `UNKNOWN` |
| `DELETE` | `/v1/contacts/{id}` | `handlers::chat_sessions::delete_contact` | `UNKNOWN` |
| `GET` | `/v1/contacts/{id}` | `handlers::chat_sessions::get_contact` | `UNKNOWN` |
| `GET` | `/v1/contacts/{id}/interactions` | `handlers::chat_sessions::get_contact_interactions` | `UNKNOWN` |
| `POST` | `/v1/cp/promote` | `handlers::cp_promote` | `UNKNOWN` |
| `POST` | `/v1/cp/promote/dry-run` | `handlers::cp_dry_run_promote` | `UNKNOWN` |
| `GET` | `/v1/cp/promotion-gates/{cpid}` | `handlers::promotion_gates` | `UNKNOWN` |
| `GET` | `/v1/cp/promotions` | `handlers::get_promotion_history` | `UNKNOWN` |
| `POST` | `/v1/cp/rollback` | `handlers::cp_rollback` | `UNKNOWN` |
| `GET` | `/v1/dashboard/config` | `handlers::dashboard::get_dashboard_config` | `UNKNOWN` |
| `PUT` | `/v1/dashboard/config` | `handlers::dashboard::update_dashboard_config` | `UNKNOWN` |
| `POST` | `/v1/dashboard/config/reset` | `handlers::dashboard::reset_dashboard_config` | `UNKNOWN` |
| `GET` | `/v1/datasets` | `handlers::datasets::list_datasets` | `UNKNOWN` |
| `POST` | `/v1/datasets` | `handlers::datasets::upload_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/by-codebase/{codebase_id}/versions` | `handlers::datasets::list_versions_by_codebase` | `UNKNOWN` |
| `POST` | `/v1/datasets/chunked-upload/cleanup` | `handlers::datasets::cleanup_expired_sessions` | `UNKNOWN` |
| `POST` | `/v1/datasets/chunked-upload/initiate` | `handlers::datasets::initiate_chunked_upload` | `UNKNOWN` |
| `GET` | `/v1/datasets/chunked-upload/sessions` | `handlers::datasets::list_upload_sessions` | `UNKNOWN` |
| `DELETE` | `/v1/datasets/chunked-upload/{session_id}` | `handlers::datasets::cancel_chunked_upload` | `UNKNOWN` |
| `POST` | `/v1/datasets/chunked-upload/{session_id}/chunk` | `handlers::datasets::upload_chunk` | `UNKNOWN` |
| `PUT` | `/v1/datasets/chunked-upload/{session_id}/chunk` | `handlers::datasets::retry_chunk` | `UNKNOWN` |
| `POST` | `/v1/datasets/chunked-upload/{session_id}/complete` | `handlers::datasets::complete_chunked_upload` | `UNKNOWN` |
| `GET` | `/v1/datasets/chunked-upload/{session_id}/status` | `handlers::datasets::get_upload_session_status` | `UNKNOWN` |
| `POST` | `/v1/datasets/from-documents` | `handlers::datasets::create_dataset_from_documents` | `UNKNOWN` |
| `POST` | `/v1/datasets/upload` | `handlers::datasets::upload_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/upload/progress` | `handlers::datasets::dataset_upload_progress` | `UNKNOWN` |
| `DELETE` | `/v1/datasets/{dataset_id}` | `handlers::datasets::delete_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}` | `handlers::datasets::get_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/evidence` | `handlers::evidence::get_dataset_evidence` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/files` | `handlers::datasets::get_dataset_files` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/preview` | `handlers::datasets::preview_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/statistics` | `handlers::datasets::get_dataset_statistics` | `UNKNOWN` |
| `POST` | `/v1/datasets/{dataset_id}/trust_override` | `handlers::datasets::apply_dataset_trust_override` | `UNKNOWN` |
| `POST` | `/v1/datasets/{dataset_id}/validate` | `handlers::datasets::validate_dataset` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/versions` | `handlers::datasets::list_dataset_versions` | `UNKNOWN` |
| `POST` | `/v1/datasets/{dataset_id}/versions` | `handlers::datasets::create_dataset_version` | `UNKNOWN` |
| `GET` | `/v1/datasets/{dataset_id}/versions/{revision}` | `handlers::datasets::get_dataset_version` | `UNKNOWN` |
| `POST` | `/v1/datasets/{dataset_id}/versions/{version_id}/safety` | `handlers::datasets::update_dataset_version_safety` | `UNKNOWN` |
| `POST` | `/v1/datasets/{dataset_id}/versions/{version_id}/trust-override` | `handlers::datasets::apply_dataset_version_trust_override` | `UNKNOWN` |
| `GET` | `/v1/debug/coreml_verification_status` | `handlers::coreml_verification_status` | `UNKNOWN` |
| `POST` | `/v1/dev/bootstrap` | `handlers::auth_enhanced::dev_bootstrap_handler` | `UNKNOWN` |
| `POST` | `/v1/diag/bundle` | `handlers::diag_bundle::create_bundle_export` | `UNKNOWN` |
| `GET` | `/v1/diag/bundle/{export_id}` | `handlers::diag_bundle::get_bundle_export` | `UNKNOWN` |
| `GET` | `/v1/diag/bundle/{export_id}/download` | `handlers::diag_bundle::download_bundle` | `UNKNOWN` |
| `POST` | `/v1/diag/diff` | `handlers::diagnostics::diff_diag_runs` | `UNKNOWN` |
| `POST` | `/v1/diag/export` | `handlers::diagnostics::export_diag_run` | `UNKNOWN` |
| `GET` | `/v1/diag/runs` | `handlers::diagnostics::list_diag_runs` | `UNKNOWN` |
| `GET` | `/v1/diag/runs/{trace_id}` | `handlers::diagnostics::get_diag_run` | `UNKNOWN` |
| `GET` | `/v1/diag/runs/{trace_id}/events` | `handlers::diagnostics::list_diag_events` | `UNKNOWN` |
| `GET` | `/v1/diagnostics/determinism-status` | `handlers::diagnostics::get_determinism_status` | `UNKNOWN` |
| `GET` | `/v1/diagnostics/quarantine-status` | `handlers::diagnostics::get_quarantine_status` | `UNKNOWN` |
| `GET` | `/v1/documents` | `handlers::documents::list_documents` | `UNKNOWN` |
| `GET` | `/v1/documents/failed` | `handlers::documents::list_failed_documents` | `UNKNOWN` |
| `POST` | `/v1/documents/upload` | `handlers::documents::upload_document` | `UNKNOWN` |
| `DELETE` | `/v1/documents/{id}` | `handlers::documents::delete_document` | `UNKNOWN` |
| `GET` | `/v1/documents/{id}` | `handlers::documents::get_document` | `UNKNOWN` |
| `GET` | `/v1/documents/{id}/chunks` | `handlers::documents::list_document_chunks` | `UNKNOWN` |
| `GET` | `/v1/documents/{id}/download` | `handlers::documents::download_document` | `UNKNOWN` |
| `POST` | `/v1/documents/{id}/process` | `handlers::documents::process_document` | `UNKNOWN` |
| `POST` | `/v1/documents/{id}/retry` | `handlers::documents::retry_document` | `UNKNOWN` |
| `GET` | `/v1/domain-adapters` | `domain_adapters::list_domain_adapters` | `UNKNOWN` |
| `POST` | `/v1/domain-adapters` | `domain_adapters::create_domain_adapter` | `UNKNOWN` |
| `DELETE` | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::delete_domain_adapter` | `UNKNOWN` |
| `GET` | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::get_domain_adapter` | `UNKNOWN` |
| `POST` | `/v1/domain-adapters/{adapter_id}/execute` | `domain_adapters::execute_domain_adapter` | `UNKNOWN` |
| `POST` | `/v1/domain-adapters/{adapter_id}/load` | `domain_adapters::load_domain_adapter` | `UNKNOWN` |
| `GET` | `/v1/domain-adapters/{adapter_id}/manifest` | `domain_adapters::get_domain_adapter_manifest` | `UNKNOWN` |
| `POST` | `/v1/domain-adapters/{adapter_id}/test` | `domain_adapters::test_domain_adapter` | `UNKNOWN` |
| `POST` | `/v1/domain-adapters/{adapter_id}/unload` | `domain_adapters::unload_domain_adapter` | `UNKNOWN` |
| `GET` | `/v1/evidence` | `handlers::evidence::list_evidence` | `UNKNOWN` |
| `POST` | `/v1/evidence` | `handlers::evidence::create_evidence` | `UNKNOWN` |
| `GET` | `/v1/evidence/runs/{run_id}/export` | `handlers::aliases::run_evidence::download_run_evidence_alias` | `UNKNOWN` |
| `DELETE` | `/v1/evidence/{id}` | `handlers::evidence::delete_evidence` | `UNKNOWN` |
| `GET` | `/v1/evidence/{id}` | `handlers::evidence::get_evidence` | `UNKNOWN` |
| `GET` | `/v1/federation/quarantine` | `handlers::federation::get_federation_quarantine_status` | `UNKNOWN` |
| `POST` | `/v1/federation/release-quarantine` | `handlers::federation::release_quarantine` | `UNKNOWN` |
| `GET` | `/v1/federation/status` | `handlers::federation::get_federation_status` | `UNKNOWN` |
| `GET` | `/v1/federation/sync-status` | `handlers::federation::get_federation_sync_status` | `UNKNOWN` |
| `GET` | `/v1/git/branches` | `handlers::git::list_git_branches` | `UNKNOWN` |
| `POST` | `/v1/git/repositories` | `handlers::git_repository::register_git_repository` | `UNKNOWN` |
| `GET` | `/v1/git/repositories/{repo_id}/analysis` | `handlers::git_repository::get_repository_analysis` | `UNKNOWN` |
| `POST` | `/v1/git/repositories/{repo_id}/train` | `handlers::git_repository::train_repository_adapter` | `UNKNOWN` |
| `POST` | `/v1/git/sessions/start` | `handlers::git::start_git_session` | `UNKNOWN` |
| `POST` | `/v1/git/sessions/{session_id}/end` | `handlers::git::end_git_session` | `UNKNOWN` |
| `GET` | `/v1/git/status` | `handlers::git::git_status` | `UNKNOWN` |
| `POST` | `/v1/golden/compare` | `handlers::golden::golden_compare` | `UNKNOWN` |
| `GET` | `/v1/golden/runs` | `handlers::golden::list_golden_runs` | `UNKNOWN` |
| `GET` | `/v1/golden/runs/{name}` | `handlers::golden::get_golden_run` | `UNKNOWN` |
| `POST` | `/v1/golden/{run_id}/approve` | `handlers::promotion::approve_or_reject_promotion` | `UNKNOWN` |
| `GET` | `/v1/golden/{run_id}/gates` | `handlers::promotion::get_gate_status` | `UNKNOWN` |
| `POST` | `/v1/golden/{run_id}/promote` | `handlers::promotion::request_promotion` | `UNKNOWN` |
| `GET` | `/v1/golden/{run_id}/promotion` | `handlers::promotion::get_promotion_status` | `UNKNOWN` |
| `POST` | `/v1/golden/{stage}/rollback` | `handlers::promotion::rollback_promotion` | `UNKNOWN` |
| `POST` | `/v1/infer` | `handlers::infer` | `UNKNOWN` |
| `POST` | `/v1/infer/batch` | `handlers::batch::batch_infer` | `UNKNOWN` |
| `GET` | `/v1/infer/paused` | `handlers::review::list_paused` | `UNKNOWN` |
| `POST` | `/v1/infer/stream` | `handlers::streaming_infer::streaming_infer` | `UNKNOWN` |
| `POST` | `/v1/infer/stream/progress` | `handlers::streaming_infer::streaming_infer_with_progress` | `UNKNOWN` |
| `POST` | `/v1/infer/{inference_id}/review` | `handlers::review::submit_review` | `UNKNOWN` |
| `GET` | `/v1/infer/{inference_id}/state` | `handlers::review::get_inference_state` | `UNKNOWN` |
| `GET` | `/v1/jobs` | `handlers::list_jobs` | `UNKNOWN` |
| `GET` | `/v1/journeys/{journey_type}/{id}` | `handlers::journeys::get_journey` | `UNKNOWN` |
| `GET` | `/v1/logs/query` | `handlers::telemetry::query_logs` | `UNKNOWN` |
| `GET` | `/v1/logs/stream` | `handlers::telemetry::stream_logs` | `UNKNOWN` |
| `GET` | `/v1/memory/adapters` | `handlers::memory_detail::get_adapter_memory_usage` | `UNKNOWN` |
| `GET` | `/v1/memory/uma-breakdown` | `handlers::memory_detail::get_uma_memory_breakdown` | `UNKNOWN` |
| `GET` | `/v1/memory/usage` | `handlers::memory_detail::get_combined_memory_usage` | `UNKNOWN` |
| `GET` | `/v1/meta` | `handlers::meta` | `UNKNOWN` |
| `GET` | `/v1/metrics` | `handlers::metrics_handler` | `UNKNOWN` |
| `GET` | `/v1/metrics/adapters` | `handlers::adapters::get_adapter_metrics` | `UNKNOWN` |
| `GET` | `/v1/metrics/current` | `handlers::metrics_time_series::get_metrics_snapshot` | `UNKNOWN` |
| `GET` | `/v1/metrics/quality` | `handlers::adapters::get_quality_metrics` | `UNKNOWN` |
| `GET` | `/v1/metrics/series` | `handlers::telemetry::get_metrics_series` | `UNKNOWN` |
| `GET` | `/v1/metrics/snapshot` | `handlers::telemetry::get_metrics_snapshot` | `UNKNOWN` |
| `GET` | `/v1/metrics/system` | `handlers::adapters::get_system_metrics` | `UNKNOWN` |
| `GET` | `/v1/metrics/time-series` | `handlers::metrics_time_series::get_metrics_time_series` | `UNKNOWN` |
| `GET` | `/v1/models` | `handlers::models::list_models_with_stats` | `UNKNOWN` |
| `GET` | `/v1/models/download-progress` | `handlers::models::get_download_progress` | `UNKNOWN` |
| `POST` | `/v1/models/import` | `handlers::models::import_model` | `UNKNOWN` |
| `GET` | `/v1/models/status` | `handlers::infrastructure::get_base_model_status` | `UNKNOWN` |
| `GET` | `/v1/models/status/all` | `handlers::models::get_all_models_status` | `UNKNOWN` |
| `POST` | `/v1/models/{model_id}/load` | `handlers::models::load_model` | `UNKNOWN` |
| `GET` | `/v1/models/{model_id}/status` | `handlers::models::get_model_status` | `UNKNOWN` |
| `POST` | `/v1/models/{model_id}/unload` | `handlers::models::unload_model` | `UNKNOWN` |
| `GET` | `/v1/models/{model_id}/validate` | `handlers::models::validate_model` | `UNKNOWN` |
| `GET` | `/v1/monitoring/alerts` | `handlers::monitoring::list_alerts` | `UNKNOWN` |
| `POST` | `/v1/monitoring/alerts/{alert_id}/acknowledge` | `handlers::monitoring::acknowledge_alert` | `UNKNOWN` |
| `POST` | `/v1/monitoring/alerts/{alert_id}/resolve` | `handlers::monitoring::resolve_alert` | `UNKNOWN` |
| `GET` | `/v1/monitoring/anomalies` | `handlers::monitoring::list_process_anomalies` | `UNKNOWN` |
| `POST` | `/v1/monitoring/anomalies/{anomaly_id}/status` | `handlers::update_process_anomaly_status` | `UNKNOWN` |
| `GET` | `/v1/monitoring/dashboards` | `handlers::list_process_monitoring_dashboards` | `UNKNOWN` |
| `POST` | `/v1/monitoring/dashboards` | `handlers::create_process_monitoring_dashboard` | `UNKNOWN` |
| `GET` | `/v1/monitoring/health-metrics` | `handlers::list_process_health_metrics` | `UNKNOWN` |
| `GET` | `/v1/monitoring/reports` | `handlers::list_process_monitoring_reports` | `UNKNOWN` |
| `POST` | `/v1/monitoring/reports` | `handlers::create_process_monitoring_report` | `UNKNOWN` |
| `GET` | `/v1/monitoring/rules` | `handlers::monitoring::list_monitoring_rules` | `UNKNOWN` |
| `POST` | `/v1/monitoring/rules` | `handlers::monitoring::create_monitoring_rule` | `UNKNOWN` |
| `DELETE` | `/v1/monitoring/rules/{rule_id}` | `handlers::monitoring::delete_monitoring_rule` | `UNKNOWN` |
| `PUT` | `/v1/monitoring/rules/{rule_id}` | `handlers::monitoring::update_monitoring_rule` | `UNKNOWN` |
| `GET` | `/v1/nodes` | `handlers::list_nodes` | `UNKNOWN` |
| `POST` | `/v1/nodes/register` | `handlers::register_node` | `UNKNOWN` |
| `DELETE` | `/v1/nodes/{node_id}` | `handlers::evict_node` | `UNKNOWN` |
| `GET` | `/v1/nodes/{node_id}/details` | `handlers::get_node_details` | `UNKNOWN` |
| `POST` | `/v1/nodes/{node_id}/offline` | `handlers::mark_node_offline` | `UNKNOWN` |
| `POST` | `/v1/nodes/{node_id}/ping` | `handlers::test_node_connection` | `UNKNOWN` |
| `GET` | `/v1/notifications` | `handlers::notifications::list_notifications` | `UNKNOWN` |
| `POST` | `/v1/notifications/read-all` | `handlers::notifications::mark_all_notifications_read` | `UNKNOWN` |
| `GET` | `/v1/notifications/summary` | `handlers::notifications::get_notification_summary` | `UNKNOWN` |
| `POST` | `/v1/notifications/{notification_id}/read` | `handlers::notifications::mark_notification_read` | `UNKNOWN` |
| `POST` | `/v1/orchestration/analyze` | `handlers::orchestration::analyze_orchestration_prompt` | `UNKNOWN` |
| `GET` | `/v1/orchestration/config` | `handlers::orchestration::get_orchestration_config` | `UNKNOWN` |
| `PUT` | `/v1/orchestration/config` | `handlers::orchestration::update_orchestration_config` | `UNKNOWN` |
| `GET` | `/v1/orchestration/metrics` | `handlers::orchestration::get_orchestration_metrics` | `UNKNOWN` |
| `POST` | `/v1/patch/propose` | `handlers::code::propose_patch` | `UNKNOWN` |
| `GET` | `/v1/plans` | `handlers::list_plans` | `UNKNOWN` |
| `POST` | `/v1/plans/build` | `handlers::build_plan` | `UNKNOWN` |
| `POST` | `/v1/plans/compare` | `handlers::compare_plans` | `UNKNOWN` |
| `GET` | `/v1/plans/{plan_id}/details` | `handlers::get_plan_details` | `UNKNOWN` |
| `GET` | `/v1/plans/{plan_id}/manifest` | `handlers::export_plan_manifest` | `UNKNOWN` |
| `POST` | `/v1/plans/{plan_id}/rebuild` | `handlers::rebuild_plan` | `UNKNOWN` |
| `GET` | `/v1/plugins` | `handlers::plugins::list_plugins` | `UNKNOWN` |
| `GET` | `/v1/plugins/{name}` | `handlers::plugins::plugin_status` | `UNKNOWN` |
| `POST` | `/v1/plugins/{name}/disable` | `handlers::plugins::disable_plugin` | `UNKNOWN` |
| `POST` | `/v1/plugins/{name}/enable` | `handlers::plugins::enable_plugin` | `UNKNOWN` |
| `GET` | `/v1/policies` | `handlers::list_policies` | `UNKNOWN` |
| `POST` | `/v1/policies/apply` | `handlers::apply_policy` | `UNKNOWN` |
| `POST` | `/v1/policies/assign` | `handlers::tenant_policies::assign_policy` | `UNKNOWN` |
| `GET` | `/v1/policies/assignments` | `handlers::tenant_policies::list_policy_assignments` | `UNKNOWN` |
| `POST` | `/v1/policies/compare` | `handlers::compare_policy_versions` | `UNKNOWN` |
| `POST` | `/v1/policies/validate` | `handlers::validate_policy` | `UNKNOWN` |
| `GET` | `/v1/policies/violations` | `handlers::tenant_policies::list_violations` | `UNKNOWN` |
| `GET` | `/v1/policies/{cpid}` | `handlers::get_policy` | `UNKNOWN` |
| `GET` | `/v1/policies/{cpid}/export` | `handlers::export_policy` | `UNKNOWN` |
| `POST` | `/v1/policies/{cpid}/sign` | `handlers::sign_policy` | `UNKNOWN` |
| `GET` | `/v1/policies/{cpid}/verify` | `handlers::verify_policy_signature` | `UNKNOWN` |
| `GET` | `/v1/promotions/{id}` | `handlers::get_promotion` | `UNKNOWN` |
| `GET` | `/v1/registry/status` | `handlers::registry::get_registry_status` | `UNKNOWN` |
| `POST` | `/v1/replay` | `handlers::replay_inference::execute_replay` | `UNKNOWN` |
| `GET` | `/v1/replay/check/{inference_id}` | `handlers::replay_inference::check_availability` | `UNKNOWN` |
| `GET` | `/v1/replay/history/{inference_id}` | `handlers::replay_inference::get_replay_history` | `UNKNOWN` |
| `GET` | `/v1/replay/sessions` | `handlers::replay::list_replay_sessions` | `UNKNOWN` |
| `POST` | `/v1/replay/sessions` | `handlers::replay::create_replay_session` | `UNKNOWN` |
| `GET` | `/v1/replay/sessions/{id}` | `handlers::replay::get_replay_session` | `UNKNOWN` |
| `POST` | `/v1/replay/sessions/{id}/execute` | `handlers::replay::execute_replay_session` | `UNKNOWN` |
| `POST` | `/v1/replay/sessions/{id}/verify` | `handlers::replay::verify_replay_session` | `UNKNOWN` |
| `GET` | `/v1/repos` | `handlers::repos::list_repos` | `UNKNOWN` |
| `POST` | `/v1/repos` | `handlers::repos::create_repo` | `UNKNOWN` |
| `GET` | `/v1/repos/{repo_id}` | `handlers::repos::get_repo` | `UNKNOWN` |
| `PATCH` | `/v1/repos/{repo_id}` | `handlers::repos::update_repo` | `UNKNOWN` |
| `POST` | `/v1/repos/{repo_id}/rollback/{branch}` | `handlers::repos::rollback_version` | `UNKNOWN` |
| `GET` | `/v1/repos/{repo_id}/timeline` | `handlers::repos::get_timeline` | `UNKNOWN` |
| `GET` | `/v1/repos/{repo_id}/training-jobs` | `handlers::repos::list_training_jobs` | `UNKNOWN` |
| `GET` | `/v1/repos/{repo_id}/versions` | `handlers::repos::list_versions` | `UNKNOWN` |
| `GET` | `/v1/repos/{repo_id}/versions/{version_id}` | `handlers::repos::get_version` | `UNKNOWN` |
| `POST` | `/v1/repos/{repo_id}/versions/{version_id}/promote` | `handlers::repos::promote_version` | `UNKNOWN` |
| `POST` | `/v1/repos/{repo_id}/versions/{version_id}/tag` | `handlers::repos::tag_version` | `UNKNOWN` |
| `POST` | `/v1/repos/{repo_id}/versions/{version_id}/train` | `handlers::repos::start_training` | `UNKNOWN` |
| `GET` | `/v1/repositories` | `handlers::list_repositories_legacy` | `UNKNOWN` |
| `GET` | `/v1/reviews/paused` | `handlers::review::list_paused_reviews` | `UNKNOWN` |
| `POST` | `/v1/reviews/submit` | `handlers::review::submit_review_response` | `UNKNOWN` |
| `GET` | `/v1/reviews/{pause_id}` | `handlers::review::get_pause_details` | `UNKNOWN` |
| `GET` | `/v1/reviews/{pause_id}/context` | `handlers::review::export_review_context` | `UNKNOWN` |
| `GET` | `/v1/routing/chain` | `handlers::routing_decisions::get_routing_decision_chain` | `UNKNOWN` |
| `POST` | `/v1/routing/debug` | `handlers::routing_decisions::debug_routing` | `UNKNOWN` |
| `GET` | `/v1/routing/decisions` | `handlers::routing_decisions::get_routing_decisions` | `UNKNOWN` |
| `GET` | `/v1/routing/decisions/{id}` | `handlers::routing_decisions::get_routing_decision_by_id` | `UNKNOWN` |
| `GET` | `/v1/routing/history` | `handlers::routing_decisions::get_routing_history` | `UNKNOWN` |
| `GET` | `/v1/routing/sessions/{request_id}` | `handlers::routing_decisions::get_session_router_view` | `UNKNOWN` |
| `GET` | `/v1/runs/{run_id}/evidence` | `handlers::run_evidence::download_run_evidence` | `UNKNOWN` |
| `GET` | `/v1/runtime/session` | `handlers::runtime::get_current_session` | `UNKNOWN` |
| `GET` | `/v1/runtime/sessions` | `handlers::runtime::list_sessions` | `UNKNOWN` |
| `GET` | `/v1/search` | `handlers::search::global_search` | `UNKNOWN` |
| `POST` | `/v1/services/essential/start` | `handlers::services::start_essential_services` | `UNKNOWN` |
| `POST` | `/v1/services/essential/stop` | `handlers::services::stop_essential_services` | `UNKNOWN` |
| `GET` | `/v1/services/{service_id}/logs` | `handlers::services::get_service_logs` | `UNKNOWN` |
| `POST` | `/v1/services/{service_id}/restart` | `handlers::services::restart_service` | `UNKNOWN` |
| `POST` | `/v1/services/{service_id}/start` | `handlers::services::start_service` | `UNKNOWN` |
| `POST` | `/v1/services/{service_id}/stop` | `handlers::services::stop_service` | `UNKNOWN` |
| `GET` | `/v1/settings` | `handlers::settings::get_settings` | `UNKNOWN` |
| `PUT` | `/v1/settings` | `handlers::settings::update_settings` | `UNKNOWN` |
| `POST` | `/v1/stacks/validate-name` | `handlers::validate_stack_name` | `UNKNOWN` |
| `GET` | `/v1/status` | `handlers::get_status` | `UNKNOWN` |
| `GET` | `/v1/storage/kv-isolation/health` | `handlers::kv_isolation::get_kv_isolation_health` | `UNKNOWN` |
| `POST` | `/v1/storage/kv-isolation/scan` | `handlers::kv_isolation::trigger_kv_isolation_scan` | `UNKNOWN` |
| `GET` | `/v1/storage/mode` | `handlers::storage::get_storage_mode` | `UNKNOWN` |
| `GET` | `/v1/storage/stats` | `handlers::storage::get_storage_stats` | `UNKNOWN` |
| `GET` | `/v1/storage/tenant-usage` | `handlers::storage::get_tenant_storage_usage` | `UNKNOWN` |
| `GET` | `/v1/stream/activity/{workspace_id}` | `handlers::streaming::activity_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/adapters` | `handlers::streams::adapter_state_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/boot-progress` | `handlers::streaming::boot_progress_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/messages/{workspace_id}` | `handlers::streaming::messages_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/metrics` | `handlers::streams::system_metrics_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/notifications` | `handlers::streaming::notifications_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/stack-policies/{id}` | `handlers::adapter_stacks::stack_policy_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/telemetry` | `handlers::streams::telemetry_events_stream` | `UNKNOWN` |
| `GET` | `/v1/stream/trace-receipts` | `handlers::streaming::trace_receipts_stream` | `UNKNOWN` |
| `GET` | `/v1/streams/contacts` | `handlers::discovery::contacts_stream` | `UNKNOWN` |
| `GET` | `/v1/streams/discovery` | `handlers::discovery::discovery_stream` | `UNKNOWN` |
| `GET` | `/v1/streams/file-changes` | `handlers::git::file_changes_stream` | `UNKNOWN` |
| `GET` | `/v1/streams/training` | `handlers::streams::training_stream` | `UNKNOWN` |
| `GET` | `/v1/system/integrity` | `handlers::system::get_system_integrity` | `UNKNOWN` |
| `GET` | `/v1/system/memory` | `handlers::system_info::get_uma_memory` | `UNKNOWN` |
| `GET` | `/v1/system/memory/gpu` | `handlers::capacity::get_memory_report` | `UNKNOWN` |
| `GET` | `/v1/system/overview` | `handlers::system_overview::get_system_overview` | `UNKNOWN` |
| `GET` | `/v1/system/pilot-status` | `handlers::pilot_status::get_pilot_status` | `UNKNOWN` |
| `GET` | `/v1/system/resource-usage` | `handlers::system_info::get_resource_usage` | `UNKNOWN` |
| `GET` | `/v1/system/state` | `handlers::system_state::get_system_state` | `crates/adapteros-server-api/tests/system_state_test.rs` |
| `GET` | `/v1/system/status` | `handlers::system_status::get_system_status` | `UNKNOWN` |
| `GET` | `/v1/telemetry/bundles` | `handlers::list_telemetry_bundles` | `UNKNOWN` |
| `POST` | `/v1/telemetry/bundles/purge` | `handlers::purge_old_bundles` | `UNKNOWN` |
| `GET` | `/v1/telemetry/bundles/{bundle_id}/export` | `handlers::export_telemetry_bundle` | `UNKNOWN` |
| `POST` | `/v1/telemetry/bundles/{bundle_id}/verify` | `handlers::verify_bundle_signature` | `UNKNOWN` |
| `GET` | `/v1/telemetry/events/recent` | `handlers::telemetry::get_recent_activity` | `UNKNOWN` |
| `GET` | `/v1/telemetry/events/recent/stream` | `handlers::telemetry::recent_activity_stream` | `UNKNOWN` |
| `POST` | `/v1/telemetry/routing` | `handlers::routing_decisions::ingest_router_decision` | `UNKNOWN` |
| `GET` | `/v1/tenants` | `handlers::list_tenants` | `UNKNOWN` |
| `POST` | `/v1/tenants` | `handlers::create_tenant` | `UNKNOWN` |
| `PUT` | `/v1/tenants/{tenant_id}` | `handlers::update_tenant` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/adapters` | `handlers::assign_tenant_adapters` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/archive` | `handlers::archive_tenant` | `UNKNOWN` |
| `DELETE` | `/v1/tenants/{tenant_id}/default-stack` | `handlers::clear_default_stack` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/default-stack` | `handlers::get_default_stack` | `UNKNOWN` |
| `PUT` | `/v1/tenants/{tenant_id}/default-stack` | `handlers::set_default_stack` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/execution-policy` | `handlers::execution_policy::get_execution_policy` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/execution-policy` | `handlers::execution_policy::create_execution_policy` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/execution-policy/history` | `handlers::execution_policy::get_execution_policy_history` | `UNKNOWN` |
| `DELETE` | `/v1/tenants/{tenant_id}/execution-policy/{policy_id}` | `handlers::execution_policy::deactivate_execution_policy` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/manifests/{manifest_hash}` | `handlers::worker_manifests::fetch_manifest_by_hash` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/pause` | `handlers::pause_tenant` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/policies` | `handlers::assign_tenant_policies` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/policy-bindings` | `handlers::list_tenant_policy_bindings` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/policy-bindings/{policy_pack_id}/toggle` | `handlers::toggle_tenant_policy` | `UNKNOWN` |
| `POST` | `/v1/tenants/{tenant_id}/revoke-all-tokens` | `handlers::tenants::revoke_tenant_tokens` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/router/config` | `handlers::router_config::get_router_config` | `UNKNOWN` |
| `GET` | `/v1/tenants/{tenant_id}/usage` | `handlers::get_tenant_usage` | `UNKNOWN` |
| `GET` | `/v1/topology` | `handlers::topology::get_topology` | `UNKNOWN` |
| `GET` | `/v1/traces/search` | `handlers::telemetry::search_traces` | `UNKNOWN` |
| `GET` | `/v1/traces/{trace_id}` | `handlers::telemetry::get_trace` | `UNKNOWN` |
| `GET` | `/v1/training/dataset_versions/{dataset_version_id}/manifest` | `handlers::training_datasets::get_training_dataset_manifest` | `UNKNOWN` |
| `GET` | `/v1/training/dataset_versions/{dataset_version_id}/rows` | `handlers::training_datasets::stream_training_dataset_rows` | `UNKNOWN` |
| `POST` | `/v1/training/datasets/from-upload` | `handlers::training_datasets::create_training_dataset_from_upload` | `UNKNOWN` |
| `GET` | `/v1/training/jobs` | `handlers::list_training_jobs` | `UNKNOWN` |
| `POST` | `/v1/training/jobs` | `handlers::create_training_job` | `UNKNOWN` |
| `POST` | `/v1/training/jobs/batch-status` | `handlers::training::batch_training_status` | `UNKNOWN` |
| `GET` | `/v1/training/jobs/{job_id}` | `handlers::get_training_job` | `UNKNOWN` |
| `POST` | `/v1/training/jobs/{job_id}/cancel` | `handlers::cancel_training` | `UNKNOWN` |
| `GET` | `/v1/training/jobs/{job_id}/chat_bootstrap` | `handlers::get_chat_bootstrap` | `UNKNOWN` |
| `POST` | `/v1/training/jobs/{job_id}/export/coreml` | `handlers::export_coreml_training_job` | `UNKNOWN` |
| `GET` | `/v1/training/jobs/{job_id}/logs` | `handlers::training::get_training_logs` | `UNKNOWN` |
| `GET` | `/v1/training/jobs/{job_id}/metrics` | `handlers::training::get_training_metrics` | `UNKNOWN` |
| `PATCH` | `/v1/training/jobs/{job_id}/priority` | `handlers::training::update_training_priority` | `UNKNOWN` |
| `GET` | `/v1/training/jobs/{job_id}/progress` | `handlers::training::stream_training_progress` | `UNKNOWN` |
| `POST` | `/v1/training/jobs/{job_id}/retry` | `handlers::retry_training` | `UNKNOWN` |
| `GET` | `/v1/training/queue` | `handlers::training::get_training_queue` | `UNKNOWN` |
| `POST` | `/v1/training/repos/{repo_id}/versions/{version_id}/promote` | `handlers::promote_version` | `UNKNOWN` |
| `POST` | `/v1/training/sessions` | `handlers::training::create_training_session` | `UNKNOWN` |
| `POST` | `/v1/training/start` | `handlers::start_training` | `UNKNOWN` |
| `GET` | `/v1/training/templates` | `handlers::training::list_training_templates` | `UNKNOWN` |
| `GET` | `/v1/training/templates/{template_id}` | `handlers::training::get_training_template` | `UNKNOWN` |
| `GET` | `/v1/tutorials` | `handlers::tutorials::list_tutorials` | `UNKNOWN` |
| `DELETE` | `/v1/tutorials/{tutorial_id}/complete` | `handlers::tutorials::unmark_tutorial_completed` | `UNKNOWN` |
| `POST` | `/v1/tutorials/{tutorial_id}/complete` | `handlers::tutorials::mark_tutorial_completed` | `UNKNOWN` |
| `DELETE` | `/v1/tutorials/{tutorial_id}/dismiss` | `handlers::tutorials::unmark_tutorial_dismissed` | `UNKNOWN` |
| `POST` | `/v1/tutorials/{tutorial_id}/dismiss` | `handlers::tutorials::mark_tutorial_dismissed` | `UNKNOWN` |
| `GET` | `/v1/workers` | `handlers::list_workers` | `UNKNOWN` |
| `POST` | `/v1/workers/fatal` | `handlers::receive_worker_fatal` | `UNKNOWN` |
| `GET` | `/v1/workers/health/summary` | `handlers::get_worker_health_summary` | `UNKNOWN` |
| `POST` | `/v1/workers/register` | `handlers::workers::register_worker` | `UNKNOWN` |
| `POST` | `/v1/workers/spawn` | `handlers::worker_spawn` | `UNKNOWN` |
| `POST` | `/v1/workers/status` | `handlers::workers::notify_worker_status` | `UNKNOWN` |
| `GET` | `/v1/workers/{worker_id}/crashes` | `handlers::list_process_crashes` | `UNKNOWN` |
| `POST` | `/v1/workers/{worker_id}/debug` | `handlers::start_debug_session` | `UNKNOWN` |
| `GET` | `/v1/workers/{worker_id}/detail` | `handlers::worker_detail::get_worker_detail` | `UNKNOWN` |
| `GET` | `/v1/workers/{worker_id}/history` | `handlers::workers::get_worker_history` | `UNKNOWN` |
| `GET` | `/v1/workers/{worker_id}/incidents` | `handlers::list_worker_incidents` | `UNKNOWN` |
| `GET` | `/v1/workers/{worker_id}/logs` | `handlers::list_process_logs` | `UNKNOWN` |
| `POST` | `/v1/workers/{worker_id}/stop` | `handlers::stop_worker` | `UNKNOWN` |
| `POST` | `/v1/workers/{worker_id}/troubleshoot` | `handlers::run_troubleshooting_step` | `UNKNOWN` |
| `GET` | `/v1/workspaces` | `handlers::workspaces::list_workspaces` | `UNKNOWN` |
| `POST` | `/v1/workspaces` | `handlers::workspaces::create_workspace` | `UNKNOWN` |
| `GET` | `/v1/workspaces/me` | `handlers::workspaces::list_user_workspaces` | `UNKNOWN` |
| `DELETE` | `/v1/workspaces/{workspace_id}` | `handlers::workspaces::delete_workspace` | `UNKNOWN` |
| `GET` | `/v1/workspaces/{workspace_id}` | `handlers::workspaces::get_workspace` | `UNKNOWN` |
| `PUT` | `/v1/workspaces/{workspace_id}` | `handlers::workspaces::update_workspace` | `UNKNOWN` |
| `GET` | `/v1/workspaces/{workspace_id}/active` | `handlers::workspaces::get_workspace_active_state` | `UNKNOWN` |
| `POST` | `/v1/workspaces/{workspace_id}/active` | `handlers::workspaces::set_workspace_active_state` | `UNKNOWN` |
| `GET` | `/v1/workspaces/{workspace_id}/active-state` | `handlers::aliases::workspaces::get_workspace_active_state_alias` | `UNKNOWN` |
| `GET` | `/v1/workspaces/{workspace_id}/members` | `handlers::workspaces::list_workspace_members` | `UNKNOWN` |
| `POST` | `/v1/workspaces/{workspace_id}/members` | `handlers::workspaces::add_workspace_member` | `UNKNOWN` |
| `DELETE` | `/v1/workspaces/{workspace_id}/members/{member_id}` | `handlers::workspaces::remove_workspace_member` | `UNKNOWN` |
| `PUT` | `/v1/workspaces/{workspace_id}/members/{member_id}` | `handlers::workspaces::update_workspace_member` | `UNKNOWN` |
| `GET` | `/v1/workspaces/{workspace_id}/resources` | `handlers::workspaces::list_workspace_resources` | `UNKNOWN` |
| `POST` | `/v1/workspaces/{workspace_id}/resources` | `handlers::workspaces::share_workspace_resource` | `UNKNOWN` |
| `DELETE` | `/v1/workspaces/{workspace_id}/resources/{resource_id}` | `handlers::workspaces::unshare_workspace_resource` | `UNKNOWN` |
| `GET` | `/version` | `handlers::infrastructure::get_version` | `UNKNOWN` |

## Route Categories

### Health & System
- `/healthz`, `/readyz` - Liveness and readiness probes
- `/v1/status`, `/v1/system/*` - System status and configuration

### Authentication
- `/v1/auth/*` - Login, logout, MFA, sessions

### Adapters
- `/v1/adapters/*` - Adapter CRUD, lifecycle, versions
- `/v1/adapter-repositories/*` - Adapter repository management
- `/v1/adapter-stacks/*` - Stack composition

### Training
- `/v1/training/*` - Training jobs, datasets, templates

### Inference
- `/v1/infer/*` - Synchronous and streaming inference
- `/v1/chat/*` - Chat sessions and messages

### Data
- `/v1/datasets/*` - Dataset management and uploads
- `/v1/documents/*` - Document management
- `/v1/collections/*` - Collection management

### Diagnostics
- `/v1/diag/*` - Diagnostic runs and bundles
- `/v1/traces/*` - Trace search and retrieval

### Admin
- `/v1/tenants/*` - Tenant management
- `/v1/workspaces/*` - Workspace management
- `/v1/policies/*` - Policy management

## Maintenance

### Regenerating This Document

```bash
./scripts/dev/generate_route_map.sh
```

### CI Freshness Check

This document is verified by CI. If routes.rs changes without updating
this document, CI will fail with instructions to regenerate.

---

*See [docs/engineering/HANDLER_HYGIENE.md](../engineering/HANDLER_HYGIENE.md) for handler size audit.*
