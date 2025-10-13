# AdapterOS Policy Packs

This document is auto-generated from the policy registry metadata. It describes the 20 canonical policy packs that enforce compliance, security, and quality in AdapterOS.

## Policy Registry Overview

The policy registry (`adapteros-policy`) contains exactly 20 policy packs, each with:
- **ID**: Unique identifier
- **Name**: Human-readable name
- **Description**: Purpose and scope
- **Severity**: Enforcement level (Critical, High, Medium, Low)
- **Implementation Status**: Whether the policy is implemented

## The 20 Canonical Policy Packs

### 1. Egress
**ID**: `Egress`  
**Severity**: Critical  
**Status**: Implemented  
**Description**: Control outbound network and protocols. Enforces zero network egress during serving mode, requires PF (Packet Filter) enforcement, and blocks all outbound sockets.

### 2. Determinism
**ID**: `Determinism`  
**Severity**: Critical  
**Status**: Implemented  
**Description**: Enforce executor, hashes, replay, and epsilon bounds. Ensures reproducible outputs through precompiled kernels, HKDF seeding, and deterministic retrieval ordering.

### 3. Router
**ID**: `Router`  
**Severity**: High  
**Status**: Implemented  
**Description**: Deterministic tie-break and route selection. Implements K-sparse adapter selection with Q15 quantized gates and entropy floor to prevent single-adapter collapse.

### 4. Evidence
**ID**: `Evidence`  
**Severity**: High  
**Status**: Implemented  
**Description**: Trace, signatures, and audit artifacts. Mandatory open-book grounding with evidence retrieval before generation for regulated domains.

### 5. Refusal
**ID**: `Refusal`  
**Severity**: High  
**Status**: Implemented  
**Description**: Deny unsafe operations and redact outputs. Abstains when evidence spans are insufficient or confidence falls below threshold.

### 6. Numeric
**ID**: `Numeric`  
**Severity**: Medium  
**Status**: Implemented  
**Description**: Precision modes, epsilon budgets, and strict math. Normalizes units internally and validates numeric claims through unit sanity checks.

### 7. RAG
**ID**: `Rag`  
**Severity**: High  
**Status**: Implemented  
**Description**: Retrieval provenance and cache rules. Per-tenant index isolation with deterministic ordering and embedding model hash validation.

### 8. Isolation
**ID**: `Isolation`  
**Severity**: Critical  
**Status**: Implemented  
**Description**: Process, memory, and adapter sandbox. Multi-tenant isolation with unique UID/GID per tenant and capability-scoped directory handles.

### 9. Telemetry
**ID**: `Telemetry`  
**Severity**: Medium  
**Status**: Implemented  
**Description**: Deterministic logging and metrics. Canonical JSON serialization with BLAKE3 hashing and configurable sampling rates.

### 10. Retention
**ID**: `Retention`  
**Severity**: Medium  
**Status**: Implemented  
**Description**: Data lifetime, TTL, and purge proof. Bounded storage with configurable retention policies for bundles and audit trails.

### 11. Performance
**ID**: `Performance`  
**Severity**: High  
**Status**: Implemented  
**Description**: Throughput budgets without non-deterministic paths. Enforces latency budgets (p95 < 24ms) and router overhead limits.

### 12. Memory
**ID**: `Memory`  
**Severity**: High  
**Status**: Implemented  
**Description**: UMA behavior, pinning, and page-out guards. Maintains ≥15% unified memory headroom with configurable eviction order.

### 13. Artifacts
**ID**: `Artifacts`  
**Severity**: Critical  
**Status**: Implemented  
**Description**: Models, adapters, and build outputs as signed objects. Requires Ed25519 signatures and SBOM validation for all imported bundles.

### 14. Secrets
**ID**: `Secrets`  
**Severity**: Critical  
**Status**: Implemented  
**Description**: Vault use, zero egress, zero logs. Secure Enclave integration with ephemeral session key derivation and key rotation.

### 15. Build/Release
**ID**: `BuildRelease`  
**Severity**: High  
**Status**: Implemented  
**Description**: Toolchain pins, kernel hashes, SBOM. Enforces deterministic builds with signed Plan packs and rollback capability.

### 16. Compliance
**ID**: `Compliance`  
**Severity**: High  
**Status**: Implemented  
**Description**: CMMC/ITAR policy hooks and reports. Maps controls to evidence files and requires adversarial testing for tenant isolation.

### 17. Incident
**ID**: `Incident`  
**Severity**: High  
**Status**: Implemented  
**Description**: Freeze, capture, and post-mortem bundles. Predictable incident response procedures with state machine guards.

### 18. Output
**ID**: `Output`  
**Severity**: Medium  
**Status**: Implemented  
**Description**: Canonical formats, normalization, PII filters. JSON-serializable response shapes with required trace information.

### 19. Adapters
**ID**: `Adapters`  
**Severity**: Medium  
**Status**: Implemented  
**Description**: Load order, composition, capability ACLs. Adapter lifecycle management with activation thresholds and quality metrics.

### 20. Deterministic I/O
**ID**: `DeterministicIo`  
**Severity**: High  
**Status**: Implemented  
**Description**: File reads/writes via hashed wrappers, no wall-clock, stubbed network under strict mode. Ensures deterministic I/O operations.

## Policy Enforcement

Policies are enforced through the `adapteros-policy` crate, which provides:

- **Registry Access**: `list_policies()`, `get_policy()`, `explain_policy()`
- **Enforcement**: `Policy::enforce(&PolicyContext) -> Result<Audit, Violation>`
- **CLI Integration**: `aosctl policy list|explain|enforce`

## CLI Usage

```bash
# List all policy packs
aosctl policy list

# List only implemented policies
aosctl policy list --implemented

# Explain a specific policy
aosctl policy explain Egress
aosctl policy explain 1

# Enforce all policies (dry run)
aosctl policy enforce --all --dry-run

# Enforce specific policy
aosctl policy enforce --pack Determinism
```

## Policy Context

Each policy receives a `PolicyContext` containing:
- **Tenant ID**: For multi-tenant isolation
- **Request Metadata**: For evidence and routing decisions
- **System State**: For memory and performance monitoring
- **Configuration**: For policy-specific settings

## Violations and Audits

When a policy is violated, it returns a `Violation` with:
- **Policy ID**: Which policy was violated
- **Severity**: Critical, High, Medium, or Low
- **Message**: Human-readable description
- **Details**: Additional context or remediation steps

Successful policy enforcement returns an `Audit` with:
- **Policy ID**: Which policy was enforced
- **Status**: Pass, Warning, or Info
- **Evidence**: Supporting data or metrics
- **Timestamp**: When the audit occurred

## Integration Points

Policies are enforced at various points in the AdapterOS pipeline:

- **Startup**: Egress, Isolation, Artifacts, Secrets
- **Request Processing**: Router, Evidence, Refusal, Output
- **Inference**: Determinism, Performance, Memory
- **Post-Processing**: Numeric, Telemetry, Retention
- **Build/Deploy**: Build/Release, Compliance

## Compliance and Auditing

The policy system supports compliance frameworks including:
- **CMMC**: Cybersecurity Maturity Model Certification
- **ITAR**: International Traffic in Arms Regulations
- **SOC 2**: Service Organization Control 2
- **ISO 27001**: Information Security Management

All policy enforcement events are logged and can be audited for compliance reporting.

---

*This document is auto-generated from the policy registry. Last updated: $(date)*
