# Checkpoint Fixtures

This directory contains golden checkpoint files for testing the checkpoint
parsing and validation logic in `tests/training_resume_e2e.rs`.

## Files

- `golden_epoch_003.ckpt.json` - Valid checkpoint at epoch 3 with all fields

## Regenerating Fixtures

To create new golden files from actual training runs:

```bash
# Run minimal training
aosctl train --model-path var/models/llama-3.2-1B \
    --adapter-name test-checkpoint \
    --epochs 3 \
    --output-dir /tmp/checkpoint-test

# Copy checkpoint to fixtures
cp /tmp/checkpoint-test/checkpoints/epoch_003.ckpt.json \
   tests/fixtures/checkpoints/golden_epoch_003.ckpt.json
```

## Schema

See `CheckpointData` struct in `tests/training_resume_e2e.rs` for the
expected schema. Key fields:

- `version` - Schema version (must be > 0)
- `epoch` - Training epoch number
- `step` - Global step count
- `loss` - Loss value at checkpoint
- `config_hash` - B3 hash of training config
- `lora_rank` - LoRA rank (must be > 0)
- `weights_checksum` - B3 checksum of weights blob
