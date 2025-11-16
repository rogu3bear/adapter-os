# CLI Coverage Analysis for Operator Playbooks

**Purpose:** Gap analysis between operator playbooks and CLI/API implementation
**Date:** 2025-01-16

---

## Summary

This document analyzes which commands from `OPERATOR_PLAYBOOKS.md` are implemented and which need to be added.

## Coverage Status

### ✅ Fully Implemented Commands

| Command | Location | Playbook Usage |
|---------|----------|----------------|
| `aosctl init-tenant` | `commands/init_tenant.rs` | Playbook 1 (Onboarding) |
| `aosctl register-adapter` | `commands/register_adapter.rs` | Playbooks 1, 2, 5 |
| `aosctl list-adapters` | `commands/list_adapters.rs` | Playbooks 1, 2, 6 |
| `aosctl adapter list` | `commands/adapter.rs:List` | Playbooks 1, 2, 6 |
| `aosctl adapter-info` | `commands/adapter_info.rs` | Playbooks 1, 2, 3, 5 |
| `aosctl adapter profile` | `commands/adapter.rs:Profile` | Playbook 7 |
| `aosctl adapter pin` | `commands/adapter.rs:Pin` | Playbooks 1, 3 |
| `aosctl adapter unpin` | `commands/adapter.rs:Unpin` | Playbook 7 |
| `aosctl adapter-swap` | `commands/adapter_swap.rs` | Playbooks 2, 3, 5 |
| `aosctl ingest-docs` | `commands/ingest_docs.rs` | Playbook 4 |
| `aosctl train` | `commands/train.rs` | Playbook 5 |
| `aosctl infer` | `commands/infer.rs` | Playbooks 2, 5, 6 |
| `aosctl drift-check` | `commands/drift_check.rs` | Playbooks 6, 8 |
| `aosctl node-verify` | `commands/node_verify.rs` | Playbook 6 |
| `aosctl telemetry-show` | `commands/telemetry_show.rs` | Playbooks 2, 9, 10 |
| `aosctl telemetry-verify` | `commands/verify_telemetry.rs` | Playbook 10 |
| `aosctl federation-verify` | `commands/verify_federation.rs` | Playbook 10 |
| `aosctl audit` | `commands/audit.rs` | Playbook 10 |
| `aosctl verify` | `commands/verify.rs` | Playbooks 2, 4, 8 |
| `aosctl sync-registry` | `commands/sync_registry.rs` | Playbook 1 |
| `aosctl node-list` | `commands/node_list.rs` | Playbook 6 |

### ⚠️ Partially Implemented / Needs Enhancement

| Command | Status | Location | Missing Functionality |
|---------|--------|----------|----------------------|
| `aosctl adapter evict` | Partial | `commands/adapter.rs:send_adapter_command` | No CLI subcommand, only internal function |
| `aosctl list-pinned` | Missing | N/A | Need to add as separate command or flag |
| `aosctl train generate-dataset` | Unknown | `commands/train.rs` | Need to verify subcommands exist |

### ❌ Missing Commands

| Command | Playbook Reference | Needed For |
|---------|-------------------|------------|
| `aosctl adapter evict` | Playbook 7 | Manual memory pressure response |
| `aosctl list-pinned` | Playbooks 1, 7 | Listing pinned adapters per tenant |

---

## API Endpoint Coverage

### ✅ Implemented Endpoints (from playbooks)

| Endpoint | Playbook Usage | Implementation |
|----------|----------------|----------------|
| `POST /api/tenants/:id/quota` | Playbook 1 | Server API |
| `POST /api/training/datasets` | Playbooks 4, 5 | Server API |
| `GET /api/training/datasets/:id` | Playbook 4 | Server API |
| `POST /api/training/jobs` | Playbook 5 | Server API |
| `GET /api/training/jobs/:id` | Playbook 5 | Server API |
| `GET /api/training/templates` | Playbook 5 | Server API |
| `GET /api/adapters` | Playbooks 2, 3, 7 | Server API |
| `POST /api/adapters/:id/evict` | Playbook 7 | Server API |
| `GET /api/memory/usage` | Playbooks 3, 7 | Server API |
| `POST /api/adapter-stacks` | Playbook 5 | Server API |
| `GET /api/adapter-stacks` | Playbook 5 | Server API |
| `POST /api/chat/completions` | Playbooks 5, 8 | Server API |

