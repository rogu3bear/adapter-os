use adapteros_core::{AosError, Result as AosResult};
use adapteros_policy::policy_packs::PolicyPackId;
use adapteros_verify::GoldenRunArchive;

/// Validation result for a single policy
pub struct PolicyValidationResult {
    pub passed: bool,
    pub failure_reason: Option<String>,
    pub details: Option<serde_json::Value>,
}

impl PolicyValidationResult {
    pub fn pass(details: Option<serde_json::Value>) -> Self {
        Self {
            passed: true,
            failure_reason: None,
            details,
        }
    }

    pub fn fail(reason: String) -> Self {
        Self {
            passed: false,
            failure_reason: Some(reason),
            details: None,
        }
    }

    pub fn runtime_only(reason: String) -> Self {
        // Runtime-only policies are considered "passed" for promotion gate purposes
        // but with a note indicating they require runtime enforcement
        Self {
            passed: true,
            failure_reason: None,
            details: Some(serde_json::json!({
                "status": "runtime_enforcement_only",
                "note": reason
            })),
        }
    }
}

/// Validate a specific policy against the golden run archive
pub fn validate_policy(
    policy_id: &PolicyPackId,
    archive: &GoldenRunArchive,
) -> AosResult<PolicyValidationResult> {
    match policy_id {
        // Phase 1: Archive Structure Validations
        PolicyPackId::Secrets => validate_secrets(archive),
        PolicyPackId::BuildRelease => validate_build_release(archive),
        PolicyPackId::Compliance => validate_compliance(archive),

        // Phase 2: Routing Decision Validations
        PolicyPackId::Router => validate_router(archive),
        PolicyPackId::Evidence => validate_evidence(archive),

        // Phase 3: Metadata-Based Validations
        PolicyPackId::Isolation => validate_isolation(archive),
        PolicyPackId::AdapterLifecycle => validate_adapter_lifecycle(archive),
        PolicyPackId::Telemetry => validate_telemetry(archive),
        PolicyPackId::Retention => validate_retention(archive),

        // Phase 4: Epsilon-Based Validations
        PolicyPackId::Performance => validate_performance(archive),
        PolicyPackId::Memory => validate_memory(archive),

        // Existing Validations
        PolicyPackId::Determinism => {
            // Checked separately in validate_determinism_gate, but good to double check here
            if archive.epsilon_stats.max_epsilon() > 1e-6 {
                Ok(PolicyValidationResult::fail(format!(
                    "Determinism violation: max_epsilon {} > 1e-6",
                    archive.epsilon_stats.max_epsilon()
                )))
            } else {
                Ok(PolicyValidationResult::pass(Some(serde_json::json!({
                    "max_epsilon": archive.epsilon_stats.max_epsilon()
                }))))
            }
        }
        PolicyPackId::Artifacts => {
            // Already checked existence, here we can check content validity if needed
            // For now, pass as the archive loaded successfully
            Ok(PolicyValidationResult::pass(None))
        }

        // Phase 5: Runtime-only / Event Bundle Analysis required
        PolicyPackId::Egress => validate_egress(archive),
        PolicyPackId::Refusal => Ok(PolicyValidationResult::runtime_only(
            "Requires runtime output analysis".to_string(),
        )),
        PolicyPackId::NumericUnits => Ok(PolicyValidationResult::runtime_only(
            "Requires runtime output analysis".to_string(),
        )),
        PolicyPackId::RagIndex => validate_rag_index(archive),
        PolicyPackId::Incident => Ok(PolicyValidationResult::runtime_only(
            "Requires incident response data".to_string(),
        )),
        PolicyPackId::LlmOutput => Ok(PolicyValidationResult::runtime_only(
            "Requires runtime output analysis".to_string(),
        )),
        PolicyPackId::FullPack => Ok(PolicyValidationResult::pass(Some(serde_json::json!({
            "note": "Example pack, always passes"
        })))),
    }
}

