# PR Compliance Checklist

**Purpose**: Gate all PRs against repository coding standards and policy compliance  
**Source**: [CONTRIBUTING.md L110-L170], [ui/AUDIT_BASELINE.md#3]

---

## Pre-Submit Checklist

Run this checklist before submitting any PR. All items must pass.

### Code Formatting & Linting

- [ ] **Rust Code**: `cargo fmt --all` passes without changes
- [ ] **Rust Linting**: `cargo clippy --workspace -- -D warnings` passes
- [ ] **TypeScript/JavaScript**: Linter passes (check with `pnpm lint` if available)
- [ ] **No Formatting Changes**: Only intentional code changes, no auto-formatting noise

**Commands**:
```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# TypeScript (if applicable)
pnpm lint
```

---

### Logging Standards

- [ ] **No `println!`**: All Rust logging uses `tracing` (not `println!`)
- [ ] **No `console.log`**: All JavaScript/TypeScript logging uses proper logger (if applicable)
- [ ] **Appropriate Levels**: Uses correct log levels (`trace`, `debug`, `info`, `warn`, `error`)
- [ ] **Structured Fields**: Includes structured fields for querying (e.g., `info!(tenant_id = %id, "Message")`)

**Examples**:
```rust
// ✅ GOOD
use tracing::{info, warn, error};
info!(adapter_id = %adapter.id, "Adapter loaded");

// ❌ BAD
println!("Adapter loaded: {}", adapter.id);
```

---

### Error Handling

- [ ] **Result<T> Preferred**: Uses `Result<T>` over `Option<T>` for error handling where appropriate
- [ ] **Error Propagation**: Uses `?` operator for error propagation
- [ ] **Error Context**: Adds context to errors when propagating

**Examples**:
```rust
// ✅ GOOD
let data = load_data(path)
    .await
    .map_err(|e| AosError::Io(format!("Failed to load {}: {}", path.display(), e)))?;

// ❌ BAD
let data = load_data(path).await?; // No context
```

---

### Documentation

- [ ] **Public APIs Documented**: All public functions, types, and modules have doc comments
- [ ] **Examples Provided**: Complex functions include usage examples
- [ ] **README Updated**: User-facing changes update relevant README sections
- [ ] **Code Comments**: Complex logic has explanatory comments

**Example**:
```rust
/// Loads an adapter from the specified path.
///
/// # Arguments
/// * `path` - Path to the adapter file
///
/// # Errors
/// Returns `AosError::NotFound` if the file doesn't exist.
///
/// # Example
/// ```no_run
/// let adapter = loader.load_from_path("./adapters/my_adapter.aos").await?;
/// ```
pub async fn load_from_path(path: &Path) -> Result<Adapter> {
    // Implementation
}
```

---

### Testing

- [ ] **Tests Added**: New functionality has corresponding tests
- [ ] **Tests Pass**: `cargo test --workspace` passes
- [ ] **Integration Tests**: Complex integrations have integration tests
- [ ] **Test Coverage**: Critical paths have test coverage

**Commands**:
```bash
cargo test --workspace
cargo test --workspace -- --nocapture  # For debug output
```

---

### Policy Compliance

- [ ] **Policy Packs**: Changes comply with 20 canonical policy packs
- [ ] **Security Review**: Security-sensitive code flagged for review
- [ ] **Performance**: Performance changes include benchmarks (if applicable)
- [ ] **Breaking Changes**: Breaking changes include migration guides

**Checklist**:
- [ ] No network egress in production mode (UDS-only)
- [ ] All randomness is seeded and deterministic
- [ ] Router uses Q15 quantization
- [ ] Evidence tracked for policy decisions
- [ ] Telemetry events use canonical JSON
- [ ] Input validation on all user inputs
- [ ] Tenant isolation enforced

---

### Commit Message Format

- [ ] **Conventional Format**: Follows `type(scope): description` format
- [ ] **Detailed Description**: Includes what, why, and any breaking changes
- [ ] **Issue Linkage**: References issue numbers (`Fixes #123`)

**Format**:
```
type(scope): brief description

Detailed description of changes, including:
- What was changed
- Why it was changed
- Any breaking changes or migration notes

Fixes #123
```

**Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

**Examples**:
```
feat(ui): integrate recent activity API

Adds REST endpoint integration for dashboard recent activity:
- Replace mock data in Dashboard.tsx with API calls
- Connect to SSE stream for live updates
- Add graceful degradation if SSE unavailable

Fixes #1
```

```
fix(api): add query parameter auth for SSE endpoints

Implements token extraction from query parameter for SSE streams:
- Extract token from ?token=xxx query param
- Fallback to header-based auth for backward compatibility
- Validate JWT and reject unauthorized connections

Fixes #3
```

---

### Integration Debt

- [ ] **No Mock Data**: No hardcoded mock data where APIs exist
- [ ] **Feature Flags**: Incomplete features behind feature flags
- [ ] **Documentation**: Mock-only areas documented with TODO/issue links

**Check**: Review [ui/AUDIT_BASELINE.md](./AUDIT_BASELINE.md) for known debt items

---

### Backend Coordination

- [ ] **API Gaps**: No new API gaps introduced
- [ ] **Existing Gaps**: Known gaps tracked in [ISSUES.md](./ISSUES.md)
- [ ] **Backend Changes**: Backend-required changes have issue tickets

**Check**: Review [ui/ISSUES.md](./ISSUES.md) for backend dependencies

---

## CI/CD Integration

This checklist should be automated in CI/CD:

### Automated Checks
- ✅ `cargo fmt --check`
- ✅ `cargo clippy --workspace -- -D warnings`
- ✅ `cargo test --workspace`
- ✅ No `println!` in Rust code (grep check)
- ✅ Commit message format validation

### Manual Checks (Reviewer)
- 📋 Documentation completeness
- 📋 Policy compliance review
- 📋 Security review (if applicable)
- 📋 Performance impact (if applicable)

---

## Exceptions

If any item cannot be completed:
1. Document the exception in PR description
2. Link to tracking issue
3. Set timeline for resolution
4. Get approval from maintainer

**Example**:
```
Exception: Policy update endpoint not yet implemented (Issue #4)
- Local UI update only (backend endpoint pending)
- Expected resolution: Week 2
- Issue: #4
```

---

## Quick Reference

### Rust Commands
```bash
# Format
cargo fmt --all

# Lint
cargo clippy --workspace -- -D warnings

# Test
cargo test --workspace

# Check for println!
grep -r "println!" crates/ --include="*.rs"
```

### Commit Message Template
```
type(scope): brief description

Detailed description:
- What changed
- Why changed
- Breaking changes (if any)

Fixes #issue
```

### Logging Template
```rust
use tracing::{info, warn, error, debug};

// Structured logging
info!(
    adapter_id = %adapter.id,
    tenant_id = %tenant.id,
    "Adapter loaded successfully"
);
```

---

**Last Updated**: 2025-01-15  
**Review Frequency**: Quarterly

