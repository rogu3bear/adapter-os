//! Advanced patch proposal example
//!
//! This example demonstrates advanced usage of the patch proposal system
//! including custom evidence retrieval, policy configuration, and telemetry integration.

use adapteros_lora_worker::{
    evidence::{EvidencePolicy, EvidenceRequest, EvidenceRetriever, EvidenceSpan, EvidenceType},
    patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator},
    patch_telemetry::{EvidenceMetrics, PatchGenerationMetrics, PatchTelemetry, ValidationMetrics},
    patch_validator::{CodePolicy, PatchValidator, ValidationResult},
};
use adapteros_manifest::Policies;
use adapteros_policy::PolicyEngine;
use adapteros_telemetry::TelemetryWriter;
use std::collections::HashMap;
#[cfg(feature = "extended-tests")]
use std::time::Instant;
#[cfg(feature = "extended-tests")]
use tokio;

#[cfg(not(feature = "extended-tests"))]
fn main() {
    eprintln!("Enable the `extended-tests` feature to run the advanced patch proposal example.");
}

#[cfg(feature = "extended-tests")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("[ROCKET] AdapterOS Patch Proposal System - Advanced Example");
    println!("====================================================");

    // 1. Set up telemetry with writer
    println!("[CHART] Setting up telemetry...");
    let telemetry_writer = TelemetryWriter::new("/tmp/telemetry", 1000, 1024 * 1024)?;
    let mut telemetry = PatchTelemetry::new_with_writer(telemetry_writer);
    println!("[CHECK] Telemetry configured");

    // 2. Configure custom evidence policy
    println!("[SEARCH] Configuring evidence policy...");
    let evidence_policy = EvidencePolicy {
        min_spans: 3,
        min_sources: 2,
        min_avg_score: 0.8,
        max_retrieval_time_ms: 200,
    };
    println!(
        "[CHECK] Evidence policy configured: min_spans={}, min_sources={}, min_avg_score={:.1}",
        evidence_policy.min_spans, evidence_policy.min_sources, evidence_policy.min_avg_score
    );

    // 3. Create comprehensive evidence spans
    println!("[BOOK] Creating comprehensive evidence spans...");
    let evidence_spans = create_comprehensive_evidence();
    println!("[CHECK] Created {} evidence spans", evidence_spans.len());

    // 4. Validate evidence quality
    println!("[SEARCH] Validating evidence quality...");
    let evidence_result = validate_evidence_quality(&evidence_spans, &evidence_policy)?;
    println!(
        "[CHECK] Evidence validation: {}",
        if evidence_result.is_valid {
            "PASSED"
        } else {
            "FAILED"
        }
    );
    if !evidence_result.is_valid {
        for error in evidence_result.errors {
            println!("   [CROSS] {}", error);
        }
    }

    // 5. Generate patch proposal with performance monitoring
    println!("[LIGHTNING] Generating patch proposal with performance monitoring...");
    let start_time = Instant::now();
    let proposal = generate_patch_proposal_advanced(evidence_spans).await?;
    let generation_time = start_time.elapsed();
    println!("[CHECK] Patch proposal generated in {:?}", generation_time);
    println!("   Proposal ID: {}", proposal.proposal_id);
    println!("   Confidence: {:.3}", proposal.confidence);
    println!("   Files: {}", proposal.patches.len());
    println!("   Citations: {}", proposal.citations.len());

    // 6. Log evidence retrieval telemetry
    println!("[CHART] Logging evidence retrieval telemetry...");
    let evidence_metrics = EvidenceMetrics {
        query: "Advanced authentication middleware".to_string(),
        sources_used: vec![
            "symbol".to_string(),
            "test".to_string(),
            "documentation".to_string(),
            "framework".to_string(),
        ],
        spans_found: 4,
        retrieval_time_ms: 75,
        avg_relevance_score: 0.89,
        min_score_threshold: 0.8,
    };
    telemetry.log_evidence_retrieval(
        "advanced_tenant",
        evidence_metrics,
        Some(&proposal.proposal_id),
    );
    println!("[CHECK] Evidence telemetry logged");

    // 7. Log patch generation telemetry
    println!("[CHART] Logging patch generation telemetry...");
    let generation_metrics = PatchGenerationMetrics {
        proposal_id: proposal.proposal_id.clone(),
        description: proposal.rationale.clone(),
        target_files: vec!["src/middleware/mod.rs".to_string()],
        evidence_count: proposal.citations.len(),
        patch_count: proposal.patches.len(),
        total_lines: proposal.patches.iter().map(|p| p.total_lines).sum(),
        generation_time_ms: generation_time.as_millis() as u64,
        confidence_score: proposal.confidence,
    };
    telemetry.log_patch_generation("advanced_tenant", generation_metrics);
    println!("[CHECK] Generation telemetry logged");

    // 8. Advanced policy validation
    println!("[SHIELD]  Performing advanced policy validation...");
    let validation_result = validate_patch_advanced(&proposal.patches).await?;
    println!(
        "[CHECK] Advanced validation: {}",
        if validation_result.is_valid {
            "PASSED"
        } else {
            "FAILED"
        }
    );
    println!("   Confidence: {:.3}", validation_result.confidence);
    println!("   Errors: {}", validation_result.errors.len());
    println!("   Warnings: {}", validation_result.warnings.len());
    println!("   Violations: {}", validation_result.violations.len());

    // 9. Log validation telemetry
    println!("[CHART] Logging validation telemetry...");
    let validation_metrics = ValidationMetrics {
        proposal_id: proposal.proposal_id.clone(),
        is_valid: validation_result.is_valid,
        error_count: validation_result.errors.len(),
        warning_count: validation_result.warnings.len(),
        violation_count: validation_result.violations.len(),
        validation_time_ms: 25,
        confidence_score: validation_result.confidence,
        violations: validation_result
            .violations
            .into_iter()
            .map(
                |v| adapteros_lora_worker::patch_telemetry::ViolationMetric {
                    violation_type: format!("{:?}", v.violation_type),
                    severity: format!("{:?}", v.severity),
                    file_path: v.file_path,
                    line_number: v.line_number,
                    description: v.description,
                },
            )
            .collect(),
    };
    telemetry.log_patch_validation("advanced_tenant", validation_metrics);
    println!("[CHECK] Validation telemetry logged");

    // 10. Performance analysis
    println!("[TRENDING-UP] Performance analysis...");
    let performance_metrics = telemetry.get_performance_metrics();
    for (metric_name, value) in performance_metrics {
        println!("   {}: {:.2}", metric_name, value);
    }

    // 11. Security analysis
    println!("[LOCK] Security analysis...");
    let security_events = telemetry
        .get_events()
        .iter()
        .filter(|e| {
            matches!(
                e.event_type,
                adapteros_lora_worker::patch_telemetry::PatchEventType::SecurityViolation
            )
        })
        .count();
    println!("   Security violations detected: {}", security_events);

    // 12. Display comprehensive patch details
    display_advanced_patch_details(&proposal, &validation_result);

    // 13. Export telemetry data
    println!("[EXPORT] Exporting telemetry data...");
    let events = telemetry.get_events();
    println!("   Total events logged: {}", events.len());
    println!(
        "   Event types: {:?}",
        events.iter().map(|e| &e.event_type).collect::<Vec<_>>()
    );

    println!("\n[SUCCESS] Advanced patch proposal example completed successfully!");
    Ok(())
}

