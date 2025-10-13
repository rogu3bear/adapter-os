//! Basic patch proposal example
//!
//! This example demonstrates the basic usage of the patch proposal system
//! for generating code patches with evidence citations and policy validation.

use mplora_kernel_mtl::MetalKernels;
use mplora_manifest::Policies;
use mplora_policy::PolicyEngine;
use mplora_worker::{
    evidence::{EvidenceRequest, EvidenceSpan, EvidenceType},
    patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator},
    patch_telemetry::{EvidenceMetrics, PatchTelemetry},
    patch_validator::{CodePolicy, PatchValidator},
    InferenceRequest, PatchProposalRequest, RequestType, Worker,
};
use std::collections::HashMap;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 AdapterOS Patch Proposal System - Basic Example");
    println!("==================================================");

    // 1. Create evidence spans for the patch proposal
    let evidence_spans = create_evidence_spans();
    println!("✅ Created {} evidence spans", evidence_spans.len());

    // 2. Generate patch proposal
    let proposal = generate_patch_proposal(evidence_spans).await?;
    println!("✅ Generated patch proposal: {}", proposal.proposal_id);
    println!("   Confidence: {:.3}", proposal.confidence);
    println!("   Files: {}", proposal.patches.len());
    println!("   Citations: {}", proposal.citations.len());

    // 3. Validate patch against policy
    let validation_result = validate_patch(&proposal.patches).await?;
    println!(
        "✅ Patch validation: {}",
        if validation_result.is_valid {
            "PASSED"
        } else {
            "FAILED"
        }
    );
    println!("   Confidence: {:.3}", validation_result.confidence);

    if !validation_result.errors.is_empty() {
        println!("   Errors:");
        for error in &validation_result.errors {
            println!("     - {}", error);
        }
    }

    if !validation_result.warnings.is_empty() {
        println!("   Warnings:");
        for warning in &validation_result.warnings {
            println!("     - {}", warning);
        }
    }

    // 4. Log telemetry
    let mut telemetry = PatchTelemetry::new();
    log_telemetry(&mut telemetry, &proposal).await;

    // 5. Display patch details
    display_patch_details(&proposal);

    println!("\n🎉 Patch proposal example completed successfully!");
    Ok(())
}

fn create_evidence_spans() -> Vec<EvidenceSpan> {
    vec![
        EvidenceSpan {
            doc_id: "auth_doc".to_string(),
            rev: "v1".to_string(),
            span_hash: "auth_hash".to_string(),
            score: 0.95,
            evidence_type: EvidenceType::Symbol,
            file_path: "src/middleware/auth.rs".to_string(),
            start_line: 15,
            end_line: 25,
            content: "pub struct AuthMiddleware { jwt_secret: String }".to_string(),
            metadata: HashMap::new(),
        },
        EvidenceSpan {
            doc_id: "test_auth".to_string(),
            rev: "v1".to_string(),
            span_hash: "test_auth_hash".to_string(),
            score: 0.88,
            evidence_type: EvidenceType::Test,
            file_path: "tests/middleware/auth_test.rs".to_string(),
            start_line: 30,
            end_line: 40,
            content: "#[test] fn test_auth_middleware()".to_string(),
            metadata: HashMap::new(),
        },
        EvidenceSpan {
            doc_id: "doc_auth".to_string(),
            rev: "v1".to_string(),
            span_hash: "doc_auth_hash".to_string(),
            score: 0.82,
            evidence_type: EvidenceType::Documentation,
            file_path: "docs/middleware.md".to_string(),
            start_line: 50,
            end_line: 60,
            content: "## Authentication Middleware".to_string(),
            metadata: HashMap::new(),
        },
    ]
}

async fn generate_patch_proposal(
    evidence_spans: Vec<EvidenceSpan>,
) -> Result<mplora_worker::patch_generator::PatchProposal, Box<dyn std::error::Error>> {
    let request = PatchGenerationRequest {
        repo_id: "auth_service".to_string(),
        commit_sha: Some("def456".to_string()),
        target_files: vec!["src/middleware/mod.rs".to_string()],
        description: "Add JWT authentication middleware with proper error handling and logging"
            .to_string(),
        evidence: evidence_spans,
        context: HashMap::new(),
    };

    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        mplora_worker::patch_generator::PatchParser::new(),
        mplora_worker::patch_generator::CitationExtractor::new(),
    );

    let proposal = generator.generate_patch(request).await?;
    Ok(proposal)
}

