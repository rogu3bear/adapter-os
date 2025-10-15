# AdapterOS Consolidation Patch Plan

**Date:** January 15, 2025  
**Version:** alpha-v0.01-1 → alpha-v0.02  
**Status:** Ready for Execution  
**Compliance:** Agent Hallucination Prevention Framework + 20 Policy Packs

---

## Executive Summary

This plan systematically addresses **7 major overlapping areas** identified across the AdapterOS codebase, consolidating duplicate implementations and standardizing patterns per codebase standards documented in `CLAUDE.md`, `CONTRIBUTING.md`, and `.cursor/rules/global.mdc`.

**Scope:** 9 phases covering logging, API clients, error handling, telemetry, memory management, policy enforcement, database access, testing frameworks, and verification.

**Estimated Effort:** ~40-48 hours (6-8 days)

---

## Codebase Standards Reference

### From CONTRIBUTING.md L116-136
```markdown
Code Standards:
- Follow Rust naming conventions
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Prefer `Result<T>` over `Option<T>` for error handling
- Use `tracing` for logging (not `println!`)
- Document all public APIs
- All changes must comply with 20 policy packs
- Security-sensitive code requires review
```

### From CLAUDE.md L118-133
```rust
// Code Style:
- Use `tracing` for logging (not `println!`)
- Errors via `adapteros_core::AosError` and `Result<T>`
- Telemetry via `TelemetryWriter::log(event_type, data)`
- No network I/O in worker (Unix domain sockets only)
```

### From .cursor/rules/global.mdc
```
Policy Pack #2 (Determinism): MUST derive all RNG from seed_global and HKDF labels
Policy Pack #9 (Telemetry): MUST log events with canonical JSON
Policy Pack #18 (LLM Output): MUST emit JSON-serializable response shapes
```

---

## Phase 1: Logging Consolidation (High Priority)

### Current State
- **Rust Backend**: `tracing` crate (canonical JSON, BLAKE3 hashing)
- **UI Frontend**: Custom `logger.ts` (structured logging, telemetry integration)
- **CLI**: Mix of `println!` and `tracing` (127+ violations)
- **Audit**: Separate `AuditLogger` in `adapteros-secd`

### Violations

#### V1.1: Multiple Logging Systems
**Gap:** Inconsistent logging patterns across layers  
**Target State:** Single `tracing`-based system with UI integration

**Citations:**
- CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
- CLAUDE.md L130: "Use `tracing` for logging (not `println!`)"
- Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"

#### Implementation

**Patch 1.1: CLI Logging Standardization**
```rust
// File: crates/adapteros-cli/src/lib.rs

use tracing::{info, warn, error, debug};
use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging() -> Result<()> {
    // Replace: println! with tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .with_thread_ids(true)
        .init();
    
    Ok(())
}

// Replace all println! calls with appropriate tracing levels
// Example: println!("Starting worker") → info!("Starting worker")
```

**Patch 1.2: UI Logger Integration**
```typescript
// File: ui/src/utils/logger.ts

// Enhance existing logger to match backend telemetry format
export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  context: LogContext;
  error?: {
    name: string;
    message: string;
    stack?: string;
  };
  // Add backend compatibility fields
  event_type: string;
  component: string;
  operation: string;
  tenant_id?: string;
  user_id?: string;
}

class Logger {
  private async sendToTelemetry(logEntry: LogEntry) {
    // Send to backend telemetry endpoint with canonical JSON
    await fetch('/api/v1/telemetry/events', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(logEntry), // Canonical JSON per Policy Pack #9
    });
  }
}
```

**Patch 1.3: Audit Logger Consolidation**
```rust
// File: crates/adapteros-secd/src/audit.rs

use adapteros_telemetry::TelemetryWriter;
use tracing::{info, error};

pub struct AuditLogger {
    telemetry: TelemetryWriter,
}

impl AuditLogger {
    pub fn new(telemetry: TelemetryWriter) -> Self {
        Self { telemetry }
    }

    pub async fn log_operation(
        &self,
        operation: &str,
        artifact_hash: Option<&str>,
        result: Result<(), String>,
    ) {
        // Use centralized telemetry instead of separate audit system
        let event = AuditEvent {
            operation: operation.to_string(),
            artifact_hash: artifact_hash.map(|s| s.to_string()),
            result: match result {
                Ok(_) => "success".to_string(),
                Err(e) => e,
            },
            timestamp: chrono::Utc::now(),
        };
        
        self.telemetry.log("audit.operation", &event).await;
    }
}
```

**Verification Checklist:**
- [ ] All `println!` calls replaced with `tracing`
- [ ] UI logger sends canonical JSON to backend
- [ ] Audit logging uses centralized telemetry
- [ ] Log levels consistent across all components
- [ ] Telemetry format matches Policy Pack #9

---

## Phase 2: API Client Consolidation (High Priority)

