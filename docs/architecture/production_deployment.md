# Production Testing & Deployment

This document describes the production testing suites, CI gates, and deployment workflows for AdapterOS.

## CI Gates

### GitHub Actions Workflow
- **Location**: `.github/workflows/ci.yml`
- **Triggers**: Push to main/develop, pull requests

### CI Jobs

1. **Format Check** (`fmt`)
   - Runs `cargo fmt --all -- --check`
   - Ensures consistent code formatting

2. **Clippy Lint** (`clippy`)
   - Runs `cargo clippy --workspace --all-features -- -D warnings`
   - Catches common Rust errors and style issues

3. **Test Suite** (`test`)
   - Runs `cargo test --workspace --exclude adapteros-lora-mlx-ffi`
   - Excludes MLX FFI tests that require Apple Silicon

4. **Deterministic Execution Tests** (`deterministic-tests`)
   - Runs router determinism tests
   - Runs deterministic executor tests
   - Validates seeded randomness and Q15 routing

5. **Build Release** (`build`)
   - Builds release binaries with `cargo build --release --locked`
   - Ensures production builds succeed

6. **UI Build** (`ui-build`)
   - Installs pnpm dependencies
   - Builds production UI bundle
   - Validates UI compilation

7. **Policy Pack Validation** (`policy-validation`)
   - Runs policy pack tests
   - Validates all 20+ policy packs

8. **Integration Tests** (`integration-tests`)
   - Runs end-to-end integration tests
   - Requires build to complete first

9. **Deployment Verification** (`deployment-verification`)
   - Runs `scripts/verify-deployment.sh`
   - Validates deployment readiness

## Makefile Targets

### Development Targets
- `make fmt` - Format code
- `make clippy` - Run clippy
- `make test` - Run tests
- `make check` - Run fmt + clippy + test

### Build Targets
- `make build` - Build release binaries
- `make ui` - Build production UI
- `make metal` - Build Metal shaders

### Deployment Targets
- `make verify-artifacts` - Verify and sign artifacts
- `make sbom` - Generate SBOM
- `make determinism-report` - Generate determinism report

## Production Deployment

### Deployment Script
- **Location**: `scripts/deploy-production.sh`
- **Requirements**: Root access for UDS socket creation

### Deployment Steps

1. **Configuration Validation**
   - Verifies `production_mode = true`
   - Verifies `jwt_mode = "eddsa"`
   - Verifies `uds_socket` configuration
   - Checks `require_pf_deny = true`

2. **UDS Socket Setup**
   - Creates UDS socket directory with restrictive permissions (700)
   - Ensures directory exists before service start

3. **JWT Key Verification**
   - Verifies Ed25519 public key file exists
   - Verifies Ed25519 signing key file exists
   - Validates key file paths from config

4. **Zero-Egress Verification**
   - Checks PF (Packet Filter) status on macOS
   - Validates egress blocking rules
   - Warns if PF not enabled

5. **Build Release Binary**
   - Builds optimized release binary
   - Uses `--locked` flag for reproducible builds

6. **Database Migrations**
   - Runs migrations via `--migrate-only` flag
   - Creates database if needed
   - Validates migration success

7. **Systemd Service Creation**
   - Creates systemd service file
   - Configures security hardening (NoNewPrivileges, PrivateTmp, ProtectSystem)
   - Sets up logging to journald
   - Configures auto-restart

8. **Service Activation**
   - Reloads systemd daemon
   - Enables service for auto-start
   - Provides start instructions

### Post-Deployment Verification

```bash
# Check service status
sudo systemctl status adapteros-cp

# View logs
sudo journalctl -u adapteros-cp -f

# Verify UDS socket
ls -l /var/run/adapteros/control-plane.sock

# Test health endpoint (via UDS proxy or direct connection)
```

## Deterministic Test Suites

### Router Determinism
- **Location**: `crates/adapteros-lora-router/tests/determinism.rs`
- **Validates**: Identical seeds produce identical router decisions
- **Run**: `cargo test -p adapteros-lora-router determinism`