### ⚠️ Assumed Endpoints (need verification)

| Endpoint | Playbook Usage | Notes |
|----------|----------------|-------|
| `POST /api/adapters/:id/load` | Playbook 5 | May be handled by adapter-swap |
| `GET /api/adapters/:id/pinned` | Playbook 1 | Filter on GET /api/adapters? |

---

## Gap Analysis by Playbook

### Playbook 1: Onboarding a New Tenant

**Coverage:** 95%

**Gaps:**
- ❌ `aosctl list-pinned` - Need CLI command (currently requires database query or API call)
- ⚠️ Creating adapter stacks requires direct SQL - should have CLI command

**Recommendations:**
1. Add `aosctl adapter list-pinned --tenant <id>` command
2. Add `aosctl stack create --name <name> --adapters <ids> --workflow <type>` command

### Playbook 2: Rolling Back a Bad Adapter

**Coverage:** 100%

All commands exist! ✅

### Playbook 3: Hot-Swapping Adapters in Production

**Coverage:** 100%

All commands exist! ✅

### Playbook 4: Creating Training Datasets from Documents

**Coverage:** 100%

All commands exist! ✅

### Playbook 5: Training and Deploying a New Adapter

**Coverage:** 95%

**Gaps:**
- ⚠️ `aosctl adapter load` - May not be needed if adapter-swap handles it
- ⚠️ Creating adapter stacks requires API call - should have CLI command

**Recommendations:**
1. Add `aosctl stack create` command
2. Add `aosctl stack list` command
3. Clarify if `adapter load` is needed vs `adapter-swap`

### Playbook 6: Verifying Determinism Across Cluster

**Coverage:** 100%

All commands exist! ✅

### Playbook 7: Responding to Memory Pressure

**Coverage:** 85%

**Gaps:**
- ❌ `aosctl adapter evict` - Exists as internal function, needs CLI subcommand
- ⚠️ Manual eviction currently requires curl to API

**Recommendations:**
1. Add `aosctl adapter evict <adapter_id> --tenant <id> --reason <reason>` command
2. Add `aosctl adapter list-pinned` command

### Playbook 8: Drift Detection and Baseline Management

**Coverage:** 100%

All commands exist! ✅

### Playbook 9: Incident Response - Adapter Failures

**Coverage:** 100%

All commands exist! ✅

### Playbook 10: Telemetry Audit Trail Verification

**Coverage:** 100%

All commands exist! ✅

---

## Priority Implementation List

### Priority 1: Critical for Operations

1. **`aosctl adapter evict`** - Manual eviction for memory pressure
   - Add to `AdapterCommand` enum in `commands/adapter.rs`
   - Wire to existing `send_adapter_command("evict", ...)` function
   - Example: `aosctl adapter evict <id> --tenant <tenant>`

2. **`aosctl adapter list-pinned`** - List pinned adapters
   - Add flag to `adapter list` command: `--pinned-only`
   - Or: Add to `AdapterListPinned` command in main.rs
   - Example: `aosctl adapter list --pinned-only --tenant <tenant>`

### Priority 2: Quality of Life

3. **`aosctl stack create`** - Create adapter stacks without SQL
   - New command: `commands/stack.rs`
   - Subcommands: `create`, `list`, `delete`, `update`
   - Example: `aosctl stack create --name <name> --adapters <ids> --workflow <type>`

4. **`aosctl stack list`** - List adapter stacks
   - Part of stack.rs module
   - Example: `aosctl stack list [--tenant <id>]`

