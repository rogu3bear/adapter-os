# Product Requirements Document: Unified Configuration System

**PRD ID:** PRD-CONFIG-001
**Author:** James KC Auchterlonie
**Date:** 2025-11-22
**Status:** Draft
**Version:** 1.0

---

## 1. Executive Summary

AdapterOS currently relies on fragmented configuration scattered across 15+ files and 50+ environment variables with inconsistent naming conventions (`AOS_*`, `ADAPTEROS_*`, `DATABASE_URL`, `MLX_PATH`). This PRD defines requirements for unifying all configuration into a single `.env`-based system with clear precedence rules, ensuring deterministic configuration loading, backward compatibility, and improved developer experience.

---

## 2. Problem Statement

### 2.1 Current Pain Points

1. **Fragmented Configuration Sources**
   - Environment variables spread across 54+ files reading `std::env::var`
   - Multiple `.env` files: root `.env`, `crates/adapteros-secd/.env`, `ui/.env.local`
   - TOML manifests, JSON config files, and inline defaults
   - No single source of truth for system configuration

2. **Inconsistent Naming Conventions**
   - `AOS_*` prefix for model-related vars (e.g., `AOS_MODEL_PATH`, `AOS_MODEL_BACKEND`)
   - `ADAPTEROS_*` prefix for server/system vars (e.g., `ADAPTEROS_SERVER_PORT`)
   - Legacy unprefixed vars (e.g., `DATABASE_URL`, `MLX_PATH`, `RUST_LOG`)
   - Mixed conventions cause confusion and documentation overhead

3. **No Configuration Freeze Enforcement**
   - Current guards exist (`ConfigGuards`) but not consistently applied
   - Environment variables can be read at any point, breaking determinism guarantees
   - Violations tracked but not enforced at runtime

4. **Documentation Gaps**
   - `.env.example` only documents 4 variables
   - Actual system uses 30+ distinct environment variables
   - No comprehensive configuration reference

5. **Testing Complexity**
   - Tests set environment variables directly without cleanup
   - Configuration state leaks between test runs
   - No standardized test configuration fixtures

### 2.2 Impact on Developers/Operators

| Stakeholder | Impact |
|-------------|--------|
| **Developers** | Time wasted searching for configuration options; inconsistent patterns lead to bugs |
| **Operators** | Deployment scripts require knowledge of multiple config sources; no validation on startup |
| **SREs** | Debugging configuration issues requires checking multiple files; no audit trail |
| **Security** | Sensitive configuration (JWT secrets, KMS keys) mixed with general config; no clear separation |

---

## 3. Goals and Non-Goals

### 3.1 Goals

1. **Single Source of Truth**: All configuration defined in root `.env` file with clear schema
2. **Unified Naming Convention**: Standardize on `AOS_` prefix for all environment variables
3. **Precedence Clarity**: Enforce CLI > ENV > File > Defaults with deterministic resolution
4. **Configuration Freeze**: Prevent environment variable access after application initialization
5. **Backward Compatibility**: Support legacy variable names with deprecation warnings
6. **Comprehensive Documentation**: Self-documenting `.env.example` covering all options
7. **Validation on Startup**: Fail fast with clear error messages for invalid configuration
8. **Secure Defaults**: Production-safe defaults; sensitive configs require explicit values

### 3.2 Non-Goals

1. **Runtime Configuration Changes**: Hot-reloading configuration is out of scope
2. **GUI Configuration Editor**: No UI for configuration management
3. **Distributed Configuration**: No etcd/Consul/Vault integration (use direct environment variables)
4. **Multi-Environment File Support**: No `.env.production`, `.env.staging` (use orchestration tools)
5. **Configuration Encryption**: Secrets management handled by external tools (Keychain, KMS)

---

## 4. User Stories

### 4.1 Developer Stories

| ID | Story | Acceptance Criteria |
|----|-------|---------------------|
| US-D1 | As a developer, I want a single `.env` file to configure the entire system so that I don't need to hunt for configuration options | All configuration can be set via `.env`; no hidden files required |
| US-D2 | As a developer, I want clear error messages when configuration is invalid so that I can fix issues quickly | Startup fails with specific field name, expected type, and valid options |
| US-D3 | As a developer, I want consistent naming conventions so that I can guess configuration variable names | All variables follow `AOS_CATEGORY_OPTION` pattern |
| US-D4 | As a developer, I want configuration changes to be validated before application use so that bugs are caught early | `ConfigLoader::load()` validates all values before returning |
| US-D5 | As a developer, I want to see which configuration source each value came from for debugging | `DeterministicConfig::get_source("key")` returns origin |

