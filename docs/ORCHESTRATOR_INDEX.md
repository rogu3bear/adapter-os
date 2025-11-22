# Orchestrator Gates - Complete Documentation Index

## Quick Links

| Audience | Document | Purpose |
|----------|----------|---------|
| **Developers** | [ORCHESTRATOR_DEPENDENCY_CHECKS.md](./ORCHESTRATOR_DEPENDENCY_CHECKS.md) | Technical deep-dive on architecture and implementation |
| **Operators** | [ORCHESTRATOR_OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md) | Troubleshooting, best practices, and operational guidance |
| **Integrators** | [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md) | Step-by-step guide for adding new gates |
| **Everyone** | [ORCHESTRATOR_CHANGES_SUMMARY.md](../ORCHESTRATOR_CHANGES_SUMMARY.md) | Overview of all changes made |

## What's New?

The orchestrator promotion gates system now includes:

1. **Runtime Dependency Checking** - Automatic validation of required paths, tools, and configurations
2. **Graceful Degradation** - Gates continue with reduced functionality when optional dependencies are missing
3. **Fallback Path Resolution** - Automatic discovery of alternate paths when primary locations unavailable
4. **Comprehensive Reporting** - Dependency status visible in gate reports (JSON and Markdown)
5. **Operator Guidance** - Clear messaging about missing dependencies and remediation steps

## Document Overview

### For Technical Implementation

**→ [ORCHESTRATOR_DEPENDENCY_CHECKS.md](./ORCHESTRATOR_DEPENDENCY_CHECKS.md)**

Read this if you:
- Need to understand the architecture
- Are adding a new gate or dependency
- Want to know how fallback resolution works
- Need to integrate the system with other components

Contains:
- Architecture overview
- Component descriptions (DependencyChecker, GateDependencies, etc.)
- Gate dependency definitions
- Configuration options
- Integration patterns
- Report structure
- Troubleshooting for developers

### For Operational Use

**→ [ORCHESTRATOR_OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md)**

Read this if you:
- Need to run promotion gates
- A gate is failing and you need to fix it
- Want to understand pre-flight checklist
- Need troubleshooting steps for specific errors

Contains:
- Quick reference table of all gates
- Pre-flight checklist
- How to run gates with various options
- Common issues and solutions (8+ scenarios)
- Environment variables
- Report interpretation
- Best practices
- Monitoring and alerting
- Debug mode instructions

### For Adding New Gates

**→ [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md)**

Read this if you:
- Are adding a new gate to the orchestrator
- Need to define dependencies for a new gate
- Want to follow best practices
- Need a step-by-step process

Contains:
- Step-by-step integration checklist
- Code templates for new gates
- Dependency definition examples
- Testing requirements
- Documentation requirements
- Validation procedures
- Common patterns
- File structure reference

### For Project Overview

**→ [ORCHESTRATOR_CHANGES_SUMMARY.md](../ORCHESTRATOR_CHANGES_SUMMARY.md)**

Read this if you:
- Want a high-level overview of all changes
- Need to understand what was modified
- Want to see summary statistics
- Need to check backward compatibility
- Want usage examples

Contains:
- Overview of all changes
- New modules and files
- Modified files list
- Key features summary
- Dependency definitions table
- Usage examples
- Benefits and backward compatibility
- Testing information
- Future enhancements

## Implementation Status

### New Files
- ✅ `/crates/adapteros-orchestrator/src/gates/dependencies.rs` - Dependency checker
- ✅ `/docs/ORCHESTRATOR_DEPENDENCY_CHECKS.md` - Technical documentation
- ✅ `/docs/ORCHESTRATOR_OPERATOR_GUIDE.md` - Operator guide
- ✅ `/docs/ORCHESTRATOR_INDEX.md` - This file
- ✅ `/ORCHESTRATOR_CHANGES_SUMMARY.md` - Change summary
- ✅ `/crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md` - Integration guide

