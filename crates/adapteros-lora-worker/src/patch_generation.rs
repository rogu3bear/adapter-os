//! Patch generation module
//!
//! Contains Worker methods for patch generation including:
//! - propose_patch
//! - retrieve_evidence
//! - apply_routing_policy_to_decision
//! - build_trace

use crate::{
    filter_decision_by_policy, fusion_intervals_for_mode, summarize_router_usage, CitationResponse,
    EvidenceRef, FilePatchResponse, InferenceRequest, InferenceResponse, PatchHunkResponse,
    PatchProposalRequest, PatchProposalResponse, ResponseTrace, Worker,
};
use adapteros_api_types::inference::RouterModelType;
use adapteros_core::{AosError, FusionInterval, Result};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_policy::{PolicyEngine, RefusalResponse};
use tracing::info;

/// Worker methods for patch generation
impl<K: FusedKernels + crate::StrictnessControl + Send + Sync + 'static> Worker<K> {
    /// Apply routing policy filters to a router Decision deterministically.
    ///
    /// - Preserves original decision order; only drops entries.
    /// - Does not renormalize gates to keep kernel inputs deterministic.
    /// - If all candidates are removed, returns a policy violation error.
    pub(crate) fn apply_routing_policy_to_decision(
        &self,
        decision: adapteros_lora_router::Decision,
        policy: Option<&adapteros_api_types::RoutingPolicy>,
        base_only_request: bool,
    ) -> Result<adapteros_lora_router::Decision> {
        if base_only_request {
            return Ok(decision);
        }

        let adapter_ids: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let adapter_clusters: Vec<Option<String>> = self
            .manifest
            .adapters
            .iter()
            .map(|a| {
                a.intent
                    .clone()
                    .filter(|s| !s.is_empty())
                    .or_else(|| a.id.split(['-', '_', '.']).next().map(|s| s.to_string()))
            })
            .collect();

        filter_decision_by_policy(decision, &adapter_ids, &adapter_clusters, policy)
    }

    /// Build response trace with evidence and router summary
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_trace(
        &self,
        cpid: &str,
        evidence: &[EvidenceRef],
        token_count: usize,
        router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
        router_decision_chain: Option<
            Vec<adapteros_api_types::inference::RouterDecisionChainEntry>,
        >,
        fusion_interval: FusionInterval,
        active_ids: &[String],
        base_only_request: bool,
    ) -> ResponseTrace {
        let active_pool: Vec<String> = if active_ids.is_empty() {
            self.manifest
                .adapters
                .iter()
                .map(|a| a.id.clone())
                .collect()
        } else {
            active_ids.to_vec()
        };

        let router_summary = summarize_router_usage(
            base_only_request,
            &active_pool,
            self.manifest.router.k_sparse,
            router_decisions.as_deref(),
        );

        let fusion_intervals = fusion_intervals_for_mode(
            fusion_interval,
            router_decisions.as_deref(),
            &self.manifest.base.model_hash,
        );

        let model_type = Some(RouterModelType::Dense);

        ResponseTrace {
            cpid: cpid.to_string(),
            plan_id: self.generate_plan_id(cpid),
            evidence: evidence.to_vec(),
            router_summary,
            token_count,
            router_decisions,
            router_decision_chain,
            fusion_intervals,
            model_type,
        }
    }

    /// Retrieve evidence for patch proposal using real EvidenceRetriever
    async fn retrieve_evidence(
        &mut self,
        request: &crate::evidence::EvidenceRequest,
    ) -> Result<crate::evidence::EvidenceResult> {
        use crate::evidence::{EvidenceResult, EvidenceSpan, EvidenceType};
        use std::collections::HashMap;

        // Use real evidence retriever if available
        if let Some(ref mut retriever) = self.evidence_retriever {
            retriever
                .retrieve_patch_evidence(request, "default_tenant")
                .await
                .map_err(|e| AosError::Internal(e.to_string()))
        } else {
            // Fallback to basic mock if no retriever is available
            let mock_spans = vec![
                EvidenceSpan {
                    doc_id: "mock_doc_1".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash1".to_string(),
                    score: 0.9,
                    evidence_type: EvidenceType::Symbol,
                    file_path: request
                        .target_files
                        .first()
                        .unwrap_or(&"src/test.rs".to_string())
                        .clone(),
                    start_line: 10,
                    end_line: 15,
                    content: format!("Mock evidence for: {}", request.query),
                    metadata: HashMap::new(),
                },
                EvidenceSpan {
                    doc_id: "mock_doc_2".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash2".to_string(),
                    score: 0.8,
                    evidence_type: EvidenceType::Test,
                    file_path: "tests/test.rs".to_string(),
                    start_line: 20,
                    end_line: 25,
                    content: "Mock test evidence".to_string(),
                    metadata: HashMap::new(),
                },
            ];

            Ok(EvidenceResult {
                spans: mock_spans,
                total_found: 2,
                retrieval_time_ms: 50,
                sources_used: vec![EvidenceType::Symbol, EvidenceType::Test],
            })
        }
    }

    /// Generate patch proposal with evidence retrieval
    pub async fn propose_patch(
        &mut self,
        request: InferenceRequest,
        patch_request: &PatchProposalRequest,
    ) -> Result<InferenceResponse> {
        use crate::evidence::EvidenceRequest;
        use crate::patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator};
        use crate::patch_telemetry::{
            EvidenceMetrics, PatchGenerationMetrics, PatchTelemetry, ValidationMetrics,
        };
        use crate::patch_validator::{CodePolicy, PatchValidator};

        // Guardrail: Acquire resource permit
        let limiter = self.resource_limiter.clone();
        let _permit = limiter.acquire_request().await?;

        info!(
            "Generating patch proposal for: {}",
            patch_request.description
        );

        // Compute unavailable pinned adapters (CHAT-PIN-02)
        let unavailable_pinned_adapters =
            request.pinned_adapter_ids.as_ref().and_then(|pinned_ids| {
                let loaded_adapter_ids: Vec<&str> = self
                    .manifest
                    .adapters
                    .iter()
                    .map(|a| a.id.as_str())
                    .collect();
                let unavailable: Vec<String> = pinned_ids
                    .iter()
                    .filter(|id| !loaded_adapter_ids.contains(&id.as_str()))
                    .cloned()
                    .collect();
                if unavailable.is_empty() {
                    None
                } else {
                    Some(unavailable)
                }
            });

        // Compute pinned_routing_fallback based on unavailability (PRD-6A)
        let pinned_routing_fallback =
            match (&request.pinned_adapter_ids, &unavailable_pinned_adapters) {
                (Some(pinned), Some(unavailable))
                    if !pinned.is_empty() && !unavailable.is_empty() =>
                {
                    if unavailable.len() >= pinned.len() {
                        Some("stack_only".to_string())
                    } else {
                        Some("partial".to_string())
                    }
                }
                _ => None,
            };

        // Initialize telemetry
        let mut telemetry = PatchTelemetry::new();

        // 1. Build evidence retrieval request
        let evidence_request = EvidenceRequest {
            query: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            max_results: 10,
            min_score: 0.7,
        };

        // 2. Retrieve evidence (using mock implementation for now)
        let evidence_result = self.retrieve_evidence(&evidence_request).await?;

        // Log evidence retrieval telemetry
        let evidence_metrics = EvidenceMetrics {
            query: evidence_request.query,
            sources_used: evidence_result
                .sources_used
                .iter()
                .map(|s| format!("{:?}", s))
                .collect(),
            spans_found: evidence_result.spans.len(),
            retrieval_time_ms: evidence_result.retrieval_time_ms,
            avg_relevance_score: if !evidence_result.spans.is_empty() {
                evidence_result.spans.iter().map(|s| s.score).sum::<f32>()
                    / evidence_result.spans.len() as f32
            } else {
                0.0
            },
            min_score_threshold: evidence_request.min_score,
        };
        telemetry.log_evidence_retrieval("default_tenant", evidence_metrics, None);

        let mut evidence_refs = Vec::new();

        // Convert evidence spans to trace references
        for span in &evidence_result.spans {
            evidence_refs.push(EvidenceRef {
                doc_id: span.doc_id.clone(),
                rev: span.rev.clone(),
                span_hash: adapteros_core::B3Hash::from_hex(&span.span_hash)
                    .unwrap_or_else(|_| adapteros_core::B3Hash::hash(span.span_hash.as_bytes())),
                score: span.score,
            });
        }

        // 3. Generate patch proposal
        let patch_generation_request = PatchGenerationRequest {
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            target_files: patch_request.target_files.clone(),
            description: patch_request.description.clone(),
            evidence: evidence_result.spans,
            context: std::collections::HashMap::new(),
        };

        let patch_generator = PatchGenerator::new(
            Box::new(MockLlmBackend),
            crate::patch_generator::PatchParser::new(),
            crate::patch_generator::CitationExtractor::new(),
        );

        let proposal = patch_generator
            .generate_patch(patch_generation_request)
            .await?;

        // Log patch generation telemetry
        let generation_metrics = PatchGenerationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            description: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            evidence_count: proposal.citations.len(),
            patch_count: proposal.patches.len(),
            total_lines: proposal.patches.iter().map(|p| p.total_lines).sum(),
            generation_time_ms: 100, // Mock timing
            confidence_score: proposal.confidence,
        };
        telemetry.log_patch_generation("default_tenant", generation_metrics);

        // 4. Validate patch against policy
        let policy = CodePolicy::default();
        let policy_engine = PolicyEngine::new(self.manifest.policies.clone());
        let validator = PatchValidator::new(policy, policy_engine);
        let validation_result = validator.validate(&proposal.patches).await?;

        // Log patch validation telemetry
        let validation_metrics = ValidationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            is_valid: validation_result.is_valid,
            error_count: validation_result.errors.len(),
            warning_count: validation_result.warnings.len(),
            violation_count: validation_result.violations.len(),
            validation_time_ms: 50, // Mock timing
            confidence_score: validation_result.confidence,
            violations: validation_result
                .violations
                .into_iter()
                .map(|v| crate::patch_telemetry::ViolationMetric {
                    violation_type: format!("{:?}", v.violation_type),
                    severity: format!("{:?}", v.severity),
                    file_path: v.file_path,
                    line_number: v.line_number,
                    description: v.description,
                })
                .collect(),
        };
        telemetry.log_patch_validation("default_tenant", validation_metrics);

        // 5. Build response
        let patch_proposal = if validation_result.is_valid {
            Some(PatchProposalResponse {
                proposal_id: proposal.proposal_id,
                rationale: proposal.rationale,
                patches: proposal
                    .patches
                    .clone()
                    .into_iter()
                    .map(|p| FilePatchResponse {
                        file_path: p.file_path,
                        hunks: p
                            .hunks
                            .into_iter()
                            .map(|h| PatchHunkResponse {
                                start_line: h.start_line,
                                end_line: h.end_line,
                                old_content: h.context_lines.join("\n"),
                                new_content: h.modified_lines.join("\n"),
                            })
                            .collect(),
                    })
                    .collect(),
                citations: proposal
                    .citations
                    .clone()
                    .into_iter()
                    .map(|c| CitationResponse {
                        source_type: format!("{:?}", c.evidence_type),
                        reference: format!("{}:{}", c.file_path, c.line_range.0),
                        relevance: c.relevance_score,
                    })
                    .collect(),
                confidence: proposal.confidence,
            })
        } else {
            None
        };

        let status = if validation_result.is_valid {
            "success".to_string()
        } else {
            "validation_failed".to_string()
        };

        let fusion_interval = request
            .fusion_interval
            .unwrap_or(FusionInterval::PerRequest);

        let text = if validation_result.is_valid {
            Some(format!(
                "Patch proposal generated successfully with {} files and {} citations",
                proposal.patches.len(),
                proposal.citations.len()
            ))
        } else {
            Some(format!(
                "Patch validation failed: {}",
                validation_result.errors.join(", ")
            ))
        };

        Ok(InferenceResponse {
            text,
            status,
            trace: self.build_trace(
                &request.cpid,
                &evidence_refs,
                0,
                None,
                None,
                fusion_interval,
                &self
                    .manifest
                    .adapters
                    .iter()
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>(),
                false,
            ),
            run_receipt: None,
            token_usage: None,
            refusal: if !validation_result.is_valid {
                Some(RefusalResponse {
                    status: "failed".to_string(),
                    reason: adapteros_policy::RefusalReason::MissingFields {
                        template: "patch_validation".to_string(),
                        fields: validation_result.errors.clone(),
                    },
                    message: format!(
                        "Patch validation failed: {}",
                        validation_result.errors.join(", ")
                    ),
                    suggested_actions: vec![
                        "Review and fix the validation errors".to_string(),
                        "Ensure all required fields are provided".to_string(),
                    ],
                })
            } else {
                None
            },
            patch_proposal,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            backend_used: Some(self.kernels.lock().await.device_name().to_string()),
            backend_version: Some(adapteros_core::version::VERSION.to_string()),
            fallback_triggered: false,
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            coreml_package_hash: self.coreml_package_hash.clone(),
            coreml_expected_package_hash: self
                .coreml_verification
                .as_ref()
                .and_then(|v| v.expected.clone()),
            coreml_hash_mismatch: self.coreml_verification.as_ref().map(|v| v.mismatch),
            fallback_backend: None,
            determinism_mode_applied: Some(request.determinism_mode.clone()),
            unavailable_pinned_adapters,
            pinned_routing_fallback,
            placement_trace: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            error_details: None,
        })
    }
}
