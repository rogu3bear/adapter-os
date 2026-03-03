//! Startup inference warmup orchestration.
//!
//! Warmup runs after the server reaches Ready so chat can appear quickly while
//! model-load verification proceeds in the background. FullyReady is emitted
//! only when all resolved tenant warmups succeed.

use adapteros_db::workspace_active_state::WorkspaceActiveState;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType, JWT_ISSUER};
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::handlers::models::load_model;
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::AppState;
use axum::extract::{Path, State};
use axum::Extension;
use std::collections::HashSet;
use std::future::Future;
use std::time::Duration;
use tracing::{error, info, warn};

const DEFAULT_TENANT_ID: &str = "default";
const WARMUP_WARNING_COMPONENT: &str = "inference_warmup";
const READY_POLL_INTERVAL_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TenantWarmupCandidate {
    tenant_id: String,
    active_model_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TenantWarmupTarget {
    tenant_id: String,
    model_id: Option<String>,
}

impl TenantWarmupTarget {
    fn progress_key(&self) -> String {
        let model = self.model_id.as_deref().unwrap_or("<no-model>");
        format!("{}::{}", self.tenant_id, model)
    }
}

fn collect_warmup_candidates(mut states: Vec<WorkspaceActiveState>) -> Vec<TenantWarmupCandidate> {
    states.sort_by(|a, b| {
        a.tenant_id
            .cmp(&b.tenant_id)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for state in states {
        if seen.insert(state.tenant_id.clone()) {
            candidates.push(TenantWarmupCandidate {
                tenant_id: state.tenant_id,
                active_model_hint: state.active_base_model_id,
            });
        }
    }

    if candidates.is_empty() {
        candidates.push(TenantWarmupCandidate {
            tenant_id: DEFAULT_TENANT_ID.to_string(),
            active_model_hint: None,
        });
    }

    candidates
}

fn choose_model_candidate(
    active_hint: Option<&str>,
    latest_status_model: Option<&str>,
    available_models: &[String],
) -> Option<String> {
    active_hint
        .map(ToOwned::to_owned)
        .or_else(|| latest_status_model.map(ToOwned::to_owned))
        .or_else(|| available_models.first().cloned())
}

async fn resolve_target_model_id(
    state: &AppState,
    tenant_id: &str,
    active_hint: Option<&str>,
) -> Result<Option<String>, String> {
    let latest_status_model = state
        .db
        .get_base_model_status(tenant_id)
        .await
        .map_err(|e| format!("tenant '{}' status query failed: {}", tenant_id, e))?
        .map(|status| status.model_id);

    let available_models: Vec<String> = state
        .db
        .list_models(tenant_id)
        .await
        .map_err(|e| format!("tenant '{}' model list query failed: {}", tenant_id, e))?
        .into_iter()
        .map(|model| model.id)
        .collect();

    Ok(choose_model_candidate(
        active_hint,
        latest_status_model.as_deref(),
        &available_models,
    ))
}

async fn resolve_warmup_targets(state: &AppState) -> Result<Vec<TenantWarmupTarget>, String> {
    let states = state
        .db
        .list_workspace_active_states()
        .await
        .map_err(|e| format!("failed to list workspace active states: {}", e))?;

    let mut targets = Vec::new();
    for candidate in collect_warmup_candidates(states) {
        let model_id = resolve_target_model_id(
            state,
            &candidate.tenant_id,
            candidate.active_model_hint.as_deref(),
        )
        .await?;

        targets.push(TenantWarmupTarget {
            tenant_id: candidate.tenant_id,
            model_id,
        });
    }

    Ok(targets)
}

fn boot_warmup_claims(tenant_id: &str) -> Claims {
    let now = chrono::Utc::now().timestamp();
    Claims {
        sub: "boot-warmup".to_string(),
        email: "boot-warmup@adapteros.local".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: tenant_id.to_string(),
        admin_tenants: vec![tenant_id.to_string()],
        device_id: Some("boot-warmup".to_string()),
        session_id: Some("boot-warmup".to_string()),
        mfa_level: None,
        rot_id: None,
        exp: now + 3600,
        iat: now,
        jti: format!("boot-warmup-{}", tenant_id),
        nbf: now,
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::InternalService),
    }
}

async fn warmup_model_via_models_handler(
    state: AppState,
    tenant_id: String,
    model_id: String,
) -> Result<(), String> {
    let claims = boot_warmup_claims(&tenant_id);
    match load_model(
        State(state),
        Extension(claims),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_id.clone()),
    )
    .await
    {
        Ok(response) => {
            if response.status.is_ready() {
                Ok(())
            } else {
                Err(format!(
                    "tenant '{}' model '{}' returned non-ready status '{}'",
                    tenant_id,
                    model_id,
                    response.status.as_str()
                ))
            }
        }
        Err(e) => Err(format!(
            "tenant '{}' model '{}' load failed [{}]: {}",
            tenant_id, model_id, e.code, e.message
        )),
    }
}

async fn execute_warmup_targets<F, Fut>(
    targets: &[TenantWarmupTarget],
    boot_state: &BootStateManager,
    mut loader: F,
) -> Vec<String>
where
    F: FnMut(String, String) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let mut failures = Vec::new();

    for target in targets {
        let progress_key = target.progress_key();
        boot_state.add_pending_model(progress_key.clone());

        let Some(model_id) = target.model_id.clone() else {
            let message = format!(
                "tenant '{}' has no base model candidate for warmup",
                target.tenant_id
            );
            boot_state.mark_model_failed(progress_key);
            boot_state.record_boot_warning(WARMUP_WARNING_COMPONENT, message.clone());
            failures.push(message);
            continue;
        };

        match loader(target.tenant_id.clone(), model_id.clone()).await {
            Ok(()) => {
                boot_state.mark_model_ready(progress_key);
                info!(
                    tenant_id = %target.tenant_id,
                    model_id = %model_id,
                    "Startup warmup completed"
                );
            }
            Err(e) => {
                let message = format!(
                    "tenant '{}' warmup failed for model '{}': {}",
                    target.tenant_id, model_id, e
                );
                boot_state.mark_model_failed(progress_key);
                boot_state.record_boot_warning(WARMUP_WARNING_COMPONENT, message.clone());
                failures.push(message);
            }
        }
    }

    failures
}

