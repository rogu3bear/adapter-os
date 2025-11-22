# Offline CVE Database Mode

## Overview

The dependency security policy pack includes an offline fallback mechanism that enables CVE vulnerability checking without network connectivity. This is essential for air-gapped environments, CI/CD pipelines without internet access, and resilient deployment scenarios.

## Features

- **Automatic Network Detection**: Checks network availability before making API calls
- **Graceful Fallback**: Falls back to offline database when network is unavailable
- **Multiple Data Sources**: Supports bundled NVD, OSV, and custom vulnerability databases
- **Flexible Configuration**: Easy setup and customization of offline data paths
- **Thread-Safe**: Safe concurrent access to offline database from multiple tasks
- **Non-Blocking**: Uses async I/O for database loading

## Architecture

```
DependencySecurityPolicy
├── Online Path (Network Available)
│   ├── NVD API Query
│   └── OSV API Query
├── Offline Path (Network Unavailable)
│   └── Offline Database Query
└── Cache Layer
    └── Prevents Redundant Queries
```

## Quick Start

### 1. Enable Offline Mode

```rust
use adapteros_policy::packs::{
    DependencySecurityConfig,
    DependencySecurityPolicy,
    OfflineCveDatabase,
};
use std::path::PathBuf;

let mut config = DependencySecurityConfig::default();
config.offline_database.enabled = true;
config.offline_database.database_path = PathBuf::from("./cves");

let policy = DependencySecurityPolicy::new(config);

// Load offline database from JSON files
policy.load_offline_database(None).await?;

// Now check_dependency() will use offline data when network unavailable
let assessment = policy.check_dependency("log4j", "2.14.1").await?;
```

### 2. Environment Variable for CI/CD

For CI/CD pipelines and testing, force offline mode:

```bash
export CVE_OFFLINE_MODE=1
cargo test
```

This disables network checks and always uses the offline database.

### 3. Database File Format

The offline database supports three JSON file formats in the configured directory:

#### known_vulnerabilities.json (Recommended)
```json
[
  {
    "cve_id": "CVE-2021-44228",
    "package_name": "log4j",
    "affected_versions": ["2.0", "2.1", "..."],
    "fixed_version": "2.15.0",
    "cvss_score": 10.0,
    "epss_score": 0.98,
    "severity": "Critical",
    "description": "Log4Shell RCE vulnerability",
    "published_date": "2021-12-10T00:00:00Z",
    "modified_date": "2021-12-10T00:00:00Z",
    "references": ["https://nvd.nist.gov/vuln/detail/CVE-2021-44228"],
    "cwe_ids": ["CWE-94"],
    "data_source": "Nvd"
  }
]
```

#### nvd_responses.json
Cached NVD API responses for offline use.

#### osv_responses.json
Cached OSV API responses for offline use.

## Configuration

### Default Offline Database Location

```rust
OfflineCveDatabase {
    database_path: PathBuf::from("./cves"),
    enabled: true,
    vulnerabilities: HashMap::new(),
    loaded_at: None,
}
```

### Custom Database Path

```rust
let mut config = DependencySecurityConfig::default();
config.offline_database.database_path = PathBuf::from("/etc/aos/cves");
```

## Network Detection

The policy automatically detects network availability:

1. **Check Environment Variable**: If `CVE_OFFLINE_MODE` is set, skip network
2. **DNS Resolution Check**: Attempt to resolve `8.8.8.8:53`
3. **Fallback Decision**: Use offline database if network unavailable

### Disabling Network Check

```bash
# Force offline mode
export CVE_OFFLINE_MODE=1

# Unset to re-enable network checks
unset CVE_OFFLINE_MODE
```

## Usage Examples

### Example 1: Offline Dependency Check

```rust
let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());
let fixtures_path = PathBuf::from("./tests/fixtures/cve");

// Load offline database
policy.load_offline_database(Some(&fixtures_path)).await?;

// Force offline mode for testing
std::env::set_var("CVE_OFFLINE_MODE", "1");

let assessment = policy.check_dependency("log4j", "2.14.1").await?;
println!("Max CVSS: {}", assessment.max_cvss_score);
println!("Severity: {:?}", assessment.max_severity);

std::env::remove_var("CVE_OFFLINE_MODE");
```

### Example 2: Batch Assessment in Offline Mode

```rust
let policy = DependencySecurityPolicy::new(DependencySecurityConfig::default());

// Load database
policy.load_offline_database(None).await?;

// Enable offline mode
std::env::set_var("CVE_OFFLINE_MODE", "1");

let dependencies = vec![
    ("log4j".to_string(), "2.14.1".to_string()),
    ("lodash".to_string(), "4.17.20".to_string()),
    ("express".to_string(), "4.17.1".to_string()),
];

let result = policy.assess_dependencies(&dependencies).await?;
println!("Total: {}, Vulnerable: {}, Critical: {}",
    result.total_dependencies,
    result.vulnerable_count,
    result.critical_count
);

std::env::remove_var("CVE_OFFLINE_MODE");
```

### Example 3: Concurrent Offline Access

