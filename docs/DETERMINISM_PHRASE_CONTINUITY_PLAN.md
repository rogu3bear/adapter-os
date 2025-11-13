# Determinism and Phrase Continuity Implementation Plan

**Document Type**: Implementation Plan  
**Status**: Active Planning  
**Date**: 2025-11-13  
**Version**: 1.1

---

## Executive Summary

This document outlines a comprehensive plan for implementing cryptographic verifiable phrase continuity across modular adapter stacks in AdapterOS. The plan builds on existing determinism infrastructure (deterministic executor, seeded RNG, policy enforcement) to add cryptographic masking, verification vectors, and transcript hashing for phrase-specific semantic influence tracking.

**Goal**: Enable zero-knowledge-style provenance, regulatory auditability, and federated model integrity at lightweight computational cost, without requiring homomorphic encryption or full zero-knowledge proofs.

---

## Current State Analysis

### Existing Determinism Infrastructure

**✓ Implemented**:
- **Deterministic Executor** (`adapteros-deterministic-exec`): Serial task execution with logical tick counter
- **Global Seed Management**: HKDF-based seed derivation from 32-byte global seed
- **Seeded RNG**: ChaCha20Rng with domain separation via HKDF labels
- **Policy Enforcement**: Determinism policy pack enforces seeded randomness
- **Evidence Tracking**: Policy decisions logged with structured telemetry
- **Metal Kernel Determinism**: Precompiled `.metallib` with hash verification
- **Q15 Quantization**: Router gates quantized to Q15 format for deterministic fixed-point

**⚠ Partial**:
- **Boundary Hooks**: LoRA adapter boundaries exist but no masking infrastructure
- **Cryptographic Primitives**: HKDF, SHA-256, BLAKE3 available; AEAD (AES-GCM/ChaCha20) available but not used for boundary states
- **Session Management**: Session IDs exist but not bound to cryptographic contexts
- **Verification Service**: Evidence tracking exists but no verification vector comparison

**✗ Missing**:
- **Mask Vector Generation**: No per-step, per-sample mask vectors `r_t` derived from master key
- **Masking Operations**: No XOR, modular addition, or multiplicative masking at adapter boundaries
- **Blinded Trace Tokens**: No phrase hash truncation for continuity proofs
- **Verification Vectors**: No `v_t = H(E_t ∥ τ)` computation
- **Transcript Hashes**: No `T_t = H(v_t ∥ t ∥ session_id ∥ nonce)` construction
- **Producer-Consumer Protocol**: No authenticated encryption for boundary state transport
- **Feature Permutation**: No invertible feature-wise permutation `Π_t`

---

## Implementation Phases

### Phase 1: Core Masking Infrastructure (Weeks 1-3)

**Objective**: Implement cryptographic masking at adapter boundaries with master key derivation.

#### 1.1 Master Key Management

**Tasks**:
- [ ] Extend `adapteros-crypto` with master key `K` generation and storage
- [ ] Implement HKDF-Extract for master key derivation from global seed
- [ ] Add key rotation and session revocation support
- [ ] Integrate with existing `KeyProvider` abstraction

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/mask_key.rs` - Master key management
- `crates/adapteros-crypto/src/providers/keychain.rs` - Key storage integration

**Dependencies**: Existing HKDF implementation (`hkdf` crate)

**Acceptance Criteria**:
- Master key `K` can be derived from global seed via HKDF-Extract
- Key rotation preserves backward compatibility for active sessions
- Keys stored securely in OS keychain (macOS) or encrypted keystore (Linux)

#### 1.2 Mask Vector Generation

**Tasks**:
- [ ] Implement `MaskGenerator` struct with HKDF-Expand for mask streams
- [ ] Add context binding: `r_t = HKDF-Expand(K, t ∥ session_id ∥ nonce)`
- [ ] Support multiple mask modes: XOR, modular addition, multiplicative
- [ ] Add mask clipping for multiplicative mode (avoid zero/instability)

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/mask_generator.rs` - Mask vector generation
- `crates/adapteros-lora/src/boundary.rs` - Boundary masking hooks

**Dependencies**: HKDF, SHA-256

**Acceptance Criteria**:
- Mask vectors `r_t` are deterministic given `(K, t, session_id, nonce)`
- Mask generation supports batch processing (multiple samples)
- Mask vectors match tensor shapes (broadcast over `batch × seq_len × d_model`)