### Current State
- **UI**: `ui/src/api/client.ts` (TypeScript, fetch-based)
- **Rust**: `adapteros-client` crate with multiple implementations
- **CLI**: Direct UDS connections in commands

### Violations

#### V2.1: Duplicate Client Implementations
**Gap:** Code duplication, inconsistent error handling  
**Target State:** Single client trait with platform-specific implementations

**Citations:**
- CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
- Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"

#### Implementation

**Patch 2.1: Extend Client Trait**
```rust
// File: crates/adapteros-client/src/lib.rs

use adapteros_core::{AosError, Result};

/// Unified client trait for all AdapterOS API access
pub trait AdapterOSClient {
    // Health & Auth
    async fn health(&self) -> Result<HealthResponse>;
    async fn login(&self, req: LoginRequest) -> Result<LoginResponse>;
    async fn logout(&self) -> Result<()>;
    async fn me(&self) -> Result<UserInfoResponse>;

    // Tenants
    async fn list_tenants(&self) -> Result<Vec<TenantResponse>>;
    async fn create_tenant(&self, req: CreateTenantRequest) -> Result<TenantResponse>;

    // Adapters
    async fn list_adapters(&self) -> Result<Vec<AdapterResponse>>;
    async fn register_adapter(&self, req: RegisterAdapterRequest) -> Result<AdapterResponse>;
    async fn evict_adapter(&self, adapter_id: &str) -> Result<()>;
    async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()>;

    // Memory Management
    async fn get_memory_usage(&self) -> Result<MemoryUsageResponse>;
    
    // Training
    async fn start_adapter_training(&self, req: StartTrainingRequest) -> Result<TrainingSessionResponse>;
    async fn get_training_session(&self, session_id: &str) -> Result<TrainingSessionResponse>;
    async fn list_training_sessions(&self) -> Result<Vec<TrainingSessionResponse>>;

    // Telemetry
    async fn get_telemetry_events(&self, filters: TelemetryFilters) -> Result<Vec<TelemetryEvent>>;
}

// Implement for existing clients
impl AdapterOSClient for NativeClient { /* ... */ }
impl AdapterOSClient for WasmClient { /* ... */ }
impl AdapterOSClient for UdsClient { /* ... */ }
```

**Patch 2.2: UI Client Integration**
```typescript
// File: ui/src/api/client.ts

// Implement AdapterOSClient interface compatibility
class ApiClient implements AdapterOSClient {
  // ... existing implementation
  
  // Add missing methods to match Rust trait
  async getMemoryUsage(): Promise<MemoryUsageResponse> {
    return this.request('/v1/memory/usage');
  }

  async startAdapterTraining(data: StartTrainingRequest): Promise<TrainingSessionResponse> {
    return this.request('/v1/training/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getTelemetryEvents(filters: TelemetryFilters): Promise<TelemetryEvent[]> {
    const params = new URLSearchParams();
    if (filters.limit) params.append('limit', filters.limit.toString());
    if (filters.tenantId) params.append('tenant_id', filters.tenantId);
    // ... other filters
    
    const queryString = params.toString();
    return this.request(`/v1/telemetry/events${queryString ? `?${queryString}` : ''}`);
  }
}
```

**Patch 2.3: CLI Client Usage**
```rust
// File: crates/adapteros-cli/src/commands/adapter.rs

use adapteros_client::{AdapterOSClient, UdsClient};

pub async fn list_adapters() -> Result<()> {
    // Replace: Direct UDS connections with unified client
    let client = UdsClient::new("/var/run/aos/default/worker.sock")?;
    let adapters = client.list_adapters().await?;
    
    // Display results
    for adapter in adapters {
        println!("{} - {}", adapter.id, adapter.name);
    }
    
    Ok(())
}
```

**Verification Checklist:**
- [ ] All clients implement unified trait
- [ ] UI client matches Rust interface
- [ ] CLI uses unified client instead of direct UDS
- [ ] Error handling consistent across clients
- [ ] Request ID tracking implemented everywhere

---

## Phase 3: Error Handling Unification (High Priority)

### Current State
- **Core**: `adapteros_core::AosError` (192 variants)
- **Domain**: `adapteros_domain::DomainAdapterError` (65 variants)
- **CLI**: `anyhow::Result` instead of typed errors
- **UI**: Generic `Error` objects

### Violations

#### V3.1: Multiple Error Types
**Gap:** Inconsistent error propagation, harder debugging  
**Target State:** Single `AosError` with proper error chaining

**Citations:**
- CONTRIBUTING.md L122: "Prefer `Result<T>` over `Option<T>` for error handling"
- CLAUDE.md L131: "Errors via `adapteros_core::AosError` and `Result<T>`"

#### Implementation

