# Orchestrator Runtime Dependency Checks

## Overview

This document describes the runtime dependency management system for AdapterOS orchestrator promotion gates. The system provides:

1. **Centralized dependency tracking** for all 6 gates
2. **Automatic fallback path resolution** for missing required paths
3. **Graceful degradation** when dependencies are unavailable
4. **Comprehensive reporting** of dependency status and issues

## Architecture

### Dependency Checker Module

Located in `crates/adapteros-orchestrator/src/gates/dependencies.rs`

#### Core Components

**GateDependencies** - Defines requirements per gate:
```rust
pub struct GateDependencies {
    pub gate_id: String,                                    // Gate identifier
    pub required_paths: Vec<String>,                        // Paths that must exist
    pub optional_paths: Vec<(String, Vec<String>)>,       // Fallback chains
    pub required_tools: Vec<String>,                        // CLI tools needed
    pub severity: GateSeverity,                             // Critical/Warning
}
```

**GateSeverity** - Determines failure behavior:
- `Critical`: Missing dependencies block promotion (unless `allow_degraded_mode`)
- `Warning`: Missing dependencies logged but don't block (for informational gates)

**DependencyCheckResult** - Returned from checks:
```rust
pub struct DependencyCheckResult {
    pub gate_id: String,
    pub all_available: bool,                                // All deps found
    pub required_paths: HashMap<String, PathStatus>,
    pub optional_paths: HashMap<String, PathResolution>,
    pub required_tools: HashMap<String, ToolStatus>,
    pub degradation_level: u8,                              // 0=none, 1=partial, 2=critical
    pub messages: Vec<String>,                              // Operator guidance
}
```

**DependencyChecker** - Main resolver:
```rust
pub struct DependencyChecker {
    definitions: HashMap<String, GateDependencies>,
}

impl DependencyChecker {
    pub fn check_gate(&self, gate_id: &str) -> Result<DependencyCheckResult>;
    pub fn check_gates(&self, gate_ids: &[&str]) -> Result<Vec<DependencyCheckResult>>;
    pub fn list_gates(&self) -> Vec<String>;
}
```

## Gate Dependency Definitions

### 1. Determinism Gate

**Required Paths:**
- `/srv/aos/bundles` - Primary replay bundle location

**Optional Paths (Fallbacks):**
- `replay_bundle`: `["var/bundles", "bundles", "target/bundles"]`

**Severity:** Critical

**Usage in Gate:**
```rust
let deps = checker.check_gate("determinism")?;
let bundle_path = if primary_exists {
    primary_path
} else {
    deps.get_resolved_path("replay_bundle")  // Gets first existing fallback
};
```

### 2. Security Gate

**Required Paths:**
- `deny.toml` - Dependency policy configuration

**Required Tools:**
- `cargo` - Rust package manager

**Severity:** Critical

**Graceful Degradation:**
- Skips `cargo-audit` if `cargo` not available
- Skips `cargo-deny` if `deny.toml` missing
- Logs warnings instead of failing

### 3. Metallib Gate

**Required Paths:**
- `crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib` - Metal kernel library

**Optional Paths (Fallbacks):**
- `manifests_dir`: `["manifests", "target/manifests"]`

**Severity:** Critical

**Alternate Paths Checked:**
1. `crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib`
2. `crates/mplora-kernel-mtl/shaders/aos_kernels.metallib` (legacy)
3. `target/shaders/aos_kernels.metallib`

### 4. Telemetry Gate

**Optional Paths (Fallbacks):**
- `telemetry_dir`: `["var/telemetry", ".telemetry", "/var/aos/telemetry"]`
- `bundles_dir`: `["/srv/aos/bundles", "var/bundles", "bundles"]`

**Severity:** Warning

**Graceful Degradation:**
- Attempts fallback paths for telemetry bundles
- Respects `config.require_telemetry_bundles` flag
- Logs warnings for missing bundles if not required

### 5. Metrics Gate

**Required Paths:** None (database only)

**Severity:** Warning

**Dependencies:** Database connectivity (checked at runtime)

### 6. Performance Gate

**Required Paths:** None (database only)

**Severity:** Warning