#### 1.3 Masking Operations

**Tasks**:
- [ ] Implement XOR masking: `E_t = x_t ⊕ r_t` (bitwise XOR)
- [ ] Implement modular addition: `E_t = (x_t + r_t) mod 2^w` in quantized integer ring
- [ ] Implement multiplicative masking: `E_t = x_t · r_t` with stability bounds `δ=1e-3`
- [ ] Add reversible dequantization for quantized modes
- [ ] Create Metal kernel for GPU-accelerated masking (if needed)

**Files to Create/Modify**:
- `crates/adapteros-lora/src/masking.rs` - Masking operations
- `metal/src/kernels/masking.metal` - GPU masking kernel (optional)

**Dependencies**: Quantization infrastructure (`adapteros-numerics`)

**Acceptance Criteria**:
- Masking operations are reversible (recover `x_t` from `E_t` given `r_t`)
- Error bounds: `||x_t - recover(E_t, r_t)|| < 1e-6` for XOR/mod-add
- Multiplicative masking preserves numerical stability (`r_t` clipped to `[1-δ, 1+δ]`)

---

### Phase 2: Boundary Integration (Weeks 4-6)

**Objective**: Hook masking into LoRA adapter boundaries and implement feature permutation.

#### 2.1 Boundary Detection and Hooking

**Tasks**:
- [ ] Identify adapter boundaries in LoRA forward pass
- [ ] Implement boundary hooks before/after adapter application
- [ ] Add configuration for masking mode per boundary
- [ ] Integrate with existing `MploraKernels` trait

**Files to Create/Modify**:
- `crates/adapteros-lora/src/boundary.rs` - Boundary detection and hooks
- `crates/adapteros-lora-worker/src/inference_pipeline.rs` - Integration point

**Dependencies**: LoRA adapter application code

**Acceptance Criteria**:
- Masking applied at all adapter boundaries (before adapter, after adapter)
- Masking mode configurable per boundary (XOR/mod-add/mul)
- Zero performance regression in non-masked mode

#### 2.2 Feature Permutation

**Tasks**:
- [ ] Implement seeded Fisher-Yates shuffle for permutation generation
- [ ] Create invertible permutation `Π_t` synchronized via context
- [ ] Add permutation application: `E_t' = Π_t(E_t)` before transport
- [ ] Implement inverse permutation: `E_t = Π_t^{-1}(E_t')` at consumer

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/permutation.rs` - Permutation generation
- `crates/adapteros-lora/src/boundary.rs` - Permutation application

**Dependencies**: Seeded RNG (`adapteros-deterministic-exec`)

**Acceptance Criteria**:
- Permutation `Π_t` is deterministic given seed and context
- Inverse permutation recovers original: `Π_t^{-1}(Π_t(x)) = x`
- Permutation applied before masking (optional, configurable)

---

### Phase 3: Verification Infrastructure (Weeks 7-9)

**Objective**: Implement blinded trace tokens, verification vectors, and transcript hashing.

#### 3.1 Blinded Trace Tokens

**Tasks**:
- [ ] Implement phrase hashing: `H(phrase)` using SHA-256 or BLAKE3
- [ ] Add token truncation: `τ = truncate(H(phrase), k)` where `k=32` bytes
- [ ] Create `TraceToken` struct with session binding
- [ ] Add token generation per inference request

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/trace_token.rs` - Trace token generation
- `crates/adapteros-server/src/handlers.rs` - Token generation in request handler

**Dependencies**: SHA-256/BLAKE3

**Acceptance Criteria**:
- Trace token `τ` is deterministic for same phrase
- Token does not reveal phrase (truncation provides blinding)
- Token bound to session ID and nonce

#### 3.2 Verification Vectors