**Patch 3.1: Extend AosError**
```rust
// File: crates/adapteros-core/src/error.rs

#[derive(Error, Debug)]
pub enum AosError {
    // ... existing variants ...
    
    // Add missing variants for consolidation
    #[error("UDS connection failed: {path}")]
    UdsConnectionFailed {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    
    #[error("Invalid response from worker: {reason}")]
    InvalidResponse {
        reason: String,
    },
    
    #[error("Feature disabled: {feature} - {reason}")]
    FeatureDisabled {
        feature: String,
        reason: String,
        alternative: Option<String>,
    },
    
    #[error("Worker not responding at {path}")]
    WorkerNotResponding {
        path: PathBuf,
    },
    
    #[error("Timeout waiting for response after {duration:?}")]
    Timeout {
        duration: Duration,
    },
    
    // Domain adapter errors
    #[error("Domain adapter error: {0}")]
    DomainAdapter(#[from] adapteros_domain::DomainAdapterError),
    
    // Client errors
    #[error("Client error: {0}")]
    Client(#[from] adapteros_client::ClientError),
    
    // Telemetry errors
    #[error("Telemetry error: {0}")]
    Telemetry(#[from] adapteros_telemetry::TelemetryError),
}
```

**Patch 3.2: CLI Error Migration**
```rust
// File: crates/adapteros-cli/src/lib.rs

use adapteros_core::{AosError, Result};

// Replace: anyhow::Result with typed Result
pub async fn execute_command(cmd: Command) -> Result<()> {
    match cmd {
        Command::Adapter { action } => {
            match action {
                AdapterAction::List => {
                    // Replace: anyhow with AosError
                    let client = UdsClient::new("/var/run/aos/default/worker.sock")
                        .map_err(|e| AosError::UdsConnectionFailed {
                            path: "/var/run/aos/default/worker.sock".into(),
                            source: e,
                        })?;
                    
                    let adapters = client.list_adapters().await?;
                    display_adapters(adapters);
                }
                // ... other actions
            }
        }
        // ... other commands
    }
    
    Ok(())
}
```

**Patch 3.3: Domain Error Integration**
```rust
// File: crates/adapteros-domain/src/error.rs

use adapteros_core::AosError;

// Convert domain errors to core errors
impl From<DomainAdapterError> for AosError {
    fn from(err: DomainAdapterError) -> Self {
        match err {
            DomainAdapterError::ManifestLoadError { path, source } => {
                AosError::Io(format!("Failed to load manifest from {}: {}", path, source))
            }
            DomainAdapterError::InvalidManifest { reason } => {
                AosError::InvalidManifest(reason)
            }
            DomainAdapterError::TensorShapeMismatch { expected, actual } => {
                AosError::KernelLayoutMismatch {
                    tensor: "unknown".to_string(),
                    expected: format!("{:?}", expected),
                    got: format!("{:?}", actual),
                }
            }
            // ... other conversions
            _ => AosError::DomainAdapter(err),
        }
    }
}
```

**Verification Checklist:**
- [ ] All error types convert to `AosError`
- [ ] CLI uses typed errors instead of `anyhow`
- [ ] Error chaining preserves context
- [ ] Error messages are descriptive
- [ ] Source errors preserved with `#[source]`

---

## Phase 4: Telemetry Centralization (Medium Priority)

### Current State
- **Core**: `adapteros-telemetry` (canonical JSON, Merkle trees)
- **UI**: Custom activity feed hook
- **CLI**: Separate telemetry in `cli_telemetry.rs`
- **Server**: Metrics collection in `adapteros-system-metrics`

### Violations

#### V4.1: Duplicate Telemetry Systems
**Gap:** Duplicate metrics collection, inconsistent data formats  
**Target State:** Single telemetry collector with unified schema

**Citations:**
- Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
- CLAUDE.md L132: "Telemetry via `TelemetryWriter::log(event_type, data)`"

#### Implementation

**Patch 4.1: Unified Telemetry Schema**
```rust
// File: crates/adapteros-telemetry/src/events.rs

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub level: LogLevel,
    pub message: String,
    pub component: Option<String>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

// Unified event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    // System events
    SystemStart,
    SystemStop,
    SystemError,
    
    // Adapter events
    AdapterLoaded,
    AdapterUnloaded,
    AdapterEvicted,
    AdapterPinned,
    
    // Inference events
    InferenceStart,
    InferenceComplete,
    InferenceError,
    
    // Policy events
    PolicyViolation,
    PolicyEnforcement,
    
    // Memory events
    MemoryPressure,
    MemoryEviction,
    
    // Training events
    TrainingStart,
    TrainingComplete,
    TrainingError,
    
    // User events
    UserLogin,
    UserLogout,
    UserAction,
}
```

