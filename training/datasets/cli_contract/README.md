# CLI Contract Training Data

**Purpose:** Train adapters to learn `aosctl` command patterns, argument validation, and error handling

## Overview

The adapterOS CLI (`aosctl`) provides the primary interface for adapter management, training, and system operations. Training data captures command patterns, validation rules, and expected outputs.

## Key Concepts

- **Command Hierarchy:** Subcommands (adapter, stack, train, infer)
- **Argument Validation:** Type checking, range constraints
- **Error Messages:** Structured, actionable error formatting
- **Output Formats:** JSON, YAML, table, plain text
- **Interactive Prompts:** Confirmation dialogs, progress bars

## Training Example Schema

```jsonl
{
  "input": {
    "command": "aosctl adapter register",
    "args": {
      "adapter_id": "tenant-a/eng/code-review/r001",
      "rank": 16,
      "alpha": 32,
      "tier": "tier_1"
    }
  },
  "target": {
    "exit_code": 0,
    "output": {
      "adapter_id": "tenant-a/eng/code-review/r001",
      "hash_b3": "blake3-hash",
      "status": "registered"
    },
    "stderr": null
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive"
  }
}
```

## Command Categories

### Adapter Management
- `aosctl adapter register` - Register new adapter
- `aosctl adapter load` - Load into memory
- `aosctl adapter unload` - Evict from memory
- `aosctl adapter list` - List all adapters

### Stack Operations
- `aosctl stack create` - Create adapter stack
- `aosctl stack update` - Modify stack composition
- `aosctl stack list` - List all stacks

### Training
- `aosctl train start` - Start training job
- `aosctl train status` - Check job progress
- `aosctl dataset create` - Create training dataset

### Inference
- `aosctl infer` - Run inference request
- `aosctl infer streaming` - Streaming inference

### System
- `aosctl db migrate` - Run migrations
- `aosctl init-tenant` - Initialize tenant

## Quality Criteria

- **Min Examples:** 200
- **Min Relevance:** 0.90
- **Min Confidence:** 0.90
- **Validation Coverage:** 100% of argument types

## Data Sources

1. **CLI Tests:** Integration tests in `crates/adapteros-cli/tests/`
2. **Man Pages:** Documentation in `docs/cli/`
3. **Usage Examples:** Real command invocations
4. **Error Cases:** Validation failures, edge cases

## Example Datasets

- `command_patterns/` - Valid command invocations
- `argument_validation/` - Type/range checking
- `error_messages/` - Structured error templates
- `output_formats/` - JSON/YAML/table formatting
- `interactive_prompts/` - User confirmations

## References

- `crates/adapteros-cli/src/main.rs` - CLI entry point
- `crates/adapteros-cli/src/commands/` - Command implementations
- `crates/adapteros-cli/tests/` - Integration tests
