//! Fill-in-the-Middle (FIM) inference endpoint handler
//!
//! Accepts prefix/suffix code context and generates the infill completion.
//! The handler maps the FIM request into `InferenceRequestInternal` with
//! FIM context fields set, dispatches through the standard inference pipeline
//! (routing, policy, audit), and the worker builds the FIM token sequence
//! using `build_fim_prompt()`.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::ip_extraction::ClientIp;
use crate::middleware::request_id::RequestId;
use crate::state::AppState;
use crate::types::context::InferenceRequestInternal;
use crate::types::ErrorResponse;
use adapteros_api_types::inference::{FIMRequest, FIMResponse};
use axum::{extract::State, Extension, Json};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// FIM completions endpoint
///
/// Generates code to fill the gap between a prefix (code before cursor) and
/// suffix (code after cursor) using the FIM token format.
#[utoipa::path(
    tag = "inference",
    post,
    path = "/v1/fim/completions",
    request_body = FIMRequest,
    responses(
        (status = 200, description = "FIM completion succeeded", body = FIMResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 501, description = "Model does not support FIM", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn fim_completions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    request_id: Option<Extension<RequestId>>,
    Json(req): Json<FIMRequest>,
) -> ApiResult<FIMResponse> {
    let request_id_str = request_id
        .map(|r| r.0 .0.clone())
        .unwrap_or_else(crate::id_generator::readable_request_id);

    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate inputs
    if req.prefix.is_empty() && req.suffix.is_empty() {
        return Err(ApiError::bad_request(
            "prefix and suffix cannot both be empty",
        ));
    }

    if req.max_tokens == 0 {
        return Err(ApiError::bad_request("max_tokens must be > 0"));
    }

    info!(
        request_id = %request_id_str,
        prefix_len = req.prefix.len(),
        suffix_len = req.suffix.len(),
        max_tokens = req.max_tokens,
        adapter_id = ?req.adapter_id,
        "FIM completion request"
    );

    // Build InferenceRequestInternal with FIM context.
    // The prompt field carries a text representation for policy hooks and logging.
    // The worker uses fim_prefix/fim_suffix to build the actual FIM token sequence.
    let prompt_for_policy = format!("{}\n[FIM_CURSOR]\n{}", req.prefix, req.suffix);

    let mut internal = InferenceRequestInternal::new(claims.tenant_id.clone(), prompt_for_policy);
    internal.request_id = request_id_str.clone();
    internal.fim_prefix = Some(req.prefix.clone());
    internal.fim_suffix = Some(req.suffix.clone());
    internal.max_tokens = req.max_tokens as usize;
    internal.temperature = req.temperature;
    internal.stream = req.stream;
    internal.require_step = req.stream;
    internal.claims = Some(claims.clone());
    internal.run_envelope = req.run_envelope.clone();

    if let Some(ref adapter_id) = req.adapter_id {
        internal.adapters = Some(vec![adapter_id.clone()]);
    }
    if let Some(ref stack_id) = req.stack_id {
        internal.stack_id = Some(stack_id.clone());
    }

    // Dispatch through InferenceCore — full 11-stage pipeline
    let cancel_token = CancellationToken::new();
    let state_for_task = state.clone();
    let inference_task = tokio::spawn(async move {
        let core = InferenceCore::new(&state_for_task);
        core.route_and_infer(internal, None, Some(cancel_token), None, None)
            .await
    });

    let inference_result = match inference_task.await {
        Ok(res) => res,
        Err(e) => {
            return Err(
                ApiError::internal("FIM inference task join error").with_details(e.to_string())
            );
        }
    };

    match inference_result {
        Ok(result) => {
            info!(
                request_id = %request_id_str,
                tokens_generated = result.tokens_generated,
                finish_reason = %result.finish_reason,
                "FIM completion succeeded"
            );

            Ok(Json(FIMResponse {
                completion: result.text,
                token_ids: vec![],
                tokens_generated: result.tokens_generated as u32,
                stop_reason: Some(result.finish_reason),
                adapter_id: result.adapters_used.first().cloned(),
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            }))
        }
        Err(e) => {
            warn!(
                request_id = %request_id_str,
                error = %e,
                "FIM inference failed"
            );
            Err(ApiError::internal("FIM inference failed").with_details(e.to_string()))
        }
    }
}
