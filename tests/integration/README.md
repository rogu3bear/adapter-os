# AdapterOS Integration Tests

Comprehensive integration tests for AdapterOS system workflows, focusing on multi-tenant scenarios, policy enforcement, and resource isolation.

## Test Categories

### Multi-Tenant Scenarios
- **Tenant Isolation**: Verifies complete data and resource separation between tenants
- **Concurrent Workloads**: Tests multiple tenants running inference simultaneously
- **Cross-Tenant Interference Prevention**: Ensures one tenant's activities don't affect others

### Policy Enforcement
- **Rule Validation**: Tests that tenant-specific policies are correctly applied
- **Access Control**: Verifies RBAC and permission enforcement across tenants
- **Compliance Checks**: Ensures regulatory compliance requirements are met

### Resource Isolation
- **Memory Limits**: Tests per-tenant memory allocation and enforcement
- **CPU Quotas**: Verifies CPU resource allocation and scheduling
- **Storage Isolation**: Ensures tenant data is properly segregated

## Test Structure

```
tests/integration/
├── README.md                    # This file
├── mod.rs                       # Module declarations
├── tenant_isolation.rs          # Tenant isolation tests
├── concurrent_workloads.rs      # Concurrent inference tests
├── cross_tenant_interference.rs # Interference prevention tests
├── policy_enforcement.rs        # Policy validation tests
├── resource_isolation.rs        # Resource limit tests
├── test_utils.rs                # Reusable test utilities
└── fixtures.rs                  # Test data and setup fixtures
```

## Running Tests

### Prerequisites
- Running AdapterOS instance with multi-tenant support
- Test database with tenant isolation enabled
- Proper authentication tokens for test tenants

### Execution
```bash
# Run all integration tests
cargo test --test integration -- --nocapture

# Run specific test category
cargo test --test integration tenant_isolation -- --nocapture

# Run with environment variables
MPLORA_TEST_URL=http://localhost:9443 \
TENANT_A_TOKEN=token1 \
TENANT_B_TOKEN=token2 \
cargo test --test integration -- --nocapture
```

### Test Configuration
Tests use the following environment variables:
- `MPLORA_TEST_URL`: Base URL for AdapterOS API (default: http://localhost:9443)
- `TENANT_A_TOKEN`: Authentication token for tenant A
- `TENANT_B_TOKEN`: Authentication token for tenant B
- `TENANT_C_TOKEN`: Authentication token for tenant C

## Test Utilities

The `test_utils.rs` module provides:
- `TestTenant`: Wrapper for tenant-specific operations
- `MultiTenantHarness`: Setup and teardown for multi-tenant tests
- `ResourceMonitor`: Track resource usage across tenants
- `PolicyValidator`: Verify policy enforcement
- `IsolationChecker`: Validate tenant isolation

## Key Test Scenarios

### 1. Tenant Data Isolation
- Verify tenant A cannot access tenant B's data
- Test repository isolation between tenants
- Check adapter access restrictions

### 2. Concurrent Inference
- Multiple tenants running inference simultaneously
- Resource allocation fairness
- Performance isolation under load

### 3. Policy Enforcement
- Tenant-specific policy application
- Cross-tenant policy interference prevention
- Policy violation detection and handling

### 4. Resource Limits
- Memory usage enforcement per tenant
- CPU time limits and scheduling
- Storage quota management

## Test Data

Tests use deterministic test data:
- Pre-defined tenant configurations
- Standard test repositories and adapters
- Known policy rules and constraints
- Predictable workload patterns

## Assertions and Validation

Tests validate:
- **Functional Correctness**: Operations complete successfully
- **Isolation**: No cross-tenant data leakage
- **Performance**: Resource usage within expected bounds
- **Policy Compliance**: All rules properly enforced
- **Determinism**: Identical inputs produce identical results

## Debugging Failed Tests

When tests fail:
1. Check tenant isolation logs
2. Verify authentication tokens
3. Examine resource usage metrics
4. Review policy enforcement traces
5. Check for cross-tenant interference

## Contributing

When adding new integration tests:
1. Follow the established naming conventions
2. Use the provided test utilities
3. Include proper setup and teardown
4. Add comprehensive assertions
5. Document test scenarios and expectations