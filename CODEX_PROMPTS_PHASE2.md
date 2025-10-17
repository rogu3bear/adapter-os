# AdapterOS Codex Prompts - Phase 2

**Date**: 2025-10-17  
**Status**: Ready for parallel execution  
**Context**: Phase 1 complete (6 PRs integrated successfully)

---

## Overview

These prompts address remaining gaps identified during Phase 1 integration. Each prompt is:
- **< 500 lines** of implementation
- **Non-conflicting** with integrated work
- **Standards-compliant** per CLAUDE.md
- **Independently testable**

---

## Prompt 1: Fix Pre-existing Test Failures

```
Fix 20 pre-existing test failures in adapteros-lora-worker.

TARGET: crates/adapteros-lora-worker/

REQUIREMENTS:
- Run `cargo test --package adapteros-lora-worker`
- Fix all failing tests (currently 20 failures)
- Focus on mock data issues and outdated test expectations
- Do NOT modify production code unless absolutely necessary
- Keep fixes focused on test code only
- Add comments explaining what was broken and how it was fixed
- Keep under 300 lines total changes

INTEGRATION:
- Tests should pass after fixes
- No production code changes
- Follow existing test patterns
- Use existing mocks and fixtures

Create focused PR with just test fixes.
```

---

## Prompt 2: CLI Output Writer Table Method

```
Implement missing table() method for CLI output writer.

TARGET: crates/adapteros-cli/src/output.rs

REQUIREMENTS:
- Add `table()` method to `OutputWriter` struct (currently missing)
- Support both human-readable and JSON output modes
- Use comfy_table for formatted tables in human mode
- Support column alignment and formatting options
- Add helper methods: `table_row()`, `table_header()`, `table_footer()`
- Keep under 200 lines
- Add unit tests

INTEGRATION:
- Used by various CLI commands expecting table output
- Follow existing output mode patterns
- Use existing OutputMode enum
- Integrate with is_json() checks

EXAMPLE USAGE:
```rust
let mut writer = OutputWriter::new(OutputMode::Human);
writer.table(vec!["ID", "Name", "Status"], vec![
    vec!["1", "adapter_a", "active"],
    vec!["2", "adapter_b", "inactive"],
]);
```

Create focused PR with just this enhancement.
```

---

## Prompt 3: Production Monitoring Telemetry

```
Add production-ready monitoring telemetry without deleting existing code.

TARGET: crates/adapteros-telemetry/src/monitoring.rs (new file)

REQUIREMENTS:
- Create NEW monitoring module (don't modify existing files)
- Add health check event types
- Add performance threshold monitoring
- Add alert event types for policy violations
- Integrate with existing TelemetryWriter
- Use canonical JSON format
- Keep under 400 lines
- Add comprehensive tests

INTEGRATION:
- Export from crates/adapteros-telemetry/src/lib.rs
- Use existing event infrastructure
- Follow existing telemetry patterns
- NO deletions of existing code

EVENTS TO ADD:
- health_check (system health status)
- performance_alert (threshold violations)
- policy_violation_alert (policy pack breaches)
- memory_pressure_alert (memory warnings)

Create focused PR with just this addition.
```

---

## Prompt 4: Adapter Activation Tracking

```
Implement adapter activation percentage tracking and updating.

TARGET: crates/adapteros-lora-lifecycle/src/activation_tracker.rs (new file)

REQUIREMENTS:
- Create ActivationTracker struct
- Track adapter selection frequency
- Calculate rolling activation percentages
- Update database with activation_pct
- Integrate with router decisions
- Evict adapters below 2% activation (per Policy 19)
- Keep under 350 lines
- Add tests

INTEGRATION:
- Use adapteros_db for persistence
- Hook into router selection events
- Update adapters table activation_pct column
- Follow existing lifecycle patterns

DATABASE SCHEMA (already exists):
```sql
ALTER TABLE adapters ADD COLUMN activation_pct REAL DEFAULT 0.0;
```

Create focused PR with just this functionality.
```

---

## Prompt 5: Enhanced Error Context

```
Add enhanced error context throughout the codebase.

TARGET: crates/adapteros-core/src/error.rs + selected call sites

REQUIREMENTS:
- Add `context()` and `with_context()` methods to AosError
- Implement error chain tracking
- Add structured context fields (file, line, function)
- Update 10-15 critical error sites to use new context
- Keep error handling zero-cost when not triggered
- Keep under 400 lines
- Add tests

INTEGRATION:
- Extend existing AosError enum
- Follow anyhow/eyre patterns
- Use std::backtrace when available
- Integrate with existing error handling

EXAMPLE USAGE:
```rust
db.get_adapter(id)
    .context("Failed to retrieve adapter")
    .with_context(|| format!("adapter_id={}", id))?;
```

Create focused PR with just error enhancements.
```

---

## Prompt 6: Adapter Dependency Resolution

```
Implement adapter dependency resolution and validation.

TARGET: crates/adapteros-registry/src/dependencies.rs (new file)

REQUIREMENTS:
- Create DependencyResolver struct
- Validate adapter dependencies before loading
- Check for circular dependencies
- Verify required adapters are available
- Check for conflicting adapters
- Generate dependency graph
- Keep under 350 lines
- Add comprehensive tests

INTEGRATION:
- Use existing AdapterDependencies from manifest
- Integrate with registry database
- Hook into adapter loading pipeline
- Follow existing registry patterns

MANIFEST SCHEMA (already exists):
```yaml
dependencies:
  base_model: "qwen2.5-7b"
  requires_adapters: ["code_lang_v1"]
  conflicts_with: ["legacy_adapter_v1"]
```

Create focused PR with just dependency resolution.
```

---

## Prompt 7: Batch Inference API

