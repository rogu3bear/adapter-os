# Determinism Edge Cases Dataset

**Type:** Type 5 - Router & Kernel Stress Testing
**Version:** 1.0.0
**Category:** Synthetic Adversarial
**Samples:** 192 edge case inputs

## Purpose

This dataset contains purely synthetic adversarial inputs designed to stress test the deterministic execution guarantees of adapterOS. It exposes the router and kernel components to edge cases that might cause non-deterministic behavior in naive implementations.

## Edge Case Categories

### 1. Floating-Point Precision (27 samples)
Tests floating-point rounding and precision determinism:
- Extreme values (max/min float64, inf, nan)
- Precision ambiguities (0.1 + 0.2)
- Rounding edge cases
- Repeated floating-point expressions

**Example:**
```json
{"input": "0.1 + 0.2", "metadata": {"case": "floating_point_precision", "subcategory": "extreme_values", "difficulty": "high"}}
```

### 2. Minimal Input (27 samples)
Extremely short inputs that stress tokenization:
- Single characters (a, A, 1, !, ?, .)
- Two characters (ab, AB, 12, !?)
- Empty strings
- Whitespace-only inputs

**Example:**
```json
{"input": "", "metadata": {"case": "whitespace_only", "subcategory": "minimal_input", "difficulty": "high"}}
```

### 3. Token Repetition (55 samples)
**CRITICAL for determinism verification:**
- Single tokens repeated (10, 50, 100, 500, 1000 times)
- Identical inputs repeated multiple times to verify identical outputs
- Each repeated input includes `input_hash` for verification

**Example:**
```json
{"input": "A A A A A...", "metadata": {"case": "token_repetition", "subcategory": "repeated_identical_inputs", "token": "A", "count": 1000, "difficulty": "critical"}}
```

### 4. Case Variation (7 samples)
Tests case sensitivity consistency:
- All lowercase
- All uppercase
- Title case
- Alternating case
- Random capitalization patterns

**Example:**
```json
{"input": "tHe QuIcK bRoWn FoX", "metadata": {"case": "capitalization_alternating", "subcategory": "case_variation", "difficulty": "high"}}
```

### 5. Punctuation Overload (22 samples)
Excessive punctuation to test tokenizer robustness:
- Repeated punctuation (!!!!!!!, ??????)
- Mixed punctuation (!?!?!?, .,.,.,.)
- Punctuation with words (Hello!!!!!!)

**Example:**
```json
{"input": "!!!!!!!!!!!!", "metadata": {"case": "punctuation_overload", "subcategory": "repeated_punctuation", "difficulty": "medium"}}
```

### 6. Unicode Characters (8 samples)
Non-ASCII characters including emoji and international scripts:
- Emoji sequences (🔥🔥🔥)
- Mathematical symbols (∞∞∞)
- International scripts (你好世界, مرحبا, שלום)

**Example:**
```json
{"input": "🔥🔥🔥🔥🔥", "metadata": {"case": "unicode_characters", "subcategory": "non_ascii", "difficulty": "high"}}
```

### 7. Control Characters (4 samples)
Non-printable characters that might break tokenization:
- Null bytes (\x00)
- Control sequences (\x01, \x1f, \x7f)

### 8. Boundary Conditions (11 samples)
Length extremes and deep nesting:
- Very long sequences (1000+ characters)
- Deep nesting (100 levels of parentheses)

**Example:**
```json
{"input": "AAAA...", "metadata": {"case": "boundary_long_sequence", "subcategory": "length_extreme", "length": 1000, "difficulty": "high"}}
```

### 9. Numeric Edge Cases (20 samples)
Integer boundaries and numeric repetition:
- Max/min int32, int64
- Leading zeros
- Repeated numbers

### 10. Tokenizer Stress (19 samples)
Mixed alphanumeric patterns and whitespace variations:
- Mixed alphanumeric (a1b2c3d4e5)
- Repeating patterns (abcabc...)
- Whitespace variations (\t, \n, \r\n)

## Determinism Verification

**Critical Subcategories:**
- `repeated_identical_inputs` - Same input appears multiple times
- `determinism_verification` - Each includes `input_hash` and `repetition_index`

**Expected Behavior:**
Each input with `repeated_identical_input` metadata should produce:
1. Identical router gate activations across multiple runs
2. Identical kernel outputs
3. Zero divergence in tick ledger entries