### Priority 3: Nice to Have

5. **`aosctl memory status`** - Simplified memory status
   - Wrapper around `curl /api/memory/usage`
   - Example: `aosctl memory status [--tenant <id>]`

6. **`aosctl adapter stats`** - Quick adapter statistics
   - Wrapper around profile with filtered output
   - Example: `aosctl adapter stats <id> [--metric latency|activation]`

---

## Implementation Plan

### Phase 1: Critical Commands (Week 1)

**Task 1.1: Add `adapter evict` subcommand**

File: `crates/adapteros-cli/src/commands/adapter.rs`

```rust
// Add to AdapterCommand enum
#[derive(Debug, Subcommand, Clone)]
pub enum AdapterCommand {
    // ... existing commands ...

    /// Evict adapter from memory
    #[command(
        after_help = "Examples:\n  aosctl adapter evict adapter-1\n  aosctl adapter evict adapter-1 --tenant dev --reason \"Low activation\""
    )]
    Evict {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,

        /// Reason for eviction (for audit trail)
        #[arg(long)]
        reason: Option<String>,
    },
}

// Add handler in handle_adapter_command
AdapterCommand::Evict { adapter_id, tenant, reason } => {
    evict_adapter(&adapter_id, tenant, reason.as_deref(), output).await
}

// Add evict_adapter function
async fn evict_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    reason: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, reason = ?reason, "Evicting adapter");
    let socket_path = get_worker_socket_path(tenant.as_deref());

    // Use existing send_adapter_command
    send_adapter_command(&socket_path, "evict", adapter_id, Duration::from_secs(5)).await?;

    output.success(&format!("Evicted adapter: {}", adapter_id));
    if let Some(r) = reason {
        output.result(&format!("Reason: {}", r));
    }

    Ok(())
}
```

**Task 1.2: Add `--pinned-only` flag to `adapter list`**

File: `crates/adapteros-cli/src/commands/adapter.rs`

```rust
// Update List variant
List {
    /// Output format
    #[arg(long)]
    json: bool,

    /// Tenant ID
    #[arg(long)]
    tenant: Option<String>,

    /// Show only pinned adapters
    #[arg(long)]
    pinned_only: bool,
},

// Update list_adapters function
async fn list_adapters(
    json: bool,
    tenant: Option<String>,
    pinned_only: bool,
    output: &OutputWriter
) -> Result<()> {
    // ... existing code ...

    // Filter adapters if pinned_only
    let mut adapters = connect_and_fetch_adapter_states(&socket_path, Duration::from_secs(5)).await?;
    if pinned_only {
        adapters.retain(|a| a.pinned);
    }

    // ... render table ...
}
```

### Phase 2: Stack Management (Week 2)

**Task 2.1: Create `stack.rs` command module**

File: `crates/adapteros-cli/src/commands/stack.rs`

```rust
use clap::Subcommand;
use adapteros_core::Result;
use crate::output::OutputWriter;

#[derive(Debug, Subcommand, Clone)]
pub enum StackCommand {
    /// Create a new adapter stack
    Create {
        #[arg(long)]
        name: String,

        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,

        #[arg(long)]
        workflow: String, // Sequential, Parallel, UpstreamDownstream

        #[arg(long)]
        description: Option<String>,
    },

    /// List adapter stacks
    List {
        #[arg(long)]
        json: bool,
    },

    /// Delete adapter stack
    Delete {
        name: String,
    },
}

pub async fn handle_stack_command(cmd: StackCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        StackCommand::Create { name, adapters, workflow, description } => {
            create_stack(&name, &adapters, &workflow, description.as_deref(), output).await
        }
        StackCommand::List { json } => {
            list_stacks(json, output).await
        }
        StackCommand::Delete { name } => {
            delete_stack(&name, output).await
        }
    }
}

async fn create_stack(
    name: &str,
    adapters: &[String],
    workflow: &str,
    description: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    // Call API or database directly
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "name": name,
        "adapter_ids": adapters,
        "workflow_type": workflow,
        "description": description,
    });

    let resp = client
        .post("http://localhost:8080/api/adapter-stacks")
        .json(&body)
        .send()
        .await?;

    if resp.status().is_success() {
        output.success(&format!("Created stack: {}", name));
        output.result(&format!("Adapters: {}", adapters.join(", ")));
        output.result(&format!("Workflow: {}", workflow));
    } else {
        output.error("Failed to create stack");
    }

    Ok(())
}

// ... implement list_stacks and delete_stack ...
```

