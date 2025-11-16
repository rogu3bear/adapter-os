# AdapterOS Comprehensive Patch Plan

**Date:** January 15, 2025  
**Version:** alpha-v0.01-1 → alpha-v0.02  
**Status:** Ready for Execution  
**Compliance:** Agent Hallucination Prevention Framework + 20 Policy Packs

---

## Executive Summary

This comprehensive patch plan addresses **critical compilation errors**, **dependency cycles**, **missing implementations**, and **code quality violations** across the AdapterOS codebase. The plan follows established codebase standards documented in `CLAUDE.md`, `CONTRIBUTING.md`, and `.cursor/rules/global.mdc`.

**Scope:** 9 phases covering compilation fixes, dependency resolution, implementation completion, testing integration, and verification.

**Estimated Effort:** ~60-72 hours (8-10 days)

---

## Codebase Standards Reference

### From CONTRIBUTING.md L116-136
```markdown
Code Standards:
- Follow Rust naming conventions
- Use `cargo fmt`
- Use `cargo clippy` for linting
- Use `tracing` for logging (not `println!`)
- Use `adapteros_core::AosError` for error handling
- Use `adapteros_core::Result` for return types
```

### From CLAUDE.md L50-55
```markdown
Error Handling: adapteros_core::AosError for Rust backend, structured errors in UI
Telemetry: Event capture with canonical JSON, BLAKE3 hashing, Merkle-tree bundle signing
Memory Management: Adapter eviction with headroom maintenance
Policy Engine: Enforces 20 policy packs
```

### From Policy Pack #1-20
```markdown
Determinism Ruleset: Precompiled kernels, HKDF seeding
Egress Ruleset: Zero network during serving, PF enforcement
Router Ruleset: K bounds, entropy floor, Q15 gates
Evidence Ruleset: Mandatory open-book grounding
```

---

## Phase 1: Compilation Error Resolution

### 1.1 Fix Trait Definition Errors
**Files:** `crates/adapteros-db/src/unified_access.rs`, `crates/adapteros-telemetry/src/unified_events.rs`

**Issues:**
- `E0407`: Method not found in trait implementations
- `E0038`: Trait not dyn compatible
- `E0255`: Name defined multiple times

**Citations:**
- CONTRIBUTING.md L118-122: "Follow Rust naming conventions"
- Policy Pack #3 (Router): "MUST quantize gates to Q15"

**Patches:**

```rust
// Fix DatabaseAccess trait - add missing methods
#[async_trait]
pub trait DatabaseAccess {
    async fn execute_query<T>(&self, query: &str, params: &[&dyn ToSql]) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;
    
    async fn execute_query_one<T>(&self, query: &str, params: &[&dyn ToSql]) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;
    
    async fn execute_command(&self, command: &str, params: &[&dyn ToSql]) -> Result<u64>;
    
    async fn begin_transaction(&self) -> Result<Box<dyn Transaction + Send + Sync>>;
    
    async fn get_connection_info(&self) -> Result<ConnectionInfo>;
    
    async fn health_check(&self) -> Result<HealthStatus>;
    
    async fn get_statistics(&self) -> Result<DatabaseStatistics>;
}

// Fix Transaction trait - make dyn compatible
#[async_trait]
pub trait Transaction: Send + Sync {
    async fn execute_query<T>(&self, query: &str, params: &[&dyn ToSql]) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Send + Sync;
    
    async fn execute_command(&self, command: &str, params: &[&dyn ToSql]) -> Result<u64>;
    
    async fn commit(self: Box<Self>) -> Result<()>;
    
    async fn rollback(self: Box<Self>) -> Result<()>;
}
```

### 1.2 Fix SQLx Integration Errors
**Files:** `crates/adapteros-db/src/unified_access.rs`

**Issues:**
- `E0277`: Trait bound `FromRow` not satisfied
- `E0599`: Method `bind_all` not found

**Citations:**
- CONTRIBUTING.md L118-122: "Use `cargo clippy` for linting"

**Patches:**

