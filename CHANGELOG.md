# Changelog

All notable changes to AdapterOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Targeting alpha-v0.11-unstable-pre-release

### Added
- Evidence envelopes with comprehensive test coverage
- Training and worker metadata enhancements
- Demo smoke script and e2e infrastructure
- Trace visualization components in UI
- Evidence and Receipts UI components
- CLI `db` commands and `verify-receipt`
- Testkit endpoints and contract snapshot testing
- KV quota management and purgeable memory
- StopController for inference cancellation
- SingleFlight deduplication utility for concurrent loads
- E2E test infrastructure and helper scripts

### Changed
- Major UI restructuring and backend enhancements
- Enhanced API types and telemetry
- Enhanced worker backends with model_key and generation
- Enhanced server API with improved handlers and inference
- Enhanced database layer with improved trace and audit
- Enhanced seed derivation and telemetry utilities
- Enhanced determinism testing and fixtures
- Removed deprecated packages system
- Workspace version inheritance for all crates (single source of truth)
- DATABASE_SCHEMA_VERSION updated to 212

### Fixed
- Resolved clippy warnings
- Unblocked main build
- Updated .gitignore patterns
- Policy pack count standardized to 25 across all documentation
- Redacted KMS credential debug output and zeroized secret fields

## [v0.9-unstable] - 2025-12-08

Pre-release. Includes adapter packages, routing determinism controls, UI package management, and docs refresh.

### Added
- Adapter packages system
- Routing determinism controls
- UI package management

### Changed
- Documentation refresh
- Build configuration updates

## [v0.8-beta-unstable] - 2025-11-21

Major Platform Consolidation release.

### Highlights
- Unified health diagnostics & telemetry pipeline
- Standardized error handling (AosError across all crates)
- Canonical lifecycle state machine (Unloaded→Hot→Resident)
- Modernized UI with integrated sidebar components
- Database schema normalization

### Breaking Changes
- Error types migrated from anyhow to AosError
- Removed migrations 0077, 0078 (consolidated)

### Stats
- 262 files changed
- 45+ Rust crates refactored
- 138 UI components modernized
- 32 database modules standardized

## [pre-release-alpha-v0.65] - 2025-10-14

Pre-release alpha version with core inference capabilities.

## [alpha-v0.04-unstable] - 2025-01-15

### Added
- **Naming Unification**: Complete rename from `mplora-*` to `adapteros-*` crates
- **Compatibility Shims**: Backward compatibility crates with deprecation warnings
- **Policy Registry**: 25 canonical policy packs with CLI commands (`aosctl policy list|explain|enforce`)
- **Metal Kernel Refactor**: Modular kernels with parameter structs
- **Deterministic Config System**: Configuration precedence (CLI > ENV > manifest) with freeze mechanism
- **Database Schema Lifecycle**: Versioned migrations with rollback support

### Changed
- Architecture updated to reflect policy enforcement and modular kernel design
- Complete README.md overhaul

### Fixed
- Metal compilation duplicate symbol errors
- Configuration loading CLI-to-schema key mapping
- Environment cleanup to prevent test variable leakage

### Security
- Zero Network Egress enforced during serving
- 25 canonical policy packs for compliance and security
- Deterministic execution with HKDF seeding and canonical JSON
- Ed25519 signatures and SBOM validation

---

## Earlier Releases

See git history for releases prior to alpha-v0.04-unstable.
