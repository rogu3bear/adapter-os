use crate::auth::{AuthMode, Claims, PrincipalType};
use crate::state::AppState;
use adapteros_api_types::{RunActor, RunEnvelope, API_SCHEMA_VERSION};
use adapteros_db::workers::WorkerWithBinding;
use chrono::Utc;
use tracing::warn;

pub const RUN_ENVELOPE_VERSION: &str = "v1";

/// Build a canonical run envelope at the API edge.
fn build_run_envelope(
    state: &AppState,
    claims: &Claims,
    run_id: String,
    reasoning_mode: bool,
    tick: Option<u64>,
) -> RunEnvelope {
    let mut roles = claims.roles.clone();
    roles.push(claims.role.clone());
    roles.sort();
    roles.dedup();

    let subject = match claims.auth_mode {
        AuthMode::DevBypass => "dev-bypass".to_string(),
        AuthMode::Unauthenticated => "anonymous".to_string(),
        _ => claims.sub.clone(),
    };

    let actor = RunActor {
        subject,
        roles,
        principal_type: claims
            .principal_type
            .as_ref()
            .map(principal_type_to_str)
            .map(ToOwned::to_owned),
        auth_mode: Some(auth_mode_to_str(&claims.auth_mode)),
    };

    let manifest_hash_b3 = state.manifest_hash.clone();
    if manifest_hash_b3.is_none() {
        warn!(
            run_id = %run_id,
            "Manifest hash not set on AppState; run envelope will be degraded"
        );
    }

    RunEnvelope {
        run_id,
        schema_version: API_SCHEMA_VERSION.to_string(),
        workspace_id: claims.tenant_id.clone(),
        actor,
        manifest_hash_b3,
        plan_id: None,
        policy_mask_digest_b3: None,
        router_seed: None,
        tick,
        worker_id: None,
        reasoning_mode,
        determinism_version: RUN_ENVELOPE_VERSION.to_string(),
        boot_trace_id: state.boot_state.as_ref().map(|boot| boot.boot_trace_id()),
        created_at: Utc::now(),
    }
}

/// Build a canonical run envelope at the API edge (assigns a logical tick).
pub fn new_run_envelope(
    state: &AppState,
    claims: &Claims,
    run_id: String,
    reasoning_mode: bool,
) -> RunEnvelope {
    let tick = state
        .tick_ledger
        .as_ref()
        .map(|ledger| ledger.increment_tick());
    build_run_envelope(state, claims, run_id, reasoning_mode, tick)
}

/// Build a run envelope without assigning a logical tick (used for replay paths).
pub fn new_run_envelope_no_tick(
    state: &AppState,
    claims: &Claims,
    run_id: String,
    reasoning_mode: bool,
) -> RunEnvelope {
    build_run_envelope(state, claims, run_id, reasoning_mode, None)
}

/// Record policy mask digest onto the envelope.
pub fn set_policy_mask(envelope: &mut RunEnvelope, digest: Option<&[u8; 32]>) {
    envelope.policy_mask_digest_b3 = digest.map(hex::encode);
}

/// Attach router seed to the envelope.
pub fn set_router_seed(envelope: &mut RunEnvelope, seed: Option<&String>) {
    if let Some(seed) = seed {
        envelope.router_seed = Some(seed.clone());
    }
}

/// Attach worker and plan context once selection is known.
pub fn set_worker_context(
    envelope: &mut RunEnvelope,
    worker: Option<&WorkerWithBinding>,
    manifest_hash: Option<String>,
) {
    if let Some(worker) = worker {
        envelope.worker_id = Some(worker.id.clone());
        envelope.plan_id = Some(worker.plan_id.clone());
        if envelope.manifest_hash_b3.is_none() {
            envelope.manifest_hash_b3 = worker.manifest_hash_b3.clone();
        }
    }

    if let Some(manifest_hash) = manifest_hash {
        envelope.manifest_hash_b3 = Some(manifest_hash);
    }
}

fn auth_mode_to_str(mode: &AuthMode) -> String {
    match mode {
        AuthMode::BearerToken => "bearer".to_string(),
        AuthMode::Cookie => "cookie".to_string(),
        AuthMode::ApiKey => "api_key".to_string(),
        AuthMode::DevBypass => "dev_bypass".to_string(),
        AuthMode::Unauthenticated => "unauthenticated".to_string(),
    }
}

fn principal_type_to_str(principal: &PrincipalType) -> &'static str {
    match principal {
        PrincipalType::User => "user",
        PrincipalType::ApiKey => "api_key",
        PrincipalType::DevBypass => "dev_bypass",
        PrincipalType::InternalService => "internal_service",
    }
}
