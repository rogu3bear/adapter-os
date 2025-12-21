## Offline Replay Harness

Goal: prove determinism and evidence without any network calls. The harness replays a sealed evidence bundle, regenerates routing/output, recomputes the receipt, and emits a signed-style report that records any mismatch reason.

### Inputs (evidence bundle)
- `context_manifest.json` — base model id/hash plus adapter ids/hashes and worker id flag
- `token_trace.json` — per-step `{step,input_id,output_id,gate_q15,adapter_id}` trace
- `input_tokens.json` — original prompt token ids
- `expected_report.json` — expected context digest, receipt, and output tokens (golden)

All files must exist; missing artifacts cause the command to fail closed before replay.

### Workflow (air-gapped)
1. Export or collect artifacts into a directory (see golden fixtures in `test_data/replay_fixtures`).
2. Run `aosctl replay --dir <bundle_dir> --verify [--report <path>]`.
   - Reads only local files; no network calls are performed.
   - Recomputes context digest (sorted adapters, BLAKE3), receipt (token stream + gates), and expected outputs.
3. Writes `replay_report.json` (or the supplied `--report` path) and exits non-zero when `--verify` is set and any mismatch is found.

### Report semantics
- Reasons: `metadata_mismatch:*`, `context_digest_mismatch`, `receipt_mismatch`, `output_tokens_mismatch`, `worker_mismatch`.
- Fields: computed vs expected digests, output tokens, and worker check; `status` is `pass` only when all checks agree.

### Acceptance checks
- Export then replay passes: `aosctl trace export ...` then `aosctl replay --dir ... --verify`.
- Mutate one `gate_q15` in `token_trace.json` → receipt mismatch and non-zero exit.
- Mutate an adapter hash in `context_manifest.json` → context digest mismatch and non-zero exit.

MLNavigator Inc Dec 11, 2025.