fn create_comprehensive_evidence() -> Vec<EvidenceSpan> {
    vec![
        // Symbol evidence
        EvidenceSpan {
            doc_id: "auth_symbol".to_string(),
            rev: "v1".to_string(),
            span_hash: "auth_symbol_hash".to_string(),
            score: 0.95,
            evidence_type: EvidenceType::Symbol,
            file_path: "src/middleware/auth.rs".to_string(),
            start_line: 15,
            end_line: 25,
            content: "pub struct AuthMiddleware { jwt_secret: String, rate_limiter: RateLimiter }".to_string(),
            metadata: HashMap::new(),
        },
        // Test evidence
        EvidenceSpan {
            doc_id: "auth_test".to_string(),
            rev: "v1".to_string(),
            span_hash: "auth_test_hash".to_string(),
            score: 0.92,
            evidence_type: EvidenceType::Test,
            file_path: "tests/middleware/auth_test.rs".to_string(),
            start_line: 30,
            end_line: 40,
            content: "#[test] fn test_auth_middleware_rate_limiting()".to_string(),
            metadata: HashMap::new(),
        },
        // Documentation evidence
        EvidenceSpan {
            doc_id: "auth_doc".to_string(),
            rev: "v1".to_string(),
            span_hash: "auth_doc_hash".to_string(),
            score: 0.88,
            evidence_type: EvidenceType::Doc,
            file_path: "docs/middleware.md".to_string(),
            start_line: 50,
            end_line: 60,
            content: "## Authentication Middleware with Rate Limiting".to_string(),
            metadata: HashMap::new(),
        },
        // Framework evidence
        EvidenceSpan {
            doc_id: "auth_framework".to_string(),
            rev: "v1".to_string(),
            span_hash: "auth_framework_hash".to_string(),
            score: 0.85,
            evidence_type: EvidenceType::Framework,
            file_path: "src/framework/middleware.rs".to_string(),
            start_line: 70,
            end_line: 80,
            content: "impl Middleware for AuthMiddleware { fn handle(&self, req: &mut Request) -> Result<Response, Error> }".to_string(),
            metadata: HashMap::new(),
        },
    ]
}

