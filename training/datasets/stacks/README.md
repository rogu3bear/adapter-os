# Adapter Stack Training Data

**Purpose:** Train adapters to learn stack composition patterns and workflow execution

## Overview

Adapter stacks are reusable combinations of adapters with defined execution workflows. Stacks enable predictable adapter orchestration and telemetry correlation.

## Key Concepts

- **Stack Types:** Sequential, Parallel, UpstreamDownstream
- **Effective-Stack Hash:** BLAKE3 hash of active adapters
- **Stack Versioning:** Integer version incremented on adapter changes
- **Lifecycle States:** draft → active → deprecated → retired
- **Workflow Execution:** Multi-phase adapter application

## Training Example Schema

```jsonl
{
  "input": {
    "stack_id": "stack-prod-001",
    "adapter_ids": ["adapter-a", "adapter-b", "adapter-c"],
    "workflow_type": "upstream_downstream"
  },
  "target": {
    "effective_hash": "blake3-hash",
    "version": 3,
    "execution_phases": [
      {"phase": "upstream", "adapters": ["adapter-a"]},
      {"phase": "downstream", "adapters": ["adapter-b", "adapter-c"]}
    ],
    "lifecycle_state": "active"
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive"
  }
}
```

## Workflow Types

1. **Sequential:** Adapters applied in order (A → B → C)
2. **Parallel:** Adapters applied concurrently, results merged
3. **UpstreamDownstream:** Two-phase execution (upstream context, downstream generation)

## Quality Criteria

- **Min Examples:** 300
- **Min Relevance:** 0.85
- **Min Confidence:** 0.90
- **Max Adapters per Stack:** 16

## Data Sources

1. **Stack Registry:** `adapter_stacks` table
2. **Version History:** `stack_version_history` table
3. **Workflow Executor:** `adapteros-lora-lifecycle/src/workflow.rs`
4. **Telemetry:** Stack ID/version correlation

## Example Datasets

- `sequential_workflows/` - Linear adapter chains
- `parallel_workflows/` - Concurrent adapter application
- `upstream_downstream/` - Two-phase execution patterns
- `versioning_transitions/` - Version bump examples
- `lifecycle_states/` - State transition examples
- `composition_patterns/` - Stack design patterns

## References

- `crates/adapteros-lora-lifecycle/src/workflow.rs` - Workflow executor
- `crates/adapteros-db/src/adapter_stacks.rs` - Stack management
- `migrations/0064_adapter_stacks.sql` - Schema
- `migrations/0066_stack_versioning.sql` - Versioning
- `migrations/0071_lifecycle_version_history.sql` - History tracking
