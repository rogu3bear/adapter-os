# Training the AdapterOS Base Code Adapter

This guide describes how to curate data and train the Layer 2 “code” adapter in accordance with the MasterPlan policies.

## 1. Curate the Dataset

All curated examples live under `training/datasets/base/code/adapteros/`:

- `adapteros-positive-examples.positive.jsonl` – deterministic fixes, lifecycle policies, router configuration, etc.
- `adapteros-negative-examples.negative.jsonl` – guardrails that refuse hallucinations, policy bypasses, and data hygiene violations.
- `manifest.json` – manifest consumed by the training loader (references both JSONL files with weights).

Every JSONL row uses the schema:

```json
{
  "id": "optional identifier",
  "prompt": "operator instruction or telemetry",
  "response": "desired adapter reply",
  "weight": 1.0,
  "metadata": {"category": "cli/runtime", "tags": ["policy"]}
}
```

Negative rows use a negative `weight` so the trainer pushes the model away from that behaviour. The manifest-level weight is multiplied with each sample’s weight, enabling coarse and fine grained control.

## 2. Validate Data

Run the dataset lints to ensure file naming, schema, and policy gates are respected:

```bash
cargo fmt -- training/datasets/base/code/adapteros/*.jsonl
```

To sanity check the manifest loader:

```bash
cargo test -p adapteros-lora-worker loader::tests::test_load_examples_with_encoder
```

## 3. Train the Adapter

Use the dedicated xtask command. It loads the manifest, tokenizes each sample, runs the deterministic Micro-LoRA trainer, and packages the weights:

```bash
cargo xtask train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --output-dir adapters \
  --adapter-id code_lang_v1 \
  --rank 16 --alpha 32 --epochs 4 --batch-size 8
```

The xtask now prints dataset composition (positive/negative counts, token totals) and loss after every epoch so you can track deterministic progress. The packaged manifest includes dataset name, version, and tokenizer path metadata for audit trails.

The defaults align with MasterPlan Layer 2 requirements (rank=16, alpha=32, hidden_dim=3584). After completion the packaged adapter lives in `adapters/code_lang_v1/` and includes a safetensors weights file, manifest, and signature.

## 4. Update Deployment Manifest

1. Compute the BLAKE3 hash of `weights.safetensors` (the xtask command prints it).
2. Update `manifests/qwen7b-with-code-adapter.yaml` with the new hash under the base adapter entry.
3. Commit the updated dataset manifest, JSONL files, and adapter artifacts (or upload the weights to the artifact store and reference the content hash).

## 5. Regression Checklist

- `cargo test -p adapteros-lora-router`
- `cargo test -p adapteros-lora-lifecycle`
- `cargo xtask pack-lora --help` (optional quantization tooling)

Training runs are reproducible because the Micro-LoRA trainer seeds its RNG via HKDF (see `MicroLoRATrainer::new`). Keep the dataset manifest under version control so identical runs produce the same adapter.
