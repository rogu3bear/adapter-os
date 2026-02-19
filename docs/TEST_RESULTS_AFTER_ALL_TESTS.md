# AdapterOS Benchmarks (M4 Max) — 2026-01-31

> **Snapshot** — Benchmark results from 2026-01-31. Re-run for current numbers; hardware/code changes invalidate these. See [DOCS_AUDIT_2026-02-18.md](DOCS_AUDIT_2026-02-18.md).

## Environment
- Host: MacBook Pro (Mac16,5) — Apple M4 Max, 16 cores (12P+4E)
- Memory: 48 GB
- OS: macOS 26.2 (Build 25C56), Darwin 25.2.0 (arm64)
- Repo: `<repo-root>` @ `f6f3e1548d1ebcba95573c97d85b94de4f75d6b4`
- MLX-LM: 0.30.5

## Model Inventory (var/models)
- Llama-3.2-1B-Instruct-4bit (~680 MB)
- Llama-3.2-3B-Instruct-4bit (~1.7 GB) **(primary target)**
- Llama-3.2-11B-Vision-Instruct-8bit (~11 GB)
- Llama-3.3-70B-Instruct-4bit (~37 GB)
- Meta-Llama-3-8B-Instruct-4bit (~4.9 GB)
- Qwen2.5-1.5B-Instruct-4bit (~839 MB)
- Qwen2.5-7B-Instruct-4bit (~4.0 GB)
- Mistral-7B-Instruct-v0.3-4bit (~10 GB)
- CoreML: `qwen2.5-0.5b-coreml-128.mlpackage` (~950 MB)

## Benchmarks Run
### 1) MLX-LM Inference (focus: Llama-3.2-3B-Instruct)
Command (3 runs + 1 warmup, greedy):
```
python var/bench/llama32_3b/bench_mlx_lm.py \
  --model var/models/Llama-3.2-3B-Instruct-4bit \
  --prompt "Summarize the core idea of a transformer in 3 sentences." \
  --max-tokens 128 --runs 3 --warmup 1
```
Comparison baseline:
```
python var/bench/llama32_3b/bench_mlx_lm.py \
  --model var/models/Llama-3.2-1B-Instruct-4bit \
  --prompt "Summarize the core idea of a transformer in 3 sentences." \
  --max-tokens 128 --runs 3 --warmup 1
```

### 2) Adapter Training (CPU proxy)
Dataset: `var/bench/llama32_3b/dataset` (64 examples, 2,091 tokens)

Low influence:
```
./aosctl train local \
  --config var/bench/llama32_3b/configs/train_cpu_low.json \
  --data var/bench/llama32_3b/dataset \
  --output var/bench/llama32_3b/output/adapter_low \
  --base-model var/models/Llama-3.2-3B-Instruct-4bit
```
High influence:
```
./aosctl train local \
  --config var/bench/llama32_3b/configs/train_cpu_high.json \
  --data var/bench/llama32_3b/dataset \
  --output var/bench/llama32_3b/output/adapter_high \
  --base-model var/models/Llama-3.2-3B-Instruct-4bit
```

### 3) Adapter Influence Analysis (weight-space)
Computed from LoRA weights (`lora_weights.json`) for low/high alpha:
- Low alpha = 4.0, High alpha = 64.0
- Delta = (alpha / rank) * (B @ A)
- Deterministic random vector test (seed 1337) to estimate output magnitude impact
Output: `var/bench/llama32_3b/results/adapter_influence.json`

## Results
### MLX-LM Inference Throughput (avg of 3 runs)
| Model | Prompt tokens | Gen tokens | Prompt TPS | Gen TPS | Wall TPS | Peak Mem | Load time |
|---|---:|---:|---:|---:|---:|---:|---:|
| Llama-3.2-3B-Instruct-4bit | 15 | 120 | 517.93 | 191.15 | 166.86 | 1.87 GB | 1.21 s |
| Llama-3.2-1B-Instruct-4bit | 15 | 128 | 1100.31 | 443.38 | 355.63 | 0.72 GB | 0.84 s |

Notes:
- 1B generation TPS is ~2.32× higher than 3B for the same prompt and decode length.
- Numbers are MLX-LM internal TPS (prompt + generation) and wall TPS; model load time measured once per run.

### CPU Proxy Training (LoRA)
| Config | Alpha | Rank | Tokens | Final Loss | Time |
|---|---:|---:|---:|---:|---:|
| Low influence | 4.0 | 8 | 2,091 | 0.3732 | 5 ms |
| High influence | 64.0 | 8 | 2,091 | 0.3732 | 5 ms |

### Adapter Influence (weight-space deltas)
| Metric | Low (alpha=4) | High (alpha=64) | Ratio (High/Low) |
|---|---:|---:|---:|
| ‖ΔW‖₂ | 1.78798e-4 | 2.85194e-3 | 15.95× |
| max(|ΔW|) | 6.36e-6 | 8.94e-5 | 14.06× |
| mean(|ΔW|) | 2.20e-9 | 3.55e-8 | 16.13× |
| ‖ΔW·x‖₂ (seeded test vec) | 6.81e-6 | 1.66e-3 | 244× |

