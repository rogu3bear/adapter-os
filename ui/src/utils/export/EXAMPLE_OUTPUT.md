# Example Export Output

This file demonstrates what the exported Markdown looks like for a chat session with citations.

---

# Chat Session: Product Research Discussion

## Metadata
- **Export Date**: 2025-12-12T10:30:45.123Z
- **Session ID**: session-abc123-xyz789
- **Export ID**: export-def456-uvw012

## Conversation

### **You** (12/12/2025, 10:15:23 AM)

What are the key features of the new AdapterOS system?

### **Assistant** (12/12/2025, 10:15:28 AM)

Based on the documentation, AdapterOS provides several key features for ML inference:

1. **Multi-LoRA Orchestration**: The system provides efficient adapter management for LoRA fine-tuning
2. **Offline-Capable**: Runs entirely on-device with no network egress during inference
3. **Multi-Backend Support**: Works with CoreML/ANE, Metal, and MLX backends
4. **Deterministic Replay**: All inferences can be exactly reproduced for audit purposes

**Sources:**
- Technical Overview.pdf (p.5) [95.0% relevance]
  > "AdapterOS is an ML inference platform with an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems on Apple Silicon."
- Architecture Guide.pdf (p.12) [87.3% relevance]
  > "The system supports CoreML/ANE as the primary backend, with Metal and MLX as alternatives for different use cases."

**Adapters Used:**
- Documentation Adapter (weight: 85.0%)
- Technical Summary Adapter (weight: 62.0%)

*Verified at 12/12/2025, 10:16:00 AM*

### **You** (12/12/2025, 10:16:15 AM)

How does the router decide which adapters to use?

### **Assistant** (12/12/2025, 10:16:20 AM)

The router uses a K-sparse algorithm with Q15 quantization to select and weight adapters:

1. **Scoring**: Each adapter receives a relevance score based on the input
2. **Selection**: Top K adapters are selected (where K is configurable)
3. **Weighting**: Gate values determine how much each adapter contributes
4. **Determinism**: The process is fully deterministic using HKDF-SHA256 seed derivation

The Q15 quantization means gate values are represented as integers divided by 32767.0 (not 32768) for precision.

**Sources:**
- Router Documentation.pdf (p.8) [92.1% relevance]
  > "The K-sparse router selects the top K adapters based on relevance scores, then applies Q15-quantized gate values to determine mixing weights."
- Determinism Guide.pdf (p.15) [88.7% relevance]
  > "Seed derivation uses HKDF-SHA256 with BLAKE3 global seed to ensure reproducible routing decisions."

**Adapters Used:**
- Router Explanation Adapter (weight: 78.0%)
- Technical Details Adapter (weight: 55.0%)

### **You** (12/12/2025, 10:17:00 AM)

What about policy enforcement?

### **Assistant** (12/12/2025, 10:17:05 AM)

AdapterOS includes comprehensive policy enforcement through 25 policy packs:

**Core Policies (Always Enabled):**
- Egress Control: Prevents network access during inference
- Determinism: Ensures reproducible results
- Isolation: Maintains tenant separation
- Evidence: Tracks all decisions for audit

**Policy Hooks:**
- OnRequestBeforeRouting
- OnBeforeInference
- OnAfterInference

The system uses Merkle chain audit with BLAKE3 hash chaining and Ed25519 signatures for cryptographic verification.

**Sources:**
- Policy Framework.pdf (p.22) [94.5% relevance]
  > "The policy system provides 25 policy packs with three critical hook points for enforcement throughout the inference pipeline."
- Security Architecture.pdf (p.18) [89.2% relevance]
  > "Merkle chain audit uses BLAKE3 for hash chaining with Ed25519 signatures to provide cryptographic proof of policy compliance."

**Adapters Used:**
- Policy Expert Adapter (weight: 91.0%)
- Security Documentation Adapter (weight: 68.0%)

*Verified at 12/12/2025, 10:17:30 AM*

---

*Exported from AdapterOS on 12/12/2025, 10:30:45 AM*