### 4.2 Operator Stories

| ID | Story | Acceptance Criteria |
|----|-------|---------------------|
| US-O1 | As an operator, I want to deploy with environment variables so that I can use Kubernetes ConfigMaps | All configuration settable via env vars; no file dependencies |
| US-O2 | As an operator, I want deprecated variables to emit warnings so that I can plan migrations | Deprecation warnings include replacement variable name and removal version |
| US-O3 | As an operator, I want production mode to enforce secure defaults so that misconfigurations don't expose the system | `AOS_PRODUCTION_MODE=true` requires UDS socket, EdDSA JWT, PF deny |
| US-O4 | As an operator, I want to validate configuration without starting the server so that deployments can pre-check | `aosctl config validate` command exits 0/1 with detailed report |
| US-O5 | As an operator, I want configuration to be immutable after startup so that runtime behavior is predictable | Environment variable reads after freeze raise `ConfigFreezeError` |

### 4.3 SRE Stories

| ID | Story | Acceptance Criteria |
|----|-------|---------------------|
| US-S1 | As an SRE, I want configuration to be logged at startup for debugging so that I can diagnose issues | Non-sensitive config logged at INFO level; hash of full config in debug |
| US-S2 | As an SRE, I want metrics on configuration source so that I can audit deployments | Prometheus metric `aos_config_source{key,source}` exported |
| US-S3 | As an SRE, I want configuration freeze violations tracked so that I can identify misbehaving code | Violations stored in `ConfigGuards::get_violations()` with stack traces |

---

## 5. Technical Requirements

### 5.1 Unified Environment Variable Schema

**Naming Convention:** `AOS_<CATEGORY>_<OPTION>`

| Category | Variables | Description |
|----------|-----------|-------------|
| `MODEL` | `AOS_MODEL_PATH`, `AOS_MODEL_BACKEND`, `AOS_MODEL_ARCHITECTURE` | Model loading configuration |
| `SERVER` | `AOS_SERVER_HOST`, `AOS_SERVER_PORT`, `AOS_SERVER_WORKERS` | HTTP server configuration |
| `DATABASE` | `AOS_DATABASE_URL`, `AOS_DATABASE_POOL_SIZE`, `AOS_DATABASE_TIMEOUT` | Database connection settings |
| `SECURITY` | `AOS_SECURITY_JWT_SECRET`, `AOS_SECURITY_JWT_MODE`, `AOS_SECURITY_PF_DENY` | Security configuration |
| `LOGGING` | `AOS_LOG_LEVEL`, `AOS_LOG_FORMAT`, `AOS_LOG_FILE` | Logging configuration |
| `TELEMETRY` | `AOS_TELEMETRY_ENABLED`, `AOS_TELEMETRY_ENDPOINT` | Telemetry and metrics |
| `MEMORY` | `AOS_MEMORY_HEADROOM_PCT`, `AOS_MEMORY_EVICTION_THRESHOLD` | Memory management |
| `BACKEND` | `AOS_BACKEND_COREML_ENABLED`, `AOS_BACKEND_METAL_ENABLED`, `AOS_BACKEND_MLX_ENABLED` | Backend selection |
| `FEDERATION` | `AOS_FEDERATION_ENABLED`, `AOS_FEDERATION_PEERS` | Federation settings |
| `DEBUG` | `AOS_DEBUG_DETERMINISTIC`, `AOS_DEBUG_TRACE_FFI` | Debug/development flags |

### 5.2 Legacy Variable Mapping

| Legacy Variable | New Variable | Deprecation Version |
|-----------------|--------------|---------------------|
| `DATABASE_URL` | `AOS_DATABASE_URL` | v0.02 |
| `MLX_PATH` | `AOS_MLX_PATH` | v0.02 |
| `RUST_LOG` | `AOS_LOG_LEVEL` | v0.02 (continue supporting) |
| `ADAPTEROS_SERVER_PORT` | `AOS_SERVER_PORT` | v0.02 |
| `ADAPTEROS_SERVER_HOST` | `AOS_SERVER_HOST` | v0.02 |
| `ADAPTEROS_DATABASE_URL` | `AOS_DATABASE_URL` | v0.02 |
| `ADAPTEROS_ENV` | `AOS_ENVIRONMENT` | v0.02 |
| `ADAPTEROS_KEYCHAIN_FALLBACK` | `AOS_KEYCHAIN_FALLBACK` | v0.02 |
| `AOS_DETERMINISTIC_DEBUG` | `AOS_DEBUG_DETERMINISTIC` | v0.02 |
| `AOS_SKIP_KERNEL_SIGNATURE_VERIFY` | `AOS_DEBUG_SKIP_KERNEL_SIG` | v0.02 |
| `AOS_GPU_INDEX` | `AOS_BACKEND_GPU_INDEX` | v0.02 |