```
Add batch inference API endpoint for efficient multi-request processing.

TARGET: crates/adapteros-server-api/src/handlers/batch.rs (new file)

REQUIREMENTS:
- Create batch inference handler
- Accept array of inference requests
- Process requests efficiently (shared model state)
- Return array of responses with request IDs
- Implement max batch size limit (32 requests)
- Add batch timeout handling
- Keep under 400 lines
- Add integration tests

INTEGRATION:
- Add routes to existing server
- Use existing InferRequest/Response types
- Integrate with worker pipeline
- Follow existing auth/validation patterns

API SCHEMA:
```json
{
  "requests": [
    {"id": "req-1", "prompt": "...", "max_tokens": 100},
    {"id": "req-2", "prompt": "...", "max_tokens": 50}
  ]
}
```

Create focused PR with just batch API.
```

---

## Prompt 8: Adapter Performance Profiler

```
Implement adapter-specific performance profiling.

TARGET: crates/adapteros-profiler/src/adapter_profiler.rs (new file)

REQUIREMENTS:
- Create AdapterProfiler struct
- Track per-adapter inference latency
- Track per-adapter memory usage
- Track adapter selection frequency
- Generate performance reports
- Identify slow/problematic adapters
- Keep under 350 lines
- Add tests

INTEGRATION:
- Integrate with existing profiler crate
- Hook into inference pipeline
- Use existing metrics collection
- Export JSON performance reports

METRICS TO TRACK:
- avg_latency_ms (per adapter)
- p95_latency_ms (per adapter)
- memory_usage_mb (per adapter)
- selection_count (how often selected)
- error_rate (failures per adapter)

Create focused PR with just profiling enhancement.
```

---

## Prompt 9: Configuration Validation

```
Add configuration file validation with helpful error messages.

TARGET: crates/adapteros-config/src/validation.rs (new file)

REQUIREMENTS:
- Create ConfigValidator struct
- Validate manifest YAML/TOML files
- Check for required fields
- Validate value ranges (K bounds, ranks, etc.)
- Provide helpful error messages with line numbers
- Suggest corrections for common mistakes
- Keep under 300 lines
- Add comprehensive tests

INTEGRATION:
- Hook into config loading pipeline
- Use existing config types
- Integrate with CLI config commands
- Follow existing validation patterns

VALIDATIONS TO ADD:
- K bounds (1 <= K <= max_adapters)
- Rank ranges (4 <= rank <= 128)
- Alpha validation (alpha >= rank)
- Hash format validation (b3:...)
- Path existence checks

Create focused PR with just config validation.
```

---

## Prompt 10: Graceful Shutdown Handler

```
Implement graceful shutdown with resource cleanup.

TARGET: crates/adapteros-server/src/shutdown.rs (new file)

REQUIREMENTS:
- Create ShutdownHandler struct
- Handle SIGTERM and SIGINT signals
- Drain in-flight requests (max 30s wait)
- Save adapter state to database
- Close database connections cleanly
- Flush telemetry buffers
- Keep under 300 lines
- Add integration tests

INTEGRATION:
- Integrate with server main loop
- Use tokio signal handling
- Coordinate with worker shutdown
- Follow existing shutdown patterns

SHUTDOWN SEQUENCE:
1. Stop accepting new requests
2. Wait for in-flight requests (max 30s)
3. Save adapter activation stats
4. Flush telemetry buffers
5. Close database connections
6. Exit cleanly

Create focused PR with just shutdown handling.
```

---

## Parallel Execution Safety

### No File Overlap
- Each prompt targets different files/modules
- No conflicts between prompts
- Can run truly in parallel

### Clear Integration Points
- All prompts reference existing infrastructure
- Well-defined interfaces
- Minimal cross-dependencies

### Size Constraints
- All prompts < 500 lines
- Focused on single responsibility
- Easy to review and verify

---

## Expected Output

- 10 focused PRs
- Total ~3,500 lines of new functionality
- Zero conflicts with Phase 1 integration
- All standards-compliant per CLAUDE.md
- Immediately testable and reviewable

---

## Success Criteria

### Per Prompt:
- [ ] Compiles without errors
- [ ] Tests pass
- [ ] Under line limit
- [ ] No conflicts with existing code
- [ ] Follows CLAUDE.md standards

### Overall Phase 2:
- [ ] All 10 PRs integrated
- [ ] Full workspace compilation
- [ ] All tests passing
- [ ] Production-ready features added
- [ ] No regressions from Phase 1

---

## Integration Order (Recommended)

1. **Prompt 1** (Fix tests) - Cleans up test suite
2. **Prompt 2** (CLI table) - Immediate usability improvement
3. **Prompt 5** (Error context) - Foundation for better debugging
4. **Prompt 9** (Config validation) - Prevents user errors
5. **Prompt 3** (Monitoring) - Production observability
6. **Prompt 4** (Activation tracking) - Policy compliance
7. **Prompt 6** (Dependencies) - Adapter safety
8. **Prompt 8** (Profiling) - Performance optimization
9. **Prompt 7** (Batch API) - Feature enhancement
10. **Prompt 10** (Shutdown) - Production reliability

---

## Notes for Codex

- **Additive only**: No deletions of existing code
- **Test thoroughly**: Every prompt needs tests
- **Follow patterns**: Match existing code style
- **Document changes**: Add inline comments
- **Keep focused**: Don't expand scope beyond prompt

---

## Anti-Patterns to Avoid

❌ **Don't**:
- Delete working code to replace it
- Make changes outside target files
- Exceed line limits
- Skip tests
- Ignore standards

✅ **Do**:
- Add new functionality carefully
- Integrate with existing infrastructure
- Write comprehensive tests
- Follow CLAUDE.md policy packs
- Keep changes minimal and focused

---

**Ready for Codex parallel execution! 🚀**