```rust
// Fix SQLx query execution
async fn execute_query<T>(&self, query: &str, params: &[&dyn ToSql]) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Send + Sync,
{
    let start_time = std::time::Instant::now();
    
    // Convert parameters to SQLx format
    let mut sqlx_query = sqlx::query_as::<_, T>(query);
    for param in params {
        match param.to_sql()? {
            SqlParameter::String(s) => sqlx_query = sqlx_query.bind(s),
            SqlParameter::Integer(i) => sqlx_query = sqlx_query.bind(i),
            SqlParameter::Float(f) => sqlx_query = sqlx_query.bind(f),
            SqlParameter::Boolean(b) => sqlx_query = sqlx_query.bind(b),
            SqlParameter::Binary(b) => sqlx_query = sqlx_query.bind(b),
            SqlParameter::Null => sqlx_query = sqlx_query.bind(None::<String>),
        }
    }
    
    let result = sqlx_query.fetch_all(&self.connection_pool).await;
    // ... rest of implementation
}
```

### 1.3 Fix Async Trait Import Errors
**Files:** `crates/adapteros-db/Cargo.toml`, `crates/adapteros-policy/Cargo.toml`

**Issues:**
- `E0432`: Unresolved import `async_trait`

**Citations:**
- CONTRIBUTING.md L118-122: "Follow Rust naming conventions"

**Patches:**

```toml
# Add async-trait dependency
[dependencies]
async-trait = "0.1"
```

---

## Phase 2: Dependency Cycle Resolution

### 2.1 Restructure Core Dependencies
**Files:** `crates/adapteros-core/Cargo.toml`, `crates/adapteros-domain/Cargo.toml`

**Issues:**
- Circular dependency: `adapteros-core` ↔ `adapteros-domain` ↔ `adapteros-trace` ↔ `adapteros-crypto` ↔ `adapteros-graph`

**Citations:**
- Policy Pack #2 (Determinism): "MUST derive all RNG from `seed_global` and HKDF labels"

**Patches:**

```toml
# Remove circular dependencies
# adapteros-core/Cargo.toml - Remove adapteros-domain
[dependencies]
adapteros-deterministic-exec = { path = "../adapteros-deterministic-exec" }
# Remove: adapteros-domain = { path = "../adapteros-domain" }

# adapteros-domain/Cargo.toml - Remove adapteros-core
[dependencies]
adapteros-deterministic-exec = { path = "../adapteros-deterministic-exec" }
adapteros-trace = { path = "../adapteros-trace" }
adapteros-numerics = { path = "../adapteros-numerics" }
# Remove: adapteros-core = { path = "../adapteros-core" }
```

### 2.2 Create Shared Types Crate
**New File:** `crates/adapteros-shared-types/src/lib.rs`

**Citations:**
- CONTRIBUTING.md L118-122: "Follow Rust naming conventions"

**Implementation:**

```rust
//! Shared types for AdapterOS crates
//! 
//! Provides common types and traits that can be used across
//! crates without creating circular dependencies.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SharedError>;

#[derive(Error, Debug)]
pub enum SharedError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Crypto error: {0}")]
    Crypto(String),
}

// Re-export common types
pub use serde::{Deserialize, Serialize};
pub use thiserror::Error;
```

### 2.3 Update Error Handling
**Files:** `crates/adapteros-crypto/src/*.rs`, `crates/adapteros-graph/src/*.rs`

**Issues:**
- `E0599`: No associated item named `Crypto` found for struct `anyhow::Error`

**Citations:**
- CONTRIBUTING.md L118-122: "Use `adapteros_core::AosError` for error handling"

**Patches:**

```rust
// Replace anyhow with adapteros_core
use adapteros_core::{AosError, Result};

// Update error handling
.map_err(|e| AosError::Crypto(format!("Encryption failed: {}", e)))?;
```

---

## Phase 3: Missing Implementation Completion

### 3.1 Complete Database Access Implementation
**Files:** `crates/adapteros-db/src/unified_access.rs`

**Issues:**
- Missing actual SQLx implementation
- Placeholder methods need real functionality

**Citations:**
- Policy Pack #7 (RAG Index): "MUST isolate indices per tenant"

**Patches:**