fn validate_evidence_quality(
    evidence_spans: &[EvidenceSpan],
    policy: &EvidencePolicy,
) -> Result<ValidationResult, Box<dyn std::error::Error>> {
    let mut errors = Vec::new();

    // Check minimum spans
    if evidence_spans.len() < policy.min_spans {
        errors.push(format!(
            "Insufficient evidence: {} spans < {} required",
            evidence_spans.len(),
            policy.min_spans
        ));
    }

    // Check source diversity
    let unique_sources: std::collections::HashSet<_> =
        evidence_spans.iter().map(|s| &s.evidence_type).collect();
    if unique_sources.len() < policy.min_sources {
        errors.push(format!(
            "Insufficient source diversity: {} sources < {} required",
            unique_sources.len(),
            policy.min_sources
        ));
    }

    // Check average score
    let avg_score =
        evidence_spans.iter().map(|s| s.score).sum::<f32>() / evidence_spans.len() as f32;
    if avg_score < policy.min_avg_score {
        errors.push(format!(
            "Insufficient relevance: {:.3} < {:.3} required",
            avg_score, policy.min_avg_score
        ));
    }

    Ok(ValidationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings: Vec::new(),
        confidence: if errors.is_empty() { 0.9 } else { 0.3 },
        violations: Vec::new(),
        evidence_validation: None,
        security_validation: None,
        performance_validation: None,
        test_validation: None,
        lint_validation: None,
        policy_compliance: None,
        validation_duration_ms: 0,
        telemetry_hash: None,
    })
}

async fn generate_patch_proposal_advanced(
    evidence_spans: Vec<EvidenceSpan>,
) -> Result<adapteros_lora_worker::patch_generator::PatchProposal, Box<dyn std::error::Error>> {
    let request = PatchGenerationRequest {
        repo_id: "advanced_auth_service".to_string(),
        commit_sha: Some("advanced123".to_string()),
        target_files: vec!["src/middleware/mod.rs".to_string()],
        description: "Add comprehensive JWT authentication middleware with rate limiting, error handling, logging, and security headers".to_string(),
        evidence: evidence_spans,
        context: HashMap::new(),
    };

    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    let proposal = generator.generate_patch(request).await?;
    Ok(proposal)
}

async fn validate_patch_advanced(
    patches: &[adapteros_lora_worker::patch_generator::FilePatch],
) -> Result<adapteros_lora_worker::patch_validator::ValidationResult, Box<dyn std::error::Error>> {
    let mut policy = CodePolicy::default();

    // Customize policy for advanced validation
    policy.max_patch_size_lines = 50;
    policy.allow_external_deps = false;
    policy.path_allowlist = vec!["src/**".to_string(), "tests/**".to_string()];
    policy.path_denylist = vec![".env".to_string(), "*.key".to_string()];

    let policies = create_advanced_policies();
    let policy_engine = PolicyEngine::new(policies);
    let validator = PatchValidator::new(policy, policy_engine);

    let validation_result = validator.validate(patches).await?;
    Ok(validation_result)
}

