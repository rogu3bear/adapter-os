# Duplication Prevention Guide

**Purpose:** Prevent duplicate code accumulation through automated tooling, architectural patterns, and development practices.

**Last Updated:** 2025-11-07  
**Status:** Active Prevention Strategy

---

## Overview

This guide establishes patterns and tooling to prevent duplicate code from accumulating in the codebase. All agents and developers should follow these practices.

---

## 1. Automated Duplication Detection

### Current Setup

**Tool:** `jscpd` (JavaScript Copy/Paste Detector)  
**Config:** `configs/jscpd.config.json`  
**Threshold:** 0% (strict mode)  
**Min Tokens:** 70 (configurable)

### Pre-Commit Hook

**Location:** `.githooks/pre-commit`  
**Behavior:** Advisory by default, enforcement via `JSCPD_ENFORCE=1`

**Usage:**
```bash
# Install hooks
bash scripts/install_git_hooks.sh

# Enable enforcement (blocks commits with duplicates)
export JSCPD_ENFORCE=1

# Run scan manually
make dup
```

### CI Integration

**Workflow:** `.github/workflows/duplication.yml` (if exists)  
**Enforcement:** Set repository variable `JSCPD_ENFORCE=true` for repo-wide blocking

**Recommendation:** Enable CI enforcement for `main` branch PRs.

---

## 2. Architectural Patterns for Code Sharing

### Pattern 1: Shared Constants (UI Components)

**When to Use:** Repeated string literals, CSS classes, configuration values

**Example:**
```typescript
// ✅ GOOD: Extract to shared constants
// ui/src/components/ui/utils.ts
export const MENU_ANIMATION_CLASSES = "data-[state=open]:animate-in ...";
export const FROST_POPOVER = "bg-popover/95 backdrop-blur-md";

// ❌ BAD: Duplicated across components
// dropdown-menu.tsx
className="data-[state=open]:animate-in ... bg-popover/95 backdrop-blur-md"
// menubar.tsx  
className="data-[state=open]:animate-in ... bg-popover/95 backdrop-blur-md"
```

**Citation:** 【2025-11-07†refactor(ui)†extract-style-constants】

### Pattern 2: Shared Components (React/UI)

**When to Use:** Repeated JSX patterns, indicator components, wrappers

**Example:**
```typescript
// ✅ GOOD: Extract shared component
// ui/src/components/ui/menu-indicators.tsx
export function CheckboxIndicator() {
  return <CheckIcon className="size-4" />;
}

// ❌ BAD: Duplicated in 3+ components
// dropdown-menu.tsx, menubar.tsx, context-menu.tsx all have:
<span><ItemIndicator><CheckIcon className="size-4" /></ItemIndicator></span>
```

**Citation:** 【2025-11-07†refactor(ui)†extract-menu-indicators】

### Pattern 3: Service Modules (Rust)

**When to Use:** Repeated business logic, validation patterns, database operations

**Example:**
```rust
// ✅ GOOD: Extract to service module
// crates/adapteros-server-api/src/services/alert_deduplication.rs
pub async fn create_alert_if_not_exists(
    pool: &SqlitePool,
    alert_request: &CreateAlertRequest,
    tenant_id: Option<&str>,
) -> Result<bool> {
    // Centralized deduplication logic
}

// ❌ BAD: Duplicated in 3+ handler functions
// handlers.rs lines 9896, 9983, 10046 all have:
let existing_alerts = ProcessAlert::list(...).await?;
let alert_already_exists = existing_alerts.iter().any(...);
if !alert_already_exists { ProcessAlert::create(...).await?; }
```

**Citation:** 【2025-11-07†refactor(server)†extract-alert-dedup】

### Pattern 4: Test Utilities (Rust)

**When to Use:** Repeated test setup, data creation, helper functions

**Example:**
```rust
// ✅ GOOD: Extract to test utilities
// tests/common/migration_setup.rs
pub fn create_comprehensive_test_data(db_path: &Path) -> Result<()> {
    // Shared test data creation
}

// ❌ BAD: Duplicated in multiple test files
// test_safe_migration.rs, test_registry_migration_complete.rs both have:
fn create_test_data(db_path: &Path) -> Result<()> {
    // 40+ lines of identical code
}
```

**Citation:** 【2025-11-07†refactor(tests)†consolidate-test-setup】