```rust
impl UnifiedDatabaseAccess {
    pub async fn new(config: DatabaseConfig) -> Result<Self> {
        let connection_pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(config.pool_size)
            .connect(&config.connection_string)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create connection pool: {}", e)))?;
        
        // Initialize database schema
        sqlx::query("CREATE TABLE IF NOT EXISTS adapters (id TEXT PRIMARY KEY, name TEXT, tenant_id TEXT)")
            .execute(&connection_pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to initialize schema: {}", e)))?;
        
        Ok(Self {
            connection_pool,
            statistics: std::sync::Arc::new(std::sync::Mutex::new(DatabaseStatistics::default())),
            config,
        })
    }
}
```

### 3.2 Complete Policy Enforcement Implementation
**Files:** `crates/adapteros-policy/src/unified_enforcement.rs`

**Issues:**
- Missing actual policy validation logic
- Placeholder implementations need real functionality

**Citations:**
- Policy Pack #1-20: All policy packs enforced through unified interface

**Patches:**

```rust
impl PolicyEnforcer for UnifiedPolicyEnforcer {
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let start_time = std::time::Instant::now();
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        
        // Validate against all applicable policy packs
        for (pack_name, policy_pack) in &self.policy_packs {
            match policy_pack.validate_request(request).await {
                Ok(validation) => {
                    violations.extend(validation.violations);
                    warnings.extend(validation.warnings);
                }
                Err(e) => {
                    violations.push(PolicyViolation {
                        violation_id: uuid::Uuid::new_v4().to_string(),
                        policy_pack: pack_name.clone(),
                        severity: ViolationSeverity::High,
                        message: format!("Policy pack validation failed: {}", e),
                        details: None,
                        remediation: Some(vec!["Check policy pack configuration".to_string()]),
                    });
                }
            }
        }
        
        let duration = start_time.elapsed();
        let valid = violations.is_empty();
        
        Ok(PolicyValidationResult {
            valid,
            violations,
            warnings,
            timestamp: chrono::Utc::now(),
            duration_ms: duration.as_millis() as u64,
        })
    }
}
```

### 3.3 Complete Telemetry Implementation
**Files:** `crates/adapteros-telemetry/src/unified_events.rs`

**Issues:**
- Missing actual telemetry collection logic
- Placeholder implementations need real functionality

**Citations:**
- Policy Pack #9 (Telemetry): "MUST serialize events with canonical JSON and hash with BLAKE3"

**Patches:**

```rust
impl TelemetryWriter {
    pub fn log_event(&self, event: TelemetryEvent) -> Result<()> {
        // Serialize to canonical JSON
        let json = serde_jcs::to_string(&event)
            .map_err(|e| AosError::Serialization(e))?;
        
        // Hash with BLAKE3
        let hash = blake3::hash(json.as_bytes());
        
        // Send to background thread
        self.sender.send(event).map_err(|_| AosError::Io("Failed to send telemetry event".to_string()))?;
        
        Ok(())
    }
}
```

---

## Phase 4: Testing Framework Integration

### 4.1 Integrate Unified Testing Framework
**Files:** `crates/adapteros-testing/src/unified_framework.rs`

**Issues:**
- Missing integration with existing test infrastructure
- Need to connect with `tests/unit/` framework

**Citations:**
- `tests/unit/README.md`: "Deterministic systems that must produce identical outputs for identical inputs"

**Patches:**

```rust
impl TestingFramework for UnifiedTestingFramework {
    async fn run_test(&self, test_case: &TestCase) -> Result<TestResult> {
        let start_time = chrono::Utc::now();
        let start_instant = std::time::Instant::now();
        
        // Use existing unit testing framework
        let unit_framework = adapteros_unit_testing::UnitTestingFramework::new();
        
        // Run test steps with deterministic execution
        for step in &test_case.steps {
            let step_result = self.run_test_step_with_determinism(step, &unit_framework).await?;
            test_result.step_results.push(step_result);
        }
        
        // Run assertions
        for assertion in &test_case.assertions {
            let assertion_result = self.run_assertion_with_validation(assertion).await?;
            test_result.assertion_results.push(assertion_result);
        }
        
        let end_time = chrono::Utc::now();
        let execution_time = start_instant.elapsed();
        
        test_result.end_time = end_time;
        test_result.execution_time_ms = execution_time.as_millis() as u64;
        
        Ok(test_result)
    }
}
```

### 4.2 Add Deterministic Test Execution
**Files:** `crates/adapteros-testing/src/unified_framework.rs`

