# Production Readiness Checklist

**Last Updated:** 2025-10-10  
**Status:** Development - Not Production Ready

## Overview

This document tracks features and components that are deferred to production deployment. These items are currently implemented as placeholders with appropriate warnings and must be completed before production use.

---

## 🔐 Security & Cryptography

### Secure Enclave Integration

**Status:** ⚠️ **DEFERRED - Development Mode Only**

**Location:** `crates/mplora-secd/src/enclave.rs`

**Current State:**
- Software fallback key derivation implemented
- Hardware detection in place
- Warning logs active for all operations

**Production Requirements:**

1. **LoRA Delta Encryption/Decryption**
   - Implement ChaCha20-Poly1305 encryption with enclave-derived keys
   - Use `SecKey` with `kSecAttrTokenIDSecureEnclave`
   - Add key rotation support per Secrets Ruleset #14
   - **Methods:** `seal_lora_delta()`, `unseal_lora_delta()`

2. **Signing Key Management**
   - Implement Ed25519 or P-256 key generation in Secure Enclave
   - Key caching and lifecycle management
   - Implement key rotation on Control Plane promotions
   - **Method:** `get_or_create_signing_key()`

3. **Encryption Key Management**
   - Complete security-framework API integration
   - Hardware-backed key storage
   - **Method:** `get_or_create_encryption_key()`

**Security Impact:**
- ❌ LoRA deltas stored unencrypted at rest
- ❌ Bundle signing uses software keys
- ❌ No hardware-backed key protection

**Mitigation:**
- Development systems only
- Clear warning logs on all operations
- Falls back to software crypto gracefully

**References:**
- Secrets Ruleset #14 (`docs/` - see rules in workspace)
- Isolation Ruleset #8

---

## 🔌 Unix Domain Socket (UDS) Communications

**Status:** ⚠️ **PARTIALLY IMPLEMENTED**

**Location:** `crates/mplora-cli/src/commands/`

**Current State:**
- Mock implementations with placeholder data
- No actual worker connections
- CLI commands return static responses

**Production Requirements:**

1. **UDS Client Module**
   - Create `crates/mplora-client/src/uds.rs`
   - Implement worker connection protocol
   - Handle connection pooling and errors
   - Support timeout and retry logic

2. **CLI Command Integration** (18 TODOs)
   - **adapter.rs** (6 commands): list, profile, promote, demote, pin, unpin
   - **profile.rs** (3 commands): snapshot, watch, export
   - **adapter_swap.rs**: Actual UDS connection using hyperlocal
   - **Other commands**: See CLI TODO list below

**User Impact:**
- ❌ CLI adapter commands return mock data
- ❌ Cannot control live worker processes
- ❌ Profiling data unavailable

**Mitigation:**
- CLI provides consistent interface
- Mock data demonstrates expected formats
- Direct database queries possible as workaround

---

## 📊 System Metrics & Monitoring

**Status:** ⚠️ **COMPILATION ISSUES**

**Location:** `crates/mplora-system-metrics/`, `crates/mplora-cli/src/commands/metrics.rs`

**Current State:**
- Core metrics collection implemented
- Struct field mismatches in CLI

**Production Requirements:**

1. **Field Alignment**
   - `MetricsConfig.retention_days` - missing field
   - `PerformanceThresholds` - missing warning/critical threshold fields
   - Update CLI to match actual struct definitions

2. **Database Integration**
   - Complete `SystemMetricsDb` implementation
   - Fix `Db::connect_path()` method (renamed or removed)

**Impact:**
- ⚠️ Metrics CLI commands won't compile
- ⚠️ Monitoring dashboard data incomplete

---

## 🧪 CLI Compilation Status

**Status:** ⚠️ **33 COMPILATION ERRORS**

**Location:** `crates/mplora-cli/`

**Error Summary:**
- 11× E0599: Method not found
- 11× E0609: Field not found
- 5× E0277: Trait bound not satisfied
- 4× E0308: Type mismatches
- 2× Others

**Critical Issues:**

1. **Missing OutputWriter Methods**
   - `table()` method needed for formatted output
   - Additional output formatting helpers

2. **Struct Field Mismatches**
   - Adapter fields don't match database schema
   - Metrics struct fields misaligned

3. **Type System Issues**
   - `blake3::Hash` doesn't implement `LowerHex`
   - `ProgressUpdate` missing `Deserialize` impl
   - Various f32/f64 mismatches

**Workaround:**
- Core workspace compiles successfully (`cargo check --workspace --exclude mplora-cli`)
- Database, worker, and server components functional
- Can use direct API calls instead of CLI

---

## 📝 Outstanding TODOs

### Core Module TODOs

1. **mplora-lifecycle** (3 TODOs)
   - Database update in `update_adapter_state()` (Line 506)
   - Database update in `record_adapter_activation()` (Line 584)
   - Database update in `evict_adapter()` (Line 712)
   - **Status:** Marked for future database integration iteration

2. **mplora-worker** (4 TODOs)
   - `generation.rs` - Review required
   - `signal.rs` - Review required
   - `contact_discovery.rs` (2) - Review required

3. **mplora-mlx** (1 TODO)
   - PyO3 linking issues documented
   - Temporarily disabled

4. **mplora-router** (1 TODO)
   - Review required

5. **mplora-telemetry** (1 TODO)
   - Review required

### CLI Command TODOs (18 items)

Documented above in UDS Communications section.

---

## ✅ Production Deployment Checklist

Before deploying to production, complete:

- [ ] **Secure Enclave**: Implement all 4 deferred methods
- [ ] **UDS Client**: Create and integrate client module
- [ ] **CLI Integration**: Complete all 18 command TODOs
- [ ] **Metrics**: Fix struct field mismatches
- [ ] **CLI Compilation**: Resolve all 33 errors
- [ ] **Database TODOs**: Complete lifecycle database integration
- [ ] **Testing**: Full integration test suite
- [ ] **Security Audit**: Third-party security review
- [ ] **Performance Testing**: Meet Ruleset #11 budgets
- [ ] **Compliance**: Verify Ruleset #16 evidence links

---

## 🎯 Priority Levels

### P0 - Blocker (Must have before any production use)
- Secure Enclave encryption implementation
- Security audit and penetration testing

### P1 - Critical (Required for production operations)
- UDS client and CLI command integration
- CLI compilation fixes
- Metrics struct alignment

### P2 - Important (Required for full functionality)
- Lifecycle database integration TODOs
- Worker, router, telemetry TODO reviews

### P3 - Nice to have (Can be added post-launch)
- Advanced monitoring features
- Performance optimizations

---

## 📚 References

- **Policy Rulesets**: See workspace rules (embedded in `.rules/`)
- **Architecture**: `docs/architecture.md`
- **Security**: `docs/signal-protocol-implementation.md`
- **Metrics**: `docs/system-metrics.md`
- **Database**: `docs/database-schema/`

---

## 🔄 Update Process

This document should be updated when:
1. Deferred features are implemented
2. New production requirements are identified
3. Blockers are resolved
4. Security audits identify new issues

**Responsibility:** Engineering team maintains this document as part of the release process.