**Tasks**:
- [ ] Implement verification vector computation: `v_t = H(E_t ∥ τ)`
- [ ] Use big-endian serialization for `E_t` before hashing
- [ ] Add verification vector logging per boundary
- [ ] Create `VerificationVector` struct with metadata

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/verification.rs` - Verification vector computation
- `crates/adapteros-lora/src/boundary.rs` - Vector computation at boundaries

**Dependencies**: BLAKE3/SHA-256, big-endian serialization

**Acceptance Criteria**:
- Verification vectors are deterministic given `(E_t, τ)`
- Vectors computed at all adapter boundaries
- Serialization uses big-endian format for cross-platform determinism

#### 3.3 Transcript Hashes

**Tasks**:
- [ ] Implement transcript hash: `T_t = H(v_t ∥ t ∥ session_id ∥ nonce)`
- [ ] Create `TranscriptHash` struct with chaining support
- [ ] Add transcript logging to evidence tracker
- [ ] Implement transcript verification: compare `T_t` across nodes

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/transcript.rs` - Transcript hash computation
- `crates/adapteros-policy/src/evidence_tracker.rs` - Transcript logging

**Dependencies**: BLAKE3/SHA-256, evidence tracking

**Acceptance Criteria**:
- Transcript hashes are deterministic given `(v_t, t, session_id, nonce)`
- Transcripts logged to append-only audit log
- Transcript chaining: `T_{t+1} = H(T_t ∥ v_{t+1} ∥ t+1 ∥ ...)`

---

### Phase 4: Producer-Consumer Protocol (Weeks 10-12)

**Objective**: Implement authenticated encryption for boundary state transport between nodes.

#### 4.1 AEAD Integration

**Tasks**:
- [ ] Extend `adapteros-crypto` with AEAD wrapper for boundary states
- [ ] Implement AES-256-GCM or ChaCha20-Poly1305 for encryption
- [ ] Add associated data: `(session_id, t, nonce, boundary_id)`
- [ ] Create `BoundaryEnvelope` struct for encrypted transport

**Files to Create/Modify**:
- `crates/adapteros-crypto/src/boundary_envelope.rs` - AEAD wrapper
- `crates/adapteros-server/src/transport.rs` - Transport layer (if needed)

**Dependencies**: `aes-gcm` or `chacha20poly1305` crates

**Acceptance Criteria**:
- Boundary states encrypted with IND-CCA2 security (AEAD)
- Associated data prevents replay attacks
- Encryption/decryption deterministic given same key and nonce

#### 4.2 Session and Nonce Management

**Tasks**:
- [ ] Extend session management with cryptographic nonce generation
- [ ] Implement nonce uniqueness: `nonce = H(session_id ∥ t ∥ counter)`
- [ ] Add nonce rotation per boundary or per step
- [ ] Integrate with existing session tracking

**Files to Create/Modify**:
- `crates/adapteros-server/src/session.rs` - Session and nonce management
- `crates/adapteros-crypto/src/nonce.rs` - Nonce generation

**Dependencies**: BLAKE3/SHA-256, session tracking

**Acceptance Criteria**:
- Nonces are cryptographically secure and unique per `(session_id, t)`
- Nonce generation is deterministic (seeded RNG)
- Nonce rotation prevents replay attacks

#### 4.3 Producer-Consumer API

**Tasks**:
- [ ] Design API for producer node: `produce_boundary_state(boundary_id, E_t) -> Envelope`
- [ ] Design API for consumer node: `consume_boundary_state(envelope) -> E_t`
- [ ] Add verification vector comparison: `verify_continuity(v_producer, v_consumer) -> bool`
- [ ] Implement error handling for mismatched vectors

**Files to Create/Modify**:
- `crates/adapteros-lora/src/producer_consumer.rs` - Producer-consumer API
- `crates/adapteros-server/src/handlers.rs` - API endpoints (if HTTP-based)

**Dependencies**: AEAD, verification vectors

**Acceptance Criteria**:
- Producer encrypts `E_t` with AEAD and returns envelope
- Consumer decrypts envelope and verifies `v_t` matches producer's `v_t`
- Mismatch triggers error and evidence logging

---

### Phase 5: Verifier Service (Weeks 13-15)

**Objective**: Implement centralized verifier service for transcript comparison and audit logging.

#### 5.1 Verifier Service Architecture

**Tasks**:
- [ ] Design verifier service API: `verify_transcript(session_id, transcripts) -> VerificationResult`
- [ ] Implement transcript comparison: check `T_t` equality across nodes
- [ ] Add divergence detection: identify mismatched `v_t` vectors
- [ ] Create `VerifierService` struct with async support