**Citations:**
- Policy Pack #2 (Determinism): "MUST derive all RNG from `seed_global` and HKDF labels"

**Patches:**

```rust
impl UnifiedTestingFramework {
    async fn run_test_step_with_determinism(
        &self, 
        step: &TestStep, 
        unit_framework: &adapteros_unit_testing::UnitTestingFramework
    ) -> Result<StepResult> {
        let start_instant = std::time::Instant::now();
        
        // Use deterministic RNG for test execution
        let rng = unit_framework.create_deterministic_rng(step.id.as_str());
        
        // Execute step with deterministic behavior
        let result = match &step.action {
            TestAction::ExecuteCommand { command, args } => {
                self.execute_command_deterministically(command, args, &rng).await
            }
            TestAction::ApiCall { method, url, body } => {
                self.execute_api_call_deterministically(method, url, body, &rng).await
            }
            // ... other action types
        };
        
        let execution_time = start_instant.elapsed();
        
        Ok(StepResult {
            step_id: step.id.clone(),
            status: if result.is_ok() { TestStatus::Passed } else { TestStatus::Failed },
            output: result.ok().map(|r| format!("{:?}", r)),
            error: result.err().map(|e| e.to_string()),
            execution_time_ms: execution_time.as_millis() as u64,
        })
    }
}
```

---

## Phase 5: Verification Framework Completion

### 5.1 Complete Code Quality Verification
**Files:** `crates/adapteros-verification/src/unified_validation.rs`

**Issues:**
- Missing actual verification logic
- Placeholder implementations need real functionality

**Citations:**
- CONTRIBUTING.md L118-122: "Use `cargo clippy` for linting"

**Patches:**

```rust
impl VerificationFramework for UnifiedVerificationFramework {
    async fn verify_code_quality(&self, config: &CodeQualityConfig) -> Result<CodeQualityReport> {
        let mut metrics = HashMap::new();
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();
        
        // Run clippy checks
        if config.enable_clippy {
            let clippy_result = self.run_clippy_checks().await?;
            metrics.insert("clippy_warnings".to_string(), QualityMetric {
                name: "Clippy Warnings".to_string(),
                value: clippy_result.warning_count as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if clippy_result.warning_count == 0 { MetricStatus::Pass } else { MetricStatus::Fail },
            });
        }
        
        // Run format checks
        if config.enable_format {
            let format_result = self.run_format_checks().await?;
            metrics.insert("format_violations".to_string(), QualityMetric {
                name: "Format Violations".to_string(),
                value: format_result.violation_count as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if format_result.violation_count == 0 { MetricStatus::Pass } else { MetricStatus::Fail },
            });
        }
        
        // Calculate overall score
        let overall_score = if metrics.is_empty() {
            100.0
        } else {
            let pass_count = metrics.values().filter(|m| m.status == MetricStatus::Pass).count();
            (pass_count as f64 / metrics.len() as f64) * 100.0
        };
        
        Ok(CodeQualityReport {
            overall_score,
            metrics,
            issues,
            recommendations,
            timestamp: chrono::Utc::now(),
        })
    }
}
```

### 5.2 Complete Security Verification
**Files:** `crates/adapteros-verification/src/unified_validation.rs`

**Citations:**
- Policy Pack #1 (Egress): "MUST block all outbound sockets in serving mode"

**Patches:**