### Kernel Determinism
- **Location**: Metal kernel build verification
- **Validates**: Precompiled metallib hash matches expected
- **Run**: Build-time hash verification

### Executor Determinism
- **Location**: `crates/adapteros-deterministic-exec/tests/`
- **Validates**: HKDF seed derivation produces consistent results
- **Run**: `cargo test -p adapteros-deterministic-exec`

## Integration Tests

### End-to-End Tests
- **Location**: `tests/integration_tests.rs`
- **Coverage**: Full request flow from API to worker response
- **Run**: `cargo test --test integration_tests`

### Policy Enforcement Tests
- **Location**: `crates/adapteros-policy/tests/integration_tests.rs`
- **Coverage**: All 20+ policy packs
- **Run**: `cargo test -p adapteros-policy`

## Deployment Verification

### Verification Script
- **Location**: `scripts/verify-deployment.sh`
- **Checks**:
  - Database migrations applied
  - All crates compile successfully
  - Service files exist
  - Deployment scripts available
  - Documentation complete

### Manual Verification Checklist

- [ ] Production config has `production_mode = true`
- [ ] UDS socket path configured and writable
- [ ] Ed25519 JWT keys present and readable
- [ ] PF rules configured for zero egress
- [ ] Database migrations applied
- [ ] Release binary built successfully
- [ ] Systemd service file created
- [ ] Service starts without errors
- [ ] Health endpoint responds
- [ ] UDS socket accessible
- [ ] Telemetry bundles being created
- [ ] Audit logs being written

## Production Readiness Criteria

### Required Checks
1. ✅ All CI gates pass
2. ✅ Deterministic tests pass
3. ✅ Policy enforcement tests pass
4. ✅ Integration tests pass
5. ✅ Deployment verification passes
6. ✅ Production config validated
7. ✅ UDS socket accessible
8. ✅ Zero-egress enforced
9. ✅ Ed25519 JWTs working
10. ✅ Telemetry canonical JSON verified

### Optional Enhancements
- [ ] Performance benchmarks pass
- [ ] Load testing completed
- [ ] Security audit completed
- [ ] Compliance certification obtained

## Rollback Procedure

### Service Rollback
```bash
# Stop current service
sudo systemctl stop adapteros-cp

# Revert to previous binary
sudo cp /backup/aos-cp /usr/local/bin/aos-cp

# Restart service
sudo systemctl start adapteros-cp
```

### Database Rollback
```bash
# Restore database backup
sudo systemctl stop adapteros-cp
cp /backup/aos-cp.sqlite3 /var/lib/adapteros/aos-cp.sqlite3
sudo systemctl start adapteros-cp
```

### Configuration Rollback
```bash
# Revert config file
sudo cp /backup/production-multinode.toml /etc/adapteros/config.toml
sudo systemctl restart adapteros-cp
```

## Monitoring & Alerts

### Key Metrics
- Service uptime
- Request latency (p95)
- Error rates
- Memory usage
- Adapter eviction rate
- Policy violation count

### Alert Thresholds
- Service down: Immediate alert
- Latency > 100ms (p95): Warning
- Error rate > 1%: Warning
- Memory usage > 85%: Warning
- Policy violations: Alert per violation

## Validation Status

✅ CI workflow: GitHub Actions configured  
✅ Makefile targets: Development and deployment targets  
✅ Deployment script: Production deployment automation  
✅ Deterministic tests: Router, kernel, executor tests  
✅ Integration tests: End-to-end coverage  
✅ Deployment verification: Automated checks  
✅ Rollback procedure: Documented and tested  

## Recommendations

1. **CI Enhancements**: Add performance benchmarks to CI
2. **Deployment Automation**: Add blue-green deployment support
3. **Monitoring Integration**: Add Prometheus metrics export
4. **Alerting**: Integrate with PagerDuty/Slack for alerts
5. **Chaos Testing**: Add chaos engineering tests for resilience
6. **Load Testing**: Add automated load testing before releases