### Modified Files
- ✅ `/crates/adapteros-orchestrator/src/gates/mod.rs` - Export types
- ✅ `/crates/adapteros-orchestrator/src/lib.rs` - Config + integration
- ✅ `/crates/adapteros-orchestrator/src/report.rs` - Report generation
- ✅ `/crates/adapteros-orchestrator/src/gates/determinism.rs` - Updated
- ✅ `/crates/adapteros-orchestrator/src/gates/security.rs` - Updated
- ✅ `/crates/adapteros-orchestrator/src/gates/metallib.rs` - Updated
- ✅ `/crates/adapteros-orchestrator/src/gates/telemetry.rs` - Updated
- ✅ `/crates/adapteros-orchestrator/src/gates/sbom.rs` - Updated

### Gates with Dependency Support
- ✅ DeterminismGate - Replay bundle path resolution
- ✅ SecurityGate - Tool availability checks
- ✅ MetallibGate - Metallib location resolution
- ✅ TelemetryGate - Telemetry directory resolution
- ✅ MetricsGate - Database connectivity
- ✅ PerformanceGate - Database connectivity
- ✅ SbomGate - SBOM manifest validation

## Quick Start

### For Operators Running Gates

```bash
# 1. Check dependencies are available
./target/release/aosctl gates check-deps --all

# 2. Run gates for your CPID
./target/release/aosctl gates run --cpid my-cpid

# 3. View detailed report
./target/release/aosctl gates run --cpid my-cpid --output json | jq

# 4. If failures, see OPERATOR_GUIDE.md troubleshooting
```

### For Developers Adding Gates

1. Read [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md)
2. Follow the 7-step process
3. Run validation checks
4. Commit with documentation

### For Understanding the System

1. Start with [ORCHESTRATOR_CHANGES_SUMMARY.md](../ORCHESTRATOR_CHANGES_SUMMARY.md) for overview
2. Read [ORCHESTRATOR_DEPENDENCY_CHECKS.md](./ORCHESTRATOR_DEPENDENCY_CHECKS.md) for architecture
3. Reference [ORCHESTRATOR_OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md) for specific gates

## Key Concepts

### Gate Severity

- **Critical:** Missing dependencies block promotion (production safety)
- **Warning:** Missing dependencies logged but don't block (informational gates)

### Degradation Levels

- **0 (None):** All dependencies available
- **1 (Partial):** Some optional dependencies missing
- **2 (Critical):** Required dependencies missing

### Fallback Resolution

Gates automatically try multiple paths:
```
/srv/aos/bundles (primary)
  → var/bundles (fallback 1)
    → bundles (fallback 2)
      → target/bundles (fallback 3)
        → ERROR
```

### Configuration Modes

- **Production:** All dependencies required, strict checking
- **Staging:** Most dependencies required, some optional
- **Development:** Degraded mode allowed, flexible paths

## Common Tasks

### I need to...

| Task | Document | Section |
|------|----------|---------|
| Run promotion gates | OPERATOR_GUIDE.md | Running Gates |
| Fix gate failure | OPERATOR_GUIDE.md | Common Issues & Solutions |
| Add new gate | INTEGRATION_CHECKLIST.md | Steps 1-7 |
| Understand gate logic | DEPENDENCY_CHECKS.md | Gate Dependency Definitions |
| Set up environment | OPERATOR_GUIDE.md | Pre-Flight Checklist |
| Debug gate issues | OPERATOR_GUIDE.md | Troubleshooting / Debug Mode |
| Understand architecture | DEPENDENCY_CHECKS.md | Architecture |
| Configure gates | DEPENDENCY_CHECKS.md | Configuration |
| View gate report | OPERATOR_GUIDE.md | Report Interpretation |
| Migrate existing gate | DEPENDENCY_CHECKS.md | Migration Path |

## Examples by Use Case

### Use Case 1: First Time Running Gates

1. Read: [OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md) - Pre-Flight Checklist
2. Run: `./target/release/aosctl gates check-deps --all`
3. Fix any missing dependencies
4. Run: `./target/release/aosctl gates run --cpid my-cpid`
5. Review: Report output

### Use Case 2: Gate Failing