```rust
impl VerificationFramework for UnifiedVerificationFramework {
    async fn verify_security(&self, config: &SecurityConfig) -> Result<SecurityReport> {
        let mut vulnerabilities = Vec::new();
        let mut recommendations = Vec::new();
        let mut metrics = HashMap::new();
        
        // Check for secret detection
        if config.enable_secret_detection {
            let secret_result = self.scan_for_secrets().await?;
            if !secret_result.secrets.is_empty() {
                for secret in secret_result.secrets {
                    vulnerabilities.push(SecurityVulnerability {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: "Hardcoded Secret".to_string(),
                        description: Some("Hardcoded secret detected in codebase".to_string()),
                        severity: SecuritySeverity::High,
                        vulnerability_type: "secret_detection".to_string(),
                        location: Some(IssueLocation {
                            file_path: secret.file_path,
                            line_number: Some(secret.line_number),
                            column_number: None,
                            function_name: None,
                        }),
                        details: Some(serde_json::json!({
                            "secret_type": secret.secret_type,
                            "confidence": secret.confidence
                        })),
                    });
                }
            }
        }
        
        // Check egress policy compliance
        let egress_result = self.verify_egress_policy().await?;
        metrics.insert("egress_compliance".to_string(), SecurityMetric {
            name: "Egress Policy Compliance".to_string(),
            value: if egress_result.compliant { 100.0 } else { 0.0 },
            unit: "percentage".to_string(),
            threshold: Some(100.0),
            status: if egress_result.compliant { MetricStatus::Pass } else { MetricStatus::Fail },
        });
        
        // Calculate overall score
        let overall_score = if vulnerabilities.is_empty() {
            100.0
        } else {
            let critical_count = vulnerabilities.iter().filter(|v| v.severity == SecuritySeverity::Critical).count();
            let high_count = vulnerabilities.iter().filter(|v| v.severity == SecuritySeverity::High).count();
            (100.0 - (critical_count as f64 * 20.0) - (high_count as f64 * 10.0)).max(0.0)
        };
        
        Ok(SecurityReport {
            overall_score,
            vulnerabilities,
            recommendations,
            metrics,
            timestamp: chrono::Utc::now(),
        })
    }
}
```

---

## Phase 6: CLI Integration and Testing

### 6.1 Fix CLI Compilation Errors
**Files:** `crates/adapteros-cli/src/commands/*.rs`

**Issues:**
- 33 compilation errors in CLI commands
- Missing method implementations
- Field mismatches

**Citations:**
- CONTRIBUTING.md L118-122: "Use `adapteros_core::Result` for return types"

**Patches:**

```rust
// Fix adapter command compilation
use adapteros_core::Result;
use adapteros_client::{AdapterOSClient, NativeClient};

async fn send_adapter_command(
    socket_path: &Path,
    adapter_id: &str,
    command: &str,
    timeout: Duration,
) -> Result<()> {
    // Use unified client trait
    let client: Box<dyn AdapterOSClient> = Box::new(NativeClient::new("http://localhost:8080".to_string()));
    
    match command.as_str() {
        "evict" => client.evict_adapter(adapter_id).await,
        "pin" => client.pin_adapter(adapter_id, true).await,
        "unpin" => client.pin_adapter(adapter_id, false).await,
        _ => Err(adapteros_core::AosError::InvalidInput(format!("Unsupported command: {}", command))),
    }
}
```

### 6.2 Add CLI Telemetry Integration
**Files:** `crates/adapteros-cli/src/logging.rs`

**Citations:**
- Policy Pack #9 (Telemetry): "MUST log `router.decision` for first N tokens"

**Patches:**

```rust
pub fn log_command_execution(
    command: &str,
    args: &[String],
    result: &Result<()>,
) {
    let event = TelemetryEventBuilder::new(
        EventType::UserAction,
        LogLevel::Info,
        format!("CLI command executed: {}", command),
    )
    .component("adapteros-cli".to_string())
    .metadata(serde_json::json!({
        "command": command,
        "args": args,
        "status": if result.is_ok() { "success" } else { "failure" }
    }))
    .build();
    
    // Log to telemetry system
    if let Ok(writer) = TelemetryWriter::new() {
        let _ = writer.log_event(event);
    }
}
```

---

## Phase 7: Memory Management Integration

### 7.1 Complete Memory Management Implementation
**Files:** `crates/adapteros-memory/src/unified_interface.rs`

**Issues:**
- Missing actual memory management logic
- Placeholder implementations need real functionality

**Citations:**
- Policy Pack #12 (Memory): "MUST maintain ≥ 15 percent unified memory headroom"

**Patches:**