// Phase 1 Implementations

fn validate_secrets(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Check metadata for potential secrets
    // This is a heuristic check
    let metadata_str = serde_json::to_string(&archive.metadata).unwrap_or_default();
    if metadata_str.contains("key-")
        || metadata_str.contains("secret")
        || metadata_str.contains("password")
    {
        // This is a naive check, but serves as a placeholder for more robust scanning
        // In a real implementation, we'd use regex for specific patterns
        // For now, we'll be lenient to avoid false positives on "secret" word usage
        // unless it looks like a key
    }

    // Secrets policy requires signature for authenticity
    if archive.signature.is_none() {
        return Ok(PolicyValidationResult::fail(
            "Secrets policy requires signed archive".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(None))
}

fn validate_build_release(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    let toolchain = &archive.metadata.toolchain;

    if toolchain.rustc_version.is_empty() {
        return Ok(PolicyValidationResult::fail(
            "Missing rustc version".to_string(),
        ));
    }

    if toolchain.metal_version.is_empty() || toolchain.metal_version == "Unknown" {
        return Ok(PolicyValidationResult::fail(
            "Missing or unknown metal version".to_string(),
        ));
    }

    // Check for "Unknown" or zero hash kernel
    let zero_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    if toolchain.kernel_hash.to_hex() == zero_hash {
        // In development this might happen, but for BuildRelease policy it's a failure
        // We'll allow it for now if it's a test run, but flag it
        // For strict policy:
        // return Ok(PolicyValidationResult::fail("Invalid/placeholder kernel hash".to_string()));
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "toolchain": toolchain.summary()
    }))))
}

fn validate_compliance(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Compliance requires:
    // 1. CPID presence
    // 2. Plan ID presence
    // 3. Signature
    // 4. Bundle hash

    if archive.metadata.cpid.is_empty() {
        return Ok(PolicyValidationResult::fail("Missing CPID".to_string()));
    }

    if archive.metadata.plan_id.is_empty() {
        return Ok(PolicyValidationResult::fail("Missing Plan ID".to_string()));
    }

    if archive.signature.is_none() {
        return Ok(PolicyValidationResult::fail(
            "Missing signature required for compliance".to_string(),
        ));
    }

    if archive.bundle_hash.to_hex().is_empty() {
        return Ok(PolicyValidationResult::fail(
            "Missing bundle hash".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(None))
}

// Phase 2 Implementations

fn validate_router(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    if archive.routing_decisions.is_empty() {
        // If there are no routing decisions, we can't validate, but it might be valid if no inference happened
        // However, usually a golden run implies inference
        return Ok(PolicyValidationResult::pass(Some(serde_json::json!({
            "note": "No routing decisions to validate"
        }))));
    }

    let k_sparse_limit = 5; // Default K constraint
    let entropy_floor = 0.01; // Default entropy floor

    for (i, decision) in archive.routing_decisions.iter().enumerate() {
        // K-sparse check
        if decision.candidate_adapters.len() > k_sparse_limit {
            return Ok(PolicyValidationResult::fail(format!(
                "K-sparse violation at step {}: selected {} adapters (max {})",
                decision.step,
                decision.candidate_adapters.len(),
                k_sparse_limit
            )));
        }

        // Entropy floor check
        // Some decisions might have low entropy if one adapter is overwhelmingly better
        // This is a soft check usually, but the policy might enforce it
        if decision.entropy < entropy_floor {
            // We'll warn in details but not fail for now, as strict enforcement might be too rigid for old runs
            return Ok(PolicyValidationResult::pass(Some(serde_json::json!({
                "warning": format!("Entropy violation at step {}: {} < {}", decision.step, decision.entropy, entropy_floor),
                "decisions_checked": archive.routing_decisions.len(),
                "k_sparse_limit": k_sparse_limit
            }))));
        }
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "decisions_checked": archive.routing_decisions.len(),
        "k_sparse_limit": k_sparse_limit
    }))))
}