async fn finalize_boot_state_after_warmup(
    boot_state: &BootStateManager,
    failures: &[String],
) -> bool {
    if failures.is_empty()
        && boot_state.is_ready()
        && !boot_state.is_fully_ready()
        && !boot_state.is_shutting_down()
        && !boot_state.is_draining()
    {
        boot_state.fully_ready().await;
        return true;
    }
    false
}

/// Runs one-shot startup inference warmup and promotes boot state to FullyReady
/// only if all tenant warmups succeed.
pub async fn run_startup_inference_warmup(
    state: AppState,
    boot_state: BootStateManager,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    info!("Startup inference warmup task initialized");

    loop {
        if boot_state.is_ready() {
            break;
        }
        if boot_state.is_failed() || boot_state.is_draining() || boot_state.is_shutting_down() {
            warn!("Startup warmup aborted because boot is no longer in an active startup state");
            return;
        }

        tokio::select! {
            biased;
            _ = shutdown_rx.recv() => {
                info!("Startup warmup received shutdown signal before Ready, exiting");
                return;
            }
            _ = tokio::time::sleep(Duration::from_millis(READY_POLL_INTERVAL_MS)) => {}
        }
    }

    let targets = match resolve_warmup_targets(&state).await {
        Ok(targets) => targets,
        Err(e) => {
            let message = format!("failed to resolve warmup targets: {}", e);
            boot_state.record_boot_warning(WARMUP_WARNING_COMPONENT, message.clone());
            error!(error = %e, "Startup warmup target resolution failed");
            return;
        }
    };

    let state_for_loader = state.clone();
    let failures = execute_warmup_targets(&targets, &boot_state, move |tenant_id, model_id| {
        let state = state_for_loader.clone();
        async move { warmup_model_via_models_handler(state, tenant_id, model_id).await }
    })
    .await;

    if finalize_boot_state_after_warmup(&boot_state, &failures).await {
        info!(
            tenants = targets.len(),
            "Startup warmup completed for all tenants; boot promoted to FullyReady"
        );
    } else if !failures.is_empty() {
        warn!(
            failures = ?failures,
            "Startup warmup completed with failures; FullyReady remains blocked"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_server_api::boot_state::BootState;

    async fn boot_to_ready(manager: &BootStateManager) {
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;
    }

    fn mk_state(
        tenant_id: &str,
        active_base_model_id: Option<&str>,
        updated_at: &str,
    ) -> WorkspaceActiveState {
        WorkspaceActiveState {
            tenant_id: tenant_id.to_string(),
            active_base_model_id: active_base_model_id.map(ToOwned::to_owned),
            active_plan_id: None,
            active_adapter_ids: None,
            manifest_hash_b3: None,
            updated_at: updated_at.to_string(),
        }
    }

    #[test]
    fn choose_model_candidate_prefers_active_then_status_then_inventory() {
        let inventory = vec!["model-inventory".to_string()];

        assert_eq!(
            choose_model_candidate(Some("model-active"), Some("model-status"), &inventory),
            Some("model-active".to_string())
        );
        assert_eq!(
            choose_model_candidate(None, Some("model-status"), &inventory),
            Some("model-status".to_string())
        );
        assert_eq!(
            choose_model_candidate(None, None, &inventory),
            Some("model-inventory".to_string())
        );
    }

    #[test]
    fn collect_warmup_candidates_dedupes_in_deterministic_tenant_order() {
        let states = vec![
            mk_state("tenant-b", Some("m2"), "2026-01-02T00:00:00Z"),
            mk_state("tenant-a", Some("m1"), "2026-01-01T00:00:00Z"),
            mk_state("tenant-a", Some("m1-old"), "2025-12-31T00:00:00Z"),
        ];

        let candidates = collect_warmup_candidates(states);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].tenant_id, "tenant-a");
        assert_eq!(candidates[0].active_model_hint.as_deref(), Some("m1"));
        assert_eq!(candidates[1].tenant_id, "tenant-b");
    }

    #[tokio::test]
    async fn all_tenant_success_allows_fully_ready() {
        let boot_state = BootStateManager::new();
        boot_to_ready(&boot_state).await;

        let targets = vec![
            TenantWarmupTarget {
                tenant_id: "tenant-a".to_string(),
                model_id: Some("model-a".to_string()),
            },
            TenantWarmupTarget {
                tenant_id: "tenant-b".to_string(),
                model_id: Some("model-b".to_string()),
            },
        ];

        let failures =
            execute_warmup_targets(&targets, &boot_state, |_tenant, _model| async { Ok(()) }).await;
        assert!(failures.is_empty());

        let promoted = finalize_boot_state_after_warmup(&boot_state, &failures).await;
        assert!(promoted, "warmup success should promote to FullyReady");
        assert_eq!(boot_state.current_state(), BootState::FullyReady);
    }

    #[tokio::test]
    async fn warmup_failure_blocks_fully_ready_and_records_warning() {
        let boot_state = BootStateManager::new();
        boot_to_ready(&boot_state).await;

        let targets = vec![
            TenantWarmupTarget {
                tenant_id: "tenant-a".to_string(),
                model_id: Some("model-a".to_string()),
            },
            TenantWarmupTarget {
                tenant_id: "tenant-b".to_string(),
                model_id: Some("model-b".to_string()),
            },
        ];

        let failures = execute_warmup_targets(&targets, &boot_state, |tenant, _model| async move {
            if tenant == "tenant-b" {
                Err("synthetic failure".to_string())
            } else {
                Ok(())
            }
        })
        .await;
        assert_eq!(failures.len(), 1);

        let promoted = finalize_boot_state_after_warmup(&boot_state, &failures).await;
        assert!(!promoted, "any tenant failure must block FullyReady");
        assert!(!boot_state.is_fully_ready());
        assert!(
            !boot_state.get_boot_warnings().is_empty(),
            "warmup failure should emit a boot warning"
        );
    }
}