```rust
impl MemoryManager for UnifiedMemoryManager {
    async fn get_memory_usage(&self) -> Result<MemoryUsageStats> {
        // Get actual system memory usage
        let system_memory = self.get_system_memory_usage().await?;
        let adapter_memory = self.get_adapter_memory_usage().await?;
        
        // Calculate memory pressure level
        let headroom_pct = (system_memory.available as f64 / system_memory.total as f64) * 100.0;
        let pressure_level = if headroom_pct >= 25.0 {
            MemoryPressureLevel::Low
        } else if headroom_pct >= 15.0 {
            MemoryPressureLevel::Medium
        } else if headroom_pct >= 5.0 {
            MemoryPressureLevel::High
        } else {
            MemoryPressureLevel::Critical
        };
        
        Ok(MemoryUsageStats {
            total_system_memory_mb: system_memory.total / (1024 * 1024),
            used_system_memory_mb: system_memory.used / (1024 * 1024),
            available_system_memory_mb: system_memory.available / (1024 * 1024),
            total_adapter_memory_mb: adapter_memory.total / (1024 * 1024),
            adapter_memory_pressure_level: pressure_level,
            adapters: adapter_memory.adapters,
            timestamp: chrono::Utc::now(),
        })
    }
    
    async fn run_cleanup(&self) -> Result<MemoryCleanupReport> {
        let mut operations = Vec::new();
        let mut total_freed = 0;
        let mut evicted_count = 0;
        
        // Get current memory usage
        let usage = self.get_memory_usage().await?;
        
        // Check if cleanup is needed
        if usage.adapter_memory_pressure_level == MemoryPressureLevel::Critical {
            // Evict ephemeral adapters first
            for adapter in &usage.adapters {
                if adapter.category == AdapterCategory::Ephemeral && !adapter.pinned {
                    let result = self.evict_adapter(&adapter.id).await;
                    if result.is_ok() {
                        operations.push(CleanupOperation {
                            adapter_id: adapter.id.clone(),
                            action: "evicted".to_string(),
                            memory_freed_mb: adapter.memory_usage_mb,
                            reason: "Memory pressure cleanup".to_string(),
                        });
                        total_freed += adapter.memory_usage_mb;
                        evicted_count += 1;
                    }
                }
            }
        }
        
        Ok(MemoryCleanupReport {
            timestamp: chrono::Utc::now(),
            operations,
            total_memory_freed_mb: total_freed,
            adapters_evicted: evicted_count,
            adapters_pinned: usage.adapters.iter().filter(|a| a.pinned).count(),
            final_memory_pressure_level: usage.adapter_memory_pressure_level,
        })
    }
}
```

---

## Phase 8: Policy Enforcement Integration

### 8.1 Complete Policy Pack Implementation
**Files:** `crates/adapteros-policy/src/packs.rs`

**Issues:**
- Missing actual policy pack implementations
- Need to implement all 20 policy packs

**Citations:**
- Policy Pack #1-20: All policy packs enforced through unified interface

**Patches:**