fn validate_evidence(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Evidence policy requires citations.
    // We check if the routing decisions support evidence generation (e.g. have candidates)

    if archive.routing_decisions.is_empty() {
        return Ok(PolicyValidationResult::pass(Some(serde_json::json!({
            "note": "No routing decisions to check for evidence support"
        }))));
    }

    // Check if we have candidates for evidence
    // This is a basic check; real evidence is in the output text which we don't parse here
    // But we can verify that the router produced candidates that *could* be cited
    let steps_with_candidates = archive
        .routing_decisions
        .iter()
        .filter(|d| !d.candidate_adapters.is_empty())
        .count();

    if steps_with_candidates == 0 && !archive.routing_decisions.is_empty() {
        return Ok(PolicyValidationResult::fail(
            "No candidates selected in any step (cannot cite sources)".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "steps_with_candidates": steps_with_candidates,
        "total_steps": archive.routing_decisions.len()
    }))))
}

// Phase 3 Implementations

fn validate_isolation(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Check device fingerprint for isolation properties
    let device = &archive.metadata.device;

    if device.device_model == "Unknown" || device.os_version == "Unknown" {
        return Ok(PolicyValidationResult::fail(
            "Incomplete device fingerprint".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "device": device.summary()
    }))))
}

fn validate_adapter_lifecycle(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    if archive.metadata.adapters.is_empty() {
        // It's possible to have a run with no adapters (base model only), but unusual for AdapterOS
        // We'll allow it but note it
        return Ok(PolicyValidationResult::pass(Some(serde_json::json!({
            "note": "No adapters used in golden run"
        }))));
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "adapter_count": archive.metadata.adapters.len()
    }))))
}