```rust
use tokio::task::JoinSet;

let policy = Arc::new(DependencySecurityPolicy::new(
    DependencySecurityConfig::default()
));

policy.load_offline_database(None).await?;
std::env::set_var("CVE_OFFLINE_MODE", "1");

let mut set = JoinSet::new();

for pkg_name in &["log4j", "lodash", "express"] {
    let p = policy.clone();
    let pkg = pkg_name.to_string();
    set.spawn(async move {
        p.check_dependency(&pkg, "1.0.0").await
    });
}

while let Some(result) = set.join_next().await {
    let assessment = result??;
    println!("Package: {}", assessment.dependency_name);
}

std::env::remove_var("CVE_OFFLINE_MODE");
```

## Database Management

### Creating Offline Database

1. Download CVE data from NVD or OSV
2. Convert to JSON format (see examples in `tests/fixtures/cve/`)
3. Place in configured database directory
4. Call `load_offline_database()` during policy initialization

### Updating Offline Database

The offline database is immutable at runtime. To update:

1. Replace JSON files in database directory
2. Restart the application
3. Call `load_offline_database()` again

### Database Size

- **known_vulnerabilities.json** (6 entries): ~15 KB
- **nvd_responses.json** (2 entries): ~8 KB
- **osv_responses.json** (3 entries): ~12 KB
- **Total (typical)**: ~35 KB per 11 vulnerabilities

Compress with gzip for distribution:
```bash
tar czf cves.tar.gz cves/
```

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Run tests with offline CVE mode
  env:
    CVE_OFFLINE_MODE: 1
  run: cargo test --test offline_cve_tests
```

### GitLab CI Example

```yaml
test_offline:
  variables:
    CVE_OFFLINE_MODE: 1
  script:
    - cargo test --test offline_cve_tests
```

### Docker Example

```dockerfile
FROM rust:latest

COPY cves/ /app/cves/

ENV CVE_OFFLINE_MODE=1

WORKDIR /app
RUN cargo test --test offline_cve_tests
```

## Logging

The policy logs network detection and offline fallback:

```rust
// Network available
debug!(package = %package_name, "Network available, querying CVE databases");

// Network unavailable
info!(package = %package_name, "Network unavailable, using offline database");

// Database load
debug!(path = ?path, "Loading offline CVE database");
info!(entries = count, "Offline CVE database loaded");
```

Enable debug logging:
```bash
RUST_LOG=debug cargo test
```

## Limitations & Known Behavior

1. **Data Freshness**: Offline database uses cached data. Network queries provide latest CVEs.
2. **Version Matching**: Offline database uses simple string matching on package names.
3. **Incomplete Data**: Offline database may not contain all CVEs. Online queries are comprehensive.
4. **Remediation Info**: Fixed versions may not be available in offline mode.
5. **API Features**: Some API features (EPSS scores) may be limited in offline mode.

## Troubleshooting

### Database Not Loading

```
warn!(path = ?path, "CVE JSON file has unexpected format")
```

**Solution**: Verify JSON files match expected schema (see examples)

### No CVEs Found in Offline Mode

1. Check database files exist in configured path
2. Verify `load_offline_database()` was called
3. Check package names match exactly
4. Run with `RUST_LOG=debug` to see search results

### Network Falsely Detected as Unavailable

1. Check DNS resolution to `8.8.8.8:53`
2. Verify firewall allows outbound DNS
3. Set `CVE_OFFLINE_MODE=1` to force offline

## Performance

- **Offline Database Load**: ~10-50ms (first time)
- **Cache Hit**: <1ms
- **Offline Query**: 5-20ms
- **Online Query**: 100-500ms (with network)
- **Concurrent Access**: Full parallelism with RwLock

## Thread Safety

All offline database operations are thread-safe:

```rust
let policy = Arc::new(policy);  // Safe to share across threads
policy.load_offline_database(None).await?;

// Multiple threads can query concurrently
```

## Testing

Comprehensive offline tests in `tests/offline_cve_tests.rs`:

```bash
# Run all offline tests
cargo test --test offline_cve_tests

# Run with debug logging
RUST_LOG=debug cargo test --test offline_cve_tests -- --nocapture

# Run specific test
cargo test --test offline_cve_tests test_offline_database_contains_log4j_cve
```

## See Also

- [Dependency Security Policy](../src/packs/dependency_security.rs)
- [Integration Tests](../tests/dependency_security_integration.rs)
- [Test Fixtures](../tests/fixtures/cve/)
- [NVD API Documentation](https://nvd.nist.gov/developers)
- [OSV API Documentation](https://osv.dev/)

## Contributing

To add new CVE fixtures:

1. Add entries to `tests/fixtures/cve/known_vulnerabilities.json`
2. Maintain consistent JSON schema
3. Use real CVE data from NVD or OSV
4. Update test documentation
5. Run `RUST_LOG=debug cargo test` to verify

Example contribution:

```json
{
  "cve_id": "CVE-YYYY-XXXXX",
  "package_name": "package-name",
  "affected_versions": ["1.0.0", "1.0.1"],
  "fixed_version": "1.0.2",
  "cvss_score": 7.5,
  "epss_score": 0.65,
  "severity": "High",
  "description": "Vulnerability description",
  "published_date": "YYYY-MM-DDTHH:MM:SSZ",
  "modified_date": "YYYY-MM-DDTHH:MM:SSZ",
  "references": ["https://..."],
  "cwe_ids": ["CWE-XXX"],
  "data_source": "Nvd"
}
```
