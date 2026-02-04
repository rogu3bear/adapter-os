# Dataset Test Suite (Comprehensive)

This document defines the **comprehensive dataset test suite** for AdapterOS. It has two layers:

1. **Tier 1: Structural + schema + integrity** (hard-fail)
2. **Tier 2: Safety/PII/secrets** (soft-fail with explicit status + blocking thresholds)
3. **Determinism checks** (hard-fail on hash mismatch)

The goal is deterministic, repeatable validation that is safe for training and audit.

---

## JSONL Schema (Multi-Modal Canonical)

The preferred JSONL record schema is:

```json
{
  "id": "row-uuid-or-stable-id",
  "prompt": "string",
  "response": "string",
  "metadata": {
    "split": "train|val|test",
    "source": "optional string",
    "tool_calls": [
      { "name": "tool_name", "arguments": { "key": "value" } }
    ]
  },
  "assets": [
    {
      "file_name": "image-001.png",
      "mime_type": "image/png",
      "size_bytes": 12345,
      "hash_b3": "b3:..."
    }
  ]
}
```

### Field Requirements

- `id` (or `row_id`): **required**, string, non-empty.
- `prompt`: **required**, string, non-empty.
- `response`: **required**, string, non-empty.
- `metadata`: **required**, JSON object.
- `assets`: **optional**, array of asset objects (see below).

### Asset Validation (if present)

Each asset entry must include:

- `file_name` **or** `path` (string)
- Optional `mime_type`, `size_bytes`, `hash_b3`

Asset references must exist in `dataset_files`, and if `hash_b3` is provided it must match the stored file hash.

---

## Legacy Schemas (Accepted for Compatibility)

- **Supervised legacy**: `{"prompt": "...", "completion": "..."}` (no extra fields)
- **Raw text legacy**: `{"text": "..."}` (no extra fields)

Legacy schemas remain accepted to avoid breaking existing datasets, but the canonical schema is preferred.

---

## Tier 1: Structural + Schema + Integrity (Hard-Fail)

### Structural

- Files exist, non-empty, correct extension.
- UTF-8 valid (BOM allowed but warned).
- Line length limits enforced.

### Schema

- Required fields present and correct types.
- Optional tool call schema in `metadata.tool_calls` validated.
- Assets array shape validated.

### Integrity

- File hashes match `hash_b3`.
- Dataset manifest hash matches computed hash.
- Exact duplicate rows detected by content hash (prompt+response).
- Cross-split leakage detected when identical content appears in different splits.

---

## Tier 2: Safety/PII/Secrets (Soft-Fail)

Safety scan heuristics run asynchronously and record signal statuses:

- **PII**: email, phone, SSN, credit card patterns
- **Secrets**: API keys, tokens, credentials
- **Toxicity**: hate/violence indicators
- **Anomaly**: duplicates, oversized text, parse errors

### Blocking Thresholds

The following trigger **blocking** status:

- Any detected secret marker
- Any detected SSN/credit-card pattern
- Excessive PII warnings (>= 10)
- Excessive toxicity warnings (>= 5)

These thresholds are enforced in the Tier‑2 scan and surfaced in safety check results.

---

## Determinism Checks (Hard-Fail)

- Dataset manifest hash (`hash_b3` / `dataset_hash_b3`) must match recomputed hash.
- Split determinism is verified in training pipeline (split hash stable across runs).

---

## Audit Suite Usage

To run the mixed-task evaluation corpus:

```bash
aosctl audit <cpid> --suite tests/corpora/mixed_v1.json
```

This uses the existing audit job pipeline and reports determinism + quality metrics.