Interpretation:
- Higher alpha clearly increases effective LoRA delta magnitude.
- Output-space impact (ΔW·x) amplifies more than weight-space norms for the sampled vector.

## AdapterOS Worker Bring-up (Backends)
### MLX backend (Llama-3.2-3B-Instruct-4bit)
Command:
```
./target/release/aos-worker \
  --backend mlx \
  --manifest manifests/llama3.2-3b-instruct-4bit.yaml \
  --model-path var/models/Llama-3.2-3B-Instruct-4bit \
  --tokenizer var/models/Llama-3.2-3B-Instruct-4bit/tokenizer.json \
  --uds-path var/bench/llama32_3b/llama_mlx.sock
```
Result: **fails on tokenizer load**
- Error: `Failed to load tokenizer: data did not match any variant of untagged enum ModelWrapper at line 1251004 column 1`
- This blocks MLX backend inference in AdapterOS for Llama-3.2 tokenizer JSON.

### Metal backend (Llama-3.2-3B-Instruct-4bit)
Command:
```
./target/release/aos-worker \
  --backend metal \
  --manifest manifests/llama3.2-3b-instruct-4bit.yaml \
  --model-path var/models/Llama-3.2-3B-Instruct-4bit \
  --tokenizer var/models/Llama-3.2-3B-Instruct-4bit/tokenizer.json \
  --uds-path var/bench/llama32_3b/llama_metal.sock
```
Result: **fails on 4-bit dtype**
- Error: `Kernel error: Unsupported tensor dtype: U32. Expected F32, F16, or BF16`
- Metal backend currently cannot ingest 4-bit Llama weights in this format.

### CoreML backend (Qwen2.5-0.5B CoreML)
Command (coreml-enabled worker built to `target/coreml/release/aos-worker`):
```
AOS_EMBEDDING_MODEL_PATH=var/model-cache/models/qwen2.5-0.5b-instruct-safetensors \
./target/coreml/release/aos-worker \
  --backend coreml \
  --manifest manifests/qwen0.5b-coreml.yaml \
  --model-path var/models/qwen2.5-0.5b-coreml-128.mlpackage \
  --tokenizer var/model-cache/models/qwen2.5-0.5b-instruct-safetensors/tokenizer.json \
  --uds-path var/bench/llama32_3b/qwen0.5b_coreml.sock
```
Status:
- CoreML boot smoke inference succeeded.
- Inference requests (UDS) fail with `Kernel error: CoreML inference failed with code -4`.
- Additional setup required to make CoreML inference stable for this model/package.

## Energy Measurement (Blocked)
Attempted:
```
powermetrics -n 1 -i 200 --samplers cpu_power
```
Result: `powermetrics must be invoked as the superuser`. Energy measurements were not collected due to lack of sudo access.

## Artifacts & Logs
- MLX-LM benchmark results: `var/bench/llama32_3b/results/llama32_3b_mlx.json`, `llama32_1b_mlx.json`, `llama_mlx_summary.json`
- Training logs: `var/bench/llama32_3b/logs/train_cpu_low.log`, `train_cpu_high.log`, `train_mlx.log`
- Adapter outputs: `var/bench/llama32_3b/output/adapter_low`, `adapter_high`
- Adapter influence stats: `var/bench/llama32_3b/results/adapter_influence.json`
- Worker logs (failures): `var/bench/llama32_3b/logs/worker_llama_mlx.log` (tokenizer), `worker_llama_metal.log` (U32), `worker_qwen0.5b_coreml.log` (CoreML -4)

## Summary
- **Llama-3.2-3B (MLX-LM)**: 191 gen TPS, 166 wall TPS, ~1.87 GB peak MLX memory.
- **Llama-3.2-1B (MLX-LM)**: 443 gen TPS, 355 wall TPS, ~0.72 GB peak MLX memory.
- **Adapter training (CPU proxy)**: both low/high alpha runs completed in ~5 ms with identical loss (0.3732) for 2,091 tokens.
- **Adapter influence**: high alpha increases ΔW magnitude ~16× and ΔW·x impact ~244× for the sampled vector.
- **AdapterOS worker**: MLX fails on tokenizer JSON; Metal fails on 4-bit U32; CoreML boots but inference fails with code -4.

## Next Steps (if you want me to continue)
1) Patch tokenizer loading in Rust to support Llama-3.2 tokenizer JSON (or upgrade tokenizers crate) so AdapterOS MLX inference works.
2) Add Metal path support for 4-bit (U32) weights, or provide FP16/BF16 weights for Metal.
3) Diagnose CoreML error -4 (likely input shape or runtime constraints) and validate the CoreML package against expected input lengths.
4) If energy numbers are required, run `powermetrics` with sudo or provide a privileged telemetry source.
