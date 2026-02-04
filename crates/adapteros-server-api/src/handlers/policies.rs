use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use adapteros_policy::validation::validate_customization;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use std::collections::HashMap;

/// Assign policies to tenant
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/policies",
    request_body = AssignPoliciesRequest,
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Policies assigned", body = AssignPoliciesResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn assign_tenant_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignPoliciesRequest>,
) -> Result<Json<AssignPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    crate::middleware::require_any_role(&claims, &[Role::Admin])?;
    let tenant_id = crate::id_resolver::resolve_any_id(&state.db, &tenant_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Create tenant-policy associations using Db trait method
    for policy_id in &req.policy_ids {
        state
            .db
            .assign_policy_to_tenant(&tenant_id, policy_id, &claims.sub)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to assign policy")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    tracing::info!(
        "Assigned {} policies to tenant {} by {}",
        req.policy_ids.len(),
        tenant_id,
        claims.email
    );

    Ok(Json(AssignPoliciesResponse {
        tenant_id,
        assigned_cpids: req.policy_ids,
        assigned_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// List policies
#[utoipa::path(
    get,
    path = "/v1/policies",
    responses(
        (status = 200, description = "Policy packs", body = Vec<PolicyPackResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn list_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<PolicyPackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view policies
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;

    // Query database for policy packs
    let packs = state.db.list_policy_packs(None, None).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to list policy packs");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list policy packs")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response = packs
        .into_iter()
        .map(|pack| PolicyPackResponse {
            cpid: pack.id,
            content: pack.content_json,
            hash_b3: pack.hash_b3,
            created_at: pack.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Get policy by CPID
#[utoipa::path(
    get,
    path = "/v1/policies/{cpid}",
    params(
        ("cpid" = String, Path, description = "Policy CPID")
    ),
    responses(
        (status = 200, description = "Policy pack", body = PolicyPackResponse),
        (status = 404, description = "Policy pack not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn get_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view policies
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;
    let cpid = crate::id_resolver::resolve_any_id(&state.db, &cpid)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Query database for policy pack
    let pack = state
        .db
        .get_policy_pack(&cpid)
        .await
        .map_err(|e| {
            tracing::error!(cpid = %cpid, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(PolicyPackResponse {
        cpid: pack.id,
        content: pack.content_json,
        hash_b3: pack.hash_b3,
        created_at: pack.created_at,
    }))
}

/// Validate policy (stub)
#[utoipa::path(
    post,
    path = "/v1/policies/validate",
    request_body = ValidatePolicyRequest,
    responses(
        (status = 200, description = "Validation result", body = PolicyValidationResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn validate_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ValidatePolicyRequest>,
) -> Result<Json<PolicyValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Compliance and Admin can validate policies
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::PolicyValidate,
    )?;

    let content_hash = adapteros_core::B3Hash::hash(req.content.as_bytes()).to_string();
    let parsed = match serde_json::from_str::<serde_json::Value>(&req.content) {
        Ok(value) => value,
        Err(e) => {
            if let Err(audit_err) = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::POLICY_VALIDATE,
                crate::audit_helper::resources::POLICY,
                Some(&content_hash),
                &format!("Invalid JSON: {}", e),
            )
            .await
            {
                tracing::warn!(error = %audit_err, "Audit log failed");
            }

            return Ok(Json(PolicyValidationResponse {
                valid: false,
                errors: vec![format!("Invalid JSON: {}", e)],
                hash_b3: None,
            }));
        }
    };

    let mut errors = Vec::new();

    let root = match parsed.as_object() {
        Some(root) => root,
        None => {
            errors.push("Policy must be a JSON object".to_string());
            let valid = false;
            return Ok(Json(PolicyValidationResponse {
                valid,
                errors,
                hash_b3: None,
            }));
        }
    };

    let has_pack_schema = root.contains_key("schema") || root.contains_key("packs");
    if has_pack_schema {
        if let Some(schema) = root.get("schema") {
            if schema != "adapteros.policy.v1" {
                errors.push(
                    "Invalid policy schema version (expected adapteros.policy.v1)".to_string(),
                );
            }
        } else {
            errors.push("Missing policy schema version".to_string());
        }

        let packs = root.get("packs");
        match packs.and_then(|p| p.as_object()) {
            Some(packs_obj) => {
                for (pack_name, pack_value) in packs_obj {
                    let pack_json = match serde_json::to_string(pack_value) {
                        Ok(json) => json,
                        Err(e) => {
                            errors.push(format!("pack {}: failed to serialize: {}", pack_name, e));
                            continue;
                        }
                    };

                    match validate_customization(pack_name, &pack_json) {
                        Ok(result) => {
                            for err in result.errors {
                                errors.push(format!("pack {}: {}", pack_name, err));
                            }
                            for warn in result.warnings {
                                tracing::warn!(
                                    pack = %pack_name,
                                    warning = %warn,
                                    "Policy validation warning"
                                );
                            }
                        }
                        Err(e) => {
                            errors.push(format!("pack {}: {}", pack_name, e));
                        }
                    }
                }
            }
            None => errors.push("Missing or invalid packs object".to_string()),
        }
    } else {
        let policy_type = root
            .get("type")
            .or_else(|| root.get("policy_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let policy_type = match policy_type {
            Some(policy_type) => policy_type,
            None => {
                errors.push("Policy must include a non-empty 'type' field".to_string());
                let valid = false;
                return Ok(Json(PolicyValidationResponse {
                    valid,
                    errors,
                    hash_b3: None,
                }));
            }
        };

        let config_value = if let Some(config) = root.get("config") {
            config.clone()
        } else {
            let mut config_obj = root.clone();
            config_obj.remove("type");
            config_obj.remove("policy_type");
            serde_json::Value::Object(config_obj)
        };

        if !config_value.is_object() {
            errors.push("Policy config must be a JSON object".to_string());
        } else {
            let config_json = match serde_json::to_string(&config_value) {
                Ok(json) => json,
                Err(e) => {
                    errors.push(format!("Failed to serialize policy config: {}", e));
                    String::new()
                }
            };

            if !config_json.is_empty() {
                match validate_customization(&policy_type, &config_json) {
                    Ok(result) => {
                        for err in result.errors {
                            errors.push(format!("pack {}: {}", policy_type, err));
                        }
                        for warn in result.warnings {
                            tracing::warn!(
                                pack = %policy_type,
                                warning = %warn,
                                "Policy validation warning"
                            );
                        }
                    }
                    Err(e) => {
                        errors.push(format!("pack {}: {}", policy_type, e));
                    }
                }
            }
        }
    }

    let valid = errors.is_empty();

    if valid {
        if let Err(e) = crate::audit_helper::log_success(
            &state.db,
            &claims,
            crate::audit_helper::actions::POLICY_VALIDATE,
            crate::audit_helper::resources::POLICY,
            Some(&content_hash),
        )
        .await
        {
            tracing::warn!(error = %e, "Audit log failed");
        }
    } else if let Err(audit_err) = crate::audit_helper::log_failure(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_VALIDATE,
        crate::audit_helper::resources::POLICY,
        Some(&content_hash),
        &format!("Validation failed: {}", errors.join("; ")),
    )
    .await
    {
        tracing::warn!(error = %audit_err, "Audit log failed");
    }

    Ok(Json(PolicyValidationResponse {
        valid,
        errors,
        hash_b3: valid.then_some(content_hash),
    }))
}

/// Apply policy
#[utoipa::path(
    post,
    path = "/v1/policies/apply",
    request_body = ApplyPolicyRequest,
    responses(
        (status = 200, description = "Policy applied", body = PolicyPackResponse),
        (status = 400, description = "Invalid policy", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn apply_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ApplyPolicyRequest>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (applying policies is a critical operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyApply)?;

    // Validate JSON format
    let content_value: serde_json::Value = serde_json::from_str(&req.content).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid policy JSON")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Compute hash
    let hash_b3 = adapteros_core::B3Hash::hash(req.content.as_bytes()).to_string();

    // Get or generate signing key for the tenant
    let signing_key_result = adapteros_db::sqlx::query_scalar::<_, String>(
        "SELECT signing_key FROM signing_keys WHERE tenant_id = ? AND key_type = 'ed25519' AND active = 1"
    )
    .bind(&claims.sub)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to query signing key: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to retrieve signing key").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let signing_key_hex = match signing_key_result {
        Some(key) => key,
        None => {
            // Generate new Ed25519 signing key
            use adapteros_crypto::signature::generate_keypair;
            let (secret_key, _public_key) = generate_keypair();
            let key_hex = hex::encode(secret_key.to_bytes());

            // Store the key
            adapteros_db::sqlx::query(
                "INSERT INTO signing_keys (tenant_id, key_type, signing_key, active, created_at)
                 VALUES (?, 'ed25519', ?, 1, datetime('now'))",
            )
            .bind(&claims.sub)
            .bind(&key_hex)
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                tracing::error!("Failed to store signing key: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to store signing key")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            key_hex
        }
    };

    // Sign the policy content
    let signature =
        adapteros_crypto::signature::sign_data(req.content.as_bytes(), &signing_key_hex).map_err(
            |e| {
                tracing::error!("Failed to sign policy: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Signing failed")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            },
        )?;
    let signature_hex = format!("ed25519:{}", hex::encode(signature));

    // Extract public key from signing key
    let secret_key_bytes = hex::decode(&signing_key_hex).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Invalid signing key format")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    use adapteros_crypto::signature::SigningKey;
    let signing_key_obj = SigningKey::from_bytes(&secret_key_bytes.try_into().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Invalid signing key length").with_code("INTERNAL_ERROR")),
        )
    })?);
    let public_key = signing_key_obj.verifying_key();
    let public_key_hex = hex::encode(public_key.to_bytes());

    // Extract policy type from content (default to "custom")
    let policy_type = content_value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("custom")
        .to_string();

    // Store policy pack in database
    let id = state
        .db
        .store_policy_pack(
            &req.cpid,
            "1.0", // Default version
            &policy_type,
            &req.content,
            &signature_hex,
            &public_key_hex,
            &hash_b3,
            &claims.email,
            req.description.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!(cpid = %req.cpid, error = %e, "Failed to store policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to store policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Activate the policy if requested
    if req.activate.unwrap_or(false) {
        state.db.activate_policy_pack(&id).await.map_err(|e| {
            tracing::error!(cpid = %req.cpid, error = %e, "Failed to activate policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to activate policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    let created_at = chrono::Utc::now().to_rfc3339();

    // Audit log: policy applied
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_APPLY,
        crate::audit_helper::resources::POLICY,
        Some(&req.cpid),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(PolicyPackResponse {
        cpid: id,
        content: req.content,
        hash_b3,
        created_at,
    }))
}

/// Sign policy with Ed25519 using server's configured signing key (PRD-SEC-01)
#[utoipa::path(
    post,
    path = "/v1/policies/{cpid}/sign",
    params(
        ("cpid" = String, Path, description = "Policy CPID")
    ),
    responses(
        (status = 200, description = "Policy signed", body = SignPolicyResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn sign_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<SignPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (signing policies is a critical operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicySign)?;
    let cpid = crate::id_resolver::resolve_any_id(&state.db, &cpid)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Use the server's configured Ed25519 signing key (PRD-SEC-01: no ad-hoc key generation)
    let signing_key = &state.ed25519_keypair;

    // Sign the CPID using the server's signing key
    use adapteros_crypto::signature::sign_bytes;
    let signature_bytes = sign_bytes(signing_key, cpid.as_bytes());
    let signature = format!("ed25519:{}", hex::encode(signature_bytes.to_bytes()));

    tracing::info!(
        cpid = %cpid,
        signed_by = %claims.email,
        "Policy signed using server signing key"
    );

    // Audit log: policy signed
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_SIGN,
        crate::audit_helper::resources::POLICY,
        Some(&cpid),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(SignPolicyResponse {
        cpid: cpid.clone(),
        signature,
        signed_at: chrono::Utc::now().to_rfc3339(),
        signed_by: claims.email,
    }))
}

/// Verify policy signature using server's public key (PRD-SEC-01)
#[utoipa::path(
    get,
    path = "/v1/policies/{cpid}/verify",
    params(
        ("cpid" = String, Path, description = "Policy CPID")
    ),
    responses(
        (status = 200, description = "Signature verification", body = VerifyPolicyResponse),
        (status = 404, description = "Policy not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn verify_policy_signature(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<VerifyPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Any authenticated user can verify signatures
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;
    let cpid = crate::id_resolver::resolve_any_id(&state.db, &cpid)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Fetch the policy pack to get its signature
    let policy_pack = state
        .db
        .get_policy_pack(&cpid)
        .await
        .map_err(|e| {
            tracing::error!(cpid = %cpid, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack not found").with_code("NOT_FOUND")),
            )
        })?;

    // Get the signature from the policy pack (if exists)
    let signature_str = policy_pack.signature.clone();

    // Extract the signature bytes (strip "ed25519:" prefix if present)
    let signature_hex = if let Some(stripped) = signature_str.strip_prefix("ed25519:") {
        stripped
    } else {
        &signature_str
    };

    // Parse the signature
    let signature_bytes = hex::decode(signature_hex).map_err(|e| {
        tracing::error!("Invalid signature hex: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid signature format")
                    .with_code("INVALID_SIGNATURE")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if signature_bytes.len() != 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid signature length")
                    .with_code("INVALID_SIGNATURE")
                    .with_string_details(format!(
                        "Expected 64 bytes, got {}",
                        signature_bytes.len()
                    )),
            ),
        ));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);

    // Get the server's public key
    use adapteros_crypto::signature::Signature;
    let public_key = state.ed25519_keypair.public_key();
    let public_key_hex = hex::encode(public_key.to_bytes());

    // Verify the signature
    let signature = Signature::from_bytes(&sig_array).map_err(|e| {
        tracing::error!("Failed to parse signature: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Failed to parse signature")
                    .with_code("INVALID_SIGNATURE")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let is_valid = match public_key.verify(cpid.as_bytes(), &signature) {
        Ok(()) => {
            tracing::info!(
                cpid = %cpid,
                verified_by = %claims.email,
                "Policy signature verified successfully"
            );
            true
        }
        Err(e) => {
            tracing::warn!(
                cpid = %cpid,
                verified_by = %claims.email,
                error = %e,
                "Policy signature verification failed"
            );
            false
        }
    };

    Ok(Json(VerifyPolicyResponse {
        cpid: cpid.clone(),
        signature: signature_str,
        is_valid,
        public_key: format!("ed25519:{}", public_key_hex),
        verified_at: chrono::Utc::now().to_rfc3339(),
        error: if is_valid {
            None
        } else {
            Some("Signature verification failed".to_string())
        },
    }))
}

/// Compare two policy versions
#[utoipa::path(
    post,
    path = "/v1/policies/compare",
    request_body = PolicyComparisonRequest,
    responses(
        (status = 200, description = "Policy comparison", body = PolicyComparisonResponse),
        (status = 404, description = "Policy pack not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn compare_policy_versions(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<PolicyComparisonRequest>,
) -> Result<Json<PolicyComparisonResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Fetch both policy packs
    let pack1 = state
        .db
        .get_policy_pack(&req.cpid_1)
        .await
        .map_err(|e| {
            tracing::error!(cpid = %req.cpid_1, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack 1 not found").with_code("NOT_FOUND")),
            )
        })?;

    let pack2 = state
        .db
        .get_policy_pack(&req.cpid_2)
        .await
        .map_err(|e| {
            tracing::error!(cpid = %req.cpid_2, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack 2 not found").with_code("NOT_FOUND")),
            )
        })?;

    // Parse JSON content
    let json1: serde_json::Value = serde_json::from_str(&pack1.content_json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Invalid JSON in policy pack 1")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let json2: serde_json::Value = serde_json::from_str(&pack2.content_json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Invalid JSON in policy pack 2")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Check if identical
    let identical = json1 == json2;

    // Compute differences (simple field-level comparison)
    let mut differences = Vec::new();
    if !identical {
        // Simple diff: compare as objects
        if let (Some(obj1), Some(obj2)) = (json1.as_object(), json2.as_object()) {
            // Find fields in obj1 not in obj2 or with different values
            for (key, val1) in obj1 {
                if let Some(val2) = obj2.get(key) {
                    if val1 != val2 {
                        differences.push(format!("{}: {} -> {}", key, val1, val2));
                    }
                } else {
                    differences.push(format!("Removed: {}", key));
                }
            }

            // Find fields in obj2 not in obj1
            for key in obj2.keys() {
                if !obj1.contains_key(key) {
                    differences.push(format!("Added: {}", key));
                }
            }
        } else {
            differences.push("Policies have different structures".to_string());
        }
    }

    Ok(Json(PolicyComparisonResponse {
        cpid_1: req.cpid_1,
        cpid_2: req.cpid_2,
        differences,
        identical,
    }))
}

/// Export policy as downloadable bundle
#[utoipa::path(
    get,
    path = "/v1/policies/{cpid}/export",
    params(
        ("cpid" = String, Path, description = "Policy CPID")
    ),
    responses(
        (status = 200, description = "Policy export", body = ExportPolicyResponse),
        (status = 404, description = "Policy pack not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn export_policy(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<ExportPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cpid = crate::id_resolver::resolve_any_id(&state.db, &cpid)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    // Fetch policy pack from database
    let pack = state
        .db
        .get_policy_pack(&cpid)
        .await
        .map_err(|e| {
            tracing::error!(cpid = %cpid, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(ExportPolicyResponse {
        cpid: pack.id,
        policy_json: pack.content_json,
        signature: Some(pack.signature),
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Assign a policy pack to a tenant or adapter (PRD-RBAC-01)
#[utoipa::path(
    post,
    path = "/v1/policies/assign",
    request_body = AssignPolicyRequest,
    responses(
        (status = 200, description = "Policy assigned successfully", body = PolicyAssignmentResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn assign_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<AssignPolicyRequest>,
) -> Result<Json<PolicyAssignmentResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (policy assignment is a critical operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyApply)?;

    // Validate policy pack exists
    let pack = state
        .db
        .get_policy_pack(&req.policy_pack_id)
        .await
        .map_err(|e| {
            tracing::error!(policy_pack_id = %req.policy_pack_id, error = %e, "Failed to get policy pack");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy pack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Policy pack not found").with_code("NOT_FOUND")),
            )
        })?;

    // Assign policy
    let id = state
        .db
        .assign_policy(
            &req.policy_pack_id,
            &req.target_type,
            req.target_id.as_deref(),
            &claims.email,
            req.priority,
            req.enforced,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to assign policy");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to assign policy")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Audit log: policy assigned
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_APPLY,
        crate::audit_helper::resources::POLICY,
        Some(&req.policy_pack_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(PolicyAssignmentResponse {
        id: id.clone(),
        policy_pack_id: req.policy_pack_id,
        target_type: req.target_type,
        target_id: req.target_id,
        priority: req.priority.unwrap_or(100),
        enforced: req.enforced.unwrap_or(true),
        assigned_at: chrono::Utc::now().to_rfc3339(),
        assigned_by: claims.email,
        expires_at: None,
    }))
}

/// List policy assignments (PRD-RBAC-01)
#[utoipa::path(
    get,
    path = "/v1/policies/assignments",
    params(
        ("target_type" = Option<String>, Query, description = "Filter by target type (tenant, adapter)"),
        ("target_id" = Option<String>, Query, description = "Filter by target ID")
    ),
    responses(
        (status = 200, description = "Policy assignments retrieved successfully", body = Vec<PolicyAssignmentResponse>),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn list_policy_assignments(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<PolicyAssignmentResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view policy assignments
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;

    // Filter by tenant if non-admin
    let is_admin = claims.roles.contains(&"admin".to_string());
    let target_type = params
        .get("target_type")
        .map(|s| s.as_str())
        .unwrap_or("tenant");
    let target_id = if is_admin {
        params.get("target_id").map(|s| s.as_str())
    } else {
        // Non-admin users can only see their own tenant's assignments
        Some(claims.tenant_id.as_str())
    };

    // Get assignments from database
    let assignments = state
        .db
        .get_policy_assignments(target_type, target_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to get policy assignments");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy assignments")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response = assignments
        .into_iter()
        .map(|a| PolicyAssignmentResponse {
            id: a.id,
            policy_pack_id: a.policy_pack_id,
            target_type: a.target_type,
            target_id: a.target_id,
            priority: a.priority,
            enforced: a.enforced,
            assigned_at: a.assigned_at,
            assigned_by: a.assigned_by,
            expires_at: a.expires_at,
        })
        .collect();

    Ok(Json(response))
}

/// List policy violations (PRD-RBAC-01)
#[utoipa::path(
    get,
    path = "/v1/policies/violations",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("severity" = Option<String>, Query, description = "Filter by severity (critical, high, medium, low)"),
        ("resolved" = Option<bool>, Query, description = "Filter by resolution status"),
        ("limit" = Option<i64>, Query, description = "Limit number of results (default: 100)")
    ),
    responses(
        (status = 200, description = "Policy violations retrieved successfully", body = Vec<PolicyViolationResponse>),
        (status = 403, description = "Insufficient permissions", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "policies"
)]
pub async fn list_violations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<PolicyViolationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view violations
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;

    // Filter by tenant if non-admin
    let is_admin = claims.roles.contains(&"admin".to_string());
    let tenant_id = if is_admin {
        params.get("tenant_id").map(|s| s.as_str())
    } else {
        // Non-admin users can only see their own tenant's violations
        Some(claims.tenant_id.as_str())
    };

    let resource_type = params.get("resource_type").map(|s| s.as_str());
    let severity = params.get("severity").map(|s| s.as_str());
    let resolved = params.get("resolved").and_then(|s| s.parse::<bool>().ok());
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100);

    // Get violations from database
    let violations = state
        .db
        .get_policy_violations(tenant_id, resource_type, severity, resolved, limit)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to get policy violations");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get policy violations")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response = violations
        .into_iter()
        .map(|v| PolicyViolationResponse {
            id: v.id,
            policy_pack_id: v.policy_pack_id,
            policy_assignment_id: v.policy_assignment_id,
            violation_type: v.violation_type,
            severity: v.severity,
            resource_type: v.resource_type,
            resource_id: v.resource_id,
            tenant_id: v.tenant_id,
            violation_message: v.violation_message,
            violation_details_json: v.violation_details_json,
            detected_at: v.detected_at,
            resolved_at: v.resolved_at,
            resolved_by: v.resolved_by,
            resolution_notes: v.resolution_notes,
        })
        .collect();

    Ok(Json(response))
}
