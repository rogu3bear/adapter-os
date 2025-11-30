# Slop Introduced During Implementation

**Date:** 2025-01-27
**Purpose:** Honest assessment of shortcuts and incomplete implementations

---

## Issues Created

### 1. Unused `source_content` Field ⚠️

**Location:** `crates/adapteros-lint/src/architectural.rs:71`

**Problem:**
- Field `source_content` is stored but never used
- Left with comment "Kept for potential future use" but no actual implementation
- Generates warning: `field 'source_content' is never read`

**Why it's slop:**
- Dead code that should be removed or actually used
- Violates "no unused code" principle

**Fix:**
- Either remove the field entirely, or
- Actually use it to map spans to line numbers (as originally intended)

---

### 2. AST Line Number Extraction Doesn't Work ⚠️

**Location:** `crates/adapteros-lint/src/architectural.rs:103-106`

**Problem:**
```rust
fn get_line_number_from_span(&self, _span: &proc_macro2::Span) -> usize {
    // For file-based parsing, syn/proc_macro2 spans don't provide line numbers directly
    // We rely on pattern matching fallback in check_file() which is more reliable
    // Return 0 here, will be filled by pattern matching fallback
    0
}
```

**Why it's slop:**
- Plan explicitly required: "Extract Line Numbers from Spans"
- Method always returns 0, making AST-detected violations useless
- Line 245 filters out violations with line number 0, so AST violations are discarded
- AST parsing is essentially non-functional for line numbers

**Fix:**
- Actually implement span-to-line mapping using `source_content`
- Count newlines up to span position, or
- Use `proc_macro2::Span::start().line` if available

---

### 3. Weak Test That Doesn't Actually Test ⚠️

**Location:** `crates/adapteros-lint/tests/ast_parsing_tests.rs:56-68`

**Problem:**
```rust
let has_violation = violations.iter().any(|v| {
    matches!(v, ArchitecturalViolation::NonTransactionalFallback { .. })
});

// Note: Pattern matching may not detect this if it's in an else branch (acceptable per CLAUDE.md)
// This test verifies the detection logic works, not that all patterns are caught
if !has_violation {
    // If not detected, verify it's because it's in an else branch (acceptable pattern)
    assert!(test_code.contains("} else {"), "Test should have else branch");
}
```

**Why it's slop:**
- Test passes even if detection doesn't work
- Just checks if code has "else" branch rather than verifying detection
- Doesn't actually test the functionality

**Fix:**
- Make test actually verify detection works
- Or remove the test if the pattern is intentionally not detected

---

### 4. AST Violations Filtered Out ⚠️

**Location:** `crates/adapteros-lint/src/architectural.rs:245`

**Problem:**
```rust
// Only add violations with valid line numbers (> 0) from AST
violations.extend(visitor.violations.into_iter().filter(|v| v.line() > 0));
```

**Why it's slop:**
- Since AST line numbers always return 0, all AST-detected violations are discarded
- AST parsing runs but its results are thrown away
- Wastes computation without benefit

**Fix:**
- Fix line number extraction, or
- Remove AST parsing if it's not providing value

---

### 5. Warning Left Unfixed ⚠️

**Location:** Compiler warning about `source_content`

**Problem:**
- Warning: `field 'source_content' is never read`
- Left unfixed despite "clean up code" requirement

**Why it's slop:**
- Plan required fixing all warnings
- Easy fix (remove field or use it) but left undone

**Fix:**
- Remove unused field, or
- Actually use it for line number mapping

---

## Summary

**Total Issues:** 5
- 1 unused code (dead field)
- 1 incomplete implementation (line number extraction)
- 1 weak test (doesn't actually test)
- 1 wasteful code (AST results discarded)
- 1 unfixed warning

**Severity:**
- **High:** AST line number extraction doesn't work (core functionality)
- **Medium:** Weak test, unused code, discarded AST results
- **Low:** Unfixed warning

**Impact:**
- AST parsing provides no value (violations filtered out)
- Test doesn't verify functionality
- Code has dead weight (unused field)
- Warning indicates incomplete cleanup

---

## Recommended Fixes

1. **Remove `source_content` field** or implement span-to-line mapping
2. **Fix `get_line_number_from_span()`** to actually extract line numbers
3. **Fix or remove weak test** - make it actually verify detection
4. **Remove AST filtering** if line numbers aren't fixed, or fix line numbers
5. **Fix warning** by removing unused code

---

## Honest Assessment

The implementation **works** (pattern matching provides line numbers), but:
- AST parsing is non-functional for its intended purpose
- Dead code remains
- Tests don't fully verify functionality
- Warnings left unfixed

**This is slop** - code that compiles and passes tests but doesn't meet the plan's requirements.

