# Training Adapters with AdapterOS

This guide covers local CLI training, orchestrated training via the control plane, packaging/signing, verification, registration, and troubleshooting.

## Dataset Schema

The CLI training command supports two data formats: **text-based** (auto-detected) and **pre-tokenized** (backward compatible).

### Text-Based Format (Recommended)

The CLI automatically detects text-based JSON datasets. This format includes text strings that are tokenized automatically:

```json
{
  "name": "my_dataset",
  "description": "Training dataset for code tasks",
  "version": "1.0.0",
  "examples": [
    {
      "id": "example_1",
      "input": { "Text": "Write a function to add two numbers" },
      "target": { "Text": "def add(a, b):\n    return a + b" },
      "weight": 1.0,
      "metadata": { "category": "code" },
      "tags": ["python", "function"]
    },
    {
      "input": "User asks about Rust",
      "target": "Rust is a systems programming language...",
      "weight": 1.0
    }
  ]
}
```

Input and target can be:
- Simple strings: `"input": "text here"`
- Text objects: `"input": { "Text": "text here" }`
- Code blocks: `"input": { "Code": { "content": "code", "language": "rust" } }`
- Structured JSON: `"input": { "Structured": {...} }`

### Pre-Tokenized Format (Legacy)

For backward compatibility, the CLI also supports pre-tokenized data:

```json
{
  "examples": [
    { "input": [1,2,3], "target": [4,5,6] },
    { "input": [7,8,9], "target": [10,11,12] }
  ]
}
```

Tips:
- For text-based format: ensure the tokenizer path is correct (defaults to `models/qwen2.5-7b-mlx/tokenizer.json` or use `--tokenizer`)
- For pre-tokenized format: ensure tokenization matches the inference tokenizer (e.g., Qwen tokenizer)
- Smoke test by encoding/decoding a few snippets and running a tiny training (N=2, epochs=1)

## CLI Training

### Basic Training

Train with text-based data (auto-detected):

```bash
aosctl train \
  --data data/text_dataset.json \
  --output out/train1 \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b
```

Train with pre-tokenized data (backward compatible):

```bash
aosctl train \
  --data data/pre_tokenized.json \
  --output out/train1 \
  --plan plan/qwen7b/PLAN_ID \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b
```

### Train Base Adapter from Manifest

Train from a dataset manifest (used for base adapters). Both CLI and xtask versions support directory and .aos output formats:

```bash
aosctl train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --output-dir adapters \
  --adapter-id code_lang_v1 \
  --rank 16 --alpha 32 --epochs 4 \
  --output-format aos  # Creates .aos file
```

Or use the xtask command:

```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --output-dir adapters \
  --adapter-id code_lang_v1 \
  --output-format aos
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
