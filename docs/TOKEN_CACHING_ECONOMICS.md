# Token Caching Economics

**Status**: Documented
**Date**: 2026-01-29

How prefix caching reduces attributed tokens, why speedup is non-linear, and how adapterOS receipts make it auditable.

---

## Token Attribution Formula

When a prompt prefix is cached, you skip computation for those tokens:

```
L = logical tokens (what the context logically contains)
C = cached tokens (prefix reused)
A = attributed tokens (what you pay for)

A = L − C
```

The reduction is exactly C tokens. No compression tricks. No approximations.

| Logical Tokens | Cached Tokens | Attributed Tokens | Reduction |
|----------------|---------------|-------------------|-----------|
| 4,000 | 0 | 4,000 | 0% |
| 4,000 | 1,000 | 3,000 | 25% |
| 4,000 | 2,000 | 2,000 | 50% |
| 4,000 | 3,000 | 1,000 | 75% |

---

## What Is a "Free Token"?

A free token comes from cache reuse. The model did not recompute it, so charging for it would be fiction.

### The Three Buckets

In adapterOS, tokens fall into three buckets:

| Bucket | Definition |
|--------|------------|
| **Logical tokens** | Tokens that are part of the effective context or output |
| **Cached tokens** | Tokens whose computation already exists and is reused |
| **Attributed tokens** | Tokens you are actually charged for |

The equation is simple and ruthless:

```
attributed_tokens = logical_tokens − cached_tokens
```

### Where the "Free Token" Appears

1. You submit an input with a prefix the system has seen before
2. The KV cache already contains the intermediate representations
3. No forward pass is executed for those tokens
4. Compute cost is zero for that segment
5. Those tokens become **cached tokens**
6. Cached tokens reduce attributed tokens
7. The difference is what people casually call "free tokens"

### Why This Is Not Hand-Wavy

- Cache reuse is **deterministically detected**
- The count of cached tokens is **explicitly recorded**
- The accounting is **bound into the receipt digest**
- A third party can **verify that reuse occurred**

### Why Other Systems Get This Wrong

They meter based on input length, not execution.
They log cache hits, but do not cryptographically bind them.
They ask you to trust that "we reused stuff, pinky promise".

**adapterOS does none of that.**

If a token did not incur compute, it is not billable.
If it is not billable, the receipt proves why.

---

## Why Speedup Is Non-Linear

When you reuse a prefix, you skip:

1. **Forward pass through all transformer layers** for the prefix (the big win)
2. **KV cache generation** for those tokens
3. **Memory bandwidth** for writing those KV entries

New tokens still attend over the full context (including cached KV). The savings come from skipping *computation* of prefix representations.

### What Compounds

- Memory pressure reduction (less cache thrashing, better bandwidth utilization)
- KV cache write savings across all layers
- Forward pass compute savings: O(N × layers × hidden_dim)

### What Doesn't Change

- Attention cost for new tokens over full context (cached prefix is still attended to)
- Per-position compute cost is similar regardless of position

---

## Practical Throughput Impact

On Apple Silicon class hardware (realistic ranges):

| Cache Hit Rate | Tokens/sec | Words/sec | Why |
|----------------|------------|-----------|-----|
| No cache reuse | ~30–60 | ~6–12 | Full forward pass |
| 25% prefix cached | ~45–90 | ~9–18 | Skip 25% forward pass |
| 50% prefix cached | ~70–140 | ~14–28 | Skip 50% forward pass |
| 75% prefix cached | ~120–240 | ~24–48 | Memory benefits compound |

The curve bends upward because memory pressure reduction compounds with compute savings.

---

## Tokens vs Words

```
1 token ≈ 0.75 words (English prose)

words/sec ≈ tokens/sec × 0.75
```

- **Billing** cares about tokens
- **Users** perceive words
- Don't mix these casually in performance claims

---

## Why adapterOS Receipts Matter

Most systems:
- Show you fewer billed tokens
- Do not prove why
- Cannot prove how much compute was skipped

**adapterOS**:
- Records cached token count in the receipt
- Commits it into the cryptographic digest
- Makes the reduction auditable
- Makes performance claims verifiable

The "free token" is not a discount. **It is negative work.**

### Receipt Integration

Token caching fields are embedded in `InferenceReceiptRef`:

```rust
/// From InferenceReceiptRef in evidence_envelope.rs
pub struct InferenceReceiptRef {
    // ... other fields

    /// Total logical prompt tokens before cache reuse
    pub logical_prompt_tokens: u32,

    /// Tokens satisfied by prefix cache reuse
    pub prefix_cached_token_count: u32,

    /// Billed input tokens (logical - cached, floored at 0)
    pub billed_input_tokens: u32,
}
```

The receipt digest binds all three values. Third parties can verify:
1. How many tokens were claimed as cached
2. How many were actually computed
3. The cryptographic proof that these numbers are accurate

### Verification

```bash
# Verify a receipt and view token accounting
aosctl verify-receipt --bundle var/receipts/run-123

# The receipt JSON includes:
# - logical_prompt_tokens
# - prefix_cached_token_count
# - billed_input_tokens
# All cryptographically bound to receipt_digest
```

### Implementation References

- `crates/adapteros-core/src/evidence_envelope.rs` — `InferenceReceiptRef` with token accounting
- `crates/adapteros-core/src/receipt_digest.rs` — `ReceiptDigestInput` for digest computation
- `crates/adapteros-lora-worker/src/prefix_kv_cache.rs` — `PrefixMatch::attributed_tokens()`
- `crates/adapteros-lora-worker/src/execution.rs` — `ExecutionContext::attributed_tokens()`

---

## Nuances Worth Understanding

### "Early tokens are most expensive" — Not Quite

Each token position has similar per-token compute cost. The win is skipping *any* tokens you've already computed, regardless of position.

### "Attention scales worse than O(n)" — Partially True

Self-attention is O(n²) for full computation. But KV cache reuse doesn't remove "expensive early tokens" per se. It removes:
- Forward pass compute for the prefix
- KV generation for the prefix
- Memory writes for the prefix

New tokens still attend over full context.

### "Curve bends upward" — Memory Effects

The throughput curve shows super-linear improvement because:
- Less memory pressure → better cache utilization
- Fewer memory writes → more bandwidth for reads
- Primary benefit is still linear (skip N tokens → save O(N) compute)

---

## Related Documentation

- [**MOE_FREE_TOKEN_EXPLORATION.md**](design/MOE_FREE_TOKEN_EXPLORATION.md) — MoE-specific "free token" optimization
- [**CRYPTO_RECEIPTS.md**](CRYPTO_RECEIPTS.md) — Cryptographic receipt structure
- [**DETERMINISM.md**](DETERMINISM.md) — Deterministic execution guarantees

---

*Last updated: January 29, 2026*
