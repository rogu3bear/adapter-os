# AdapterOS Architecture

## System Overview

AdapterOS is a multi-tenant inference system with deterministic execution guarantees and evidence-grounded outputs.

## Core Components

### 1. Inference Pipeline

```
Request → API → Worker → Router → Kernels → Response
                   ↓
                  RAG
```

### 2. Policy Engine

Enforces 20 policy packs covering:
- Network egress
- Determinism
- Evidence requirements
- Memory management
- Security isolation

### 3. Artifact Store

Content-addressed storage with:
- BLAKE3 hashing
- Ed25519 signatures
- SPDX SBOM validation

### 4. Registry

SQLite database tracking:
- Adapters
- Tenants
- Checkpoints
- ACLs

See [Database Schema](database-schema/README.md) for complete schema documentation and workflow animations.

### 5. Telemetry

Event system with:
- Canonical JSON (JCS)
- BLAKE3 event hashing
- Merkle-tree bundle signing
- Replay for determinism
- System metrics collection
- Policy violation tracking

## Data Flow

1. **Request arrives** via UDS socket
2. **Policy check** validates tenant and requirements
3. **Evidence retrieval** if required
4. **Router selects** K adapters with Q15 gates
5. **Kernels execute** fused attention + MLP
6. **Policy validates** output (units, confidence)
7. **Response emitted** with trace
8. **Telemetry logged** with sampling

## Isolation Model

- Process per tenant (unique UID/GID)
- Capability-scoped filesystem
- UDS-only communication
- No shared memory
- PF-enforced network isolation

## Determinism Guarantees

- Precompiled Metal kernels
- HKDF seed derivation
- Canonical JSON serialization
- Deterministic retrieval ordering
- Fixed compiler flags

## Security Boundaries

1. **Network**: Zero egress, UDS only
2. **Filesystem**: Cap-std handles
3. **Process**: Per-tenant isolation
4. **Keys**: Secure Enclave (macOS)
5. **Artifacts**: Signature verification

## Performance Optimizations

- Fused kernels (attention + MLP + LoRA)
- Triple-buffered Metal command queues
- Packed SoA adapter layout
- Q15 quantized routing
- KV cache slab allocator
- Real-time system monitoring
- Policy-driven resource management