**Verification Method:**
```bash
# Run inference with same input multiple times
for i in {1..5}; do
    aosctl infer --input "Hello world" --adapter-stack test
done

# Check tick ledger for consistency
sqlite3 var/aos-cp.sqlite3 "
SELECT task_id, entry_hash, COUNT(*)
FROM tick_ledger_entries
WHERE task_id LIKE '%Hello world%'
GROUP BY entry_hash
"
# Should return single entry_hash (all identical)
```

## Usage

### Training
```bash
# Create dataset in adapterOS
aosctl training dataset create \
  --name "determinism-edge-cases-v1" \
  --manifest training/datasets/determinism_edge_cases/manifest.json

# Train adapter
aosctl training start \
  --dataset-id <dataset-id> \
  --rank 4 \
  --alpha 8.0 \
  --adapter-name "system/determinism/stress-test/r001"
```

### Testing Router Determinism
```python
from adapteros_lora_router import Router
import json

# Load edge cases
with open("determinism-edge-cases.jsonl") as f:
    cases = [json.loads(line) for line in f]

# Test repeated inputs
repeated = [c for c in cases if c["metadata"]["case"] == "repeated_identical_input"]
for inp_hash in set(c["metadata"]["input_hash"] for c in repeated):
    matching = [c for c in repeated if c["metadata"]["input_hash"] == inp_hash]

    # All should produce identical gate activations
    results = [router.forward(c["input"]) for c in matching]
    assert all(r == results[0] for r in results), f"Non-deterministic routing for {inp_hash}"
```

## Evaluation Gates

1. ✅ Identical inputs produce identical router decisions 100% of time
2. ✅ Floating-point operations are deterministic across runs
3. ✅ Short inputs (0-2 chars) handle gracefully
4. ✅ Repeated token sequences maintain determinism
5. ✅ Unicode and special characters processed consistently

## Files

- `generate_edge_cases.py` - Dataset generation script
- `determinism-edge-cases.jsonl` - 192 edge case samples (JSONL format)
- `manifest.json` - Dataset manifest with metadata
- `README.md` - This documentation

## Statistics

```
Total samples: 192

Breakdown by category:
  token_repetition: 30
  repeated_identical_input: 25
  punctuation_overload: 12
  numeric_repetition: 12
  single_character: 10
  whitespace_only: 10
  floating_point_precision: 9
  floating_point_repetition: 9
  unicode_characters: 8
  integer_boundaries: 8
  mixed_alphanumeric: 8
  two_characters: 7
  repeating_pattern: 6
  floating_point_ambiguity: 5
  punctuation_mixed: 5
  punctuation_with_words: 5
  whitespace_variations: 5
  control_characters: 4
  boundary_long_sequence: 4
  capitalization_random: 3
  boundary_nested_structures: 3
  capitalization_lowercase: 1
  capitalization_uppercase: 1
  capitalization_titlecase: 1
  capitalization_alternating: 1
```

## Integration with AGENTS.md

This dataset directly tests the patterns documented in `/home/user/adapter-os/AGENTS.md`:

### Deterministic Executor Seeding
From `AGENTS.md#deterministic-executor-seeding`:
```rust
let global_seed = derive_seed(&manifest_hash, "executor");
```

The edge cases verify that:
- Identical manifest → Identical seeds → Identical execution
- Router uses seeded RNG for tie-breaking
- No `rand::thread_rng()` contamination

### Global Tick Ledger
From `AGENTS.md#global-tick-ledger-issue-c-6-fix`:
```rust
let entry_hash = ledger.record_tick(task_id, &event).await?;
```

The repeated identical inputs verify:
- No duplicate tick assignment under concurrent execution
- Merkle chain integrity preserved
- Atomic `fetch_add` eliminates race conditions

### Multi-Agent Coordination
From `AGENTS.md#multi-agent-coordination-dead-agent-handling-issue-c-8`:
```rust
barrier.wait("agent_a", tick).await?;
```

The edge cases stress test:
- CAS race conditions with concurrent barrier arrivals
- Deterministic barrier advancement
- No busy-wait CPU consumption

## Related Documentation

- [AGENTS.md](../../../AGENTS.md) - adapterOS developer guide
- [PRD-04](../../../docs/architecture/PRD-04-lifecycle-versioning.md) - Lifecycle versioning
- [MasterPlan](../../../docs/architecture/MasterPlan.md) - Deterministic execution engine

## Provenance

**Created by:** determinism-edge-cases-dataset-generator
**Created at:** 2025-11-18T00:00:00Z
**Last reviewed:** 2025-11-18T00:00:00Z

**Review notes:** Comprehensive adversarial dataset for stress testing deterministic router decisions and kernel execution under edge case conditions.

## License

© 2025 JKCA / James KC Auchterlonie. All rights reserved.
