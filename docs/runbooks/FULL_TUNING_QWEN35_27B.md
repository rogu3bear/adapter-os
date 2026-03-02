# Full Tuning Runbook: Qwen3.5-27B

## Scope
Full supervised tuning using AdapterOS control-plane training and promotion gates.
Target base model: `Qwen3.5-27B`.

## Inputs (must be frozen before run)
- Dataset file: `var/datasets/generated/spark/train/adapteros_train_full_real_v2.jsonl`
- Dataset manifest: `var/datasets/generated/spark/manifest_collections.json`
- Citation index: `var/datasets/generated/spark/pipeline/citation_index_collections.json`
- Hardening report: `var/datasets/generated/spark/pipeline/reports/latest_audit_real_artifacts.json`

## Stage 0: Environment lock
1. Set model vars:
   - `export AOS_MODEL_CACHE_DIR=var/models`
   - `export AOS_BASE_MODEL_ID=Qwen3.5-27B`
2. Ensure model exists:
   - `test -d var/models/Qwen3.5-27B`
3. Seed model in registry:
   - `./aosctl models seed --model-path var/models/Qwen3.5-27B`

## Stage 1: Dataset freeze + registration
1. Compute immutable fingerprint:
   - `shasum -a 256 var/datasets/generated/spark/train/adapteros_train_full_real_v2.jsonl`
2. Upload/register dataset version(s) via existing dataset/training flow.
3. Record returned `dataset_version_id`(s).

## Stage 2: Launch full training (control-plane)
1. Start training job:
   - `./aosctl --json train start <repo_id> --branch main --base-model-id Qwen3.5-27B --dataset-version-ids <dataset_version_id> --backend-policy auto --backend mlx`
2. Capture `job_id` from JSON response.
3. Track status:
   - `./aosctl --json train status <job_id>`
4. Contract gate:
   - Dataset/training contract must be `1.0` (legacy `v1` inputs are normalized at API ingress).
5. Backend gate:
   - Full-tune profile must not run on CPU fallback.

## Stage 3: Hard gates before any promotion
1. Pull training report:
   - `./aosctl --json train report --id <job_id>`
2. Run determinism audit:
   - `./aosctl audit-determinism --backend mlx --model-path var/models/Qwen3.5-27B`
3. Validate promotion gates:
   - `GET /v1/cp/promotion-gates/{cpid}`
   - `GET /v1/golden/{run_id}/gates`
4. Block promotion on any failure in:
   - determinism drift
   - policy/trust gate status
   - malformed output contract regressions

## Stage 4: Promotion and rollout
1. Request promotion only after all gates pass.
2. Canary first, then widen rollout.
3. Keep rollback ready:
   - `POST /v1/golden/{run_id}/rollback`

## Required evidence to archive per run
- Dataset fingerprint and manifest hash
- Training job JSON (start/status/report)
- Determinism audit output
- Gate snapshots (`cp` + `golden`)
- Final promotion/rollback decision with timestamp

## Notes
- Non-synthetic training requires `dataset_version_ids` and trust-allowed dataset versions.
- `data_spec_hash` mismatch versus dataset manifests must fail the run.
- Use explicit backend policy when requesting CoreML behavior (`coreml_only` or `coreml_else_fallback`).
- If a run fails, inspect `error_message` returned by `GET /v1/training/jobs/{job_id}` (now hydrated from persisted progress metadata).
