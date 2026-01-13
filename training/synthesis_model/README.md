# Document-to-Training Data Synthesis Model

This directory contains scripts and configuration for training a small model that converts document chunks into structured training data (Q&A pairs, instruction-following examples, completions).

**Fully local training** — no data leaves your machine, no API costs.

## Overview

The synthesis model is a fine-tuned Qwen2.5-1.5B-Instruct that:

1. Takes a document chunk as input
2. Outputs structured JSON with multiple training example types
3. Runs efficiently on Apple Silicon via CoreML/ANE

## Directory Structure

```
synthesis_model/
├── README.md                    # This file
├── config.yaml                  # Training configuration
├── generate_bootstrap_data.py   # Generate training data using local LLM
├── train_mlx.py                 # MLX fine-tuning script
├── convert_to_coreml.py         # Convert trained model to CoreML
├── data/                        # Training data (generated)
│   ├── bootstrap_train.jsonl
│   └── bootstrap_val.jsonl
└── output/                      # Trained model outputs
    ├── synthesis_model_mlx/     # MLX checkpoint
    └── synthesis_model.mlpackage # CoreML package
```

## Quick Start

### 1. Generate Bootstrap Data (Local)

Generate training data using a local Qwen-14B model via MLX:

```bash
# Install MLX if not already installed
pip install mlx mlx-lm

# Generate training examples (~2-4 hours on M4 Max)
# The model will be downloaded automatically on first run (~28GB)
python generate_bootstrap_data.py \
    --model Qwen/Qwen2.5-14B-Instruct \
    --output data/bootstrap_train.jsonl \
    --num-examples 5000 \
    --source-docs ../datasets/docs/ ../../docs/

# Generate validation set
python generate_bootstrap_data.py \
    --model Qwen/Qwen2.5-14B-Instruct \
    --output data/bootstrap_val.jsonl \
    --num-examples 500 \
    --source-docs ../../test_data/
```

**Note:** First run downloads the model (~28GB for Qwen-14B). Requires ~32GB+ unified memory for comfortable inference.

### 2. Fine-tune with MLX

```bash
# Install dependencies
pip install mlx mlx-lm pyyaml

# Run fine-tuning (~2-4 hours on M4 Max)
python train_mlx.py \
    --config config.yaml \
    --train-data data/bootstrap_train.jsonl \
    --val-data data/bootstrap_val.jsonl \
    --output output/synthesis_model_mlx
```

### 3. Convert to CoreML

```bash
python convert_to_coreml.py \
    --input output/synthesis_model_mlx \
    --output output/synthesis_model.mlpackage \
    --compute-units cpu_and_ne
```

### 4. Integrate with AdapterOS

Copy the CoreML package to the models directory:

```bash
cp -r output/synthesis_model.mlpackage ../../var/models/
```

## Training Data Format

### Input (document chunk)

```json
{
  "chunk": "AdapterOS uses BLAKE3 hashing for content integrity and deterministic verification. The router sorts adapter scores in descending order, with index-based tie-breaking for reproducibility.",
  "metadata": {
    "source": "docs/DETERMINISM.md",
    "chunk_index": 3
  }
}
```

### Output (synthesis model generates)

```json
{
  "qa_pairs": [
    {
      "question": "What hashing algorithm does AdapterOS use for content integrity?",
      "answer": "AdapterOS uses BLAKE3 hashing for content integrity and deterministic verification."
    },
    {
      "question": "How does the AdapterOS router handle tie-breaking?",
      "answer": "The router uses index-based tie-breaking when adapter scores are equal, ensuring reproducibility."
    }
  ],
  "instructions": [
    {
      "instruction": "Explain how AdapterOS ensures deterministic behavior.",
      "response": "AdapterOS ensures deterministic behavior through two key mechanisms: BLAKE3 hashing for content integrity verification, and a router that sorts adapter scores in descending order with index-based tie-breaking for reproducibility."
    }
  ],
  "completions": [
    {
      "context": "The AdapterOS router sorting algorithm",
      "continuation": "sorts adapter scores in descending order, with index-based tie-breaking to ensure reproducible results across runs."
    }
  ]
}
```

## Model Configuration

See `config.yaml` for full configuration. Key parameters:

| Parameter      | Value                      | Notes                             |
| -------------- | -------------------------- | --------------------------------- |
| Base Model     | Qwen2.5-1.5B-Instruct-4bit | Good balance of quality and speed |
| LoRA Rank      | 16                         | Balance of quality vs. efficiency |
| Learning Rate  | 2e-4                       | Standard for LoRA fine-tuning     |
| Batch Size     | 4                          | Fits in 32GB unified memory       |
| Max Seq Length | 2048                       | Covers most document chunks       |
| Training Steps | 5000                       | ~5K examples × 1 epoch            |

## Resource Requirements

| Phase              | Time       | Memory | Notes              |
| ------------------ | ---------- | ------ | ------------------ |
| Bootstrap Data Gen | ~2-4 hours | ~32GB  | Qwen-14B inference |
| Fine-tuning        | ~2-4 hours | ~16GB  | LoRA on 0.5B model |
| CoreML Conversion  | ~5-10 min  | ~8GB   | One-time           |

**Total:** ~4-8 hours on M4 Max, fully local, no API costs.

## Performance Targets

| Metric          | Target           | Notes                     |
| --------------- | ---------------- | ------------------------- |
| Inference Speed | 100-150 tok/s    | On M4 Max with ANE        |
| Model Size      | ~1GB             | CoreML package            |
| Quality         | >80% valid JSON  | Structured output parsing |
| Diversity       | 3+ example types | Per chunk                 |

## Evaluation

Run evaluation on held-out validation set:

```bash
python evaluate.py \
    --model output/synthesis_model_mlx \
    --data data/bootstrap_val.jsonl \
    --metrics output/eval_metrics.json
```

Metrics tracked:

- JSON parse success rate
- Q&A relevance (semantic similarity to source)
- Instruction diversity (unique instruction types)
- Factual accuracy (answer grounded in chunk)

## Troubleshooting

### Out of Memory

Reduce batch size in `config.yaml` or use gradient checkpointing.

### Poor JSON Output

Increase training steps or add more diverse examples to bootstrap data.

### Slow Inference

Ensure CoreML package is using `cpu_and_ne` compute units for ANE acceleration.

## License

Training scripts: MIT License (same as AdapterOS)
Model weights: Subject to Qwen model license (Apache 2.0)