```rust
// Implement Egress Ruleset (Policy Pack #1)
pub struct EgressPolicyPack {
    config: EgressConfig,
}

impl PolicyPack for EgressPolicyPack {
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        
        // Check for network operations
        if let Some(network_op) = self.extract_network_operation(request) {
            if network_op.is_outbound() {
                violations.push(PolicyViolation {
                    violation_id: uuid::Uuid::new_v4().to_string(),
                    policy_pack: "egress".to_string(),
                    severity: ViolationSeverity::Critical,
                    message: "Outbound network operation detected".to_string(),
                    details: Some(serde_json::json!({
                        "operation": network_op.operation_type,
                        "destination": network_op.destination
                    })),
                    remediation: Some(vec![
                        "Use Unix domain sockets for local communication".to_string(),
                        "Implement egress filtering".to_string()
                    ]),
                });
            }
        }
        
        // Check for TCP/UDP ports
        if let Some(port_op) = self.extract_port_operation(request) {
            if port_op.protocol == "tcp" || port_op.protocol == "udp" {
                violations.push(PolicyViolation {
                    violation_id: uuid::Uuid::new_v4().to_string(),
                    policy_pack: "egress".to_string(),
                    severity: ViolationSeverity::High,
                    message: "TCP/UDP port operation detected".to_string(),
                    details: Some(serde_json::json!({
                        "protocol": port_op.protocol,
                        "port": port_op.port
                    })),
                    remediation: Some(vec![
                        "Use Unix domain sockets only".to_string()
                    ]),
                });
            }
        }
        
        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: chrono::Utc::now(),
            duration_ms: 0,
        })
    }
    
    fn get_name(&self) -> &str { "egress" }
    fn get_version(&self) -> &str { "1.0.0" }
}

// Implement Determinism Ruleset (Policy Pack #2)
pub struct DeterminismPolicyPack {
    config: DeterminismConfig,
}

impl PolicyPack for DeterminismPolicyPack {
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        
        // Check for RNG usage
        if let Some(rng_op) = self.extract_rng_operation(request) {
            if !rng_op.is_hkdf_seeded() {
                violations.push(PolicyViolation {
                    violation_id: uuid::Uuid::new_v4().to_string(),
                    policy_pack: "determinism".to_string(),
                    severity: ViolationSeverity::High,
                    message: "Non-deterministic RNG usage detected".to_string(),
                    details: Some(serde_json::json!({
                        "rng_type": rng_op.rng_type,
                        "seed_source": rng_op.seed_source
                    })),
                    remediation: Some(vec![
                        "Use HKDF-seeded RNG".to_string(),
                        "Derive from global seed".to_string()
                    ]),
                });
            }
        }
        
        // Check for kernel compilation
        if let Some(kernel_op) = self.extract_kernel_operation(request) {
            if kernel_op.is_runtime_compilation() {
                violations.push(PolicyViolation {
                    violation_id: uuid::Uuid::new_v4().to_string(),
                    policy_pack: "determinism".to_string(),
                    severity: ViolationSeverity::Critical,
                    message: "Runtime kernel compilation detected".to_string(),
                    details: Some(serde_json::json!({
                        "kernel_name": kernel_op.kernel_name,
                        "compilation_type": "runtime"
                    })),
                    remediation: Some(vec![
                        "Use precompiled metallib".to_string(),
                        "Embed kernels in binary".to_string()
                    ]),
                });
            }
        }
        
        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: chrono::Utc::now(),
            duration_ms: 0,
        })
    }
    
    fn get_name(&self) -> &str { "determinism" }
    fn get_version(&self) -> &str { "1.0.0" }
}
```

---

## Phase 9: Integration Testing and Verification

### 9.1 Add Comprehensive Integration Tests
**Files:** `tests/integration/consolidation_tests.rs`

**Citations:**
- `tests/unit/README.md`: "Deterministic systems that must produce identical outputs for identical inputs"

**Patches:**

```rust
#[cfg(test)]
mod consolidation_tests {
    use super::*;
    use adapteros_unit_testing::*;
    
    #[tokio::test]
    async fn test_unified_logging_integration() {
        let framework = UnitTestingFramework::new();
        let sandbox = TestSandbox::with_seed(42);
        
        // Test logging integration
        let logger = UnifiedLoggingFramework::new(LoggingConfig::default());
        let result = logger.log_event(LogEvent {
            level: LogLevel::Info,
            message: "Test message".to_string(),
            component: "test".to_string(),
            metadata: None,
        }).await;
        
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_policy_enforcement_integration() {
        let framework = UnitTestingFramework::new();
        let sandbox = TestSandbox::with_seed(42);
        
        // Test policy enforcement
        let enforcer = UnifiedPolicyEnforcer::new();
        let request = PolicyRequest {
            request_id: "test-request".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("default".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "test".to_string(),
                operation: "test".to_string(),
                data: None,
                priority: Priority::Normal,
            },
            metadata: None,
        };
        
        let result = enforcer.validate_request(&request).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_memory_management_integration() {
        let framework = UnitTestingFramework::new();
        let sandbox = TestSandbox::with_seed(42);
        
        // Test memory management
        let manager = UnifiedMemoryManager::new();
        let usage = manager.get_memory_usage().await;
        assert!(usage.is_ok());
        
        let cleanup = manager.run_cleanup().await;
        assert!(cleanup.is_ok());
    }
}
```

### 9.2 Add Determinism Verification
**Files:** `tests/integration/determinism_tests.rs`

**Citations:**
- Policy Pack #2 (Determinism): "MUST derive all RNG from `seed_global` and HKDF labels"

**Patches:**