1. Check: Dependency status in report
2. Read: [OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md) - Common Issues
3. Find: Your specific error scenario
4. Follow: Provided solution steps
5. Test: Re-run gates

### Use Case 3: Adding New Gate

1. Study: [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md)
2. Define: Gate dependencies in `dependencies.rs`
3. Implement: Gate logic in `src/gates/newgate.rs`
4. Register: Gate in `Orchestrator::new()`
5. Test: All test cases pass
6. Document: In guides and examples

### Use Case 4: Understanding System

1. Read: [CHANGES_SUMMARY.md](../ORCHESTRATOR_CHANGES_SUMMARY.md) - Overview
2. Read: [DEPENDENCY_CHECKS.md](./ORCHESTRATOR_DEPENDENCY_CHECKS.md) - Architecture
3. Browse: Gate implementations in `src/gates/`
4. Review: Examples and patterns

## Support & Help

### Getting Help

| Issue | Solution |
|-------|----------|
| Gate is failing | See OPERATOR_GUIDE.md "Common Issues & Solutions" |
| Can't run gates | See OPERATOR_GUIDE.md "Pre-Flight Checklist" |
| Need to add gate | See INTEGRATION_CHECKLIST.md steps 1-7 |
| Understanding output | See OPERATOR_GUIDE.md "Report Interpretation" |
| Technical deep-dive | See DEPENDENCY_CHECKS.md sections on architecture |
| What changed | See CHANGES_SUMMARY.md overview |

### Documentation Structure

```
docs/
├── ORCHESTRATOR_INDEX.md                    # YOU ARE HERE
├── ORCHESTRATOR_DEPENDENCY_CHECKS.md        # Technical
├── ORCHESTRATOR_OPERATOR_GUIDE.md           # Operational
└── ... other docs

crates/adapteros-orchestrator/
├── INTEGRATION_CHECKLIST.md                 # Integration
├── src/
│   ├── gates/
│   │   └── dependencies.rs                  # Implementation
│   └── ... gate files
└── ... other code

ORCHESTRATOR_CHANGES_SUMMARY.md              # Overview
```

## Related Files in Codebase

### Core Implementation
- `crates/adapteros-orchestrator/src/gates/dependencies.rs` - DependencyChecker
- `crates/adapteros-orchestrator/src/lib.rs` - Orchestrator integration
- `crates/adapteros-orchestrator/src/report.rs` - Report generation

### Gate Implementations
- `crates/adapteros-orchestrator/src/gates/determinism.rs`
- `crates/adapteros-orchestrator/src/gates/security.rs`
- `crates/adapteros-orchestrator/src/gates/metallib.rs`
- `crates/adapteros-orchestrator/src/gates/telemetry.rs`
- `crates/adapteros-orchestrator/src/gates/metrics.rs`
- `crates/adapteros-orchestrator/src/gates/performance.rs`
- `crates/adapteros-orchestrator/src/gates/sbom.rs`

### Documentation
- This file (index)
- Technical documentation
- Operator guide
- Integration checklist
- Change summary

## Version & Compatibility

- **Implementation:** Version 1.0
- **Backward Compatible:** Yes - all changes additive
- **Requires:** AdapterOS infrastructure
- **Tested On:** macOS 14+, Linux (Ubuntu 22.04+)

## Next Steps

1. **To Get Started:** Read [ORCHESTRATOR_OPERATOR_GUIDE.md](./ORCHESTRATOR_OPERATOR_GUIDE.md)
2. **For Understanding:** Read [ORCHESTRATOR_DEPENDENCY_CHECKS.md](./ORCHESTRATOR_DEPENDENCY_CHECKS.md)
3. **For Integration:** Read [INTEGRATION_CHECKLIST.md](../crates/adapteros-orchestrator/INTEGRATION_CHECKLIST.md)
4. **For Overview:** Read [ORCHESTRATOR_CHANGES_SUMMARY.md](../ORCHESTRATOR_CHANGES_SUMMARY.md)

---

Last Updated: 2025-11-21
Maintained by: Development Team