**Dependencies:** Database connectivity (checked at runtime)

### 7. SBOM Gate

**Required Paths:**
- `target/sbom.spdx.json` - Software Bill of Materials

**Optional Paths (Fallbacks):**
- `sbom_signature`: `["target/sbom.spdx.json.sig"]`

**Severity:** Warning

## Configuration

### OrchestratorConfig Extensions

```rust
pub struct OrchestratorConfig {
    // ... existing fields ...

    /// Skip dependency checks before running gates
    pub skip_dependency_checks: bool,

    /// Allow gates to run with degraded dependencies
    pub allow_degraded_mode: bool,

    /// Require telemetry bundles to exist (Telemetry gate specific)
    pub require_telemetry_bundles: bool,
}
```

### Example Usage

```rust
let config = OrchestratorConfig {
    cpid: "my-cpid".to_string(),
    skip_dependency_checks: false,      // Check dependencies
    allow_degraded_mode: false,         // Fail on critical deps missing
    require_telemetry_bundles: false,   // Optional: allow missing telemetry
    ..Default::default()
};

let orchestrator = Orchestrator::new(config);
let report = orchestrator.run().await?;
```

## Integration with Gates

### Before/After Pattern

Each gate now follows this pattern:

```rust
#[async_trait::async_trait]
impl Gate for MyGate {
    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // 1. Check dependencies
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("my_gate")?;

        if !deps.all_available {
            debug!(messages = ?deps.messages, "Some dependencies missing");
        }

        // 2. Resolve paths with fallbacks
        let path = if primary_exists {
            primary_path
        } else {
            deps.get_resolved_path("key")
                .ok_or_else(|| anyhow!("No valid path found"))?
        };

        // 3. Log fallback usage
        if path != expected_path {
            warn!("Using fallback path: {}", path.display());
        }

        // 4. Continue with gate logic
        Ok(())
    }
}
```

### Updated Gates

1. **DeterminismGate** - `/gates/determinism.rs`
   - Resolves replay bundle path with fallbacks
   - Logs fallback usage

2. **SecurityGate** - `/gates/security.rs`
   - Gracefully skips unavailable tools
   - Logs warnings for missing configurations

3. **MetallibGate** - `/gates/metallib.rs`
   - Tries multiple metallib locations
   - Resolves manifests directory with fallbacks

4. **TelemetryGate** - `/gates/telemetry.rs`
   - Respects `require_telemetry_bundles` config
   - Resolves telemetry directory with fallbacks

5. **SbomGate** - `/gates/sbom.rs`
   - Logs missing signature as warning
   - Validates present SBOM

## Reporting

### Report Integration

The `GateReport` struct extended to include dependency checks:

```rust
pub struct GateReport {
    pub cpid: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependency_checks: Vec<DependencyCheckResult>,
    pub gates: HashMap<String, GateResult>,
    pub all_passed: bool,
}
```

### Markdown Output Example

```markdown
## Dependency Status

| Gate | Dependencies | Degradation | Messages |
|------|--------------|-------------|----------|
| determinism | ✅ All Available | None | None |
| security | ⚠️ Some Missing | Partial | deny.toml not found; skipping cargo-deny |
| metallib | ✅ All Available | None | None |
| telemetry | ✅ All Available | None | None |
| metrics | ✅ All Available | None | None |
| performance | ✅ All Available | None | None |
| sbom | ✅ All Available | None | None |

## Gate Results

| Gate | Status | Message |
|------|--------|---------|
| Determinism | ✅ PASS | Gate passed |
| Security | ✅ PASS | Gate passed |
| ...
```

### JSON Output

Includes full dependency information:
```json
{
  "cpid": "example-cpid",
  "timestamp": "2025-11-21T10:30:00Z",
  "dependency_checks": [
    {
      "gate_id": "determinism",
      "all_available": true,
      "required_paths": {
        "/srv/aos/bundles": {
          "path": "/srv/aos/bundles",
          "exists": true,
          "readable": true
        }
      },
      "optional_paths": {...},
      "required_tools": {},
      "degradation_level": 0,
      "messages": []
    }
  ],
  "gates": {...},
  "all_passed": true
}
```