**Files to Create/Modify**:
- `crates/adapteros-verifier/src/lib.rs` - Verifier service
- `crates/adapteros-verifier/src/transcript_comparison.rs` - Comparison logic

**Dependencies**: Transcript hashes, evidence tracking

**Acceptance Criteria**:
- Verifier compares transcripts from multiple nodes
- Divergence detection identifies first mismatched `v_t`
- Verification results logged to audit trail

#### 5.2 Audit Logging

**Tasks**:
- [ ] Extend evidence tracker with transcript logging
- [ ] Implement append-only audit log: `AuditLog::append(transcript)`
- [ ] Add Merkle tree or hash chain for log integrity
- [ ] Create audit log query API: `query_transcripts(session_id, time_range)`

**Files to Create/Modify**:
- `crates/adapteros-policy/src/audit_log.rs` - Audit log implementation
- `crates/adapteros-policy/src/evidence_tracker.rs` - Integration

**Dependencies**: Evidence tracking, Merkle tree (if used)

**Acceptance Criteria**:
- Audit log is append-only (no modifications)
- Log integrity verified via hash chain or Merkle tree
- Transcripts queryable by session ID and time range

#### 5.3 Zero-Knowledge Properties

**Tasks**:
- [ ] Document zero-knowledge properties: verifier learns `v_t` but not `E_t` or phrase
- [ ] Verify no information leakage: `E_t` never sent to verifier
- [ ] Add privacy analysis: what information is revealed by `v_t`?
- [ ] Create privacy documentation

**Files to Create/Modify**:
- `docs/determinism-privacy-analysis.md` - Privacy analysis
- `crates/adapteros-verifier/src/privacy.rs` - Privacy checks (if needed)

**Dependencies**: None (documentation)

**Acceptance Criteria**:
- Privacy analysis documents zero-knowledge properties
- Verifier cannot recover `E_t` or phrase from `v_t`
- Documentation explains security guarantees

---

### Phase 6: Testing and Validation (Weeks 16-18)

**Objective**: Comprehensive testing of masking, verification, and transcript infrastructure.

#### 6.1 Unit Tests

**Tasks**:
- [ ] Test mask generation: verify determinism across runs
- [ ] Test masking operations: verify reversibility and error bounds
- [ ] Test verification vectors: verify determinism and collision resistance
- [ ] Test transcript hashes: verify chaining and integrity

**Files to Create/Modify**:
- `crates/adapteros-crypto/tests/mask_tests.rs` - Mask generation tests
- `crates/adapteros-crypto/tests/verification_tests.rs` - Verification tests
- `crates/adapteros-verifier/tests/transcript_tests.rs` - Transcript tests

**Dependencies**: Test infrastructure

**Acceptance Criteria**:
- All unit tests pass with 100% coverage of masking/verification code
- Tests verify determinism: identical inputs produce identical outputs
- Error bounds validated: `||x_t - recover(E_t, r_t)|| < 1e-6`

#### 6.2 Integration Tests

**Tasks**:
- [ ] Test producer-consumer flow: verify `v_t` equality across nodes
- [ ] Test transcript verification: verify `T_t` comparison in verifier
- [ ] Test session and nonce management: verify uniqueness and rotation
- [ ] Test boundary masking: verify masking applied at all boundaries

**Files to Create/Modify**:
- `tests/integration/phrase_continuity.rs` - Integration tests
- `tests/integration/producer_consumer.rs` - Producer-consumer tests

**Dependencies**: Test infrastructure, multi-node setup

**Acceptance Criteria**:
- Integration tests verify end-to-end phrase continuity
- Producer and consumer compute identical `v_t` vectors
- Verifier detects divergence when `v_t` mismatches

#### 6.3 Performance Tests

**Tasks**:
- [ ] Benchmark masking overhead: measure latency impact
- [ ] Benchmark verification vector computation: measure hash cost
- [ ] Benchmark transcript hashing: measure chaining overhead
- [ ] Optimize hot paths if needed

**Files to Create/Modify**:
- `benches/masking_bench.rs` - Masking benchmarks
- `benches/verification_bench.rs` - Verification benchmarks

**Dependencies**: Criterion benchmark framework

**Acceptance Criteria**:
- Masking overhead < 5% of inference latency
- Verification vector computation < 1ms per boundary
- Transcript hashing < 100μs per step