### Pattern 5: Utility Modules (Swift)

**When to Use:** Repeated utility functions, clipboard operations, formatting

**Example:**
```swift
// ✅ GOOD: Extract to utility struct
// menu-bar-app/Sources/AdapterOSMenu/Utils/StatusUtils.swift
public struct StatusUtils {
    public static func copyKernelHash(_ fullHash: String) {
        NSPasteboard.general.setString(fullHash, forType: .string)
    }
}

// ❌ BAD: Duplicated in StatusMenuView
// StatusMenuView.swift has copyKernelHash() at lines 680 and 1166
```

**Citation:** 【2025-11-07†refactor(swift)†extract-status-utils】

### Pattern 6: Static Methods for Shared Logic

**When to Use:** Repeated file discovery, path resolution, configuration loading

**Example:**
```swift
// ✅ GOOD: Static method in canonical location
// StatusReader.swift
static func findStatusFileInDefaultPaths() -> String? {
    // Shared file discovery logic
}

// ❌ BAD: Duplicated in multiple classes
// StatusViewModel.findStatusFile() duplicates StatusReader.findStatusFile()
```

**Citation:** 【2025-11-07†refactor(swift)†consolidate-status-discovery】

---

## 3. Code Review Checklist

### Before Submitting PR

- [ ] Run `make dup` and review jscpd report
- [ ] Check for repeated string literals (>10 chars, used 3+ times)
- [ ] Check for repeated function patterns (similar logic in 2+ places)
- [ ] Check for repeated validation logic
- [ ] Check for repeated test setup code
- [ ] Verify all shared code is in appropriate modules (utils, services, common)

### During Code Review

**Red Flags:**
- Code blocks that look similar across multiple files
- String literals repeated 3+ times
- Validation logic duplicated in handlers
- Test setup code duplicated across test files
- Utility functions duplicated in view/component files

**Action Items:**
1. Extract to shared module/component/utility
2. Add citation to extraction
3. Update all call sites
4. Verify tests still pass

---

## 4. Development Workflow

### Step 1: Before Writing New Code

**Ask:**
- Does similar functionality already exist?
- Can I reuse existing utilities/services/components?
- Should this be extracted for reuse?

**Search:**
```bash
# Search for similar patterns
grep -r "similar_pattern" --include="*.rs" --include="*.tsx" --include="*.swift"

# Check for existing utilities
ls crates/*/src/services/
ls tests/common/
ls ui/src/components/ui/
ls menu-bar-app/Sources/*/Utils/
```

### Step 2: While Writing Code

**If you find yourself copying code:**
1. **Stop** - Don't copy-paste
2. **Extract** - Create shared function/component/constant
3. **Refactor** - Update existing code to use shared version
4. **Cite** - Add citation following codebase standards

**Example Workflow:**
```rust
// ❌ BAD: Copying alert checking logic
// handlers.rs line 9896
let existing_alerts = ProcessAlert::list(...).await?;
// handlers.rs line 9983 (duplicate)
let existing_alerts = ProcessAlert::list(...).await?;

// ✅ GOOD: Extract once, use everywhere
// 1. Create service module
// crates/adapteros-server-api/src/services/alert_deduplication.rs
pub async fn create_alert_if_not_exists(...) -> Result<bool> { ... }

// 2. Use in all handlers
create_alert_if_not_exists(state.db.pool(), &alert_request, Some("default")).await?;
```

### Step 3: Before Committing

**Run duplication check:**
```bash
make dup
# Review var/reports/jscpd/latest/jscpd-report.json
```

**If duplicates found:**
1. Review jscpd report
2. Extract shared code
3. Update all call sites
4. Re-run `make dup` to verify reduction

---

## 5. CI/CD Enforcement

### Recommended CI Workflow

Add to `.github/workflows/ci.yml`:

```yaml
duplication-check:
  name: Duplication Check
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with:
        node-version: '20'
    - name: Check for duplicates
      run: |
        npx --yes jscpd@latest --config configs/jscpd.config.json --format rust,typescript,swift --threshold 0
      env:
        JSCPD_ENFORCE: true
```

**Enforcement Levels:**
- **Advisory:** Report duplicates but don't fail (current)
- **Warning:** Fail on PRs with >2% duplication
- **Strict:** Fail on any duplication (recommended for main branch)

