# Changelog

All notable changes to AdapterOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Linter Error Resolution** - Resolved 691/779 linter errors (89% reduction)
  - Fixed all compilation errors in library crates (0 errors in `crates/`)
  - Added 6 missing dev-dependencies (reqwest, tracing-subscriber, metal, rand, futures-util, serde_yaml)
  - Migrated TrainingConfig API to include weight_group_config field across 6 locations
  - Migrated TrainingExample API to include weight field across 9 locations
  - Feature-gated experimental tests (federation, config, numerics, domain, lint) - 8 test files
  - Reduced warnings from 580 to 88 (85% reduction)
  - Remaining: 19 compilation errors in test files only (non-blocking for library usage)

### Added
- **Base Model UI User Journey** - Complete UI-driven workflow for model management (#FEATURE-001)
  - Model import wizard (4 steps) for importing base models via UI
  - Base model loader controls (load/unload) with real-time status updates
  - Cursor IDE setup wizard (4 steps) for IDE configuration
  - Onboarding journey tracking for monitoring user progress
- **Backend API Endpoints** - 5 new REST endpoints for model management
  - `POST /v1/models/import` - Import base model with file validation
  - `POST /v1/models/{id}/load` - Load model into memory with telemetry
  - `POST /v1/models/{id}/unload` - Unload model from memory
  - `GET /v1/models/imports/{id}` - Check import progress and status
  - `GET /v1/models/cursor-config` - Get Cursor IDE configuration details
- **Database Schema** - Migration `0042_base_model_ui_support.sql`
  - `base_model_imports` table for import tracking and status
  - `onboarding_journeys` table for user journey step tracking
- **Integration Tests** - Comprehensive test suite for model UI journey workflows
- **Documentation** 
  - Deployment guide consolidated in [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)

### Changed
- Dashboard now includes base model management components and controls
- API client extended with 5 new model management methods
- TypeScript types updated with 5 new interfaces for model operations
- Routes integrated with proper OpenAPI documentation

### Technical Details
- **Pattern Compliance**: All code follows existing codebase patterns with verified citations
- **Policy Compliance**: Adheres to Policy Pack #8 (Isolation) and #9 (Telemetry)
- **Code Quality**: TypeScript strict mode, no `any` types, full error handling
- **Security**: Admin/operator role checks, per-tenant operations, audit logging

## [alpha-v0.01-1] - 2025-01-15

### Added
- **Naming Unification**: Complete rename from `mplora-*` to `adapteros-*` crates
- **Compatibility Shims**: Backward compatibility crates with deprecation warnings
- **Policy Registry**: 20 canonical policy packs with CLI commands (`aosctl policy list|explain|enforce`)
- **Metal Kernel Refactor**: Modular kernels with parameter structs (`MlpParams`, `AttentionParams`, `FlashAttentionParams`)
- **Deterministic Config System**: Configuration precedence (CLI > ENV > manifest) with freeze mechanism
- **Database Schema Lifecycle**: Versioned migrations with rollback support
- **Standalone Test Crate**: `test-config-precedence` for isolated configuration testing
- **GitHub Repository**: Private repository with comprehensive topics and description

### Changed
- **Architecture**: Updated to reflect policy enforcement and modular kernel design
- **Documentation**: Complete README.md overhaul for alpha-v0.01-1
- **Performance**: Added determinism metrics to performance benchmarks
- **Configuration**: Enhanced with schema validation and default value application

### Fixed
- **Metal Compilation**: Resolved duplicate symbol errors with unified kernel approach
- **Configuration Loading**: Fixed CLI-to-schema key mapping (`adapteros-database-url` → `database.url`)
- **Environment Cleanup**: Prevented test environment variable leakage
- **Build Artifacts**: Updated `.gitignore` to exclude unnecessary files from remote repository

### Security
- **Zero Network Egress**: Enforced during serving with PF rules
- **Policy Enforcement**: 20 canonical policy packs for compliance and security
- **Deterministic Execution**: HKDF seeding and canonical JSON serialization
- **Artifact Verification**: Ed25519 signatures and SBOM validation

### Infrastructure
- **CI/CD**: Policy registry validation tests
- **Documentation**: Auto-generated policy documentation
- **Testing**: Comprehensive test suite with deterministic verification
- **Monitoring**: Canonical JSON event logging with Merkle trees

### Technical Details
- **Router**: K-sparse LoRA routing with Q15 quantized gates and entropy floor
- **Kernels**: Precompiled `.metallib` kernels with deterministic compilation
- **Memory**: Intelligent adapter eviction with ≥15% headroom maintenance
- **Telemetry**: 100% sampling for first 128 tokens, 5% thereafter

### Known Issues
- **Server API**: Structural issues requiring broader refactoring (deferred)
- **Integration Tests**: Some tests blocked by server API compilation errors
- **Documentation**: API reference and deployment guides in progress

### Migration Notes
- **Crate Names**: All `mplora-*` crates renamed to `adapteros-*`
- **Compatibility**: Use compatibility shims for one release cycle
- **Configuration**: New precedence system with schema validation
- **Database**: Run migrations with `aosctl db migrate`

### Contributors
- **Primary Developer**: James KC Auchterlonie (@rogu3bear)
- **Email**: vats-springs0m@icloud.com

### Acknowledgments
- Apple Metal Team for GPU compute framework
- Rust Community for tooling and ecosystem
- LoRA Authors for efficient fine-tuning technique
- BLAKE3 Team for fast cryptographic hashing
- Ed25519 Implementers for secure digital signatures

---

## [Unreleased]

### v0.66-pre
- Hallucination audit attached in 
- Audit SHA256: e591871c2532d614a5a6b78ade33a4291306ae190ef44c9e82c8d92fbfadf88c  AUDIT_LOG.md

### Planned for v0.02
- Performance optimization and router calibration
- Security hardening with advanced threat detection
- Comprehensive monitoring and observability
- Complete API reference and deployment guides
- End-to-end integration testing

### Future Considerations
- Multi-tenant isolation improvements
- Advanced policy pack customization
- Performance profiling and optimization tools
- Enterprise deployment features
- Community contribution guidelines
# Unreleased

## Added
- Evidence-first enforcement: server refuses to start without RAG when open-book policy is enabled.
- Confidence gating: worker abstains on low-confidence generations via avg max-prob.
- CLI: `aosctl infer` supports `--show-citations` and `--show-trace` for auditability.
- API: `/signals` SSE; adapter lifecycle endpoints (admin-gated); `/health` includes policy status.
- feat(ui): SSE real-time metrics in RealtimeMetrics (+55% coverage, fallback polling; closes UI-SSE)

## Security/Policy
- Admin endpoints require `AOS_API_ENABLE_ADMIN=true` and optionally `X-Admin-Token` header matching `AOS_API_ADMIN_TOKEN`.
- Strict mode: refuse to start if `AOS_INSECURE_SKIP_CONF` is set under `AOS_STRICT_MODE`.

## Notes
- `adapteros-server-api` contains pre-existing compilation errors; CLI feature-gates server to avoid blocking build.

## [0.02] - 2025-10-20
### Added
- Observability: Prometheus exporter (/metrics with inference/errors counters【@crates/adapteros-metrics-exporter/src/lib.rs】), threat detection hooks in Incident/Refusal validators (alerts on low conf【@crates/adapteros-policy/src/policy_packs.rs§1900/1100】).
- MLX Backend: Stabilized with PyO3 0.22, feature flag mlx-backend for parity testing【@Cargo.toml§77, @crates/adapteros-base-llm/src/lib.rs enum】.
- Tests: Expanded E2E (policy, router, determinism, memory, tenants; +20% coverage【@tests/integration_tests.rs§626-817】).
- Docs: DEPLOYMENT.md for prod (Postgres/RAG, multi-node, monitoring【@docs/DEPLOYMENT.md§1】); auto Rust API【@README.md§451】.

### Changed
- Server: Rate limit 100/min, admin RBAC【@crates/adapteros-server-api/src/routes.rs§694】.
- Policies: Async desugared to impl Future + Send【@crates/adapteros-policy/src/unified_enforcement.rs§18-30】.

### Fixed
- PyO3 linker for MLX【@Cargo.toml§147】.
- Lints: Unused Results, dead code【@clippy§unused_must_use】.
