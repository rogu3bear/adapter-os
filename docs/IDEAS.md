# IDEAS

**Status:** Ideas/roadmap — not implementation spec. Code is authoritative.  
**Last Updated:** 2026-02-18

This document captures concrete, system-aligned ideas for leveraging AdapterOS determinism, receipts, and caching in IDE workflows.

**Note:** Placed in `docs/` to respect repo hygiene guidelines (avoid new files in repo root).

## Goals

- Reduce input token cost and latency without sacrificing correctness.
- Preserve deterministic, reproducible behavior for IDE-generated code changes.
- Make routing and resource attribution verifiable via receipts.

## Core primitives (already in the system)

- Deterministic routing with fixed tie-breaks and Q15 quantization. [1]
- Cryptographic receipts that bind inputs, routing, and outputs. [2]
- Prefix KV cache with deterministic keying and longest-prefix reuse. [3]
- Token accounting bound into receipts (`logical`, `cached`, `billed`). [4]
- Server-side deterministic chat context building. [5]
- OpenAI-compatible streaming inference for IDE clients. [6]
- Directory adapters for codebase-derived adapters. [7]

## Ideas worth building

1. Cache-aware IDE prompt templates
- Keep a stable, canonical prefix (AGENTS.md + repo metadata + formatting rules).
- Append only the delta context (diff, error output, targeted snippets).
- Track cache effectiveness via `prefix_cached_token_count` in receipts. [3] [4]

2. Cache-aware worker selection
- Prefer workers with recent prefix cache hits for the same context digest.
- Extend worker capabilities or health score to reflect cache hit rate.
- Deterministically bias selection without changing routing logic. [1] [3]

3. Deterministic IDE sessions
- Use `session_id` so the server assembles prior messages deterministically.
- Use fixed sampling params (`temperature=0`, `top_k=1`) and set `seed`.
- Pin adapter stacks to avoid changes in candidate sets. [1] [5] [6]

4. Context pack protocol
- Define a canonical “context pack” format for IDE clients.
- Deterministic ordering, truncation, and hashing of files and diagnostics.
- Include a digest in the prompt for replay verification. [5]

5. Directory-adapter bootstrap + routing policy
- Use `/v1/adapters/directory/upsert` to create a repo-specific adapter.
- Keep a narrow adapter allowlist for repo scope to reduce routing noise.
- Prefer deterministic routing mode for repeatable behavior. [1] [7]

6. Receipt-driven regression testing
- Store receipts for “golden” coding tasks.
- Replay requests and compare receipt digests to detect drift.
- Use this as an automated gate for adapter promotion. [2] [4]

7. Deterministic “review mode”
- Generate patches with strict determinism and replayable prompts.
- Compare receipts across runs to ensure same decision chain.
- Use receipts as a review artifact in PRs. [2] [5]

## Non-goals (keep scope tight)

- No new model training or backend kernels unless proven necessary.
- No changes to receipt schema unless required for verification gaps.
- No coupling of KV cache directly to adapters (KV is prompt-specific). [3]

## Suggested next experiment

- Build a stable IDE prompt template with a fixed prefix.
- Run 10 repeated requests with small suffix changes.
- Verify `prefix_cached_token_count` grows and `billed_input_tokens` shrinks.
- Confirm routing stability via consistent receipt digests. [1] [4]

## Sources

[1] `crates/adapteros-lora-router/src/router.rs`
[2] `crates/adapteros-core/src/receipt_digest.rs`
[3] `crates/adapteros-core/src/prefix_kv_key.rs`
[4] `docs/TOKEN_CACHING_ECONOMICS.md`
[5] `crates/adapteros-server-api/src/chat_context.rs`
[6] `crates/adapteros-server-api/src/handlers/streaming_infer.rs`
[7] `crates/adapteros-server-api/src/handlers/directory_adapters.rs`