### 5.3 Configuration Precedence

```
Priority (highest to lowest):
1. CLI arguments (--model-path /path/to/model)
2. Environment variables (AOS_MODEL_PATH=/path/to/model)
3. .env file in working directory
4. .env file in project root (searched up to 5 levels)
5. Manifest/config file (if specified)
6. Compiled defaults
```

**Resolution Rules:**
- First non-empty value wins
- Explicit `""` (empty string) treated as "unset"
- Use `AOS_<VAR>_UNSET=true` to force unset behavior

### 5.4 Configuration Freeze

```rust
// Example usage in main.rs
fn main() -> Result<()> {
    // Load configuration (reads env vars, .env, CLI)
    let config = ConfigLoader::new()
        .load(std::env::args().collect(), None)?;

    // Configuration is now frozen
    // Any subsequent env::var() calls will:
    // 1. Log a warning
    // 2. Record a violation
    // 3. Return error (strict mode) or cached value (permissive mode)

    // Access config through the frozen object
    let model_path = config.get("model.path")?;

    // Get configuration source for debugging
    let source = config.get_source("model.path"); // "environment:AOS_MODEL_PATH"

    Ok(())
}
```

### 5.5 Validation Requirements

| Requirement | Implementation |
|-------------|----------------|
| Type validation | All values parsed to expected types on load |
| Required fields | Missing required fields cause startup failure |
| Enum validation | Backend preference validated against known values |
| Path validation | File/directory paths checked for existence (optional) |
| Range validation | Numeric values checked against min/max bounds |
| Format validation | URLs, durations, sizes parsed with clear error messages |
| Cross-field validation | Related fields validated together (e.g., JWT mode requires secret) |

### 5.6 Sensitive Configuration Handling

**Sensitive Variables (never logged):**
- `AOS_SECURITY_JWT_SECRET`
- `AOS_KEYCHAIN_FALLBACK`
- `AOS_KMS_ACCESS_KEY`
- `AOS_DATABASE_PASSWORD` (if separated from URL)

**Handling:**
- Logged as `"***REDACTED***"` in startup logs
- Excluded from configuration hash (use separate security hash)
- Never written to telemetry events

---

## 6. Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Configuration-related bug reports | -80% vs. current | GitHub issues tagged `config` |
| Time to first successful deployment | <5 minutes | User survey / onboarding funnel |
| Configuration documentation coverage | 100% | Variables in `.env.example` / Variables in codebase |
| Configuration freeze violations in production | 0 | `aos_config_freeze_violations_total` metric |
| Deprecation warning adoption | >90% migration in 2 releases | Telemetry on legacy var usage |

---

## 7. Implementation Phases

### Phase 1: Core Model Configuration (Completed)

**Status:** Done

**Deliverables:**
- [x] `AOS_MODEL_PATH` environment variable support
- [x] `AOS_MODEL_BACKEND` environment variable support
- [x] `.env` file auto-loading via `dotenvy`
- [x] `ModelConfig::from_env()` implementation
- [x] `.env.example` with model configuration
- [x] `ConfigLoader` with precedence: CLI > ENV > Manifest

**Files Modified:**
- `crates/adapteros-config/src/model.rs`
- `crates/adapteros-config/src/loader.rs`
- `.env.example`

### Phase 2: Server and Database Configuration

**Status:** Planned

**Deliverables:**
- [ ] Migrate `DATABASE_URL` to `AOS_DATABASE_URL` with deprecation warning
- [ ] Migrate `ADAPTEROS_SERVER_*` to `AOS_SERVER_*` with deprecation warning
- [ ] Add `AOS_DATABASE_POOL_SIZE`, `AOS_DATABASE_TIMEOUT` variables
- [ ] Add `AOS_SERVER_WORKERS`, `AOS_SERVER_TIMEOUT` variables
- [ ] Update `.env.example` with server/database sections
- [ ] Implement deprecation warning system in `ConfigLoader`

