# adapterOS Training Datasets

**Canonical training data taxonomy for LoRA adapter training**

Last Updated: 2025-11-18
Owner: JKCA / James KC Auchterlonie

---

## Dataset Categories

### 1. **behaviors/**
Adapter behavior patterns and runtime characteristics
- Tier promotion/demotion examples
- Lifecycle state transitions
- Memory pressure responses
- Hot-swap decision patterns
- Eviction policy training data

### 2. **routing/**
K-sparse router decision training
- Gate score distributions
- Entropy thresholds
- Top-K adapter selection patterns
- Per-adapter scoring examples
- Temperature (tau) tuning data
- Q15 quantization patterns

### 3. **stacks/**
Adapter stack composition and workflows
- Sequential workflow examples
- Parallel workflow patterns
- Upstream/Downstream compositions
- Stack versioning transitions
- Effective-stack hash validation
- Stack lifecycle examples

### 4. **replay/**
Deterministic replay and verification
- RNG snapshot sequences
- HKDF seed derivation examples
- Global tick ledger entries
- Cross-host consistency proofs
- Barrier coordination patterns

### 5. **determinism/**
Determinism guardrail training
- FIFO task execution patterns
- Seeded randomness examples
- Gate perturbation consistency
- Dropout mask reproducibility
- Token sampling determinism

### 6. **metrics/**
Telemetry and observability patterns
- Router decision telemetry
- Barrier coordination events
- Lifecycle transition events
- Memory pressure signals
- Tick ledger consistency reports

### 7. **cli_contract/**
CLI command patterns and contracts
- `aosctl` command examples
- Argument validation patterns
- Error message templates
- Output formatting examples
- Interactive prompt patterns

### 8. **code_ingest/**
Document/code ingestion training
- PDF parsing examples
- Code extraction patterns
- Training example generation
- Identity/QA strategies
- Tokenization patterns

### 9. **docs_derived/**
Documentation-derived training data
- AGENTS.md policy examples
- Architecture pattern examples
- API contract examples
- Error handling patterns
- Configuration examples

---

## Dataset Standards

### Format
- **Positive examples:** `*.positive.jsonl`
- **Negative examples:** `*.negative.jsonl`
- **Metadata:** `manifest.json` (required)
- **Documentation:** `README.md` (required)

### Manifest Schema
```json
{
  "dataset_id": "unique-id",
  "category": "behaviors|routing|stacks|...",
  "version": "1.0.0",
  "created_at": "2025-11-18T00:00:00Z",
  "num_examples": 1000,
  "hash_b3": "blake3-hash",
  "validation_status": "valid|pending|invalid",
  "tags": ["tag1", "tag2"]
}
```

### JSONL Schema
```json
{
  "input": "Input text or code",
  "target": "Expected output",
  "metadata": {
    "source": "file.rs:123",
    "quality": 0.95,
    "label": "positive"
  }
}
```

---

## Usage

### Creating a New Dataset
```bash
# 1. Create category subdirectory
mkdir -p training/datasets/routing/my_dataset

# 2. Add training examples
cat > training/datasets/routing/my_dataset/data.positive.jsonl <<EOF
{"input": "...", "target": "...", "metadata": {...}}
EOF

# 3. Create manifest
cat > training/datasets/routing/my_dataset/manifest.json <<EOF
{
  "dataset_id": "routing-my-dataset-v1",
  "category": "routing",
  "version": "1.0.0",
  "num_examples": 100
}
EOF

# 4. Validate
cargo run -p adapteros-cli -- dataset validate training/datasets/routing/my_dataset
```

### Training an Adapter
```bash
# Use the training orchestrator
./aosctl train \
  --dataset training/datasets/routing/my_dataset \
  --rank 16 \
  --alpha 32 \
  --adapter-id "tenant-a/routing/my-adapter/r001"
```

---

## Quality Thresholds

| Category | Min Examples | Min Relevance | Min Confidence |
|----------|--------------|---------------|----------------|
| behaviors | 500 | 0.85 | 0.90 |
| routing | 1000 | 0.90 | 0.95 |
| stacks | 300 | 0.85 | 0.90 |
| replay | 200 | 0.95 | 0.95 |
| determinism | 500 | 0.95 | 0.95 |
| metrics | 300 | 0.80 | 0.85 |
| cli_contract | 200 | 0.90 | 0.90 |
| code_ingest | 1000 | 0.85 | 0.90 |
| docs_derived | 500 | 0.90 | 0.90 |

---

## References

- [AGENTS.md](../../AGENTS.md) - Canonical adapterOS reference
- [Training Pipeline](../../crates/adapteros-lora-worker/training/) - Trainer implementation
- [Dataset Manager](../../crates/adapteros-orchestrator/src/training_dataset_integration.rs) - DB integration
- [Evidence Policy](../../crates/adapteros-policy/src/packs/evidence.rs) - Quality enforcement

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