fn create_advanced_policies() -> Policies {
    use adapteros_core::B3Hash;
    use adapteros_manifest::{
<<<<<<< HEAD
        ArtifactsPolicy, DeterminismPolicy, DriftPolicy, EgressPolicy, EvidencePolicy,
        IsolationPolicy, MemoryPolicy, NumericPolicy, PerformancePolicy, RagPolicy, RefusalPolicy,
=======
        ArtifactsPolicy, DeterminismPolicy, EgressPolicy, EvidencePolicy, IsolationPolicy,
        MemoryPolicy, NumericPolicy, PerformancePolicy, RagPolicy, RefusalPolicy,
>>>>>>> integration-branch
    };

    Policies {
        egress: EgressPolicy {
            mode: "deny_all".to_string(),
            serve_requires_pf: true,
            allow_tcp: false,
            allow_udp: false,
            uds_paths: vec!["/var/run/aos/*.sock".to_string()],
        },
        determinism: DeterminismPolicy {
            require_metallib_embed: true,
            require_kernel_hash_match: true,
            rng: "hkdf_seeded".to_string(),
            retrieval_tie_break: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
        },
        evidence: EvidencePolicy {
            require_open_book: true,
            min_spans: 3,
            prefer_latest_revision: true,
            warn_on_superseded: true,
        },
        refusal: RefusalPolicy {
            abstain_threshold: 0.7,
            missing_fields_templates: std::collections::HashMap::new(),
        },
        numeric: NumericPolicy {
            canonical_units: std::collections::HashMap::new(),
            max_rounding_error: 0.1,
            require_units_in_trace: true,
        },
        rag: RagPolicy {
            index_scope: "per_tenant".to_string(),
            doc_tags_required: vec!["doc_id".to_string(), "rev".to_string()],
            embedding_model_hash: B3Hash::hash(b"advanced"),
            topk: 10,
            order: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
        },
        isolation: IsolationPolicy {
            process_model: "per_tenant".to_string(),
            uds_root: "/var/run/aos".to_string(),
            forbid_shm: true,
        },
        performance: PerformancePolicy {
            latency_p95_ms: 20,
            router_overhead_pct_max: 5,
            throughput_tokens_per_s_min: 50,
        },
        memory: MemoryPolicy {
            min_headroom_pct: 20u8,
            evict_order: vec!["ephemeral_ttl".to_string()],
            k_reduce_before_evict: true,
        },
        artifacts: ArtifactsPolicy {
            require_signature: true,
            require_sbom: true,
            cas_only: true,
        },
        drift: DriftPolicy {
            os_build_tolerance: 1,
            gpu_driver_tolerance: 0,
            env_hash_tolerance: 0,
            allow_warnings: true,
            block_on_critical: true,
        },
    }
}

fn display_advanced_patch_details(
    proposal: &adapteros_lora_worker::patch_generator::PatchProposal,
    validation_result: &ValidationResult,
) {
    println!("\n[CLIPBOARD] Advanced Patch Proposal Details");
    println!("===================================");
    println!("Proposal ID: {}", proposal.proposal_id);
    println!("Rationale: {}", proposal.rationale);
    println!("Confidence: {:.3}", proposal.confidence);
    println!(
        "Validation: {}",
        if validation_result.is_valid {
            "PASSED"
        } else {
            "FAILED"
        }
    );
    println!("Validation Confidence: {:.3}", validation_result.confidence);

    println!("\n[FILE] Files Modified:");
    for (i, patch) in proposal.patches.iter().enumerate() {
        println!(
            "  {}. {} ({} lines)",
            i + 1,
            patch.file_path,
            patch.total_lines
        );
        for (j, hunk) in patch.hunks.iter().enumerate() {
            println!(
                "     Hunk {}: lines {}-{}",
                j + 1,
                hunk.start_line,
                hunk.end_line
            );
            for line in &hunk.modified_lines {
                println!("       + {}", line);
            }
        }
    }

    println!("\n[BOOK] Citations (sorted by relevance):");
    for (i, citation) in proposal.citations.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.3})",
            i + 1,
            citation.rationale,
            citation.relevance_score
        );
        println!("     File: {}", citation.file_path);
        println!(
            "     Lines: {}-{}",
            citation.line_range.0, citation.line_range.1
        );
    }

    if !validation_result.errors.is_empty() {
        println!("\n[CROSS] Validation Errors:");
        for error in &validation_result.errors {
            println!("   - {}", error);
        }
    }

    if !validation_result.warnings.is_empty() {
        println!("\n[WARNING]  Validation Warnings:");
        for warning in &validation_result.warnings {
            println!("   - {}", warning);
        }
    }

    if !validation_result.violations.is_empty() {
        println!("\n[ALERT] Policy Violations:");
        for violation in &validation_result.violations {
            println!(
                "   - {:?}: {}",
                violation.violation_type, violation.description
            );
            if let Some(file_path) = &violation.file_path {
                println!("     File: {}", file_path);
            }
            if let Some(line_number) = violation.line_number {
                println!("     Line: {}", line_number);
            }
        }
    }
}