**Patch 4.2: Centralized Telemetry Collector**
```rust
// File: crates/adapteros-telemetry/src/collector.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use crossbeam::channel::{unbounded, Receiver, Sender};

pub struct TelemetryCollector {
    sender: Sender<TelemetryEvent>,
    receiver: Receiver<TelemetryEvent>,
    events: Arc<RwLock<Vec<TelemetryEvent>>>,
}

impl TelemetryCollector {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            sender,
            receiver,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn log(&self, event_type: EventType, data: &impl Serialize) -> Result<()> {
        let event = TelemetryEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: format!("{:?}", event_type),
            level: LogLevel::Info,
            message: serde_json::to_string(data)?,
            component: None,
            tenant_id: None,
            user_id: None,
            metadata: Some(serde_json::to_value(data)?),
            trace_id: None,
            span_id: None,
        };

        self.sender.send(event)?;
        Ok(())
    }

    pub async fn get_events(&self, filters: TelemetryFilters) -> Result<Vec<TelemetryEvent>> {
        let events = self.events.read().await;
        let filtered: Vec<TelemetryEvent> = events
            .iter()
            .filter(|event| {
                if let Some(tenant_id) = &filters.tenant_id {
                    if event.tenant_id.as_ref() != Some(tenant_id) {
                        return false;
                    }
                }
                if let Some(event_type) = &filters.event_type {
                    if event.event_type != *event_type {
                        return false;
                    }
                }
                if let Some(level) = &filters.level {
                    if event.level != *level {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        Ok(filtered)
    }
}
```

**Patch 4.3: UI Telemetry Integration**
```typescript
// File: ui/src/hooks/useActivityFeed.ts

// Replace custom implementation with backend integration
export function useActivityFeed(options: UseActivityFeedOptions = {}): UseActivityFeedReturn {
  const { enabled = true, maxEvents = 50, tenantId, userId } = options;
  
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchEvents = async () => {
    if (!enabled) return;
    
    setLoading(true);
    setError(null);
    
    try {
      // Use unified telemetry API
      const telemetryEvents = await apiClient.getTelemetryEvents({
        limit: maxEvents,
        tenantId,
        userId,
        startTime: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
      });

      setEvents(telemetryEvents);
      
      logger.info('Activity feed updated', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        eventCount: telemetryEvents.length,
        tenantId,
        userId
      });
      
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch activity events';
      setError(errorMessage);
      
      logger.error('Failed to fetch activity events', {
        component: 'useActivityFeed',
        operation: 'fetchEvents',
        tenantId,
        userId
      }, err instanceof Error ? err : new Error(String(err)));
      
    } finally {
      setLoading(false);
    }
  };

  // ... rest of implementation
}
```

**Verification Checklist:**
- [ ] Single telemetry schema used everywhere
- [ ] Canonical JSON format per Policy Pack #9
- [ ] UI uses backend telemetry API
- [ ] CLI telemetry integrated with core system
- [ ] Event filtering and querying unified

---

## Phase 5: Memory Management Consolidation (Medium Priority)

### Current State
- **Worker**: `adapteros-lora-worker/src/memory.rs`
- **Core**: Memory pressure handling in worker
- **UI**: `AdapterMemoryMonitor.tsx` with separate API calls
- **CLI**: Memory commands with mock implementations

### Violations

#### V5.1: Scattered Memory Management
**Gap:** Inconsistent memory policies, duplicate eviction logic  
**Target State:** Centralized memory management service

**Citations:**
- Policy Pack #12 (Memory): "MUST maintain ≥ 15 percent unified memory headroom"
- CLAUDE.md L245-249: Memory management guidelines

#### Implementation

**Patch 5.1: Centralized Memory Manager**
```rust
// File: crates/adapteros-memory/src/lib.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use adapteros_core::{AosError, Result};
use adapteros_telemetry::TelemetryWriter;

pub struct MemoryManager {
    adapters: Arc<RwLock<Vec<Adapter>>>,
    total_memory: u64,
    min_headroom_percent: f64,
    telemetry: TelemetryWriter,
}

impl MemoryManager {
    pub fn new(total_memory: u64, telemetry: TelemetryWriter) -> Self {
        Self {
            adapters: Arc::new(RwLock::new(Vec::new())),
            total_memory,
            min_headroom_percent: 15.0, // Policy Pack #12
            telemetry,
        }
    }

    pub async fn get_memory_usage(&self) -> Result<MemoryUsageResponse> {
        let adapters = self.adapters.read().await;
        let used_memory: u64 = adapters.iter().map(|a| a.memory_usage_mb).sum();
        let available_memory = self.total_memory - used_memory;
        let headroom_percent = (available_memory as f64 / self.total_memory as f64) * 100.0;

        Ok(MemoryUsageResponse {
            adapters: adapters.clone(),
            total_memory_mb: self.total_memory,
            available_memory_mb: available_memory,
            memory_pressure_level: self.get_pressure_level(headroom_percent),
        })
    }

    pub async fn evict_adapter(&self, adapter_id: &str) -> Result<()> {
        let mut adapters = self.adapters.write().await;
        
        if let Some(pos) = adapters.iter().position(|a| a.id == adapter_id) {
            let adapter = adapters.remove(pos);
            
            // Log eviction event
            self.telemetry.log("memory.eviction", &EvictionEvent {
                adapter_id: adapter.id.clone(),
                reason: "manual".to_string(),
                memory_freed_mb: adapter.memory_usage_mb,
            }).await?;
            
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Adapter {} not found", adapter_id)))
        }
    }

    pub async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()> {
        let mut adapters = self.adapters.write().await;
        
        if let Some(adapter) = adapters.iter_mut().find(|a| a.id == adapter_id) {
            adapter.pinned = pinned;
            
            // Log pin event
            self.telemetry.log("memory.pin", &PinEvent {
                adapter_id: adapter.id.clone(),
                pinned,
            }).await?;
            
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Adapter {} not found", adapter_id)))
        }
    }

    fn get_pressure_level(&self, headroom_percent: f64) -> MemoryPressureLevel {
        if headroom_percent < 5.0 {
            MemoryPressureLevel::Critical
        } else if headroom_percent < 10.0 {
            MemoryPressureLevel::High
        } else if headroom_percent < 15.0 {
            MemoryPressureLevel::Medium
        } else {
            MemoryPressureLevel::Low
        }
    }
}
```

