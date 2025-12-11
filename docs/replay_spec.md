## Replay Harness and Verifier

- **Purpose:** replay a recorded inference from artifacts only (no network) and prove determinism via context digest + receipt.
- **Commands:** `aosctl trace export --request <id> --out <dir>` then `aosctl replay --dir <dir> --verify`.
- **Artifacts:** `context_manifest.json` (base/adapters, request/plan ids, worker id, allow_cross_worker), `token_trace.json` (seed + per-step input_id/output_id/gate_q15/adapter_id), `input_tokens.json` (prompt tokens). `trace export` writes `expected_report.json` with digests plus `replay_report.json` on demand.
- **Digest rules:** context_digest = BLAKE3(canonical JSON of base_model.id/hash + adapters sorted by id). receipt = BLAKE3("aos-replay-v1" || context_digest || input_tokens (u32 LE) || for each step: step/index/input_id/output_id/gate_q15/adapter_id bytes). Output tokens are recomputed from trace steps.
- **Verification:** `aosctl replay --verify` recomputes context_digest and receipt, compares to `expected_report.json`, enforces worker match unless `allow_cross_worker=true`, and writes `replay_report.json` summarizing pass/fail plus reasons.
- **Tamper checks:** adapter hash change ⇒ context digest mismatch; gate modification or token edit ⇒ receipt mismatch; worker swap without allow flag ⇒ worker_mismatch; output token edits ⇒ output_tokens_mismatch. Reports stay usable offline for CI fixtures.
- **Fixtures:** `test_data/replay_fixtures/basic` (happy path) and `test_data/replay_fixtures/cross_worker` (allows worker mismatch). `trace export` copies fixtures into a temp dir and embeds expectations so later edits trigger failures.
- **Acceptance cues:** Export then replay → pass; tweak a gate → receipt mismatch; tweak adapter hash → context digest mismatch; cross-worker fixture → pass (or explicit worker_mismatch if allow flag removed).

MLNavigator Inc Thursday Dec 11, 2025.