**Files to Modify:**
- `crates/adapteros-config/src/loader.rs`
- `crates/adapteros-db/src/lib.rs`
- `crates/adapteros-server-api/src/lib.rs`
- `.env.example`

### Phase 3: Security Configuration

**Status:** Planned

**Deliverables:**
- [ ] Migrate `ADAPTEROS_KEYCHAIN_FALLBACK` to `AOS_KEYCHAIN_FALLBACK`
- [ ] Add `AOS_SECURITY_JWT_SECRET`, `AOS_SECURITY_JWT_MODE` variables
- [ ] Add `AOS_SECURITY_PF_DENY`, `AOS_SECURITY_PRODUCTION_MODE` variables
- [ ] Implement sensitive value redaction in logs
- [ ] Add production mode enforcement

**Files to Modify:**
- `crates/adapteros-crypto/src/providers/keychain.rs`
- `crates/adapteros-server-api/src/middleware_security.rs`
- `crates/adapteros-config/src/guards.rs`

### Phase 4: Backend and Debug Configuration

**Status:** Planned

**Deliverables:**
- [ ] Migrate `AOS_GPU_INDEX` to `AOS_BACKEND_GPU_INDEX`
- [ ] Migrate `AOS_DETERMINISTIC_DEBUG` to `AOS_DEBUG_DETERMINISTIC`
- [ ] Add `AOS_BACKEND_COREML_ENABLED`, `AOS_BACKEND_METAL_ENABLED`, `AOS_BACKEND_MLX_ENABLED`
- [ ] Add `AOS_DEBUG_TRACE_FFI`, `AOS_DEBUG_VERBOSE` flags
- [ ] Consolidate all kernel/backend env var reads

**Files to Modify:**
- `crates/adapteros-lora-kernel-mtl/src/lib.rs`
- `crates/adapteros-lora-kernel-mtl/src/debug.rs`
- `crates/adapteros-lora-kernel-mtl/src/manifest.rs`
- `crates/adapteros-lora-mlx-ffi/build.rs`
- `crates/adapteros-lora-kernel-coreml/build.rs`

### Phase 5: Configuration Freeze Enforcement

**Status:** Planned

**Deliverables:**
- [ ] Audit all `std::env::var` calls in codebase (54+ files)
- [ ] Replace with `safe_env_var()` or `strict_env_var()` where appropriate
- [ ] Enable freeze enforcement in production mode
- [ ] Add `aos_config_freeze_violations_total` Prometheus metric
- [ ] Implement stack trace collection for violations

**Files to Modify:**
- All 54 files currently calling `std::env::var`
- `crates/adapteros-config/src/guards.rs`
- `crates/adapteros-telemetry/src/metrics/mod.rs`

### Phase 6: Documentation and Tooling

**Status:** Planned

**Deliverables:**
- [ ] Comprehensive `.env.example` with all variables documented
- [ ] `aosctl config validate` command
- [ ] `aosctl config show` command (shows effective configuration with sources)
- [ ] `aosctl config migrate` command (migrates legacy variables)
- [ ] Update `CLAUDE.md` configuration section
- [ ] Create `docs/CONFIGURATION.md` reference

**Files to Create/Modify:**
- `.env.example` (comprehensive update)
- `crates/adapteros-cli/src/commands/config.rs` (new)
- `docs/CONFIGURATION.md` (new)
- `CLAUDE.md`

---

## 8. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing deployments | Medium | High | Deprecation warnings for 2 releases; legacy support for 4 releases |
| Performance overhead from freeze checks | Low | Medium | Inline freeze check (~1ns); optional strict mode |
| Incomplete variable migration | Medium | Medium | Automated audit script; CI check for `std::env::var` |
| Test suite instability | High | Medium | Test isolation fixtures; `ConfigGuards::reset_for_testing()` |
| Third-party crate env var access | Medium | Low | Document exceptions; allowlist in freeze check |

---

## 9. Dependencies

### 9.1 Internal Dependencies

| Dependency | Required For | Status |
|------------|--------------|--------|
| `adapteros-core` | `AosError` types | Available |
| `adapteros-config` | Configuration loading | Available |
| `adapteros-telemetry` | Metrics export | Available |
| `adapteros-cli` | CLI commands | Available |