---

### Phase 7: Documentation and Deployment (Weeks 19-20)

**Objective**: Complete documentation and production deployment preparation.

#### 7.1 Documentation

**Tasks**:
- [ ] Write user guide: "Using Phrase Continuity Verification"
- [ ] Write developer guide: "Implementing Custom Masking Modes"
- [ ] Write security analysis: "Privacy and Security Guarantees"
- [ ] Update architecture documentation

**Files to Create/Modify**:
- `docs/determinism-phrase-continuity.md` - User guide
- `docs/determinism-developer-guide.md` - Developer guide
- `docs/determinism-security-analysis.md` - Security analysis

**Dependencies**: Implementation complete

**Acceptance Criteria**:
- Documentation covers all features and APIs
- Security analysis documents zero-knowledge properties
- Examples demonstrate usage patterns

#### 7.2 Production Deployment

**Tasks**:
- [ ] Add configuration options: masking mode, verification enabled/disabled
- [ ] Add feature flags: `phrase-continuity` feature gate
- [ ] Update production mode validation: require masking in production
- [ ] Add monitoring: track verification failures and divergence

**Files to Create/Modify**:
- `crates/adapteros-config/src/config.rs` - Configuration options
- `crates/adapteros-server/src/main.rs` - Production mode validation
- `crates/adapteros-telemetry/src/events.rs` - Monitoring events

**Dependencies**: Configuration system, telemetry

**Acceptance Criteria**:
- Configuration allows enabling/disabling phrase continuity
- Production mode enforces masking and verification
- Monitoring tracks verification metrics

---

## Technical Specifications

### Mask Vector Generation

```rust
pub struct MaskGenerator {
    master_key: [u8; 32],
    hkdf: Hkdf<Sha256>,
}

impl MaskGenerator {
    pub fn generate_mask(
        &self,
        step: u64,
        session_id: &[u8],
        nonce: &[u8],
        shape: &[usize],
    ) -> Result<Tensor> {
        // Context: t || session_id || nonce
        let context = [
            step.to_be_bytes().as_slice(),
            session_id,
            nonce,
        ].concat();
        
        // HKDF-Expand to generate mask stream
        let mask_bytes = self.hkdf.expand(&context, shape.iter().product() * 4)?;
        
        // Reshape to tensor shape
        Tensor::from_bytes(mask_bytes, shape)
    }
}
```

### Verification Vector Computation

```rust
pub fn compute_verification_vector(
    masked_state: &Tensor,
    trace_token: &[u8; 32],
) -> Result<[u8; 32]> {
    // Serialize masked state in big-endian format
    let state_bytes = masked_state.to_bytes_be()?;
    
    // Concatenate: E_t || τ
    let input = [state_bytes.as_slice(), trace_token.as_slice()].concat();
    
    // Hash: v_t = H(E_t || τ)
    Ok(blake3::hash(&input).into())
}
```

### Transcript Hash Construction

```rust
pub fn construct_transcript_hash(
    verification_vector: &[u8; 32],
    step: u64,
    session_id: &[u8],
    nonce: &[u8],
) -> [u8; 32] {
    // Concatenate: v_t || t || session_id || nonce
    let input = [
        verification_vector.as_slice(),
        &step.to_be_bytes(),
        session_id,
        nonce,
    ].concat();
    
    // Hash: T_t = H(v_t || t || session_id || nonce)
    blake3::hash(&input).into()
}
```

---

## Dependencies and Prerequisites

### External Dependencies

- **HKDF**: `hkdf` crate (already in use)
- **SHA-256**: `sha2` crate (already in use)
- **BLAKE3**: `blake3` crate (already in use)
- **AEAD**: `aes-gcm` or `chacha20poly1305` (already available)
- **Quantization**: `adapteros-numerics` (existing crate)

### Internal Dependencies

- **Deterministic Executor**: `adapteros-deterministic-exec` (existing)
- **Crypto Providers**: `adapteros-crypto` (existing)
- **LoRA Infrastructure**: `adapteros-lora` (existing)
- **Policy Engine**: `adapteros-policy` (existing)
- **Evidence Tracking**: `adapteros-policy::evidence_tracker` (existing)

### Prerequisites

- Global seed management working
- Session management working
- LoRA adapter boundaries identified
- Evidence tracking infrastructure ready