**Patch 5.2: UI Memory Monitor Integration**
```typescript
// File: ui/src/components/AdapterMemoryMonitor.tsx

// Replace custom implementation with backend integration
const handleRefresh = async () => {
  setIsRefreshing(true);
  try {
    // Use centralized memory API
    const memoryData = await apiClient.getMemoryUsage();
    onMemoryDataUpdate?.(memoryData);
    
    logger.info('Memory data refreshed successfully', {
      component: 'AdapterMemoryMonitor',
      operation: 'refreshMemoryData',
      adapterCount: memoryData.adapters.length
    });
    
  } catch (error) {
    logger.error('Failed to refresh memory data', {
      component: 'AdapterMemoryMonitor',
      operation: 'refreshMemoryData'
    }, error instanceof Error ? error : new Error(String(error)));
  } finally {
    setIsRefreshing(false);
  }
};

const handleEvictAdapter = async (adapterId: string) => {
  try {
    // Use centralized memory API
    await apiClient.evictAdapter(adapterId);
    onEvictAdapter(adapterId);
    
    logger.info('Adapter evicted successfully', {
      component: 'AdapterMemoryMonitor',
      operation: 'evictAdapter',
      adapterId
    });
  } catch (error) {
    logger.error('Failed to evict adapter', {
      component: 'AdapterMemoryMonitor',
      operation: 'evictAdapter',
      adapterId
    }, error instanceof Error ? error : new Error(String(error)));
  }
};
```

**Verification Checklist:**
- [ ] Single memory management service
- [ ] 15% headroom policy enforced
- [ ] UI uses centralized memory API
- [ ] CLI memory commands implemented
- [ ] Eviction logic centralized

---

## Phase 6: Policy Enforcement Centralization (Medium Priority)

### Current State
- **Core**: `adapteros-policy` crate (20 policy packs)
- **Worker**: Policy checks in inference pipeline
- **UI**: Policy validation in components
- **CLI**: Policy commands with TODOs

### Violations

#### V6.1: Scattered Policy Enforcement
**Gap:** Inconsistent policy application, security gaps  
**Target State:** Centralized policy engine with consistent enforcement

**Citations:**
- Policy Pack #1-20: All policy packs must be enforced consistently
- CLAUDE.md L207-212: Policy enforcement guidelines

#### Implementation

**Patch 6.1: Centralized Policy Engine**
```rust
// File: crates/adapteros-policy/src/engine.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use adapteros_core::{AosError, Result};
use adapteros_telemetry::TelemetryWriter;

pub struct PolicyEngine {
    policies: Arc<RwLock<Vec<PolicyPack>>>,
    telemetry: TelemetryWriter,
}

impl PolicyEngine {
    pub fn new(telemetry: TelemetryWriter) -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
            telemetry,
        }
    }

    pub async fn enforce_policy(
        &self,
        policy_name: &str,
        context: &PolicyContext,
    ) -> Result<PolicyResult> {
        let policies = self.policies.read().await;
        
        if let Some(policy) = policies.iter().find(|p| p.name == policy_name) {
            let result = policy.enforce(context).await;
            
            // Log policy enforcement
            self.telemetry.log("policy.enforcement", &PolicyEnforcementEvent {
                policy_name: policy_name.to_string(),
                result: result.clone(),
                context: context.clone(),
            }).await?;
            
            Ok(result)
        } else {
            Err(AosError::Policy(format!("Policy {} not found", policy_name)))
        }
    }

    pub async fn validate_operation(
        &self,
        operation: &str,
        context: &PolicyContext,
    ) -> Result<()> {
        // Enforce all applicable policies
        let applicable_policies = self.get_applicable_policies(operation).await;
        
        for policy_name in applicable_policies {
            let result = self.enforce_policy(&policy_name, context).await?;
            if !result.allowed {
                return Err(AosError::PolicyViolation(format!(
                    "Policy {} violation: {}",
                    policy_name, result.reason
                )));
            }
        }
        
        Ok(())
    }

    async fn get_applicable_policies(&self, operation: &str) -> Vec<String> {
        // Return list of policies applicable to the operation
        match operation {
            "inference" => vec![
                "egress".to_string(),
                "determinism".to_string(),
                "router".to_string(),
                "evidence".to_string(),
                "refusal".to_string(),
            ],
            "adapter_load" => vec![
                "artifacts".to_string(),
                "isolation".to_string(),
                "memory".to_string(),
            ],
            "telemetry" => vec![
                "telemetry".to_string(),
                "retention".to_string(),
            ],
            _ => vec![],
        }
    }
}
```

