# Qwen3.5-27B Quantization Release Runbook

Release command: `aosctl models quantize-qwen35`.

## Scope

- Backend: MLX-only.
- Primary profile: `int4-g64`.
- Fallback profile: `int4-g128`.
- Artifact policy: manifest + metadata in git, quantized shards in release assets/object storage.

## Preflight

1. Confirm hardware profile for release run (`M4 Max 48GB`) and local memory headroom.
2. Confirm input model directory exists and contains `.safetensors` + `config.json` + tokenizer.
3. Confirm evaluation datasets exist:
- golden prompts JSONL: exactly 100 chat-formatted entries.
- calibration JSONL: 2000-5000 chat-formatted entries.
4. Confirm baseline FP16 artifact path for same pinned source revision.
5. Confirm `DATABASE_URL` points at release control-plane database if automatic registration is desired.

## Release Invocation

```bash
aosctl models quantize-qwen35 \
  --input var/models/Qwen3.5-27B \
  --output . \
  --hf-repo Qwen/Qwen3.5-27B \
  --revision auto \
  --group-size 64 \
  --context-default 8192 \
  --context-max 16384 \
  --seed 42 \
  --golden-prompts data/golden_prompts.jsonl \
  --calibration data/calibration.jsonl \
  --baseline-fp16 artifacts/fp16/qwen3.5-27b \
  --enable-native-probes \
  --probe-max-samples 8 \
  --enforce-gates
```

Compatibility mode (not default): add `--metrics-from-flags` and pass explicit metric flags.

Native probe mode (phase 1):
- `--enable-native-probes` enables best-effort MLX runtime probes.
- `--probe-max-samples` controls deterministic probe sample count.
- Probe values are recorded for evidence/telemetry only.
- Probe status contract:
  - `disabled`: probes were not requested.
  - `unavailable`: probe prerequisites/runtime context were not satisfied.
  - `failed`: probe was attempted but runtime/model probe execution failed.
  - `success`: probe completed and emitted probe metrics.
- In multi-backend mode, probe execution is runtime-dependent and best-effort.
- Gate pass/fail authority remains deterministic policy-computed metrics (`gate_source=policy_metrics`).
- This pass does not add a dedicated integration test with a real MLX model/runtime.

## Beginner Assisted Flow

Use guided setup when running this for the first time:

```bash
aosctl models quantize-qwen35 \
  --input var/models/Qwen3.5-27B \
  --output . \
  --guided
```

Use `--dry-run` to validate inputs/revision and preview execution without writing quantized artifacts.

## Gate Policy

The command evaluates and enforces:

- `logit_cosine_mean >= 0.985`
- `ppl_delta_pct <= 8.0`
- `task_proxy_delta_abs <= 3.0`
- `tok_s_1k >= 25`
- `tok_s_8k >= 12`
- `rss_mb_peak <= 43008`
- `human_critical_regressions <= 0`

If primary `int4-g64` fails, command retries with `int4-g128`. If fallback fails, command exits with failed gates and does not register artifacts.

## Exit Codes

- `0`: passed + registered
- `2`: quantization/eval completed but gate failure; no registration
- `3`: input/revision/infrastructure failure

## Artifact Layout

Artifacts are written under:

- `artifacts/models/qwen3.5-27b/quant-int4-g64/<revision>/`
- `artifacts/models/qwen3.5-27b/quant-int4-g128/<revision>/` (fallback only)

Each artifact directory includes:

- `manifest.json`
- quantized tensor shard outputs
- copied tokenizer/config files
- per-file BLAKE3 checksums and aggregate checksum in manifest
- gate decisions and evaluator provenance in manifest

## Promotion and Registry

1. Register only the passing selected profile (`g64` or `g128`).
2. Verify registry metadata contains:
- quant profile
- source revision SHA
- aggregate checksum
- reproducibility digest
3. Publish quantized shard binaries to approved external storage/release assets.
4. Commit manifest + metadata only (no normal-git shard commits).

## Rollback

If post-registration validation fails:

1. Deactivate/remove newly registered quantized model entry.
2. Restore prior active model entry.
3. Keep failed artifact manifest for audit, but do not promote.
4. Re-run with corrected inputs/profile only after root-cause review.

## Release Checklist

- Deterministic rerun check passed (identical checksums/reproducibility digest).
- All quantization gates passed.
- Registry entry validated against manifest metadata.
- Docs/runbook references updated for current release.
- Rollback flow executed in dry-run or staging before production cut.