## Fallback Resolution Strategy

### Priority Chains

Fallback paths are checked in order:

```
1. Primary path (usually hard-coded)
2. First fallback option
3. Second fallback option
4. ... continue until found or exhausted
```

Example for Determinism gate:
```
1. /srv/aos/bundles/{cpid}_replay.ndjson
2. var/bundles/{cpid}_replay.ndjson
3. bundles/{cpid}_replay.ndjson
4. target/bundles/{cpid}_replay.ndjson
```

### Resolution Result

```rust
pub struct PathResolution {
    pub primary_path: String,        // First attempted path
    pub resolved_path: Option<String>,  // Actually found path (if any)
    pub is_fallback: bool,           // Whether using non-primary path
}
```

## Error Handling

### Critical vs. Warning Severity

**Critical Gates** (block promotion if missing):
- Determinism (replay bundles)
- Security (cargo tools)
- Metallib (kernel library)

**Warning Gates** (logged but don't block):
- Telemetry (bundles)
- Metrics (database)
- Performance (database)
- SBOM (manifest)

### Degraded Mode

When `allow_degraded_mode: true`:
- Critical gates can run with missing optional dependencies
- Degradation level tracked (0=none, 1=partial, 2=critical)
- Warnings logged for operators
- Promotion may proceed with caveats

## Migration Path

### For Existing Code

If you have gates that don't use the new system yet:

1. Add dependency check at start of `check()` method
2. Use `deps.get_resolved_path()` for optional paths
3. Log warnings for fallback usage
4. Update error messages to include checked locations

### Example Refactor

**Before:**
```rust
let path = Path::new("/hardcoded/path");
if !path.exists() {
    anyhow::bail!("Path not found");
}
```

**After:**
```rust
let checker = DependencyChecker::new();
let deps = checker.check_gate("my_gate")?;
let path = deps.get_resolved_path("my_key")
    .ok_or_else(|| anyhow!("No valid path found"))?;
```

## Testing

### Unit Tests

Tests included in `dependencies.rs`:
- Dependency checker creation
- Unknown gate handling
- Path status creation
- Degradation level tracking

### Integration Tests

Run gates with mock dependencies:
```bash
cargo test -p adapteros-orchestrator --lib
```

### Manual Testing

Check specific gate dependencies:
```rust
let checker = DependencyChecker::new();
let result = checker.check_gate("determinism")?;
println!("{:?}", result);
```

## Troubleshooting

### Missing Required Path Error

If you see: `Required path not accessible: /srv/aos/bundles`

**Solution:**
1. Check if path exists: `ls -la /srv/aos/bundles`
2. Verify permissions: `ls -ld /srv/aos/bundles`
3. Check fallback paths (see gate definitions above)
4. Consider `allow_degraded_mode: true` if temporary

### Fallback Path Resolution

If paths are resolved unexpectedly:
1. Check `DependencyCheckResult.optional_paths` in report
2. Look for `is_fallback: true` flag
3. Warnings logged: "Using fallback path for..."
4. Verify primary path hasn't moved

### Gate-Specific Issues

- **Determinism:** Check `/srv/aos/bundles/{cpid}_replay.ndjson` exists
- **Security:** Verify `cargo` in PATH, `deny.toml` present
- **Metallib:** Check kernel library location matches crate structure
- **Telemetry:** Confirm telemetry bundles generated before promotion
- **Metrics/Performance:** Verify database accessible at `config.db_path`
- **SBOM:** Run `cargo xtask sbom` to generate manifest

## Future Enhancements

1. **Dynamic gate addition:** Register custom gates with dependencies
2. **Dependency healing:** Automatically create missing directories
3. **Path caching:** Cache resolved paths between checks
4. **Health monitoring:** Track dependency availability over time
5. **Configuration file:** Define custom fallback paths per environment

## References

- Implementation: `crates/adapteros-orchestrator/src/gates/dependencies.rs`
- Gate files: `crates/adapteros-orchestrator/src/gates/*.rs`
- Config: `crates/adapteros-orchestrator/src/lib.rs` (OrchestratorConfig)
- Report: `crates/adapteros-orchestrator/src/report.rs`