### 9.2 External Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `dotenvy` | 0.15+ | `.env` file loading |
| `tracing` | 0.1+ | Deprecation warnings |
| `serde` | 1.0+ | Configuration serialization |
| `chrono` | 0.4+ | Feature flag date conditions |

---

## 10. Implementation Steps

1. **Step 1:** Complete Phase 2 (Server/Database migration)
   - Update loader with deprecation system
   - Migrate database configuration
   - Migrate server configuration
   - Update tests

2. **Step 2:** Complete Phase 3 (Security configuration)
   - Migrate keychain/KMS variables
   - Implement sensitive value handling
   - Add production mode enforcement

3. **Step 3:** Complete Phase 4 (Backend/Debug configuration)
   - Migrate kernel environment variables
   - Add backend enable/disable flags
   - Consolidate debug flags

4. **Step 4:** Complete Phase 5 (Freeze enforcement)
   - Audit and replace `std::env::var` calls
   - Enable freeze in production
   - Add violation metrics

5. **Step 5:** Complete Phase 6 (Documentation/Tooling)
   - Comprehensive `.env.example`
   - CLI validation commands
   - Migration tooling

6. **Step 6:** Validation and release
   - Integration testing
   - Documentation review
   - Deprecation communication

---

## 11. Appendix

### A. Current Environment Variable Inventory

**Documented in `.env.example` (4):**
- `AOS_MODEL_PATH`
- `AOS_MODEL_BACKEND`
- `RUST_LOG`
- `DATABASE_URL` (implied from `.env`)

**Undocumented but in use (30+):**
- `ADAPTEROS_SERVER_HOST`, `ADAPTEROS_SERVER_PORT`
- `ADAPTEROS_DATABASE_URL`
- `ADAPTEROS_ENV`
- `ADAPTEROS_KEYCHAIN_FALLBACK`
- `AOS_ADAPTER_PATH`
- `AOS_DETERMINISTIC_DEBUG`
- `AOS_GPU_INDEX`
- `AOS_SKIP_KERNEL_SIGNATURE_VERIFY`
- `AOS_SERVER_PORT`
- `AOS_MANIFEST_PATH`
- `MLX_PATH`
- `SUPERVISOR_LOG_DIR`
- `DATABASE_URL`
- ... and more across 54 files

### B. Configuration File Locations

| File | Purpose | Priority |
|------|---------|----------|
| `.env` | Root configuration | Primary |
| `crates/adapteros-secd/.env` | Enclave-specific | Secondary |
| `ui/.env.local` | UI development | UI only |
| `.cargo/config.toml` | Rust build config | Build only |

### C. Related Documents

