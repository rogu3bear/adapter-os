# Code Intelligence Workflow

## Overview

Shows how code analysis generates adapters through repository integration, commit tracking, and ephemeral adapter creation. This workflow demonstrates the complete code intelligence pipeline from repository registration to adapter deployment.

## Workflow Animation

```mermaid
flowchart LR
    subgraph "Repository Analysis"
        A[Repository] --> B[Commit Analysis]
        B --> C[Symbol Detection]
        C --> D[Framework Detection]
    end
    
    subgraph "Adapter Generation"
        D --> E[Ephemeral Adapter]
        E --> F[Patch Proposal]
        F --> G[Validation]
    end
    
    subgraph "Deployment"
        G --> H[Quality Check]
        H --> I[Deploy to Worker]
        I --> J[Monitor Performance]
    end
    
    style A fill:#f3e5f5
    style E fill:#e8f5e8
    style J fill:#e1f5fe
```

## Database Tables Involved

### Primary Tables

#### `repositories`
- **Purpose**: Registered code repositories with language detection
- **Key Fields**:
  - `id` (PK), `repo_id` (UK)
  - `path`, `languages`, `default_branch`
  - `status` - registered|scanning|ready|error
  - `frameworks_json`, `file_count`, `symbol_count`

#### `commits`
- **Purpose**: Commit metadata and analysis with symbol tracking
- **Key Fields**:
  - `id` (PK), `repo_id` (FK), `sha`, `author`, `date`
  - `message`, `branch`, `changed_files_json`
  - `impacted_symbols_json`, `test_results_json`
  - `ephemeral_adapter_id`

#### `patch_proposals`
- **Purpose**: AI-generated code patches with validation
- **Key Fields**:
  - `id` (PK), `repo_id`, `commit_sha`
  - `description`, `target_files_json`, `patch_json`
  - `validation_result_json`, `status`, `created_by`

#### `ephemeral_adapters`
- **Purpose**: Commit-aware temporary adapters
- **Key Fields**: `id` (PK), `adapter_data`, `created_at`

#### `adapters`
- **Purpose**: LoRA adapters (ephemeral category)
- **Key Fields**: `id`, `category` = 'ephemeral', `scope` = 'commit', `commit_sha`, `repo_id`

## Related Workflows

- [Adapter Lifecycle](adapter-lifecycle.md) - Ephemeral adapter deployment
- [Promotion Pipeline](promotion-pipeline.md) - Quality checks

## Related Documentation

- [Schema Diagram](../schema-diagram.md) - Complete database structure
- [Code Intelligence](../../code-intelligence/README.md) - Complete code analysis stack

---

**Code Intelligence**: Repository integration and ephemeral adapter generation for commit-specific code assistance.