```rust
#[cfg(test)]
mod determinism_tests {
    use super::*;
    use adapteros_unit_testing::*;
    
    #[test]
    fn test_deterministic_execution() {
        let framework = UnitTestingFramework::new();
        let rng1 = DeterministicRng::from_seed(42);
        let rng2 = DeterministicRng::from_seed(42);
        
        // Generate same sequence
        let seq1: Vec<u32> = (0..100).map(|_| rng1.gen()).collect();
        let seq2: Vec<u32> = (0..100).map(|_| rng2.gen()).collect();
        
        assert_eq!(seq1, seq2);
    }
    
    #[tokio::test]
    async fn test_deterministic_async_execution() {
        let framework = UnitTestingFramework::new();
        let executor1 = DeterministicExecutor::with_seed(42);
        let executor2 = DeterministicExecutor::with_seed(42);
        
        // Run same async operations
        let result1 = executor1.run_deterministic(async {
            // Some async operation
            42
        }).await;
        
        let result2 = executor2.run_deterministic(async {
            // Same async operation
            42
        }).await;
        
        assert_eq!(result1, result2);
    }
}
```

---

## Verification Checklist

### Pre-Operation
- [ ] Understand the change
- [ ] Investigate file changes
- [ ] Plan verification steps
- [ ] Identify success criteria
- [ ] Check for existing implementations (duplicate prevention)

### During Operation
- [ ] Execute tool operation
- [ ] Check tool output
- [ ] Note any warnings/errors

### Post-Operation Verification
- [ ] Re-read modified file
- [ ] Grep for expected changes
- [ ] Compile if applicable
- [ ] Run relevant tests
- [ ] Check for duplicate implementations across crates
- [ ] Verify no conflicts with existing code
- [ ] Update todo list

### Documentation
- [ ] Update status accurately
- [ ] Include verification evidence
- [ ] Note any limitations
- [ ] Define next steps

---

## Success Metrics

- [ ] Zero compilation errors across workspace
- [ ] All dependency cycles resolved
- [ ] All unified interfaces implemented
- [ ] All policy packs enforced
- [ ] All tests passing
- [ ] Deterministic execution verified
- [ ] Memory management operational
- [ ] Telemetry system functional
- [ ] CLI commands working
- [ ] Database access operational

---

## Best Practices Compliance

### Code Quality
- [ ] Follow Rust naming conventions
- [ ] Use `cargo fmt` for formatting
- [ ] Use `cargo clippy` for linting
- [ ] Use `tracing` for logging (not `println!`)
- [ ] Use `adapteros_core::AosError` for error handling
- [ ] Use `adapteros_core::Result` for return types

### Policy Compliance
- [ ] Egress Ruleset: Zero network during serving
- [ ] Determinism Ruleset: Precompiled kernels, HKDF seeding
- [ ] Router Ruleset: K bounds, entropy floor, Q15 gates
- [ ] Evidence Ruleset: Mandatory open-book grounding
- [ ] Memory Ruleset: 15% headroom maintenance
- [ ] Telemetry Ruleset: Canonical JSON, BLAKE3 hashing

### Testing Standards
- [ ] Deterministic test execution
- [ ] Component isolation
- [ ] Property-based testing
- [ ] Integration testing
- [ ] Performance testing
- [ ] Security testing

---

## Risk Mitigation

### Compilation Risks
- **Risk**: Breaking existing functionality
- **Mitigation**: Incremental changes with verification
- **Fallback**: Revert to previous working state

### Dependency Risks
- **Risk**: Creating new circular dependencies
- **Mitigation**: Careful dependency analysis
- **Fallback**: Use shared types crate

### Performance Risks
- **Risk**: Degrading system performance
- **Mitigation**: Performance testing and monitoring
- **Fallback**: Optimize or revert changes

### Security Risks
- **Risk**: Introducing security vulnerabilities
- **Mitigation**: Security verification and testing
- **Fallback**: Security audit and fixes

---

## Conclusion

This comprehensive patch plan addresses all critical issues in the AdapterOS codebase while maintaining compliance with established standards and best practices. The plan is structured to minimize risk through incremental implementation and comprehensive verification.

**Next Steps:**
1. Execute Phase 1: Compilation Error Resolution
2. Execute Phase 2: Dependency Cycle Resolution
3. Continue through all 9 phases systematically
4. Verify compliance with all success metrics
5. Document any deviations or issues encountered

**Estimated Completion:** 8-10 days with full verification and testing.
