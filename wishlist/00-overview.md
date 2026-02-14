# Self-Writing AdapterOS: Master Plan

## Objective

Make AdapterOS capable of training a LoRA adapter on its own codebase, then using
that adapter to contribute to its own development. This is a bootstrap problem:
the system must understand itself well enough to improve itself.

## Current State (Verified)

| Component | Status | Gap |
|-----------|--------|-----|
| Tree-sitter AST (Rust) | Working | Extracts symbols, not function bodies with context |
| CodeGraph → QA pairs | Working | QA about code, not code generation pairs |
| Training engine (MLX FFI) | Working | No blockers |
| Loader token limits | Broken | Hard-coded 256/128 tokens, needs 2048/1024 |
| train-from-code CLI | Gated | PLAN_4 returns error in validate() |
| Codebase ingestion | Working | Generates comprehension pairs, not generation pairs |
| Adapter routing | Working | Already has "codebase" adapter type |
| RAG system | Working | Not wired to generation pipeline |
| FIM inference | Missing | No fill-in-the-middle support |
| Eval harness | Working | Recall@K/nDCG, needs compile-gate for code |
| Self-training loop | Missing | The recursive step doesn't exist |

## Phases

| Phase | Title | Hours | Dependencies |
|-------|-------|-------|-------------|
| 1 | Unblock the Token Bottleneck | 40 | None |
| 2 | Code-Aware Training Data Generation | 120 | Phase 1 |
| 3 | Ungate and Rewire train-from-code | 60 | Phase 2 |
| 4 | Compilation-Gated Eval Harness | 80 | Phase 3 |
| 5 | FIM Training and Inference | 160 | Phase 3 |
| 6 | RAG-Grounded Code Generation | 120 | Phase 4, Phase 5 |
| 7 | The Bootstrap Loop | 200 | Phase 6 |
| 8 | Quality Ratchet and Regression Guard | 120 | Phase 7 |

Total: ~900 engineering hours

## Principles

1. **Reuse, don't reinvent.** The tree-sitter parser, RAG system, eval harness,
   and training pipeline exist. Wire them together.
2. **Taste over volume.** Better to train on 5,000 high-quality function-level
   pairs than 50,000 garbage QA pairs.
3. **Compile-gate everything.** If generated code doesn't compile, it's not
   training data. If a trained adapter produces code that doesn't compile,
   it doesn't get promoted.
4. **Determinism is non-negotiable.** Every training run must be reproducible.
   The existing HKDF-SHA256 seed derivation and BLAKE3 hashing infrastructure
   serves this.
5. **The adapter must earn its keep.** Quality metrics must improve monotonically
   or the adapter is reverted. No "it should be better" — prove it.

## Architecture Sketch

```
┌─────────────────────────────────────────────────────────┐
│                    Self-Training Loop                     │
│                                                          │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐            │
│  │ CodeGraph │──>│ DataGen  │──>│ Training │            │
│  │ (existing)│   │ (Phase 2)│   │ (existing)│            │
│  └──────────┘   └──────────┘   └──────────┘            │
│       │                              │                   │
│       │         ┌──────────┐         │                   │
│       └────────>│ RAG Index│<────────┘                   │
│                 │ (existing)│                             │
│                 └──────────┘                             │
│                      │                                   │
│                 ┌──────────┐   ┌──────────┐             │
│                 │ Inference│──>│ Eval Gate│             │
│                 │ + FIM    │   │ (Phase 4)│             │
│                 │ (Phase 5)│   └──────────┘             │
│                 └──────────┘        │                    │
│                                     │                    │
│                              ┌──────────┐               │
│                              │ Promote / │               │
│                              │ Revert    │               │
│                              │ (Phase 7) │               │
│                              └──────────┘               │
└─────────────────────────────────────────────────────────┘
```

## Files Modified Per Phase

Phase 1: `crates/adapteros-lora-worker/src/training/loader.rs`, `limits.rs`
Phase 2: `crates/adapteros-orchestrator/src/codebase_ingestion.rs`, new `code_training_gen.rs`
Phase 3: `crates/adapteros-cli/src/commands/train_from_code.rs`
Phase 4: New `crates/adapteros-eval/` or extend `adapteros-retrieval/src/eval.rs`
Phase 5: `crates/adapteros-lora-worker/src/inference_pipeline.rs`, training loader
Phase 6: Wire `adapteros-retrieval` RAG to inference pipeline
Phase 7: New `crates/adapteros-self-train/` or extend orchestrator
Phase 8: Extend eval harness with regression detection
