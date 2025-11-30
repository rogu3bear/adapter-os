# CLAUDE.md Patterns Analysis

**Purpose:** Analysis of what CLAUDE.md actually says vs. what the codebase does

**Last Updated:** 2025-01-27

---

## What CLAUDE.md Says

### 1. Lifecycle Manager Usage

**Location:** CLAUDE.md lines 269-275

```rust
use adapteros_lora_lifecycle::LifecycleManager;
let manager = LifecycleManager::new_with_db(adapter_names, &policies, path, telemetry, k, db);
manager.record_router_decision(&selected).await?; // Auto-promote
manager.check_memory_pressure(total_mem, 0.85).await?; // Auto-evict
```

**What it says:**
- ✅ Use `LifecycleManager::new_with_db()` (with database integration)
- ✅ Use `record_router_decision()` for auto-promotion
- ✅ Use `check_memory_pressure()` for auto-eviction

**What it doesn't say:**
- ❌ How to manually load adapters via API
- ❌ How to manually promote/demote adapters
- ❌ Whether handlers should update database separately
- ❌ What methods update database automatically

**Gap:** CLAUDE.md only shows router-driven and memory-pressure-driven operations, not manual API operations.

---

### 2. Database Access Pattern

**Location:** CLAUDE.md lines 479-483

```rust
query("SELECT * FROM adapters WHERE tenant_id = ?").bind(&tenant_id).fetch_all(&db.pool).await
    .map_err(|e| AosError::Database(format!("Query failed: {}", e)))?;
```

**What it says:**
- ✅ Direct SQL queries are acceptable
- ✅ Use `db.pool()` to access SQLite pool
- ✅ Map errors to `AosError::Database`

**What it doesn't say:**
- ❌ When to use `Db` trait methods vs direct SQL
- ❌ Whether direct SQL is preferred or just acceptable
- ❌ What operations should go through `Db` trait

**Gap:** CLAUDE.md shows direct SQL as the example pattern, not `Db` trait methods.

---

### 3. Async Task Spawning

**Location:** CLAUDE.md lines 485-488

```rust
spawn(async move { if let Err(e) = do_work().await { error!(error = %e, "Task failed"); } });
```

**What it says:**
- ✅ Use generic `spawn()` for async tasks
- ✅ Handle errors with logging

**What it doesn't say:**
- ❌ When to use `spawn_deterministic()` vs `spawn()`
- ❌ What contexts require deterministic execution
- ❌ How to determine if a task needs determinism

**Gap:** CLAUDE.md shows generic `spawn()` pattern, but codebase has deterministic executor that should be used in some contexts.

---

### 4. Compilation Quality Warning

**Location:** CLAUDE.md lines 70-74

> **IMPORTANT:** Do not confuse "make it compile" with "make it work." If code compiles but the architecture is incompatible with a feature or service, this is worse than compilation errors because:
> - Compilation errors block progress visibly
> - Runtime panics create silent failures that ship to production
> - Always verify runtime correctness, not just compilation success

**What it says:**
- ✅ Code that compiles but violates architecture is worse than compilation errors
- ✅ Runtime correctness matters more than compilation success

**Implication:** This is exactly the "slop" problem - code compiles but doesn't follow architecture.

---

## What CLAUDE.md Doesn't Say

### Missing Guidance:

1. **Lifecycle Manager Integration:**
   - How should handlers load adapters?
   - Should handlers update database separately or rely on lifecycle manager?
   - What's the correct pattern for manual state transitions?

2. **Database Layer:**
   - When should `Db` trait methods be used vs direct SQL?
   - What's the abstraction boundary?
   - Are direct SQL queries acceptable or just tolerated?

3. **Deterministic Execution:**
   - When is deterministic execution required?
   - When is `tokio::spawn` acceptable?
   - What's the decision criteria?

4. **Architectural Patterns:**
   - What are the correct integration patterns?
   - What are examples of correct usage?
   - What are the actual requirements vs preferences?

---

## Implications for De-Slop

### The Problem:

**CLAUDE.md is incomplete** - it shows some patterns but doesn't provide comprehensive guidance on:
- Integration patterns
- When to use which abstraction
- Correct vs incorrect usage

**Result:** Developers (and AI assistants) make assumptions, leading to inconsistent patterns.

### What Needs to Happen:

1. **Clarify CLAUDE.md:**
   - Add explicit patterns for handler code
   - Document lifecycle manager integration
   - Specify when to use deterministic execution
   - Clarify database access patterns

2. **Document Correct Patterns:**
   - Show examples of correct handler code
   - Show examples of incorrect handler code
   - Explain why patterns are correct/incorrect

3. **Enforce Patterns:**
   - Add linting rules
   - Add architectural checks
   - Make patterns discoverable

---

## Conclusion

**CLAUDE.md says:**
- Use lifecycle manager for router decisions and memory pressure
- Direct SQL queries are acceptable
- Generic `spawn()` is the pattern shown
- Code that compiles but violates architecture is bad

**CLAUDE.md doesn't say:**
- How to integrate lifecycle manager in handlers
- When to use which abstraction layer
- What the correct patterns are for manual operations

**The gap:** CLAUDE.md provides high-level guidance but lacks detailed integration patterns, leading to inconsistent implementation.