**Patch 6.2: UI Policy Integration**
```typescript
// File: ui/src/components/Policies.tsx

// Replace custom policy handling with backend integration
const handleSignPolicy = async (policy: Policy) => {
  try {
    // Use centralized policy API
    const result = await apiClient.signPolicy(policy.cpid);
    setSignResult(result);
    setSelectedPolicy(policy);
    setShowSignModal(true);
    toast.success(`Policy ${policy.cpid} signed successfully`);
    
    logger.info('Policy signed successfully', {
      component: 'Policies',
      operation: 'signPolicy',
      policyId: policy.cpid,
      tenantId: selectedTenant,
      userId: user.id
    });
    
  } catch (err) {
    toast.error('Failed to sign policy');
    logger.error('Failed to sign policy', {
      component: 'Policies',
      operation: 'signPolicy',
      policyId: policy.cpid,
      tenantId: selectedTenant,
      userId: user.id
    }, err instanceof Error ? err : new Error(String(err)));
  }
};
```

**Verification Checklist:**
- [ ] Single policy engine used everywhere
- [ ] All 20 policy packs enforced consistently
- [ ] UI uses centralized policy API
- [ ] CLI policy commands implemented
- [ ] Policy violations logged and tracked

---

## Phase 7: Database Access Consolidation (Low Priority)

### Current State
- **Core**: `adapteros-db` crate with SQLite
- **RAG**: `adapteros-lora-rag` with dual SQLite/PostgreSQL
- **CLI**: Direct database access in commands
- **Server**: Separate database layer

### Violations

#### V7.1: Multiple Database Access Patterns
**Gap:** Connection pool conflicts, inconsistent schemas  
**Target State:** Single database abstraction layer

**Citations:**
- CONTRIBUTING.md L122: "Prefer `Result<T>` over `Option<T>` for error handling"
- CLAUDE.md L235-242: Database usage guidelines

#### Implementation

**Patch 7.1: Unified Database Abstraction**
```rust
// File: crates/adapteros-db/src/lib.rs

use std::sync::Arc;
use tokio::sync::RwLock;
use adapteros_core::{AosError, Result};
use sqlx::{SqlitePool, PostgresPool};

pub enum DatabaseBackend {
    Sqlite(SqlitePool),
    Postgres(PostgresPool),
}

pub struct Database {
    backend: DatabaseBackend,
    telemetry: TelemetryWriter,
}

impl Database {
    pub async fn new_sqlite(database_url: &str, telemetry: TelemetryWriter) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        Ok(Self {
            backend: DatabaseBackend::Sqlite(pool),
            telemetry,
        })
    }

    pub async fn new_postgres(database_url: &str, telemetry: TelemetryWriter) -> Result<Self> {
        let pool = PostgresPool::connect(database_url).await?;
        Ok(Self {
            backend: DatabaseBackend::Postgres(pool),
            telemetry,
        })
    }

    pub async fn get_tenant(&self, tenant_id: &str) -> Result<Tenant> {
        match &self.backend {
            DatabaseBackend::Sqlite(pool) => {
                let tenant = sqlx::query_as!(
                    Tenant,
                    "SELECT * FROM tenants WHERE id = ?",
                    tenant_id
                )
                .fetch_one(pool)
                .await?;
                Ok(tenant)
            }
            DatabaseBackend::Postgres(pool) => {
                let tenant = sqlx::query_as!(
                    Tenant,
                    "SELECT * FROM tenants WHERE id = $1",
                    tenant_id
                )
                .fetch_one(pool)
                .await?;
                Ok(tenant)
            }
        }
    }

    pub async fn list_adapters(&self, tenant_id: Option<&str>) -> Result<Vec<Adapter>> {
        match &self.backend {
            DatabaseBackend::Sqlite(pool) => {
                let adapters = if let Some(tenant_id) = tenant_id {
                    sqlx::query_as!(
                        Adapter,
                        "SELECT * FROM adapters WHERE tenant_id = ?",
                        tenant_id
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(Adapter, "SELECT * FROM adapters")
                        .fetch_all(pool)
                        .await?
                };
                Ok(adapters)
            }
            DatabaseBackend::Postgres(pool) => {
                let adapters = if let Some(tenant_id) = tenant_id {
                    sqlx::query_as!(
                        Adapter,
                        "SELECT * FROM adapters WHERE tenant_id = $1",
                        tenant_id
                    )
                    .fetch_all(pool)
                    .await?
                } else {
                    sqlx::query_as!(Adapter, "SELECT * FROM adapters")
                        .fetch_all(pool)
                        .await?
                };
                Ok(adapters)
            }
        }
    }
}
```