**Task 2.2: Wire up stack command in main.rs**

File: `crates/adapteros-cli/src/main.rs`

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Adapter stack management
    #[command(subcommand)]
    Stack(stack::StackCommand),
}

// In main() match:
Commands::Stack(cmd) => stack::handle_stack_command(cmd, &output).await?,
```

### Phase 3: Testing (Week 2)

**Task 3.1: Integration tests for new commands**

File: `tests/playbook_cli_integration.rs`

```rust
#[tokio::test]
async fn test_adapter_evict_command() {
    // Test evict command
    let output = run_cli_command(["adapter", "evict", "test-adapter"]).await;
    assert!(output.contains("Evicted adapter"));
}

#[tokio::test]
async fn test_adapter_list_pinned_only() {
    // Test pinned-only filter
    let output = run_cli_command(["adapter", "list", "--pinned-only"]).await;
    // Verify only pinned adapters shown
}

#[tokio::test]
async fn test_stack_create() {
    // Test stack creation
    let output = run_cli_command([
        "stack", "create",
        "--name", "test-stack",
        "--adapters", "adapter1,adapter2",
        "--workflow", "Sequential"
    ]).await;
    assert!(output.contains("Created stack"));
}
```

---

## Testing Strategy

### Unit Tests

- ✅ Command parsing (clap)
- ✅ Adapter ID validation
- ✅ JSON serialization
- ⚠️ Mock socket communication

### Integration Tests

- ⚠️ End-to-end playbook execution
- ⚠️ Multi-command workflows
- ⚠️ Error handling

### Scenario Tests

Create scenario tests that map directly to playbooks:

**File:** `tests/playbook_scenarios.rs`

```rust
#[tokio::test]
async fn test_playbook_1_tenant_onboarding() {
    // Execute all steps from Playbook 1
    // Verify each step succeeds
}

#[tokio::test]
async fn test_playbook_2_adapter_rollback() {
    // Execute rollback scenario
    // Verify old adapter is restored
}

// ... one test per playbook ...
```

---

## Documentation Updates

### Files to Update

1. **README.md** - Add references to operator playbooks
2. **CLAUDE.md** - Add CLI coverage section
3. **crates/adapteros-cli/README.md** - Document new commands

### Command Documentation

Each new command needs:
- Help text with examples
- Man page entry
- Quick reference in README

---

## Success Criteria

### Phase 1 Complete When:
- ✅ `aosctl adapter evict` command works
- ✅ `aosctl adapter list --pinned-only` works
- ✅ Unit tests pass
- ✅ Integration tests added

### Phase 2 Complete When:
- ✅ `aosctl stack create/list/delete` commands work
- ✅ All playbooks executable without SQL
- ✅ Integration tests pass

### Full Coverage When:
- ✅ All playbook commands implemented
- ✅ 100% command coverage
- ✅ Integration tests for each playbook
- ✅ Documentation complete

---

## Next Steps

1. **Implement Priority 1 commands** (this week)
2. **Add integration tests** (this week)
3. **Implement Priority 2 commands** (next week)
4. **Generate scenario tests from playbooks** (next week)
5. **Update documentation** (ongoing)

---

**Status:** Ready for implementation
**Owner:** Development team
**Review Date:** 2025-01-23
