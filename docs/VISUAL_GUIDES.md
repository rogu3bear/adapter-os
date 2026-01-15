# Visual Guides: Understanding adapterOS

**Purpose:** Visual explanations of adapterOS concepts, comparisons with typical LLM systems, and token flow diagrams.

**Last Updated:** 2025-12-18

---

## Table of Contents

1. [Typical LLM vs adapterOS](#typical-llm-vs-adapteros)
2. [Token Flow Through a Conversation](#token-flow-through-a-conversation)
3. [KV Cache Comparison](#kv-cache-comparison)
4. [The Determinism Guarantee](#the-determinism-guarantee)
5. [Receipt Generation and Verification](#receipt-generation-and-verification)

---

## Typical LLM vs adapterOS

### The Fundamental Difference

```mermaid
flowchart LR
    subgraph TYPICAL["Typical LLM System"]
        T1["Input → Black Box → Output"]
        T2["What happened inside? Unknown"]
        T3["Was cache used? Unknown"]
        T4["Can it be reproduced? No"]
        T5["Can it be verified? No"]
        T6["Trust required"]
    end
    
    subgraph ADAPTEROS["adapterOS"]
        A1["Input → Recorded Process → Output"]
        A2["Every decision captured"]
        A3["Cache usage proven in receipt"]
        A4["Reproducible: bit-exact replay"]
        A5["Verifiable: by mathematics"]
        A6["Verify, don't trust"]
    end
    
    T1 --> T2 --> T3 --> T4 --> T5 --> T6
    A1 --> A2 --> A3 --> A4 --> A5 --> A6
```

### Feature Comparison

| Aspect | Typical LLM | adapterOS |
|--------|-------------|-----------|
| **Determinism** | Same input → different output | Same input → identical output |
| **Routing** | Hidden, variable | Quantized (Q15), recorded |
| **Cache usage** | May exist, unproven | Proven in receipt |
| **Token accounting** | Trust the count | Cryptographically verified |
| **Stop reason** | Often unstated | Enumerated, committed |
| **Receipt** | Log files (mutable) | Signed digest (immutable) |
| **Verification** | Requires provider access | Anyone, anywhere, offline |
| **Replay** | Approximate at best | Bit-exact reproduction |
| **Dispute resolution** | He-said-she-said | Mathematical proof |
| **Network dependency** | Required | Zero (UDS only) |

---

## Token Flow Through a Conversation

### Multi-Turn Conversation Example

```mermaid
flowchart TD
    subgraph "Turn 1: First Question"
        T1A["User: 'What is machine learning?'<br/>8 tokens"]
        T1B["System prompt: 45 tokens"]
        T1C["Total context: 53 tokens"]
        T1D["Cache: 0 (first message)"]
        T1E["Computed: 53 tokens"]
        T1F["Response: 95 tokens"]
        T1G["Billed: 148 tokens"]
    end
    
    subgraph "Turn 2: Follow-up"
        T2A["User: 'Give me an example'<br/>5 tokens"]
        T2B["Context grows: 148 + 5 = 153"]
        T2C["Cache hit: 148 tokens"]
        T2D["Computed: only 5 tokens"]
        T2E["Response: 78 tokens"]
        T2F["Billed: 83 tokens"]
    end
    
    subgraph "Turn 3: Deeper Question"
        T3A["User: 'Explain backpropagation'<br/>4 tokens"]
        T3B["Context: 231 + 4 = 235"]
        T3C["Cache hit: 231 tokens"]
        T3D["Computed: only 4 tokens"]
        T3E["Response: 112 tokens"]
        T3F["Billed: 116 tokens"]
    end
    
    T1A --> T1B --> T1C --> T1D --> T1E --> T1F --> T1G
    T1G --> T2A --> T2B --> T2C --> T2D --> T2E --> T2F
    T2F --> T3A --> T3B --> T3C --> T3D --> T3E --> T3F
```

### Token Accounting Summary

| Turn | Logical In | Cached | Computed | Output | Billed |
|------|------------|--------|----------|--------|--------|
| 1 | 53 | 0 | 53 | 95 | 148 |
| 2 | 153 | 148 | 5 | 78 | 83 |
| 3 | 235 | 231 | 4 | 112 | 116 |
| **Total** | — | **379** | **62** | **285** | **347** |

**Without cache credits:** 441 + 285 = 726 tokens billed  
**With adapterOS:** 347 tokens billed  
**Savings:** 52%

### Visual Token Timeline

```mermaid
flowchart LR
    subgraph "Token Usage Over Time"
        subgraph "Turn 1"
            T1["████████ 53 in<br/>████████████████████ 95 out<br/>Billed: 148"]
        end
        
        subgraph "Turn 2"
            T2["░░░░░░░░░░░░░░░░░░░░░░░░ 148 cached<br/>█ 5 in<br/>████████████████ 78 out<br/>Billed: 83"]
        end
        
        subgraph "Turn 3"
            T3["░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 231 cached<br/>█ 4 in<br/>██████████████████████████ 112 out<br/>Billed: 116"]
        end
    end
    
    T1 --> T2 --> T3
```

---

## KV Cache Comparison

### Typical LLM KV Cache

```mermaid
flowchart TD
    subgraph "Typical System"
        TK1["Cache populated during inference"]
        TK2["Follow-up arrives"]
        TK3["Is prefix in cache? Maybe"]
        TK4["No guarantee, no proof"]
        TK5["User billed for everything"]
        TK6["Adapter swap: cache may be stale"]
    end
    
    TK1 --> TK2 --> TK3 --> TK4 --> TK5 --> TK6
```

### adapterOS KV Cache

```mermaid
flowchart TD
    subgraph "adapterOS System"
        AK1["Cache keyed by BLAKE3(prefix)"]
        AK2["Follow-up arrives"]
        AK3["Prefix lookup: deterministic"]
        AK4["Cache hit proven in receipt"]
        AK5["User credited for cached tokens"]
        AK6["Adapter swap: coherence check"]
        AK7["Generation mismatch → cache cleared"]
    end
    
    AK1 --> AK2 --> AK3 --> AK4 --> AK5 --> AK6 --> AK7
```

### Cache Coherence on Adapter Hot-Swap

```mermaid
flowchart TD
    subgraph "Coherence Mechanism"
        C1["Adapter Stack: [A, B, C]<br/>stack_generation: 5"]
        C2["KV Cache populated<br/>cache_generation: 5"]
        C3["Hot-swap: B replaced with D<br/>stack_generation: 6"]
        C4["Next inference starts"]
        C5["ensure_cache_coherence(6)"]
        C6["cache_gen (5) ≠ stack_gen (6)"]
        C7["Cache automatically cleared"]
        C8["Fresh computation<br/>No stale data leaks"]
    end
    
    C1 --> C2 --> C3 --> C4 --> C5 --> C6 --> C7 --> C8
```

---

## The Determinism Guarantee

### How Identical Inputs Produce Identical Outputs

```mermaid
flowchart TD
    subgraph "Same Inputs"
        SI1["Same model weights"]
        SI2["Same adapters loaded"]
        SI3["Same configuration"]
        SI4["Same prompt"]
    end
    
    subgraph "Determinism Mechanisms"
        DM1["manifest_hash computed"]
        DM2["Seeds derived via HKDF"]
        DM3["Adapter scores computed"]
        DM4["Quantized to Q15 integers"]
        DM5["Tie-breaking by index"]
        DM6["Precompiled kernels execute"]
        DM7["Seeded sampling selects tokens"]
    end
    
    subgraph "Same Outputs"
        SO1["Identical response text"]
        SO2["Identical receipt_digest"]
        SO3["Verifiable proof"]
    end
    
    SI1 --> DM1
    SI2 --> DM1
    SI3 --> DM1
    SI4 --> DM1
    DM1 --> DM2 --> DM3 --> DM4 --> DM5 --> DM6 --> DM7
    DM7 --> SO1 --> SO2 --> SO3
```

### The Determinism Chain

```mermaid
flowchart LR
    M["Manifest hash<br/>(system state)"]
    S["Seeds derived<br/>(HKDF)"]
    Q["Q15 gates<br/>(no float drift)"]
    T["Tie-breaking<br/>(by index)"]
    K["Kernels verified<br/>(hash-checked)"]
    R["Recording<br/>(every decision)"]
    H["Hash chain<br/>(tamper-evident)"]
    SIG["Signature<br/>(proof of origin)"]
    
    M --> S --> Q --> T --> K --> R --> H --> SIG
```

---

## Receipt Generation and Verification

### What Goes Into a Receipt

```mermaid
flowchart TD
    subgraph "Receipt Components"
        subgraph "Context"
            C1["tenant_namespace"]
            C2["stack_hash"]
            C3["prompt_tokens"]
            C4["→ context_digest"]
        end
        
        subgraph "Decisions"
            D1["Per-token routing"]
            D2["Adapter IDs + gates"]
            D3["Chained hashes"]
            D4["→ run_head_hash"]
        end
        
        subgraph "Output"
            O1["Generated tokens"]
            O2["→ output_digest"]
        end
        
        subgraph "Accounting"
            A1["logical_prompt_tokens"]
            A2["prefix_cached_count"]
            A3["billed_input_tokens"]
            A4["logical_output_tokens"]
            A5["billed_output_tokens"]
        end
        
        subgraph "Final Receipt"
            R1["All components combined"]
            R2["BLAKE3 hash"]
            R3["→ receipt_digest"]
            R4["Ed25519 signature"]
        end
    end
    
    C1 --> C4
    C2 --> C4
    C3 --> C4
    D1 --> D4
    D2 --> D4
    D3 --> D4
    O1 --> O2
    
    C4 --> R1
    D4 --> R1
    O2 --> R1
    A1 --> R1
    A2 --> R1
    A3 --> R1
    A4 --> R1
    A5 --> R1
    R1 --> R2 --> R3 --> R4
```

### Offline Verification

```mermaid
flowchart TD
    subgraph "Verification Without Trust"
        V1["Receive receipt bundle"]
        V2["Recompute context_digest<br/>from context fields"]
        V3["Recompute run_head_hash<br/>by replaying decision chain"]
        V4["Recompute output_digest<br/>from output tokens"]
        V5["Recompute receipt_digest<br/>from all components"]
        V6["Verify Ed25519 signature"]
        
        C1{"All match?"}
        C2["VERIFIED<br/>Proof is valid"]
        C3["FAILED<br/>Tampering detected"]
    end
    
    V1 --> V2 --> V3 --> V4 --> V5 --> V6 --> C1
    C1 -->|Yes| C2
    C1 -->|No| C3
```

### Failure Reason Codes

| Code | Meaning |
|------|---------|
| `CONTEXT_MISMATCH` | Context fields don't hash to claimed digest |
| `TRACE_TAMPER` | Decision chain doesn't match run_head_hash |
| `OUTPUT_MISMATCH` | Output tokens don't match output_digest |
| `SIGNATURE_INVALID` | Ed25519 signature verification failed |
| `POLICY_MISMATCH` | Policy mask doesn't match expected |
| `BACKEND_MISMATCH` | Backend ID doesn't match expected |

---

## Related Documentation

- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System architecture and components
- **[DETERMINISM.md](DETERMINISM.md)** - Determinism guarantees and mechanisms
- **[replay_spec.md](replay_spec.md)** - Replay harness and verification
- **[SECURITY.md](SECURITY.md)** - Security model and cryptographic proofs

---

MLNavigator Inc 2025-12-18.