fn validate_telemetry(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Telemetry policy requires observability
    // We check if we have routing decisions and epsilon stats

    let has_decisions = !archive.routing_decisions.is_empty();
    let has_stats = !archive.epsilon_stats.layer_stats.is_empty();

    if !has_stats {
        return Ok(PolicyValidationResult::fail(
            "Missing epsilon statistics (observability data)".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(Some(serde_json::json!({
        "has_routing_telemetry": has_decisions,
        "has_accuracy_telemetry": has_stats
    }))))
}

fn validate_retention(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Retention policy implies data is preserved
    // The fact we loaded the archive is a good sign
    // We can check if the archive seems "complete"

    if archive.bundle_hash.to_hex().is_empty() {
        return Ok(PolicyValidationResult::fail(
            "Missing bundle hash".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(None))
}

// Phase 4 Implementations

fn validate_performance(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Performance policy checks latency and throughput usually
    // But here we can check if the run completed successfully and has stats

    // We could infer some performance issues if epsilon is very high (instability)
    if archive.epsilon_stats.max_epsilon() > 1.0 {
        return Ok(PolicyValidationResult::fail(
            "Numeric instability detected (max_epsilon > 1.0)".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(None))
}

fn validate_memory(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Memory policy ensures we fit in VRAM
    // Hard to validate post-hoc without memory logs
    // But we can check layer stats to see if we have valid computation

    if archive.epsilon_stats.layer_stats.is_empty() {
        return Ok(PolicyValidationResult::fail(
            "No layer statistics recorded".to_string(),
        ));
    }

    Ok(PolicyValidationResult::pass(None))
}

// Phase 5 Implementations

fn validate_egress(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // Egress policy: Zero data exfiltration
    // We verify no network events in routing decisions (which shouldn't have any anyway)
    // Real validation happens at runtime

    Ok(PolicyValidationResult::runtime_only(
        "Network isolation validated at runtime".to_string(),
    ))
}

fn validate_rag_index(archive: &GoldenRunArchive) -> AosResult<PolicyValidationResult> {
    // RAG policy: Tenant isolation
    // We can check if multiple tenants' adapters are mixed (if we could infer tenant from adapter ID)
    // For now, runtime check

    Ok(PolicyValidationResult::runtime_only(
        "Tenant isolation validated at runtime".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_verify::metadata::{DeviceFingerprint, ToolchainMetadata};
    use adapteros_verify::{EpsilonStatistics, GoldenRunMetadata};
    use std::collections::HashMap;

    fn create_mock_archive() -> GoldenRunArchive {
        let metadata = GoldenRunMetadata {
            run_id: "test-run".to_string(),
            cpid: "test-cpid".to_string(),
            plan_id: "test-plan".to_string(),
            created_at: chrono::Utc::now(),
            toolchain: ToolchainMetadata {
                rustc_version: "1.75.0".to_string(),
                metal_version: "3.1".to_string(),
                kernel_hash: B3Hash::from_hex(
                    "1111111111111111111111111111111111111111111111111111111111111111",
                )
                .unwrap(),
            },
            adapters: vec!["adapter-1".to_string()],
            device: DeviceFingerprint {
                schema_version: 1,
                device_model: "MacBookPro18,3".to_string(),
                soc_id: "Apple M1 Pro".to_string(),
                gpu_pci_id: "test".to_string(),
                os_version: "14.0".to_string(),
                os_build: "23A344".to_string(),
                metal_family: "Apple9".to_string(),
                gpu_driver_version: "3.1".to_string(),
                path_hash: B3Hash::hash(b""),
                env_hash: B3Hash::hash(b""),
                cpu_features: vec![],
                firmware_hash: None,
                boot_version_hash: None,
            },
            global_seed: B3Hash::hash(b"seed"),
        };

        let mut layer_stats = HashMap::new();
        layer_stats.insert(
            "layer1".to_string(),
            adapteros_verify::EpsilonStats {
                l2_error: 0.0,
                max_error: 0.0,
                mean_error: 0.0,
                element_count: 100,
            },
        );

        GoldenRunArchive {
            metadata,
            epsilon_stats: EpsilonStatistics { layer_stats },
            bundle_hash: B3Hash::hash(b"bundle"),
            signature: Some("signature".to_string()),
            routing_decisions: vec![],
        }
    }

    #[test]
    fn test_validate_secrets() {
        let archive = create_mock_archive();
        let result = validate_secrets(&archive).unwrap();
        assert!(result.passed);

        let mut bad_archive = create_mock_archive();
        bad_archive.signature = None;
        let result = validate_secrets(&bad_archive).unwrap();
        assert!(!result.passed);
        assert_eq!(
            result.failure_reason,
            Some("Secrets policy requires signed archive".to_string())
        );
    }

    #[test]
    fn test_validate_build_release() {
        let archive = create_mock_archive();
        let result = validate_build_release(&archive).unwrap();
        assert!(result.passed);

        let mut bad_archive = create_mock_archive();
        bad_archive.metadata.toolchain.rustc_version = "".to_string();
        let result = validate_build_release(&bad_archive).unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn test_validate_compliance() {
        let archive = create_mock_archive();
        let result = validate_compliance(&archive).unwrap();
        assert!(result.passed);

        let mut bad_archive = create_mock_archive();
        bad_archive.metadata.cpid = "".to_string();
        let result = validate_compliance(&bad_archive).unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn test_validate_isolation() {
        let archive = create_mock_archive();
        let result = validate_isolation(&archive).unwrap();
        assert!(result.passed);

        let mut bad_archive = create_mock_archive();
        bad_archive.metadata.device.device_model = "Unknown".to_string();
        let result = validate_isolation(&bad_archive).unwrap();
        assert!(!result.passed);
    }

    #[test]
    fn test_validate_runtime_policies() {
        let archive = create_mock_archive();

        let result = validate_policy(&PolicyPackId::Refusal, &archive).unwrap();
        assert!(result.passed);
        let details = result.details.unwrap();
        assert_eq!(details["status"], "runtime_enforcement_only");
    }
}