**Verification Checklist:**
- [ ] Single database abstraction layer
- [ ] Consistent schema across backends
- [ ] Connection pooling managed centrally
- [ ] All database access goes through abstraction
- [ ] Migration support for both backends

---

## Phase 8: Testing Framework Consolidation (Low Priority)

### Current State
- **Unit Tests**: `tests/unit/` with specialized framework
- **Integration Tests**: `tests/integration/` with separate utilities
- **E2E Tests**: `tests/e2e/` with orchestration system
- **Benchmarks**: `tests/benchmark/` with performance testing

### Violations

#### V8.1: Multiple Testing Frameworks
**Gap:** Inconsistent testing patterns, duplicate utilities  
**Target State:** Unified testing framework with specialized modules

**Citations:**
- CONTRIBUTING.md L129: "Add tests for new functionality"
- CLAUDE.md L223-231: Testing determinism guidelines

#### Implementation

**Patch 8.1: Unified Testing Framework**
```rust
// File: tests/unit/lib.rs

// Enhance existing framework with integration capabilities
pub mod unit;
pub mod integration;
pub mod e2e;
pub mod benchmark;

// Re-export all testing utilities
pub use unit::*;
pub use integration::*;
pub use e2e::*;
pub use benchmark::*;

/// Unified test configuration
pub struct TestConfig {
    pub deterministic: bool,
    pub timeout: Duration,
    pub isolation: bool,
    pub telemetry: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            deterministic: true,
            timeout: Duration::from_secs(30),
            isolation: true,
            telemetry: false,
        }
    }
}

/// Test runner with unified configuration
pub struct TestRunner {
    config: TestConfig,
    telemetry: Option<TelemetryWriter>,
}

impl TestRunner {
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            telemetry: None,
        }
    }

    pub async fn run_unit_test<F>(&self, test: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        if self.config.isolation {
            let sandbox = TestSandbox::new();
            sandbox.run(test).await
        } else {
            test()
        }
    }

    pub async fn run_integration_test<F>(&self, test: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        let timeout = Timeout::new(self.config.timeout);
        timeout.run(test).await
    }
}
```

**Verification Checklist:**
- [ ] Single testing framework entry point
- [ ] Consistent test configuration
- [ ] Deterministic testing support
- [ ] Integration with telemetry system
- [ ] Performance benchmarking included

---

## Phase 9: Verification and Validation (High Priority)

### Comprehensive Testing Strategy

#### V9.1: Consolidation Verification
**Gap:** Ensure all consolidations work correctly  
**Target State:** Comprehensive test coverage for all consolidated systems

**Citations:**
- CONTRIBUTING.md L129: "Add tests for new functionality"
- CLAUDE.md L223-231: Testing determinism guidelines

#### Implementation

