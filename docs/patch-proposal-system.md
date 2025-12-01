# Patch Proposal System

## Overview

The Patch Proposal System is a comprehensive evidence-first, security-first, performance-first system for generating code patches with citations and policy validation. It implements the full pipeline from evidence retrieval to patch generation, validation, and telemetry.

## Architecture

The system consists of several key components:

### 1. Evidence Retrieval (`crates/mplora-worker/src/evidence.rs`)

Multi-source evidence retrieval with RAG integration:

- **RAG System Integration**: Primary evidence source using existing RAG infrastructure
- **Symbol-based Retrieval**: Highest priority evidence from code symbols
- **Test-based Retrieval**: Evidence from test cases and coverage
- **Documentation Retrieval**: API usage and documentation evidence
- **Code Pattern Retrieval**: Similar implementation patterns
- **Framework-specific Retrieval**: Framework-specific evidence

```rust
use adapteros_lora_worker::evidence::{EvidenceRetriever, EvidenceRequest, EvidenceType};

let retriever = EvidenceRetriever::new(
    rag_system,
    Box::new(symbol_index),
    Box::new(test_index),
    Box::new(doc_index),
    Box::new(code_index),
    Box::new(framework_index),
);

let request = EvidenceRequest {
    query: "Add error handling to authentication".to_string(),
    target_files: vec!["src/auth.rs".to_string()],
    repo_id: "auth_service".to_string(),
    commit_sha: Some("abc123".to_string()),
    max_results: 10,
    min_score: 0.7,
};

let result = retriever.retrieve_patch_evidence(&request, "tenant_id").await?;
```

### 2. Patch Generation (`crates/mplora-worker/src/patch_generator.rs`)

LLM-integrated patch generation with structured output:

- **LLM Backend Integration**: Pluggable LLM backend for patch generation
- **Patch Parsing**: Unified diff format parsing
- **Citation Extraction**: Automatic citation generation from evidence
- **Confidence Scoring**: Confidence assessment for generated patches

```rust
use adapteros_lora_worker::patch_generator::{PatchGenerator, PatchGenerationRequest, MockLlmBackend};

let generator = PatchGenerator::new(
    Box::new(MockLlmBackend),
    PatchParser::new(),
    CitationExtractor::new(),
);

let request = PatchGenerationRequest {
    repo_id: "auth_service".to_string(),
    commit_sha: Some("abc123".to_string()),
    target_files: vec!["src/auth.rs".to_string()],
    description: "Add comprehensive error handling".to_string(),
    evidence: evidence_spans,
    context: HashMap::new(),
};

let proposal = generator.generate_patch(request).await?;
```

### 3. Policy Validation (`crates/mplora-worker/src/patch_validator.rs`)

Comprehensive policy validation and security checks:

- **Path Restrictions**: File path allowlist/denylist validation
- **Secret Detection**: Automatic secret scanning
- **Forbidden Operations**: Detection of dangerous operations
- **Dependency Validation**: External dependency policy enforcement
- **Size Limits**: Patch size and resource limit validation
- **Global Policy Engine**: Integration with AdapterOS policy engine

```rust
use adapteros_lora_worker::patch_validator::{PatchValidator, CodePolicy};
use adapteros_policy::PolicyEngine;

let policy = CodePolicy::default();
let policy_engine = PolicyEngine::new(policies);
let validator = PatchValidator::new(policy, policy_engine);

let validation_result = validator.validate(&proposal.patches).await?;

if validation_result.is_valid {
    println!("Patch validation passed with confidence: {:.3}", validation_result.confidence);
} else {
    for error in validation_result.errors {
        println!("Validation error: {}", error);
    }
}
```

### 4. API Integration (`crates/mplora-server-api/src/handlers.rs`)

RESTful API for patch proposal requests:

- **Authentication**: JWT-based authentication with role-based access
- **Input Validation**: Comprehensive request validation
- **Worker Integration**: Direct integration with worker backend
- **Response Formatting**: Structured response with status and metadata

```bash
curl -X POST http://localhost:8080/v1/propose-patch \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "repo_id": "auth_service",
    "commit_sha": "abc123",
    "description": "Add error handling to authentication",
    "target_files": ["src/auth.rs"]
  }'
```

### 5. Telemetry Integration (`crates/mplora-worker/src/patch_telemetry.rs`)

Comprehensive telemetry and monitoring:

- **Event Logging**: Detailed event logging for all operations
- **Performance Monitoring**: Performance threshold monitoring
- **Security Violation Tracking**: Security violation logging
- **Telemetry Writer Integration**: Integration with AdapterOS telemetry system

```rust
use adapteros_lora_worker::patch_telemetry::{PatchTelemetry, EvidenceMetrics};
use adapteros_telemetry::TelemetryWriter;

let telemetry_writer = TelemetryWriter::new("./var/telemetry", 1000, 1024*1024)?;
let mut telemetry = PatchTelemetry::new_with_writer(telemetry_writer);

let metrics = EvidenceMetrics {
    query: "error handling".to_string(),
    sources_used: vec!["symbol".to_string(), "test".to_string()],
    spans_found: 5,
    retrieval_time_ms: 50,
    avg_relevance_score: 0.8,
    min_score_threshold: 0.7,
};

telemetry.log_evidence_retrieval("tenant_id", metrics, Some("proposal_123"));
```

## Usage Examples

### Basic Patch Proposal

