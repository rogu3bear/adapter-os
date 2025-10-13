# Changelog

All notable changes to AdapterOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