---

## 6. Architectural Guidelines

### Code Organization Hierarchy

```
Shared Code Location Priority:
1. Core utilities → crates/adapteros-core/src/
2. Service modules → crates/*/src/services/
3. Test utilities → tests/common/
4. UI utilities → ui/src/components/ui/utils.ts
5. UI components → ui/src/components/ui/
6. Swift utilities → menu-bar-app/Sources/*/Utils/
```

### When to Extract

**Extract immediately if:**
- Code appears in 3+ locations
- String literal >50 chars, used 2+ times
- Function logic >10 lines, duplicated 2+ times
- Test setup code duplicated across test files

**Consider extracting if:**
- Code appears in 2 locations and likely to grow
- Pattern is common (validation, formatting, error handling)
- Code is testable in isolation

**Don't extract if:**
- Code is truly unique to one context
- Extraction would create unnecessary abstraction
- Code is likely to diverge soon

---

## 7. Citation Requirements

All code extractions must include citations:

**Format:** `【YYYY-MM-DD†category†identifier】`

**Categories:**
- `refactor(ui)` - UI component refactoring
- `refactor(server)` - Server-side refactoring
- `refactor(tests)` - Test code refactoring
- `refactor(swift)` - Swift code refactoring
- `refactor(build)` - Build system refactoring

**Example:**
```rust
//! Alert deduplication service
//! 【2025-11-07†refactor(server)†extract-alert-dedup】
//!
//! Consolidates duplicate alert checking logic from handlers.rs.
```

---

## 8. Monitoring and Metrics

### Track Duplication Over Time

**Baseline (2025-11-07):** 5.17% duplication  
**Target:** <2% duplication  
**Current Threshold:** 0% (strict mode)

**Monitor:**
```bash
# Generate report
make dup

# Check percentage
cat var/reports/jscpd/latest/jscpd-report.json | jq '.statistics.percentage'
```

### Success Metrics

- **Duplication Percentage:** Decrease over time
- **Code Reuse:** Increase in shared module usage
- **Maintainability:** Single source of truth for shared code
- **Test Coverage:** Shared code has comprehensive tests

---

## 9. Common Duplication Patterns to Watch

### Pattern A: Validation Logic

**Symptom:** Similar validation checks in multiple handlers

**Solution:** Extract to `crates/adapteros-server-api/src/validation.rs`

**Example:**
```rust
// ✅ GOOD: Centralized validation
use crate::validation::validate_adapter_id;
validate_adapter_id(&req.adapter_id)?;

// ❌ BAD: Inline validation duplicated
if req.adapter_id.is_empty() || !req.adapter_id.chars().all(|c| c.is_alphanumeric()) {
    return Err(...);
}
```

### Pattern B: Error Response Formatting

**Symptom:** Similar error response creation in multiple handlers

**Solution:** Use `ErrorResponseExt` trait or middleware

**Example:**
```rust
// ✅ GOOD: Use error extension trait
use crate::errors::ErrorResponseExt;
return Err(AosError::NotFound("message").into_response());

// ❌ BAD: Manual error response creation duplicated
return Err((StatusCode::NOT_FOUND, Json(ErrorResponse::new("message"))));
```

### Pattern C: Database Query Patterns

**Symptom:** Similar query logic in multiple handlers

**Solution:** Extract to database module methods

**Example:**
```rust
// ✅ GOOD: Database method
adapteros_db::process_monitoring::ProcessAlert::list(pool, filters).await?;

// ❌ BAD: Raw query duplicated
sqlx::query("SELECT * FROM process_alerts WHERE tenant_id = ? AND status = ?")
    .bind(tenant_id).bind("active").fetch_all(pool).await?;
```

### Pattern D: UI State Management

**Symptom:** Similar state management patterns in multiple components

**Solution:** Extract to shared hooks or context providers

**Example:**
```typescript
// ✅ GOOD: Shared hook
const { status, refresh } = useStatus();

// ❌ BAD: Duplicated useState/useEffect patterns
const [status, setStatus] = useState<Status | null>(null);
useEffect(() => { fetchStatus().then(setStatus); }, []);
```

---

## 10. Enforcement Strategy

### Phase 1: Advisory (Current)

- Pre-commit hook runs jscpd (advisory)
- CI reports duplicates but doesn't fail
- Manual review encouraged