- [docs/ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - System architecture
- [CLAUDE.md](../CLAUDE.md) - Developer guide (configuration section)
- [crates/adapteros-config/README.md](../crates/adapteros-config/README.md) - Config crate documentation

---

## 12. CLI Tool Specifications

### 12.1 Config Validate Command

**Command:** `aosctl config validate`

**Purpose:** Validate configuration files and environment variables without starting the server, enabling pre-deployment validation in CI/CD pipelines.

#### Syntax

```bash
aosctl config validate [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--env-file` | `-e` | PATH | `.env` | Path to .env file to validate |
| `--strict` | `-s` | FLAG | false | Fail on deprecation warnings, not just errors |
| `--production` | `-p` | FLAG | false | Validate against production requirements |
| `--format` | `-f` | STRING | `text` | Output format: `text`, `json`, `sarif` |
| `--quiet` | `-q` | FLAG | false | Only output errors, suppress info/warnings |
| `--manifest` | `-m` | PATH | None | Optional manifest file to validate against |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All validations passed |
| 1 | Validation errors found |
| 2 | Configuration file not found or unreadable |
| 3 | Invalid command arguments |

#### Output Format (Text)

```
Configuration Validation Report
===============================
Source: /path/to/.env
Mode: Development (use --production for production checks)

✓ AOS_MODEL_PATH: ./models/qwen2.5-7b (valid path, exists)
✓ AOS_MODEL_BACKEND: mlx (valid: auto|coreml|metal|mlx)
✓ AOS_DATABASE_URL: sqlite:var/aos-cp.sqlite3 (valid URL)
⚠ DATABASE_URL: deprecated → use AOS_DATABASE_URL (removal: v0.03)
✗ AOS_SERVER_PORT: abc (invalid: expected integer 1-65535)
✗ AOS_MEMORY_HEADROOM_PCT: 150 (invalid: expected 0.0-1.0)

Summary:
  Valid:      4
  Warnings:   1
  Errors:     2

Status: FAILED
```

#### Output Format (JSON)

```json
{
  "source": "/path/to/.env",
  "mode": "development",
  "timestamp": "2025-11-22T10:30:00Z",
  "variables": [
    {
      "name": "AOS_MODEL_PATH",
      "value": "./models/qwen2.5-7b",
      "status": "valid",
      "source": "env_file",
      "type": "path",
      "validation": { "exists": true, "readable": true }
    },
    {
      "name": "DATABASE_URL",
      "value": "***REDACTED***",
      "status": "deprecated",
      "replacement": "AOS_DATABASE_URL",
      "removal_version": "v0.03"
    },
    {
      "name": "AOS_SERVER_PORT",
      "value": "abc",
      "status": "error",
      "error": "Expected integer in range 1-65535, got 'abc'"
    }
  ],
  "summary": {
    "valid": 4,
    "warnings": 1,
    "errors": 2
  },
  "passed": false
}
```

#### Production Mode Checks

When `--production` is specified, additional validations are enforced:

| Check | Requirement | Error Message |
|-------|-------------|---------------|
| UDS Socket | `AOS_SERVER_UDS_SOCKET` must be set | "Production mode requires UDS socket (AOS_SERVER_UDS_SOCKET)" |
| JWT Mode | `AOS_SECURITY_JWT_MODE=eddsa` | "Production mode requires EdDSA JWT (AOS_SECURITY_JWT_MODE=eddsa)" |
| PF Deny | `AOS_SECURITY_PF_DENY=true` | "Production mode requires PF deny (AOS_SECURITY_PF_DENY=true)" |
| No Debug | `AOS_DEBUG_*` vars should not be set | "Production mode should not have debug flags enabled" |
| Secure Secrets | JWT secret must not be default | "Production mode requires custom JWT secret" |

#### Validation Rules

```rust
// Type validations
pub enum ConfigType {
    String,                          // Any string
    Path { must_exist: bool },       // File/directory path
    Url,                             // Valid URL format
    Integer { min: i64, max: i64 },  // Bounded integer
    Float { min: f64, max: f64 },    // Bounded float
    Bool,                            // true/false, 1/0, yes/no
    Enum { values: Vec<String> },    // One of allowed values
    Duration,                        // e.g., "30s", "5m", "1h"
    ByteSize,                        // e.g., "512MB", "2GB"
}
```

#### Usage Examples

```bash
# Basic validation
aosctl config validate

# Validate specific .env file
aosctl config validate --env-file /etc/aos/production.env

# CI/CD pipeline validation (JSON output, strict mode)
aosctl config validate --production --strict --format json

# Pre-deployment check
aosctl config validate --production || exit 1
```

---

### 12.2 Config Migrate Command

**Command:** `aosctl config migrate`

**Purpose:** Automatically migrate legacy environment variables to the new unified naming convention, with optional backup and dry-run support.

#### Syntax

```bash
aosctl config migrate [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--input` | `-i` | PATH | `.env` | Source .env file to migrate |
| `--output` | `-o` | PATH | same as input | Destination file (use `-` for stdout) |
| `--dry-run` | `-n` | FLAG | false | Show changes without writing |
| `--backup` | `-b` | FLAG | true | Create .env.backup before writing |
| `--no-backup` | | FLAG | false | Skip backup creation |
| `--format` | `-f` | STRING | `text` | Output format: `text`, `json`, `diff` |
| `--interactive` | | FLAG | false | Prompt for each migration decision |
| `--remove-deprecated` | | FLAG | false | Remove deprecated vars after migration |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Migration successful (or no changes needed) |
| 1 | Migration failed |
| 2 | Source file not found |
| 3 | Write permission denied |
| 4 | User cancelled (interactive mode) |

#### Migration Mapping

The command uses the legacy variable mapping from Section 5.2:

```rust
const MIGRATION_MAP: &[(&str, &str)] = &[
    ("DATABASE_URL", "AOS_DATABASE_URL"),
    ("MLX_PATH", "AOS_MLX_PATH"),
    ("ADAPTEROS_SERVER_PORT", "AOS_SERVER_PORT"),
    ("ADAPTEROS_SERVER_HOST", "AOS_SERVER_HOST"),
    ("ADAPTEROS_DATABASE_URL", "AOS_DATABASE_URL"),
    ("ADAPTEROS_ENV", "AOS_ENVIRONMENT"),
    ("ADAPTEROS_KEYCHAIN_FALLBACK", "AOS_KEYCHAIN_FALLBACK"),
    ("AOS_DETERMINISTIC_DEBUG", "AOS_DEBUG_DETERMINISTIC"),
    ("AOS_SKIP_KERNEL_SIGNATURE_VERIFY", "AOS_DEBUG_SKIP_KERNEL_SIG"),
    ("AOS_GPU_INDEX", "AOS_BACKEND_GPU_INDEX"),
    ("AOS_MLX_FFI_MODEL", "AOS_MODEL_PATH"),
];
```

#### Output Format (Text - Dry Run)

```
Configuration Migration Preview
===============================
Source: .env
Target: .env (in-place)

Migrations:
  DATABASE_URL → AOS_DATABASE_URL
    Current: sqlite:var/aos-cp.sqlite3

  ADAPTEROS_SERVER_PORT → AOS_SERVER_PORT
    Current: 3000

  AOS_MLX_FFI_MODEL → AOS_MODEL_PATH
    Current: ./models/qwen2.5-7b-mlx

No changes made (dry-run mode).
Run without --dry-run to apply changes.

Summary:
  Variables to migrate: 3
  Already migrated: 12
  Unknown variables: 2
```

#### Output Format (Diff)

```diff
--- .env.original
+++ .env.migrated
@@ -1,6 +1,6 @@
-DATABASE_URL=sqlite:var/aos-cp.sqlite3
+AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3

-ADAPTEROS_SERVER_PORT=3000
-ADAPTEROS_SERVER_HOST=127.0.0.1
+AOS_SERVER_PORT=3000
+AOS_SERVER_HOST=127.0.0.1

-AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
+AOS_MODEL_PATH=./models/qwen2.5-7b-mlx
```

#### Output Format (JSON)

```json
{
  "source": ".env",
  "target": ".env",
  "backup": ".env.backup",
  "dry_run": false,
  "migrations": [
    {
      "from": "DATABASE_URL",
      "to": "AOS_DATABASE_URL",
      "value": "***REDACTED***",
      "status": "migrated"
    },
    {
      "from": "ADAPTEROS_SERVER_PORT",
      "to": "AOS_SERVER_PORT",
      "value": "3000",
      "status": "migrated"
    }
  ],
  "unchanged": [
    { "name": "AOS_MODEL_BACKEND", "reason": "already_using_new_name" },
    { "name": "RUST_LOG", "reason": "no_migration_available" }
  ],
  "summary": {
    "migrated": 3,
    "unchanged": 12,
    "errors": 0
  }
}
```

#### Interactive Mode

When `--interactive` is specified:

```
Migration: DATABASE_URL → AOS_DATABASE_URL
  Current value: sqlite:var/aos-cp.sqlite3

  [M]igrate  [S]kip  [K]eep both  [Q]uit
  > m

Migration: ADAPTEROS_SERVER_PORT → AOS_SERVER_PORT
  Current value: 3000

  [M]igrate  [S]kip  [K]eep both  [Q]uit
  > k
```

#### Usage Examples

```bash
# Preview migrations (dry-run)
aosctl config migrate --dry-run

# Migrate with backup
aosctl config migrate --backup

# Migrate and output to new file
aosctl config migrate --input .env.old --output .env.new

# Generate diff for code review
aosctl config migrate --dry-run --format diff > migration.patch

# Interactive migration
aosctl config migrate --interactive

# CI/CD: Check if migration is needed
aosctl config migrate --dry-run --format json | jq '.summary.migrated > 0'
```

#### Safety Features

1. **Backup by default**: Creates `.env.backup` before any modification
2. **Conflict detection**: If both legacy and new variable exist, warns and keeps both
3. **Value preservation**: Never modifies values, only renames variables
4. **Atomic write**: Uses temp file + rename for crash safety
5. **Comment preservation**: Maintains all comments and blank lines in .env file

---

### 12.3 Config Show Command

**Command:** `aosctl config show`

**Purpose:** Display the effective configuration with source attribution for each value, useful for debugging configuration precedence issues.

#### Syntax

```bash
aosctl config show [OPTIONS] [FILTER]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--format` | `-f` | STRING | `table` | Output format: `table`, `json`, `env` |
| `--category` | `-c` | STRING | all | Filter by category: `model`, `server`, `database`, `security` |
| `--show-defaults` | | FLAG | false | Include default values |
| `--show-unset` | | FLAG | false | Include unset optional variables |
| `--no-redact` | | FLAG | false | Show sensitive values (requires confirmation) |

#### Output Format (Table)

```
Effective Configuration
=======================
Source priority: CLI > ENV > .env > Manifest > Defaults

Category: Model
  AOS_MODEL_PATH         = ./models/qwen2.5-7b     [.env]
  AOS_MODEL_BACKEND      = auto                    [default]
  AOS_MODEL_ARCHITECTURE = qwen2                   [.env]

Category: Server
  AOS_SERVER_HOST        = 127.0.0.1              [env]
  AOS_SERVER_PORT        = 3000                   [.env]
  AOS_SERVER_WORKERS     = 4                      [default]

Category: Security
  AOS_SECURITY_JWT_SECRET = ***REDACTED***        [env]
  AOS_SECURITY_JWT_MODE   = eddsa                 [.env]

Category: Database
  AOS_DATABASE_URL       = ***REDACTED***         [.env]
  AOS_DATABASE_POOL_SIZE = 10                     [default]

Legend: [cli] [env] [.env] [manifest] [default]
```

#### Usage Examples

```bash
# Show all configuration
aosctl config show

# Show only model configuration
aosctl config show --category model

# Export as .env format (for copying to another machine)
aosctl config show --format env > exported.env

# JSON output for scripting
aosctl config show --format json | jq '.model.path'
```

---

## 13. Implementation: CLI Commands Module

### 13.1 File Structure

```
crates/adapteros-cli/src/commands/
├── mod.rs          # Add config module
├── config.rs       # NEW: Config commands implementation
└── ...
```

### 13.2 Command Registration

```rust
// In crates/adapteros-cli/src/main.rs
#[derive(Subcommand)]
enum Commands {
    // ... existing commands

    /// Configuration management commands
    Config(ConfigArgs),
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Validate configuration files
    Validate(ValidateArgs),
    /// Migrate legacy environment variables
    Migrate(MigrateArgs),
    /// Show effective configuration
    Show(ShowArgs),
}
```

### 13.3 Validation Implementation Outline

```rust
// crates/adapteros-cli/src/commands/config.rs
pub async fn validate(args: ValidateArgs) -> Result<()> {
    // 1. Load .env file
    let env_path = args.env_file.unwrap_or_else(|| PathBuf::from(".env"));

    // 2. Parse all variables
    let variables = parse_env_file(&env_path)?;

    // 3. Validate each variable against schema
    let results: Vec<ValidationResult> = variables
        .iter()
        .map(|(k, v)| validate_variable(k, v, &CONFIG_SCHEMA))
        .collect();

    // 4. Run production checks if requested
    if args.production {
        results.extend(validate_production_requirements(&variables)?);
    }

    // 5. Output results
    match args.format.as_str() {
        "json" => output_json(&results)?,
        "sarif" => output_sarif(&results)?,
        _ => output_text(&results)?,
    }

    // 6. Exit with appropriate code
    if results.iter().any(|r| r.is_error()) {
        std::process::exit(1);
    }

    Ok(())
}
```

### 13.4 Migration Implementation Outline

```rust
pub async fn migrate(args: MigrateArgs) -> Result<()> {
    // 1. Read source file
    let content = std::fs::read_to_string(&args.input)?;

    // 2. Parse preserving comments and structure
    let mut lines = parse_env_with_structure(&content)?;

    // 3. Apply migrations
    let migrations = apply_migrations(&mut lines, &MIGRATION_MAP)?;

    // 4. Handle dry-run
    if args.dry_run {
        output_preview(&migrations, args.format)?;
        return Ok(());
    }

    // 5. Create backup if requested
    if args.backup {
        let backup_path = format!("{}.backup", args.input.display());
        std::fs::copy(&args.input, &backup_path)?;
    }

    // 6. Write atomically
    let output_path = args.output.unwrap_or(args.input);
    atomic_write(&output_path, &serialize_env(&lines))?;

    // 7. Output summary
    output_summary(&migrations, args.format)?;

    Ok(())
}
```

---

**Document History:**

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-11-22 | James KC Auchterlonie | Initial draft |
| 1.1 | 2025-11-22 | James KC Auchterlonie | Added CLI tool specifications (Sections 12-13) |
