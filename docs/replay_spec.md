# Replay Harness and Verifier

**Purpose:** Replay a recorded inference from artifacts only (no network) and prove determinism via context digest + receipt.

---

## Overview

```mermaid
flowchart LR
    subgraph "Original Inference"
        O1["Request executed"]
        O2["Telemetry captured"]
        O3["Receipt generated"]
    end
    
    subgraph "Export"
        E1["aosctl trace export"]
        E2["Artifacts written"]
    end
    
    subgraph "Replay"
        R1["aosctl replay --verify"]
        R2["Digests recomputed"]
        R3["Compared to expected"]
    end
    
    subgraph "Result"
        V1{"Match?"}
        V2["PASS: Determinism proven"]
        V3["FAIL: Reason codes"]
    end
    
    O1 --> O2 --> O3 --> E1 --> E2 --> R1 --> R2 --> R3 --> V1
    V1 -->|Yes| V2
    V1 -->|No| V3
```

---

## Commands

```bash
# Export trace artifacts
aosctl trace export --request <id> --out <dir>

# Replay and verify
aosctl replay --dir <dir> --verify
```

---

## Artifacts

| File | Contents |
|------|----------|
| `context_manifest.json` | Base model, adapters, request/plan IDs, worker ID, `allow_cross_worker` |
| `token_trace.json` | Seed + per-step: input_id, output_id, gate_q15, adapter_id |
| `input_tokens.json` | Prompt tokens (array of u32) |
| `expected_report.json` | Expected digests (written by export) |
| `replay_report.json` | Verification results (written by replay) |

---

## Receipt Generation

```mermaid
flowchart TD
    subgraph "Building the Receipt Digest"
        subgraph "Context Digest"
            C1["Base model ID + hash"]
            C2["Adapters sorted by ID"]
            C3["Canonical JSON"]
            C4["BLAKE3 hash"]
            C5["context_digest"]
        end
        
        subgraph "Per-Token Chain"
            T1["For each generation step:"]
            T2["step index"]
            T3["input_id (u32 LE)"]
            T4["output_id (u32 LE)"]
            T5["gate_q15 values"]
            T6["adapter_id"]
            T7["Concatenate all bytes"]
        end
        
        subgraph "Final Receipt"
            R1["'aos-replay-v1' prefix"]
            R2["+ context_digest"]
            R3["+ input_tokens bytes"]
            R4["+ per-step bytes"]
            R5["BLAKE3 hash"]
            R6["receipt_digest"]
        end
    end
    
    C1 --> C3
    C2 --> C3
    C3 --> C4 --> C5
    
    T1 --> T2 --> T3 --> T4 --> T5 --> T6 --> T7
    
    C5 --> R2
    T7 --> R4
    R1 --> R2 --> R3 --> R4 --> R5 --> R6
```

---

## Verification Flow

```mermaid
flowchart TD
    subgraph "Verification Process"
        V1["Load artifacts from directory"]
        V2["Recompute context_digest"]
        V3["Recompute receipt from trace"]
        V4["Load expected_report.json"]
        
        subgraph "Checks"
            C1{"context_digest match?"}
            C2{"receipt match?"}
            C3{"worker match?"}
            C4{"output_tokens match?"}
        end
        
        V5["Write replay_report.json"]
    end
    
    V1 --> V2 --> V3 --> V4
    V4 --> C1
    C1 -->|No| V5
    C1 -->|Yes| C2
    C2 -->|No| V5
    C2 -->|Yes| C3
    C3 -->|No, flag not set| V5
    C3 -->|Yes or flag set| C4
    C4 --> V5
```

---

## Tamper Detection

| Tampering | Detection | Reason Code |
|-----------|-----------|-------------|
| Adapter hash changed | context_digest mismatch | `CONTEXT_MISMATCH` |
| Gate value modified | receipt mismatch | `RECEIPT_MISMATCH` |
| Token edited | receipt mismatch | `RECEIPT_MISMATCH` |
| Worker swapped (flag=false) | worker check fails | `WORKER_MISMATCH` |
| Output tokens edited | output comparison fails | `OUTPUT_TOKENS_MISMATCH` |

---

## Offline Verification

Reports are fully usable offline:
- No network calls required
- CI fixtures can embed expectations
- Verification is pure computation (BLAKE3 hashing)

---

## Test Fixtures

| Fixture | Purpose |
|---------|---------|
| `test_data/replay_fixtures/basic` | Happy path verification |
| `test_data/replay_fixtures/cross_worker` | Cross-worker replay allowed |

---

## Acceptance Criteria

| Action | Expected Result |
|--------|-----------------|
| Export then replay | PASS |
| Tweak a gate value | FAIL: receipt mismatch |
| Tweak adapter hash | FAIL: context digest mismatch |
| Cross-worker with flag | PASS |
| Cross-worker without flag | FAIL: worker_mismatch |

---

**See also:** [DETERMINISM.md](DETERMINISM.md) for full determinism guarantees.

MLNavigator Inc 2025-12-18.