**Patch 9.1: Consolidation Test Suite**
```rust
// File: tests/consolidation/mod.rs

use adapteros_unit_testing::*;
use adapteros_core::{AosError, Result};
use adapteros_client::{AdapterOSClient, UdsClient};
use adapteros_telemetry::TelemetryWriter;
use adapteros_policy::PolicyEngine;
use adapteros_memory::MemoryManager;
use adapteros_db::Database;

#[tokio::test]
async fn test_logging_consolidation() -> Result<()> {
    // Test that all components use unified logging
    let telemetry = TelemetryWriter::new()?;
    
    // Test CLI logging
    let cli_logger = CliLogger::new(telemetry.clone());
    cli_logger.info("Test message");
    
    // Test UI logging integration
    let ui_logger = UiLogger::new(telemetry.clone());
    ui_logger.info("Test message");
    
    // Verify events are in canonical JSON format
    let events = telemetry.get_events().await?;
    assert!(!events.is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_api_client_consolidation() -> Result<()> {
    // Test that all clients implement unified trait
    let uds_client = UdsClient::new("/var/run/aos/default/worker.sock")?;
    let native_client = NativeClient::new("http://localhost:8080".to_string());
    let wasm_client = WasmClient::new("http://localhost:8080".to_string());
    
    // Test unified interface
    let _: Box<dyn AdapterOSClient> = Box::new(uds_client);
    let _: Box<dyn AdapterOSClient> = Box::new(native_client);
    let _: Box<dyn AdapterOSClient> = Box::new(wasm_client);
    
    Ok(())
}

#[tokio::test]
async fn test_error_handling_consolidation() -> Result<()> {
    // Test that all errors convert to AosError
    let domain_error = adapteros_domain::DomainAdapterError::InvalidManifest {
        reason: "test".to_string(),
    };
    
    let aos_error: AosError = domain_error.into();
    assert!(matches!(aos_error, AosError::DomainAdapter(_)));
    
    Ok(())
}

#[tokio::test]
async fn test_telemetry_consolidation() -> Result<()> {
    // Test unified telemetry schema
    let telemetry = TelemetryWriter::new()?;
    
    telemetry.log("test.event", &TestEvent {
        message: "test".to_string(),
    }).await?;
    
    let events = telemetry.get_events().await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "test.event");
    
    Ok(())
}

#[tokio::test]
async fn test_memory_management_consolidation() -> Result<()> {
    // Test centralized memory management
    let telemetry = TelemetryWriter::new()?;
    let memory_manager = MemoryManager::new(1024 * 1024 * 1024, telemetry);
    
    let usage = memory_manager.get_memory_usage().await?;
    assert_eq!(usage.total_memory_mb, 1024 * 1024);
    
    Ok(())
}

#[tokio::test]
async fn test_policy_enforcement_consolidation() -> Result<()> {
    // Test centralized policy enforcement
    let telemetry = TelemetryWriter::new()?;
    let policy_engine = PolicyEngine::new(telemetry);
    
    let context = PolicyContext {
        operation: "inference".to_string(),
        tenant_id: Some("test".to_string()),
        user_id: Some("test".to_string()),
    };
    
    let result = policy_engine.validate_operation("inference", &context).await?;
    assert!(result.is_ok());
    
    Ok(())
}

#[tokio::test]
async fn test_database_access_consolidation() -> Result<()> {
    // Test unified database abstraction
    let telemetry = TelemetryWriter::new()?;
    let db = Database::new_sqlite(":memory:", telemetry).await?;
    
    let tenant = db.get_tenant("test").await;
    // Should return NotFound error, not panic
    assert!(matches!(tenant, Err(AosError::NotFound(_))));
    
    Ok(())
}

#[tokio::test]
async fn test_testing_framework_consolidation() -> Result<()> {
    // Test unified testing framework
    let config = TestConfig::default();
    let runner = TestRunner::new(config);
    
    let result = runner.run_unit_test(|| {
        // Test function
        Ok(())
    }).await;
    
    assert!(result.is_ok());
    
    Ok(())
}
```

**Verification Checklist:**
- [ ] All consolidation tests pass
- [ ] No regressions in existing functionality
- [ ] Performance benchmarks maintained
- [ ] Determinism tests pass
- [ ] Policy compliance verified

---

## Implementation Timeline

### Week 1: Core Consolidations
- **Day 1-2**: Phase 1 (Logging Consolidation)
- **Day 3-4**: Phase 2 (API Client Consolidation)
- **Day 5**: Phase 3 (Error Handling Unification)

### Week 2: System Consolidations
- **Day 1-2**: Phase 4 (Telemetry Centralization)
- **Day 3-4**: Phase 5 (Memory Management Consolidation)
- **Day 5**: Phase 6 (Policy Enforcement Centralization)

### Week 3: Infrastructure Consolidations
- **Day 1-2**: Phase 7 (Database Access Consolidation)
- **Day 3-4**: Phase 8 (Testing Framework Consolidation)
- **Day 5**: Phase 9 (Verification and Validation)

---

## Success Metrics

### Quantitative Metrics
- **Code Reduction**: 30-40% reduction in duplicate code
- **Performance**: No regression in inference latency
- **Reliability**: 99.9% uptime maintained
- **Maintainability**: 50% reduction in maintenance overhead

### Qualitative Metrics
- **Consistency**: Unified patterns across all components
- **Documentation**: Complete API documentation
- **Testing**: 90%+ test coverage for consolidated systems
- **Compliance**: 100% policy pack compliance

---

## Risk Mitigation

### Technical Risks
- **Breaking Changes**: Comprehensive testing and gradual rollout
- **Performance Impact**: Benchmarking and optimization
- **Integration Issues**: Phased implementation with rollback plans

### Operational Risks
- **Downtime**: Blue-green deployment strategy
- **Data Loss**: Backup and recovery procedures
- **User Impact**: Feature flags and gradual migration

---

## Conclusion

This consolidation plan systematically addresses all overlapping areas in the AdapterOS codebase, providing:

1. **Unified Patterns**: Consistent implementations across all layers
2. **Reduced Complexity**: Elimination of duplicate code and systems
3. **Improved Maintainability**: Single source of truth for each concern
4. **Enhanced Reliability**: Centralized error handling and logging
5. **Policy Compliance**: Consistent enforcement of all 20 policy packs

The plan follows AdapterOS codebase standards and best practices, ensuring compliance with `CLAUDE.md`, `CONTRIBUTING.md`, and `.cursor/rules/global.mdc` while maintaining the system's deterministic and secure characteristics.

**Total Estimated Effort**: 40-48 hours (6-8 days)  
**Expected Benefits**: 30-40% code reduction, 50% maintenance overhead reduction  
**Risk Level**: Medium (mitigated through phased implementation and comprehensive testing)
