<!-- 4859b9d0-2b79-4bba-9656-b744eb53f2b8 620a1404-dda5-4d7d-9a21-8aca10cb35ba -->
# AdapterOS Full Integration Execution Plan

## Overview
Execute all 6 phases of the integration plan with OpenAI API for LLM, direct database migration application, and comprehensive test writing (execution deferred).

## Phase 1: Critical Integration Fixes

### 1.1 Fix API Type Conflicts
**File**: `crates/mplora-server-api/src/handlers/code.rs`
- Add explicit type imports for StatusCode disambiguation
- Convert between `reqwest::StatusCode` and `axum::http::StatusCode`
- Fix all handler return types

### 1.2 Apply Database Migration
**File**: `var/aos.db`
- Execute `migrations/0005_code_intelligence.sql` directly
- Create repositories, commits, and code_policies tables
- Verify table creation with test queries

### 1.3 Fix Tenant Context
**Files**: `crates/mplora-ui-web/src/pages/code_*.rs` (7 files)
- Replace hardcoded "default" with auth context
- Extract tenant_id from AuthContext
- Update all API calls to use dynamic tenant

## Phase 2: LLM Integration

### 2.1 Add OpenAI Dependencies
**File**: `crates/mplora-worker/Cargo.toml`
- Add `async-openai = "0.20"`
- Add `tokio` with full features

### 2.2 Implement LLM Provider
**File**: `crates/mplora-worker/src/patch_pipeline.rs`
- Replace `generate_code()` stub with OpenAI API calls
- Implement `validate_patches()` with real validation
- Add configuration for API key and model selection

### 2.3 Evidence Citation Generation
**File**: `crates/mplora-worker/src/patch_pipeline.rs`
- Implement `generate_citations()` from retrieval results
- Add `validate_evidence_requirements()` policy checks
- Map evidence types to citation types

## Phase 3: UI Styling & Polish

### 3.1 Add CSS Styling
**File**: `crates/mplora-ui-web/style.css`
- Add routing inspector styles
- Add patch lab diff highlighting
- Add policy editor modal styles
- Add metrics dashboard grid styles

### 3.2 Enhance Error Handling
**Files**: `crates/mplora-ui-web/src/pages/code_*.rs` (7 files)
- Add error and success signals
- Implement toast-style notifications
- Add loading states with spinners
- Improve error messages with context

## Phase 4: Security & Production Hardening

### 4.1 Input Validation
**File**: `crates/mplora-server-api/src/handlers/code.rs`
- Add regex validation for repository IDs
- Validate file paths and git repository structure
- Check language support
- Validate all user inputs

### 4.2 Privilege Dropping
**File**: `crates/mplora-cli/src/commands/serve.rs`
- Implement `drop_privileges()` with nix crate
- Add tenant UID/GID lookup
- Update `spawn_worker()` to drop privileges
- Add logging for privilege changes

## Phase 5: Integration Testing

### 5.1 Write Test Suite
**File**: `tests/integration_tests.rs` (new)
- Repository registration workflow test
- Patch proposal workflow test
- RBAC enforcement test
- End-to-end code intelligence test
- Tests written but not executed

## Phase 6: Performance Optimization

### 6.1 WASM Bundle Optimization
**File**: `crates/mplora-ui-web/Cargo.toml`
- Add wasm-pack profile with size optimization
- Set opt-level = "z", enable LTO
- Configure panic = "abort"

### 6.2 API Response Caching
**File**: `crates/mplora-server-api/src/handlers/code.rs`
- Implement `ResponseCache` struct
- Add caching to frequently accessed endpoints
- Set appropriate TTL values

## Implementation Order

1. Phase 1: Critical fixes (type conflicts, database, tenant context)
2. Phase 2: LLM integration (OpenAI API, evidence citations)
3. Phase 3: UI polish (CSS, error handling)
4. Phase 4: Security (input validation, privilege dropping)
5. Phase 5: Testing (write comprehensive test suite)
6. Phase 6: Performance (WASM optimization, caching)

## Files Modified (Estimated 25+ files)

**Backend** (10 files):
- `crates/mplora-server-api/src/handlers/code.rs`
- `crates/mplora-worker/src/patch_pipeline.rs`
- `crates/mplora-worker/Cargo.toml`
- `crates/mplora-cli/src/commands/serve.rs`
- `var/aos.db`

**Frontend** (8 files):
- `crates/mplora-ui-web/style.css`
- `crates/mplora-ui-web/src/pages/code_*.rs` (7 files)
- `crates/mplora-ui-web/Cargo.toml`

**Testing** (1 file):
- `tests/integration_tests.rs` (new)

**Configuration** (1 file):
- `.env` (document OpenAI API key requirement)

## Success Criteria

- All compilation errors resolved
- Database migration applied successfully
- OpenAI integration functional (with API key)
- UI components styled and functional
- Security hardening implemented
- Test suite written (execution deferred)
- Performance optimizations applied

## Estimated Time: 2-3 hours
## Risk: Medium (significant changes across codebase)

### To-dos

- [ ] Fix API type conflicts in handlers
- [ ] Apply database migration for code intelligence
- [ ] Replace hardcoded tenant IDs with auth context
- [ ] Add OpenAI dependencies to worker crate
- [ ] Implement OpenAI LLM integration in patch pipeline
- [ ] Implement evidence citation generation
- [ ] Add CSS styling for code intelligence UI
- [ ] Enhance error handling in UI components
- [ ] Add input validation to API handlers
- [ ] Implement privilege dropping in serve command
- [ ] Write integration test suite
- [ ] Optimize WASM bundle size
- [ ] Implement API response caching