---

## Risk Assessment

### High Risk

1. **Performance Impact**: Masking operations may add latency to inference
   - **Mitigation**: Benchmark early, optimize hot paths, consider GPU acceleration
   - **Acceptance**: < 5% latency overhead

2. **Numerical Stability**: Multiplicative masking may cause overflow/underflow
   - **Mitigation**: Clipping bounds (`δ=1e-3`), extensive testing
   - **Acceptance**: Error bounds `||x_t - recover(E_t, r_t)|| < 1e-6`

3. **Key Management**: Master key rotation and session revocation complexity
   - **Mitigation**: Use existing `KeyProvider` abstraction, incremental rollout
   - **Acceptance**: Key rotation preserves active sessions

### Medium Risk

1. **Cross-Platform Determinism**: Big-endian serialization may differ across platforms
   - **Mitigation**: Explicit big-endian serialization, cross-platform testing
   - **Acceptance**: Identical `v_t` vectors across platforms

2. **Verification Overhead**: Transcript comparison may be expensive for large sessions
   - **Mitigation**: Batch verification, incremental comparison
   - **Acceptance**: Verification < 10ms per session

### Low Risk

1. **Documentation Completeness**: Complex system requires comprehensive docs
   - **Mitigation**: Incremental documentation, code examples
   - **Acceptance**: All APIs documented with examples

---

## Success Metrics

### Functional Metrics

- [ ] Masking applied at all adapter boundaries
- [ ] Verification vectors computed correctly
- [ ] Transcript hashes constructed and logged
- [ ] Producer-consumer protocol working
- [ ] Verifier service detects divergence

### Performance Metrics

- [ ] Masking overhead < 5% of inference latency
- [ ] Verification vector computation < 1ms per boundary
- [ ] Transcript hashing < 100μs per step
- [ ] Memory overhead < 10% for mask storage

### Security Metrics

- [ ] Zero information leakage: verifier cannot recover `E_t` or phrase
- [ ] Replay resistance: nonce uniqueness prevents replay attacks
- [ ] Audit trail integrity: transcript log append-only and verifiable

---

## Timeline Summary

| Phase | Duration | Key Deliverables |
|-------|----------|-----------------|
| Phase 1: Core Masking | Weeks 1-3 | Master key management, mask generation, masking operations |
| Phase 2: Boundary Integration | Weeks 4-6 | Boundary hooks, feature permutation |
| Phase 3: Verification Infrastructure | Weeks 7-9 | Trace tokens, verification vectors, transcript hashes |
| Phase 4: Producer-Consumer Protocol | Weeks 10-12 | AEAD integration, session/nonce management, API |
| Phase 5: Verifier Service | Weeks 13-15 | Verifier service, audit logging, privacy analysis |
| Phase 6: Testing and Validation | Weeks 16-18 | Unit tests, integration tests, performance tests |
| Phase 7: Documentation and Deployment | Weeks 19-20 | Documentation, production deployment |

**Total Duration**: 20 weeks (5 months)

---

## Next Steps

1. **Review and Approve Plan**: Stakeholder review of implementation plan
2. **Set Up Development Environment**: Ensure all dependencies available
3. **Create Feature Branch**: `feature/phrase-continuity` branch for development
4. **Begin Phase 1**: Start with master key management implementation
5. **Weekly Progress Reviews**: Track progress against milestones

---

## References

- [Determinism Audit](determinism-audit.md) - Current determinism infrastructure
- [Determinism Attestation](determinism-attestation.md) - Kernel determinism guarantees
- [Policy Enforcement](POLICY_ENFORCEMENT.md) - Policy pack enforcement
- [Crypto Providers](../crates/adapteros-crypto/) - Cryptographic infrastructure
- [LoRA Infrastructure](../crates/adapteros-lora/) - LoRA adapter implementation

---

## Integration with Recent Enhancements

**Note**: The phrase continuity infrastructure integrates seamlessly with the recent service supervisor enhancements. The supervisor's health checks and metrics collection can monitor masking overhead and verification latency. Transcript hashes can be included in service telemetry for comprehensive audit trails.

**Document Status**: Reviewed and Updated  
**Last Updated**: 2025-11-13  
**Next Review**: 2025-12-13