### Phase 2: Warning (Recommended)

- CI fails on PRs with >3% duplication
- Pre-commit hook warns on >2% duplication
- Requires explicit override for new duplicates

### Phase 3: Strict (Future)

- CI fails on any duplication >1%
- Pre-commit hook blocks commits with duplicates
- Requires architectural review for exceptions

---

## 11. Refactoring Guidelines

### When Refactoring Duplicates

**Process:**
1. **Identify** - Use jscpd report or code review
2. **Plan** - Determine extraction location and approach
3. **Extract** - Create shared module/component/utility
4. **Update** - Replace all duplicates with shared version
5. **Test** - Verify all functionality still works
6. **Cite** - Add citations following standards
7. **Verify** - Re-run jscpd to confirm reduction

**Example Refactoring:**
```bash
# 1. Identify duplicates
make dup
# Review var/reports/jscpd/latest/jscpd-report.json

# 2. Extract shared code
# Create crates/adapteros-server-api/src/services/alert_deduplication.rs

# 3. Update call sites
# Replace 3 instances in handlers.rs

# 4. Verify
cargo test -p adapteros-server-api
make dup  # Should show reduction
```

---

## 12. Agent-Specific Guidelines

### For AI Agents

**Before generating code:**
1. Search codebase for similar patterns
2. Check for existing utilities/services/components
3. Prefer reusing over creating new code
4. Extract shared code proactively

**When writing code:**
1. If copying >5 lines, extract to shared module
2. If string literal >20 chars, extract to constant
3. If validation logic, use centralized validation module
4. If test setup, use test utilities

**After generating code:**
1. Run `make dup` to check for duplicates
2. Extract any detected duplicates
3. Add citations to all extractions
4. Verify tests pass

### For Human Developers

**Code Review Focus:**
- Look for similar code blocks
- Check for repeated patterns
- Suggest extraction when appropriate
- Verify citations are present

**Development Focus:**
- Search before writing
- Extract proactively
- Follow established patterns
- Update documentation

---

## 13. Quick Reference

### Commands

```bash
# Check for duplicates
make dup

# View latest report
open var/reports/jscpd/$(ls -1t var/reports/jscpd | head -1)/index.html

# Enable enforcement
export JSCPD_ENFORCE=1

# Install git hooks
bash scripts/install_git_hooks.sh
```

### File Locations

- **Shared Constants:** `ui/src/components/ui/utils.ts`
- **Shared Components:** `ui/src/components/ui/menu-indicators.tsx`
- **Service Modules:** `crates/*/src/services/`
- **Test Utilities:** `tests/common/`
- **Swift Utilities:** `menu-bar-app/Sources/*/Utils/`

### Citation Format

```rust
//! Module description
//! 【YYYY-MM-DD†refactor(category)†identifier】
```

---

## 14. Success Stories

### Example 1: UI Component Deduplication

**Before:** 180+ character animation class string duplicated 8+ times  
**After:** Single constant `MENU_ANIMATION_CLASSES` in `utils.ts`  
**Impact:** ~500+ lines consolidated, single source of truth  
**Citation:** 【2025-11-07†refactor(ui)†extract-menu-indicators】

### Example 2: Alert Deduplication

**Before:** Alert checking logic duplicated 3 times in handlers.rs  
**After:** Single service function `create_alert_if_not_exists()`  
**Impact:** Consistent behavior, easier maintenance  
**Citation:** 【2025-11-07†refactor(server)†extract-alert-dedup】

### Example 3: Test Setup Consolidation

**Before:** 43+ lines of test data creation duplicated in 2 test files  
**After:** Shared `migration_setup::create_comprehensive_test_data()`  
**Impact:** Easier test maintenance, consistent test data  
**Citation:** 【2025-11-07†refactor(tests)†consolidate-test-setup】

---

## References

- [Duplication Monitoring Setup](DUPLICATION_MONITORING.md)
- [Code Standards](DEVELOPER_GUIDE.md#code-standards)
- [Citation Standards](CITATIONS.md)
- [Contributing Guidelines](../CONTRIBUTING.md)
- [Deprecated Patterns](DEPRECATED_PATTERNS.md)

---

**Remember:** When in doubt, extract. It's easier to consolidate shared code than to untangle duplicates later.

