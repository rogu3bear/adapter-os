use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::middleware::request_id::RequestId;
use crate::middleware::require_any_role;
use crate::security;
use crate::state::AppState;
use crate::types::{ErrorResponse, TokenizeRequest, TokenizeResponse, MAX_REPLAY_TEXT_SIZE};
use adapteros_db::users::Role;
use axum::{extract::State, Extension, Json};
use std::path::PathBuf;
use tokenizers::Tokenizer;
use tracing::info;

/// Tokenize raw text with the model's tokenizer (non-streaming).
#[utoipa::path(
    post,
    path = "/v1/tokenize",
    request_body = TokenizeRequest,
    responses(
        (status = 200, description = "Tokenization succeeded", body = TokenizeResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Model not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tokenize"
)]
pub async fn tokenize(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    request_id: Option<Extension<RequestId>>,
    Json(req): Json<TokenizeRequest>,
) -> Result<Json<TokenizeResponse>, ApiError> {
    // Enforce role guard
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Basic input guard aligned with inference validators
    if req.text.is_empty() {
        return Err(ApiError::bad_request("text cannot be empty"));
    }
    if req.text.len() > MAX_REPLAY_TEXT_SIZE {
        return Err(ApiError::bad_request("text too long").with_details(format!(
            "max {} characters; got {}",
            MAX_REPLAY_TEXT_SIZE,
            req.text.len()
        )));
    }

    let result = async {
        let model = state
            .db
            .get_model(&req.model_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("model").with_details(format!("Model ID: {}", req.model_id))
            })?;

        // Tenant isolation: model.tenant_id must match unless admin with admin_tenants grants
        if let Some(model_tenant) = &model.tenant_id {
            let is_admin = claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
            if !is_admin && model_tenant != &claims.tenant_id {
                return Err(ApiError::forbidden("cross-tenant access denied")
                    .with_details("Tokenize requires tenant match"));
            }
            if is_admin {
                security::validate_tenant_isolation(&claims, model_tenant)?;
            }
        }

        let model_path = model.model_path.clone().ok_or_else(|| {
            ApiError::bad_request("model_path missing")
                .with_details("Model must set model_path to use /v1/tokenize")
        })?;

        let tokenizer_path = PathBuf::from(model_path).join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(ApiError::not_found("tokenizer.json")
                .with_details(tokenizer_path.display().to_string()));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            ApiError::internal("failed to load tokenizer").with_details(e.to_string())
        })?;

        let encoding = tokenizer.encode(req.text.clone(), true).map_err(|e| {
            ApiError::bad_request("failed to tokenize input").with_details(e.to_string())
        })?;

        let token_ids = encoding.get_ids().to_vec();
        let token_count = token_ids.len();

        // Compute metadata
        let tokenizer_hash_b3 = adapteros_core::B3Hash::hash_file(&tokenizer_path).ok();
        if let Some(hash) = tokenizer_hash_b3 {
            let hash_hex = hash.to_hex();
            if hash_hex != model.tokenizer_hash_b3 {
                return Err(
                    ApiError::conflict("tokenizer hash mismatch").with_details(format!(
                        "Expected {}, found {} on disk",
                        model.tokenizer_hash_b3, hash_hex
                    )),
                );
            }
        }
        let tokenizer_vocab_size = Some(tokenizer.get_vocab_size(true) as u32);

        // Record basic counters for observability
        state
            .metrics_registry
            .record_metric("tokenize.requests".to_string(), 1.0)
            .await;
        state
            .metrics_registry
            .record_metric("tokenize.tokens".to_string(), token_count as f64)
            .await;

        let request_id_str = request_id.clone().map(|r| r.0 .0).unwrap_or_default();

        info!(
            request_id = %request_id_str,
            model_id = %req.model_id,
            token_count,
            text_len = req.text.len(),
            "Tokenization request processed"
        );

        crate::audit_helper::log_success_or_warn(
            &state.db,
            &claims,
            crate::audit_helper::actions::TOKENIZE_EXECUTE,
            crate::audit_helper::resources::MODEL,
            Some(&req.model_id),
        )
        .await;

        Ok(TokenizeResponse {
            token_ids,
            token_count,
            tokenizer_hash_b3: tokenizer_hash_b3
                .map(|h| h.to_hex())
                .or_else(|| Some(model.tokenizer_hash_b3)),
            tokenizer_vocab_size,
        })
    }
    .await;

    let request_id_str = request_id.map(|r| r.0 .0).unwrap_or_default();

    match result {
        Ok(resp) => {
            info!(
                request_id = %request_id_str,
                model_id = %req.model_id,
                token_count = resp.token_count,
                text_len = req.text.len(),
                "Tokenization request processed"
            );

            crate::audit_helper::log_success_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::TOKENIZE_EXECUTE,
                crate::audit_helper::resources::MODEL,
                Some(&req.model_id),
            )
            .await;

            Ok(Json(resp))
        }
        Err(err) => {
            crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::TOKENIZE_EXECUTE,
                crate::audit_helper::resources::MODEL,
                Some(&req.model_id),
                &err.to_string(),
            )
            .await
            .ok();

            Err(err)
        }
    }
}
