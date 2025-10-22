# Training Adapters with AdapterOS

This guide covers local CLI training, orchestrated training via the control plane, packaging/signing, verification, registration, and troubleshooting.

## Dataset Schema

The training pipeline expects pre-tokenized examples. Minimal JSON schema:

```json
{
  "examples": [
    { "input": [1,2,3], "target": [4,5,6] },
    { "input": [7,8,9], "target": [10,11,12] }
  ]
}
```

Tips:
- Ensure tokenization matches the inference tokenizer (e.g., Qwen tokenizer).
- Smoke test by encoding/decoding a few snippets and running a tiny training (N=2, epochs=1).

## CLI Training

Train with optional Metal kernel init:

```bash
aosctl train \
  --data data/small.json \
  --output out/train1 \
  --plan plan/qwen7b/PLAN_ID \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b
```

Package and register:

```bash
export AOS_ADAPTERS_ROOT=$PWD/adapters
aosctl train \
  --data data/small.json \
  --output out/train1 \
  --plan plan/qwen7b/PLAN_ID \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b \
  --pack --adapters-root "$AOS_ADAPTERS_ROOT" \
  --register --adapter-id demo_adapter --tier ephemeral --reg-rank 16
```

Artifacts:
- `<adapters_root>/<adapter_id>/weights.safetensors`
- `<adapters_root>/<adapter_id>/manifest.json`
- `<adapters_root>/<adapter_id>/signature.sig`
- `<adapters_root>/<adapter_id>/public_key.pem`

Use `--deterministic` to derive a repeatable seed from the dataset and configuration, or pass `--seed <u64>` for full control. The CLI enforces `--pack` when `--register` is provided to guarantee manifests and signatures exist.

Verify packaged adapter:

```bash
aosctl verify-adapter --adapters-root ./adapters --adapter-id demo_adapter --json
```

## Orchestrated Training (Server)

Start a training job via API:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/training/start \
  -H 'Authorization: Bearer adapteros-local' \
  -H 'Content-Type: application/json' \
  -d '{
    "adapter_name": "demo_adapter",
    "config": { "rank": 8, "alpha": 16, "targets": ["q_proj","v_proj"], "epochs": 1, "learning_rate": 0.0003, "batch_size": 4 },
    "dataset_path": "data/code_to_db_training.json",
    "adapters_root": "./adapters",
    "package": true,
    "register": true,
    "adapter_id": "demo_adapter",
    "tier": 8
  }'
```

Check job:

```bash
curl -H 'Authorization: Bearer adapteros-local' \
  http://127.0.0.1:8080/api/v1/training/jobs/<job_id>

curl -H 'Authorization: Bearer adapteros-local' \
  http://127.0.0.1:8080/api/v1/training/jobs/<job_id>/artifacts
```

## Troubleshooting

- Tokenization mismatch: verify tokenizer and round-trip tests before training.
- Missing artifacts: ensure `AOS_ADAPTERS_ROOT` is set and that the packaged directory exists.
- Registration inconsistencies: recompute BLAKE3 over `weights.safetensors` and compare with `manifest.json.weights_hash`.
- UI shows "Not Ready": check signature validity and hash matching.