async fn validate_patch(
    patches: &[mplora_worker::patch_generator::FilePatch],
) -> Result<mplora_worker::patch_validator::ValidationResult, Box<dyn std::error::Error>> {
    let policy = CodePolicy::default();
    let policies = create_mock_policies();
    let policy_engine = PolicyEngine::new(policies);
    let validator = PatchValidator::new(policy, policy_engine);

    let validation_result = validator.validate(patches).await?;
    Ok(validation_result)
}

fn create_mock_policies() -> Policies {
    use mplora_core::B3Hash;
    use mplora_manifest::{
        ArtifactsPolicy, DeterminismPolicy, EgressPolicy, EvidencePolicy, IsolationPolicy,
        MemoryPolicy, NumericPolicy, PerformancePolicy, RagPolicy, RefusalPolicy,
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
            min_spans: 1,
            prefer_latest_revision: true,
            warn_on_superseded: true,
        },
        refusal: RefusalPolicy {
            abstain_threshold: 0.55,
            missing_fields_templates: std::collections::HashMap::new(),
        },
        numeric: NumericPolicy {
            canonical_units: std::collections::HashMap::new(),
            max_rounding_error: 0.5,
            require_units_in_trace: true,
        },
        rag: RagPolicy {
            index_scope: "per_tenant".to_string(),
            doc_tags_required: vec!["doc_id".to_string(), "rev".to_string()],
            embedding_model_hash: B3Hash::hash(b"test"),
            topk: 5,
            order: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
        },
        isolation: IsolationPolicy {
            process_model: "per_tenant".to_string(),
            uds_root: "/var/run/aos".to_string(),
            forbid_shm: true,
        },
        performance: PerformancePolicy {
            latency_p95_ms: 24,
            router_overhead_pct_max: 8,
            throughput_tokens_per_s_min: 40,
        },
        memory: MemoryPolicy {
            min_headroom_pct: 15u8,
            evict_order: vec!["ephemeral_ttl".to_string()],
            k_reduce_before_evict: true,
        },
        artifacts: ArtifactsPolicy {
            require_signature: true,
            require_sbom: true,
            cas_only: true,
        },
    }
}

async fn log_telemetry(
    telemetry: &mut PatchTelemetry,
    proposal: &mplora_worker::patch_generator::PatchProposal,
) {
    let evidence_metrics = EvidenceMetrics {
        query: "JWT authentication middleware".to_string(),
        sources_used: vec![
            "symbol".to_string(),
            "test".to_string(),
            "documentation".to_string(),
        ],
        spans_found: 3,
        retrieval_time_ms: 50,
        avg_relevance_score: 0.88,
        min_score_threshold: 0.7,
    };

    telemetry.log_evidence_retrieval(
        "example_tenant",
        evidence_metrics,
        Some(&proposal.proposal_id),
    );

    let generation_metrics = mplora_worker::patch_telemetry::PatchGenerationMetrics {
        proposal_id: proposal.proposal_id.clone(),
        description: proposal.rationale.clone(),
        target_files: vec!["src/middleware/mod.rs".to_string()],
        evidence_count: proposal.citations.len(),
        patch_count: proposal.patches.len(),
        total_lines: proposal.patches.iter().map(|p| p.total_lines).sum(),
        generation_time_ms: 1500,
        confidence_score: proposal.confidence,
    };

    telemetry.log_patch_generation("example_tenant", generation_metrics);
}

fn display_patch_details(proposal: &mplora_worker::patch_generator::PatchProposal) {
    println!("\n📋 Patch Proposal Details");
    println!("=========================");
    println!("Proposal ID: {}", proposal.proposal_id);
    println!("Rationale: {}", proposal.rationale);
    println!("Confidence: {:.3}", proposal.confidence);

    println!("\n📁 Files Modified:");
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

    println!("\n📚 Citations:");
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
}