```rust
use adapteros_lora_worker::{Worker, InferenceRequest, RequestType, PatchProposalRequest};

let mut worker = Worker::new(config)?;

let patch_request = PatchProposalRequest {
    repo_id: "auth_service".to_string(),
    commit_sha: Some("abc123".to_string()),
    description: "Add error handling to authentication middleware".to_string(),
    target_files: vec!["src/middleware/auth.rs".to_string()],
};

let inference_request = InferenceRequest {
    cpid: "patch-proposal".to_string(),
    prompt: patch_request.description.clone(),
    max_tokens: 1000,
    require_evidence: true,
    request_type: RequestType::PatchProposal(patch_request),
};

let response = worker.infer(inference_request).await?;

if let Some(proposal) = response.patch_proposal {
    println!("Generated patch proposal: {}", proposal.proposal_id);
    println!("Confidence: {:.3}", proposal.confidence);
    println!("Files modified: {}", proposal.patches.len());
    println!("Citations: {}", proposal.citations.len());
}
```

### Real-world Scenarios

#### 1. Authentication Middleware

```rust
let request = PatchProposalRequest {
    repo_id: "auth_service".to_string(),
    commit_sha: Some("def456".to_string()),
    description: "Add JWT authentication middleware with proper error handling and logging".to_string(),
    target_files: vec!["src/middleware/mod.rs".to_string()],
};
```

#### 2. Database Migration

```rust
let request = PatchProposalRequest {
    repo_id: "user_service".to_string(),
    commit_sha: Some("ghi789".to_string()),
    description: "Add user profiles table with foreign key to users and proper indexes".to_string(),
    target_files: vec!["migrations/002_add_user_profiles.sql".to_string()],
};
```

#### 3. API Endpoint

```rust
let request = PatchProposalRequest {
    repo_id: "user_api".to_string(),
    commit_sha: Some("jkl012".to_string()),
    description: "Add POST /api/posts endpoint with input validation, rate limiting, and proper error responses".to_string(),
    target_files: vec!["src/api/posts.rs".to_string()],
};
```

#### 4. Performance Optimization

```rust
let request = PatchProposalRequest {
    repo_id: "performance_service".to_string(),
    commit_sha: Some("mno345".to_string()),
    description: "Optimize user search endpoint with caching, database indexing, and query optimization".to_string(),
    target_files: vec!["src/api/search.rs".to_string()],
};
```

#### 5. Security Fix

```rust
let request = PatchProposalRequest {
    repo_id: "secure_service".to_string(),
    commit_sha: Some("pqr678".to_string()),
    description: "Fix SQL injection vulnerability in user lookup query by using parameterized statements".to_string(),
    target_files: vec!["src/db/user_queries.rs".to_string()],
};
```

## Policy Configuration

The system enforces comprehensive policies aligned with AdapterOS rulesets:

### Evidence Ruleset (#4)
- **Open-book requirement**: All factual claims must be backed by evidence
- **Minimum spans**: At least 1 evidence span required
- **Latest revision preference**: Prefer latest document revisions
- **Supersession warnings**: Warn when using superseded documents

### Numeric Ruleset (#6)
- **Unit requirements**: All numeric claims must include units
- **Canonical units**: Use canonical units per domain (e.g., in-lbf for torque)
- **Rounding rules**: Round at end of calculation, not per step
- **Trace requirements**: Include units in trace when numbers appear

### Security Policies
- **Path restrictions**: File path allowlist/denylist validation
- **Secret detection**: Automatic scanning for API keys, passwords, tokens
- **Forbidden operations**: Detection of dangerous operations (eval, exec, etc.)
- **Dependency validation**: External dependency policy enforcement

### Performance Policies
- **Evidence retrieval**: < 100ms threshold
- **Patch generation**: < 2s threshold
- **Patch validation**: < 50ms threshold
- **Memory limits**: Patch size and resource limit validation

## Testing

The system includes comprehensive testing:

### Unit Tests
- Evidence retrieval with mock indices
- Patch generation with mock LLM backend
- Policy validation with various violations
- Telemetry logging and metrics

### Integration Tests
- End-to-end patch proposal pipeline
- API integration with worker backend
- Telemetry integration with writer

### Real-world Scenario Tests
- Authentication middleware scenarios
- Database migration scenarios
- API endpoint scenarios
- Performance optimization scenarios
- Security vulnerability fix scenarios

## Performance Requirements

The system meets strict performance requirements:

- **Evidence Retrieval**: < 100ms average
- **Patch Generation**: < 2s average
- **Patch Validation**: < 50ms average
- **Total Pipeline**: < 3s end-to-end

## Security Considerations

The system implements comprehensive security measures:

- **Authentication**: JWT-based authentication required
- **Authorization**: Role-based access control (Operator, SRE)
- **Input Validation**: Comprehensive request validation
- **Secret Detection**: Automatic secret scanning
- **Policy Enforcement**: Strict policy validation
- **Audit Logging**: Complete audit trail

## Monitoring and Observability

The system provides comprehensive monitoring:

- **Event Logging**: Detailed event logging for all operations
- **Performance Metrics**: Performance threshold monitoring
- **Security Violations**: Security violation tracking
- **Telemetry Integration**: Integration with AdapterOS telemetry
- **Health Checks**: Health and readiness endpoints

## Future Enhancements

Planned enhancements include:

- **Real LLM Integration**: Replace mock LLM backend with real LLM
- **Database Storage**: Persistent storage for patch proposals
- **Advanced Evidence**: More sophisticated evidence retrieval
- **Policy Extensions**: Additional policy rules and validations
- **Performance Optimization**: Further performance improvements
- **UI Integration**: Web UI for patch proposal management

## Conclusion

The Patch Proposal System provides a comprehensive, evidence-first approach to code patch generation with strong security, performance, and policy enforcement. It integrates seamlessly with the AdapterOS ecosystem and provides a solid foundation for automated code modification with proper citations and validation.
