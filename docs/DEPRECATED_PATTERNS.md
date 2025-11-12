# Deprecated Code Patterns - Hallucination Prevention Guide

**Purpose:** Document patterns found in deprecated code to prevent similar incomplete/speculative implementations in production code.

**Last Updated:** 2025-01-15

---

## Summary of Deprecated Features

### 1. Experimental Modules (`deprecated/adapteros-experimental/`)

**Status:** Isolated from production build  
**Pattern Issues:**
- ❌ TODOs without completion plans
- ❌ Incomplete implementations with placeholder logic
- ❌ Missing error handling
- ❌ No input validation
- ❌ Placeholder retry logic
- ❌ Missing control plane registration

**Example Anti-Pattern:**
```rust
// ❌ BAD: Placeholder implementation
pub async fn execute(&self) -> Result<()> {
    // TODO: Implement full subsystem initialization
    Ok(())
}

// ✅ GOOD: Complete implementation or explicit failure
pub async fn execute(&self) -> Result<()> {
    self.initialize().await?;
    self.validate().await?;
    Ok(())
}
```

### 2. Federation Daemon (`deprecated/federation/`)

**Status:** Explicitly marked incomplete, not integrated  
**Pattern Issues:**
- ❌ Missing daemon lifecycle management
- ❌ Incomplete cross-host chain verification
- ❌ No secure enclave integration (marked "future enhancement")
- ❌ Missing key rotation implementation

**Key Anti-Patterns:**
- Features marked as "future enhancement" without implementation
- Chain verification logic with gaps
- Missing automated daemon startup/shutdown

### 3. Git Subsystem (`deprecated/gitsubsystem/`)

**Status:** Partial implementation with dead code  
**Pattern Issues:**
- ❌ Empty function bodies with TODOs
- ❌ No file change watcher implementation
- ❌ No branch/commit handling
- ❌ Missing API handler implementations

**Example Anti-Pattern:**
```rust
// ❌ BAD: Empty implementation
pub async fn start(&mut self) -> Result<()> {
    info!("Starting Git subsystem components...");
    // TODO: Implement start logic for watcher/daemon
    Ok(())
}

// ✅ GOOD: Real implementation or panic on incomplete
pub async fn start(&mut self) -> Result<()> {
    self.watcher.start().await?;
    self.daemon.start().await?;
    Ok(())
}
```

---

## Common Hallucination Patterns

### Pattern 1: TODO Without Implementation Plan

**Anti-Pattern:**
```rust
// TODO: Implement full subsystem initialization
```

**Red Flag:** TODO with no completion criteria  
**Correct Approach:** Either implement fully or remove the function

### Pattern 2: Placeholder Logic

**Anti-Pattern:**
```rust
pub fn process_request(&self, req: Request) -> Result<Response> {
    // Placeholder retry logic
    sleep(Duration::from_millis(100)).await;
    Ok(Response::default())
}
```

**Red Flag:** Functions that don't perform their stated purpose  
**Correct Approach:** Implement real logic or return an error indicating incomplete

### Pattern 3: Missing Error Handling

**Anti-Pattern:**
```rust
pub async fn load(&self, path: &Path) -> Result<Data> {
    // Missing error handling for edge cases
    let data = fs::read(path)?;
    Ok(data)
}
```

**Red Flag:** Comments noting missing error handling  
**Correct Approach:** Implement comprehensive error handling

### Pattern 4: "Future Enhancement" Without Foundation

**Anti-Pattern:**
```rust
// Future enhancement: Hardware-backed signing
// Note: Not implemented yet
```

**Red Flag:** Features documented but not implemented  
**Correct Approach:** Don't document unimplemented features in production APIs

### Pattern 5: Incomplete Migrations

**Anti-Pattern:**
```rust
// Known Issues: Incomplete migration strategy
pub async fn migrate(&self) -> Result<()> {
    // Missing migration planning
}
```

**Red Flag:** Explicitly incomplete functionality  
**Correct Approach:** Complete or remove migration code

### ❌ Using `println!` for Logging
```rust
// ❌ BAD: println! for logging
pub fn log_event(&self, event: &str) {
    println!("Event: {}", event);
}

// ✅ GOOD: Use tracing with structure (updated example from lora-kernel-mtl/lib.rs L329)
pub fn select_device() -> Result<Device> {
    // ...
    info!(gpu_idx = idx, device_name = %device.name(), "Selected GPU");
    // ...
}
```
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs L329]

---

## Red Flags Checklist

When reviewing code, watch for:

- [ ] TODO comments without completion plans
- [ ] Functions with empty bodies or placeholder logic
- [ ] Comments stating "incomplete", "missing", or "not implemented"
- [ ] "Future enhancement" documentation for unbuilt features
- [ ] Known issues documented in code comments
- [ ] Placeholder return values (`Ok(())`, `Response::default()`)
- [ ] Missing error handling explicitly noted
- [ ] No input validation with explicit note
- [ ] Experimental features in production code paths
- [ ] Features marked "not for production use" in production crates

---

## What Should Be In Production

### ✅ Production-Ready Indicators

1. **Complete Implementations**
   - All public functions have real logic
   - Error handling for all failure modes
   - Input validation for all user inputs

2. **Comprehensive Documentation**
   - API documentation covers all methods
   - Examples show real usage
   - No "future enhancement" notes

3. **Testing**
   - Unit tests for all public functions
   - Integration tests for workflows
   - No skipped or placeholder tests

4. **Error Handling**
   - Typed errors (`AosError` variants)
   - Proper error propagation
   - No unwrapping or panicking on user input

5. **Code Quality**
   - No `#[allow(dead_code)]` attributes
   - No TODO comments
   - No "experimental" flags in production

---

## Enforcement Rules

1. **Never merge incomplete implementations** - Either complete or remove
2. **No TODOs in production code** - Only in feature branches with completion plan
3. **No "experimental" features in production crates** - Use `deprecated/` directory
4. **No placeholder logic** - Real implementations or explicit errors
5. **No "future enhancement" docs** - Only document what exists
6. **All public APIs must be complete** - No partial implementations

---

## Reference: Deprecated Code Locations

- `deprecated/adapteros-experimental/` - Incomplete experimental features
- `deprecated/federation/` - Incomplete federation daemon
- `deprecated/gitsubsystem/` - Partial Git subsystem with TODOs
- `deprecated/EXECUTION_SUMMARY.md` - Historical incomplete work
- `deprecated/PATCH_COMPLETION_REPORT.md` - Historical incomplete patches

---

**Key Takeaway:** If code has TODOs, placeholders, or incomplete implementations, it belongs in `deprecated/`, not in production crates.

