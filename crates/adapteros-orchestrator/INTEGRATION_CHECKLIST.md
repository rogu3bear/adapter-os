# Orchestrator Dependency Checks - Integration Checklist

## For Developers Adding New Gates

Use this checklist when adding a new gate to the orchestrator.

### Step 1: Define Gate Dependencies

Edit `src/gates/dependencies.rs` and add entry to `DependencyChecker::new()`:

```rust
definitions.insert(
    "my_gate".to_string(),
    GateDependencies {
        gate_id: "my_gate".to_string(),
        required_paths: vec![
            "/path/to/required/file".to_string(),
        ],
        optional_paths: vec![
            ("key_name".to_string(), vec![
                "primary/path".to_string(),
                "fallback/path".to_string(),
            ]),
        ],
        required_tools: vec![
            "tool_name".to_string(),
        ],
        severity: GateSeverity::Critical,  // or Warning
    },
);
```

**Checklist:**
- [ ] Identified all required paths
- [ ] Defined fallback paths in priority order
- [ ] Listed required CLI tools
- [ ] Set appropriate severity (Critical/Warning)
- [ ] Documented why each dependency is needed

### Step 2: Create Gate Implementation

File: `src/gates/mygate.rs`

```rust
use crate::{Gate, OrchestratorConfig, DependencyChecker};
use anyhow::Result;
use tracing::{debug, warn};

#[derive(Debug, Default)]
pub struct MyGate;

#[async_trait::async_trait]
impl Gate for MyGate {
    fn name(&self) -> String {
        "My Gate".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // 1. Check dependencies
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("my_gate")?;

        if !deps.all_available {
            debug!(messages = ?deps.messages, "Some dependencies missing");
        }

        // 2. Resolve paths with fallbacks
        let my_path = deps.get_resolved_path("key_name")
            .ok_or_else(|| anyhow::anyhow!("No valid path found"))?;

        // 3. Log fallback usage
        if my_path != "/path/to/required/file" {
            warn!("Using fallback path: {}", my_path);
        }

        // 4. Implement gate logic
        // ... your gate checks here ...

        tracing::info!(gate = %self.name(), "Gate check passed");
        Ok(())
    }
}
```

**Checklist:**
- [ ] Imports DependencyChecker
- [ ] Calls `checker.check_gate("my_gate")?` at start
- [ ] Uses `deps.get_resolved_path()` for optional paths
- [ ] Logs fallback usage with `warn!()`
- [ ] Error messages include checked paths
- [ ] Uses `tracing::info!()` not `println!()`

### Step 3: Export from Module

Edit `src/gates/mod.rs`:

```rust
pub mod mygate;

// In the use block:
pub use mygate::MyGate;
```

**Checklist:**
- [ ] Module declared
- [ ] Type exported
- [ ] Exported in parent lib.rs

### Step 4: Register in Orchestrator

Edit `src/lib.rs` - update `Orchestrator::new()`:

```rust
pub fn new(config: OrchestratorConfig) -> Self {
    let gates: Vec<Box<dyn Gate>> = vec![
        Box::new(DeterminismGate),
        Box::new(MetricsGate::default()),
        Box::new(MetallibGate),
        Box::new(SbomGate),
        Box::new(PerformanceGate::default()),
        Box::new(SecurityGate),
        Box::new(MyGate),  // ADD HERE
    ];

    let dependency_checker = DependencyChecker::new();

    Self {
        config,
        gates,
        dependency_checker,
    }
}
```

Also update `check_dependencies()` method:

```rust
let gate_ids: Vec<&str> = vec![
    "determinism",
    "metrics",
    "metallib",
    "sbom",
    "performance",
    "security",
    "my_gate",  // ADD HERE
];
```

**Checklist:**
- [ ] Gate added to gates vector
- [ ] Gate ID added to check_dependencies() list
- [ ] Gate name matches definition in dependencies.rs

### Step 5: Add Tests

In your gate file or `src/gates/dependencies.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_my_gate_success() {
        let config = OrchestratorConfig::default();
        let gate = MyGate;
        let result = gate.check(&config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_my_gate_missing_dependency() {
        let config = OrchestratorConfig {
            // Setup config to trigger missing dependency
            ..Default::default()
        };
        let gate = MyGate;
        let result = gate.check(&config).await;
        // Should fail or degrade gracefully depending on severity
    }
}
```

**Checklist:**
- [ ] Success case tested
- [ ] Missing required paths tested
- [ ] Missing optional paths tested
- [ ] Fallback resolution tested
- [ ] Error messages checked

### Step 6: Documentation

Add to `/docs/ORCHESTRATOR_DEPENDENCY_CHECKS.md`:

Under "Gate Dependency Definitions" section, add:

```markdown
### N. My Gate

**Required Paths:**
- `/path/to/required/file` - Description

**Optional Paths (Fallbacks):**
- `key_name`: `["primary/path", "fallback/path"]`

**Severity:** Critical/Warning

**Graceful Degradation:**
- Description of what happens if dependencies missing

**Dependencies:**
- Description of required services/tools
```

**Checklist:**
- [ ] Gate purpose documented
- [ ] All paths documented with descriptions
- [ ] Severity explained
- [ ] Fallback behavior documented
- [ ] Example usage shown

### Step 7: Validation

Run these checks:

```bash
# 1. Compilation
cargo check -p adapteros-orchestrator

# 2. Tests
cargo test -p adapteros-orchestrator --lib

# 3. Gate registration
cargo build -p adapteros-orchestrator 2>&1 | grep -i "mygate\|error"

# 4. Dependency check
cargo run -p adapteros-orchestrator -- gates check-deps --gate my_gate

# 5. Full run
cargo run -p adapteros-orchestrator -- gates run --cpid test-cpid
```

**Checklist:**
- [ ] Code compiles without warnings (in gate files)
- [ ] All tests pass
- [ ] Gate registers in orchestrator
- [ ] Dependency check works
- [ ] Gate runs in orchestrator

## For Operators Troubleshooting Gates

### Quick Diagnostics

When a gate fails, follow this process:

**Step 1: Check Dependency Status**
```bash
./target/release/aosctl gates check-deps --gate {GATE_NAME}
```

Look for:
- `all_available: true` → Dependencies OK, issue is in gate logic
- `all_available: false` → Missing dependencies, check messages
- `degradation_level: 2` → Critical path missing, gate will fail

**Step 2: Check Resolved Paths**
In the `optional_paths` section:
- `resolved_path: Some("...")` → Fallback path found, will use it
- `resolved_path: None` → No valid path found, gate will fail

**Step 3: Review Messages**
Common messages:
- `"Required path not accessible"` → Create/fix path
- `"Using fallback path"` → OK, less efficient, but functional
- `"Tool not available"` → Install tool or skip check

**Step 4: Run with Debug Logging**
```bash
RUST_LOG=debug ./target/release/aosctl gates run --cpid {CPID} 2>&1 | grep {GATE_NAME}
```

**Step 5: Check Detailed Report**
```bash
./target/release/aosctl gates run --cpid {CPID} --output json | jq '.dependency_checks[] | select(.gate_id == "{GATE_NAME}")'
```

## File Structure Reference

```
crates/adapteros-orchestrator/
├── src/
│   ├── lib.rs                    # OrchestratorConfig, Orchestrator
│   ├── gates/
│   │   ├── mod.rs               # Module declarations
│   │   ├── dependencies.rs       # DependencyChecker (NEW)
│   │   ├── determinism.rs        # DeterminismGate (UPDATED)
│   │   ├── security.rs           # SecurityGate (UPDATED)
│   │   ├── metallib.rs           # MetallibGate (UPDATED)
│   │   ├── telemetry.rs          # TelemetryGate (UPDATED)
│   │   ├── metrics.rs            # MetricsGate
│   │   ├── performance.rs        # PerformanceGate
│   │   └── sbom.rs               # SbomGate (UPDATED)
│   └── report.rs                 # GateReport (UPDATED)
└── INTEGRATION_CHECKLIST.md      # This file

docs/
├── ORCHESTRATOR_DEPENDENCY_CHECKS.md    # Technical docs (NEW)
├── ORCHESTRATOR_OPERATOR_GUIDE.md       # Operator guide (NEW)
└── ...

ORCHESTRATOR_CHANGES_SUMMARY.md          # Summary of all changes (NEW)
```

## Quick Reference: Common Patterns

### Pattern 1: Simple Required Path

```rust
let checker = DependencyChecker::new();
let deps = checker.check_gate("my_gate")?;

let path = Path::new("required/path");
if !path.exists() {
    anyhow::bail!("Required file not found: {}", path.display());
}
```

### Pattern 2: Fallback Path Resolution

```rust
let deps = checker.check_gate("my_gate")?;

let resolved = deps.get_resolved_path("my_key")
    .ok_or_else(|| anyhow!("No valid path found"))?;

let path = Path::new(&resolved);
```

### Pattern 3: Graceful Degradation (Optional)

```rust
let deps = checker.check_gate("my_gate")?;

if !deps.all_available {
    warn!("Operating in degraded mode: {:?}", deps.messages);
    // Continue with reduced functionality
}
```

### Pattern 4: Tool Availability Check

```rust
let deps = checker.check_gate("security")?;

if let Some(tool_status) = deps.required_tools.get("cargo") {
    if !tool_status.available {
        warn!("cargo not available, skipping check");
        return Ok(());
    }
}
```

## Maintenance Tasks

### Adding a New Fallback Path

1. Edit `dependencies.rs`
2. Update the `optional_paths` vector for the gate
3. Test with `cargo check`
4. Update documentation

### Changing Path Requirements

1. Update both `dependencies.rs` AND gate implementation
2. Ensure consistency between definition and usage
3. Update docs with new paths
4. Test thoroughly

### Adding New Gate

Use the full checklist above.

## Support

- **Technical questions:** See `/docs/ORCHESTRATOR_DEPENDENCY_CHECKS.md`
- **Operational questions:** See `/docs/ORCHESTRATOR_OPERATOR_GUIDE.md`
- **Integration help:** See this file
- **Gate-specific help:** Check gate implementation in `src/gates/`

## Verification Checklist (Before Committing)

- [ ] Code compiles: `cargo check -p adapteros-orchestrator`
- [ ] Tests pass: `cargo test -p adapteros-orchestrator --lib`
- [ ] No clippy warnings: `cargo clippy -p adapteros-orchestrator`
- [ ] Dependencies documented: `dependencies.rs` updated
- [ ] Gate implementation follows patterns
- [ ] Report handling correct (if adding new fields)
- [ ] Documentation updated
- [ ] Examples in docs are accurate
- [ ] Test coverage adequate
- [ ] Backward compatible (if modifying existing code)
