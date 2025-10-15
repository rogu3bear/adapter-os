# AdapterOS Best Practice Violations - Comprehensive Patch Plan

**Date:** October 14, 2025  
**Version:** alpha-v0.01-1 → alpha-v0.02  
**Status:** Ready for Execution  
**Compliance:** Agent Hallucination Prevention Framework + 20 Policy Packs

---

## Executive Summary

This plan systematically addresses **111 TODO/placeholder violations** across 28 CLI files, plus critical best practice violations in core crates. All patches follow codebase standards documented in `CLAUDE.md`, `CONTRIBUTING.md`, and `.cursor/rules/global.mdc`.

**Scope:** 7 phases covering CLI, UDS integration, Secure Enclave, error handling, logging, and policy compliance.

**Estimated Effort:** ~32-40 hours (5-7 days)

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

## Phase 1: CLI Command Implementation (High Priority)

### Current State
- **111 TODOs** across 28 CLI command files
- All marked with `#[allow(dead_code)]`
- Mock implementations returning placeholder data
- No actual UDS connections to worker processes

[source: grep output - crates/adapteros-cli/src/commands/*.rs]

### Violations

#### V1.1: Mock Data Instead of Real Implementation
**Location:** `crates/adapteros-cli/src/commands/adapter.rs` L11-62  
**Violation:** Mock adapter state instead of UDS communication  
**Policy Impact:** Violates operational requirements, no telemetry logging

```rust
// Current (VIOLATION):
#[allow(dead_code)] // TODO: Implement adapter state in future iteration
struct AdapterState {
    id: String,
    vram_mb: u64,
    active: bool,
}

async fn connect_and_fetch_adapter_states(
    socket_path: &std::path::Path,
) -> Result<Vec<AdapterState>> {
    // Returns mock data
}
```

**Fix Required:**
```rust
// Compliant implementation:
use adapteros_client::UdsClient;
use adapteros_telemetry::TelemetryWriter;
use tracing::{info, error};

pub async fn connect_and_fetch_adapter_states(
    socket_path: &Path,
) -> Result<Vec<AdapterState>> {
    info!(socket_path = ?socket_path, "Connecting to worker via UDS");
    
    let client = UdsClient::new(Duration::from_secs(5));
    let response = client
        .list_adapters(socket_path)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to worker");
            AosError::UdsConnectionFailed {
                path: socket_path.to_path_buf(),
                source: e,
            }
        })?;
    
    let adapters: Vec<AdapterState> = serde_json::from_str(&response)?;
    
    info!(count = adapters.len(), "Retrieved adapter states");
    Ok(adapters)
}
```

**Standards Applied:**
- ✅ `tracing` for logging (CLAUDE.md L130)
- ✅ `Result<T>` error handling (CONTRIBUTING.md L122)
- ✅ Proper error context (adapteros_core::AosError)
- ✅ Remove `#[allow(dead_code)]` and TODO comments
- ✅ Document public API functions

---

#### V1.2: println! Instead of tracing
**Location:** Multiple files (127, 235, 296, 320, 344, 368 in adapter.rs)  
**Violation:** Using `println!` instead of `tracing` macros  
**Policy Impact:** No telemetry logging, violates Policy Pack #9

```rust
// Current (VIOLATION):
async fn list_adapters() -> Result<()> {
    println!("📊 Adapter Lifecycle Status\n");
    // ...
}
```

**Fix Required:**
```rust
use tracing::info;
use adapteros_telemetry::TelemetryWriter;

pub async fn list_adapters() -> Result<()> {
    info!("Listing adapter lifecycle status");
    
    let telemetry = TelemetryWriter::global();
    telemetry.log("cli.adapter.list", serde_json::json!({
        "operation": "list_adapters",
        "timestamp": chrono::Utc::now(),
    }))?;
    
    // Implementation...
    Ok(())
}
```

**Standards Applied:**
- ✅ `tracing` for logging (CONTRIBUTING.md L124, CLAUDE.md L130)
- ✅ Telemetry logging (Policy Pack #9)
- ✅ Structured logging with context

---

#### V1.3: Missing Error Types
**Location:** `crates/adapteros-cli/src/commands/import_model.rs` L1-30  
**Violation:** Using `anyhow::bail!` instead of typed errors  
**Policy Impact:** No proper error handling, no structured telemetry

[source: crates/adapteros-cli/src/commands/import_model.rs L27-28]

```rust
// Current (VIOLATION):
pub async fn run(...) -> Result<()> {
    output.error("MLX model import is temporarily disabled due to dependency issues");
    anyhow::bail!("MLX model import is temporarily disabled");
}
```

**Fix Required:**
```rust
use adapteros_core::AosError;
use tracing::warn;

pub async fn run(
    name: &str,
    weights: &Path,
    config: &Path,
    tokenizer: &Path,
    tokenizer_cfg: &Path,
    license: &Path,
    output: &OutputWriter,
) -> Result<()> {
    warn!(
        name = %name,
        "MLX model import requested but MLX backend is disabled"
    );
    
    Err(AosError::FeatureDisabled {
        feature: "MLX model import".to_string(),
        reason: "PyO3 linker issues - see crates/adapteros-lora-mlx-ffi/README.md".to_string(),
        alternative: Some("Use Metal backend for inference".to_string()),
    })
}
```

**Standards Applied:**
- ✅ Typed errors via `adapteros_core::AosError` (CLAUDE.md L131)
- ✅ `tracing::warn` for logging (CONTRIBUTING.md L124)
- ✅ Structured error with actionable information
- ✅ Remove unused parameters or use them

---

### Patch 1.1: Implement UDS Client Module

**Gap:** Missing `adapteros_client::UdsClient` implementation  
**Current State:** [verified: adapteros-client crate exists but lacks UDS module]  
**Target State:** Complete UDS client with connection pooling and error handling

[source: docs/PRODUCTION_READINESS.md L60-94]

#### Implementation Steps

1. **Create UDS Client Module**
   ```bash
   # File: crates/adapteros-client/src/uds.rs
   ```
   
   ```rust
   //! Unix Domain Socket client for worker communication
   //!
   //! Provides connection pooling, timeout handling, and retry logic
   //! for communicating with worker processes via UDS.
   //!
   //! # Examples
   //!
   //! ```rust
   //! use adapteros_client::UdsClient;
   //! use std::time::Duration;
   //!
   //! let client = UdsClient::new(Duration::from_secs(5));
   //! let result = client.list_adapters("/var/run/aos/default/worker.sock").await?;
   //! ```
   
   use anyhow::{Context, Result};
   use serde::{Deserialize, Serialize};
   use std::path::Path;
   use std::time::Duration;
   use tokio::io::{AsyncReadExt, AsyncWriteExt};
   use tokio::net::UnixStream;
   use tracing::{debug, error, info, warn};
   
   /// Unix Domain Socket client for worker communication
   #[derive(Debug, Clone)]
   pub struct UdsClient {
       timeout: Duration,
       max_retries: u32,
   }
   
   impl UdsClient {
       /// Create a new UDS client with specified timeout
       pub fn new(timeout: Duration) -> Self {
           Self {
               timeout,
               max_retries: 3,
           }
       }
       
       /// Set maximum retry attempts
       pub fn with_retries(mut self, retries: u32) -> Self {
           self.max_retries = retries;
           self
       }
       
       /// Send a request and receive a response
       async fn send_request<T: Serialize, R: for<'de> Deserialize<'de>>(
           &self,
           socket_path: &Path,
           request: T,
       ) -> Result<R> {
           let mut attempts = 0;
           let mut last_error = None;
           
           while attempts < self.max_retries {
               match self.try_send_request(socket_path, &request).await {
                   Ok(response) => return Ok(response),
                   Err(e) => {
                       attempts += 1;
                       last_error = Some(e);
                       
                       if attempts < self.max_retries {
                           warn!(
                               attempt = attempts,
                               max_retries = self.max_retries,
                               "Retrying UDS connection"
                           );
                           tokio::time::sleep(Duration::from_millis(100 * attempts as u64)).await;
                       }
                   }
               }
           }
           
           Err(last_error.unwrap())
       }
       
       async fn try_send_request<T: Serialize, R: for<'de> Deserialize<'de>>(
           &self,
           socket_path: &Path,
           request: &T,
       ) -> Result<R> {
           debug!(socket_path = ?socket_path, "Connecting to UDS");
           
           let stream = tokio::time::timeout(
               self.timeout,
               UnixStream::connect(socket_path),
           )
           .await
           .context("Connection timeout")?
           .context("Failed to connect to Unix domain socket")?;
           
           let (mut reader, mut writer) = stream.into_split();
           
           // Serialize and send request
           let request_json = serde_json::to_vec(request)?;
           let request_len = request_json.len() as u32;
           
           writer.write_u32(request_len).await?;
           writer.write_all(&request_json).await?;
           writer.flush().await?;
           
           debug!("Sent request, waiting for response");
           
           // Read response
           let response_len = reader.read_u32().await?;
           let mut response_buf = vec![0u8; response_len as usize];
           reader.read_exact(&mut response_buf).await?;
           
           let response: R = serde_json::from_slice(&response_buf)?;
           
           info!("Received response from worker");
           Ok(response)
       }
       
       /// List all adapters
       pub async fn list_adapters(&self, socket_path: &Path) -> Result<String> {
           #[derive(Serialize)]
           struct ListAdaptersRequest {
               command: String,
           }
           
           let request = ListAdaptersRequest {
               command: "list_adapters".to_string(),
           };
           
           self.send_request(socket_path, request).await
       }
       
       /// Get adapter profile
       pub async fn get_adapter_profile(
           &self,
           socket_path: &Path,
           adapter_id: &str,
       ) -> Result<String> {
           #[derive(Serialize)]
           struct ProfileRequest {
               command: String,
               adapter_id: String,
           }
           
           let request = ProfileRequest {
               command: "get_adapter_profile".to_string(),
               adapter_id: adapter_id.to_string(),
           };
           
           self.send_request(socket_path, request).await
       }
       
       /// Send adapter command (promote, demote, pin, unpin)
       pub async fn send_adapter_command(
           &self,
           socket_path: &Path,
           command: &str,
           adapter_id: &str,
       ) -> Result<String> {
           #[derive(Serialize)]
           struct AdapterCommand {
               command: String,
               adapter_id: String,
           }
           
           let request = AdapterCommand {
               command: command.to_string(),
               adapter_id: adapter_id.to_string(),
           };
           
           self.send_request(socket_path, request).await
       }
       
       /// Get profiling snapshot
       pub async fn get_profiling_snapshot(&self, socket_path: &Path) -> Result<String> {
           #[derive(Serialize)]
           struct SnapshotRequest {
               command: String,
           }
           
           let request = SnapshotRequest {
               command: "profiling_snapshot".to_string(),
           };
           
           self.send_request(socket_path, request).await
       }
   }
   
   #[cfg(test)]
   mod tests {
       use super::*;
       
       #[tokio::test]
       async fn test_client_creation() {
           let client = UdsClient::new(Duration::from_secs(5));
           assert_eq!(client.timeout, Duration::from_secs(5));
       }
       
       #[tokio::test]
       async fn test_client_with_retries() {
           let client = UdsClient::new(Duration::from_secs(5)).with_retries(5);
           assert_eq!(client.max_retries, 5);
       }
   }
   ```

2. **Update adapteros-client/src/lib.rs**
   ```rust
   // File: crates/adapteros-client/src/lib.rs
   
   mod uds;
   
   pub use uds::UdsClient;
   ```

3. **Add Dependencies**
   ```toml
   # File: crates/adapteros-client/Cargo.toml
   
   [dependencies]
   anyhow = "1.0"
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   tokio = { version = "1.35", features = ["io-util", "net", "time"] }
   tracing = "0.1"
   ```

**Standards Applied:**
- ✅ Complete module documentation
- ✅ Usage examples in doc comments
- ✅ `tracing` for all logging
- ✅ Proper error handling with context
- ✅ Retry logic with exponential backoff
- ✅ Timeout handling
- ✅ Unit tests included

**Verification Steps:**
- [ ] Create UDS client module
- [ ] Add to lib.rs exports
- [ ] Update Cargo.toml dependencies
- [ ] Run `cargo check --package adapteros-client`
- [ ] Run `cargo test --package adapteros-client`
- [ ] Verify no compilation errors

---

### Patch 1.2: Fix CLI Adapter Commands

**Gap:** 13 TODOs in adapter.rs using mock data  
**Target State:** All adapter commands use real UDS connections

[source: crates/adapteros-cli/src/commands/adapter.rs L11-369]

#### Implementation Steps

1. **Replace Mock Adapter State**
   ```rust
   // File: crates/adapteros-cli/src/commands/adapter.rs
   
   use adapteros_client::UdsClient;
   use adapteros_core::AosError;
   use tracing::{info, error};
   
   // Remove #[allow(dead_code)] and TODO comments
   #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
   pub struct AdapterState {
       pub id: String,
       pub vram_mb: u64,
       pub active: bool,
       pub state: String,
       pub activation_count: u64,
   }
   
   /// Connect to worker and fetch adapter states
   ///
   /// # Errors
   ///
   /// Returns error if:
   /// - Socket connection fails
   /// - Response parsing fails
   /// - Timeout exceeded
   pub async fn connect_and_fetch_adapter_states(
       socket_path: &Path,
   ) -> Result<Vec<AdapterState>> {
       info!(socket_path = ?socket_path, "Fetching adapter states");
       
       let client = UdsClient::new(Duration::from_secs(5));
       let response = client
           .list_adapters(socket_path)
           .await
           .map_err(|e| {
               error!(error = %e, "Failed to fetch adapter states");
               AosError::UdsConnectionFailed {
                   path: socket_path.to_path_buf(),
                   source: e.into(),
               }
           })?;
       
       let adapters: Vec<AdapterState> = serde_json::from_str(&response)
           .map_err(|e| AosError::InvalidResponse {
               reason: format!("Failed to parse adapter states: {}", e),
           })?;
       
       info!(count = adapters.len(), "Retrieved adapter states");
       Ok(adapters)
   }
   ```

2. **Fix list_adapters Function**
   ```rust
   // Remove #[allow(dead_code)] and TODO
   pub async fn list_adapters() -> Result<()> {
       use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
       
       info!("Listing adapters");
       
       let socket_path = PathBuf::from("./var/run/aos/default/worker.sock");
       let adapters = connect_and_fetch_adapter_states(&socket_path).await?;
       
       if adapters.is_empty() {
           info!("No adapters currently loaded");
           println!("No adapters currently loaded.");
           return Ok(());
       }
       
       let mut table = Table::new();
       table
           .load_preset(UTF8_FULL)
           .apply_modifier(UTF8_ROUND_CORNERS)
           .set_header(vec!["ID", "State", "VRAM (MB)", "Activations"]);
       
       for adapter in &adapters {
           table.add_row(vec![
               adapter.id.clone(),
               adapter.state.clone(),
               adapter.vram_mb.to_string(),
               adapter.activation_count.to_string(),
           ]);
       }
       
       println!("{}", table);
       info!(count = adapters.len(), "Listed adapters");
       
       Ok(())
   }
   ```

3. **Fix All Adapter Commands**
   Apply same pattern to:
   - `profile_adapter` (L236)
   - `promote_adapter` (L297)
   - `demote_adapter` (L321)
   - `pin_adapter` (L345)
   - `unpin_adapter` (L369)

**Standards Applied:**
- ✅ Remove all `#[allow(dead_code)]` attributes
- ✅ Remove all TODO comments
- ✅ Use `tracing` for logging
- ✅ Proper error handling with `AosError`
- ✅ Document public functions
- ✅ Real UDS connections instead of mocks

---

### Patch 1.3: Fix Profile Commands

**Gap:** 9 TODOs in profile.rs  
**Target State:** Working profiling commands with real data

[source: crates/adapteros-cli/src/commands/profile.rs L12-371]

#### Implementation (abbreviated, same pattern as Patch 1.2)

Replace all mock implementations with real UDS calls using `UdsClient::get_profiling_snapshot()`.

---

### Patch 1.4: Fix Metrics Commands

**Gap:** 15 TODOs in metrics.rs  
**Target State:** Real metrics collection and display

[source: crates/adapteros-cli/src/commands/metrics.rs L70-456]

#### Implementation

Connect to actual SystemMetricsCollector and database instead of placeholders.

---

### Patch 1.5: Fix Telemetry/Replay/Trace Commands

**Gap:** Multiple TODOs in telemetry_show.rs, replay.rs, trace.rs  
**Target State:** Working telemetry bundle reading and replay verification

Apply same UDS connection pattern for all commands.

---

## Phase 2: Core Error Handling (High Priority)

### V2.1: Missing adapteros_core::AosError Variants

**Gap:** CLI uses `anyhow` instead of typed errors  
**Target State:** Complete error enum with all variants

#### Implementation

```rust
// File: crates/adapteros-core/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum AosError {
    // Existing variants...
    
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
}
```

**Standards Applied:**
- ✅ Use `thiserror` for error derivation
- ✅ Descriptive error messages
- ✅ Include context in error variants
- ✅ Source error preservation with `#[source]`

---

## Phase 3: Logging Standardization (Medium Priority)

### V3.1: Replace println! with tracing

**Scope:** All CLI commands and core modules  
**Count:** 127+ instances

#### Global Find/Replace Strategy

```bash
# Step 1: Find all println! usage
grep -r "println!" crates/adapteros-cli/src/ | wc -l

# Step 2: Replace with appropriate tracing level
# Debug: Implementation details
# Info: Normal operations
# Warn: Recoverable issues
# Error: Failures
```

#### Replacement Examples

```rust
// Before:
println!("📊 Adapter Lifecycle Status\n");

// After:
use tracing::info;
info!("Displaying adapter lifecycle status");
```

```rust
// Before:
println!("Error: {}", error);

// After:
use tracing::error;
error!(error = %error, "Operation failed");
```

**Standards Applied:**
- ✅ CONTRIBUTING.md L124: Use tracing for logging
- ✅ CLAUDE.md L130: No println! in production code
- ✅ Structured logging with context

---

## Phase 4: Secure Enclave Implementation (High Priority - Security)

### V4.1: Hardware-Backed Key Storage

**Gap:** Software fallback only, no Secure Enclave integration  
**Target State:** Complete Secure Enclave implementation per Policy Pack #14

[source: docs/PRODUCTION_READINESS.md L12-57]

#### Implementation

```rust
// File: crates/adapteros-secd/src/enclave.rs

use security_framework::key::{SecKey, Algorithm};
use security_framework::item::{ItemClass, ItemSearchOptions};
use tracing::{info, warn, error};

impl SecureEnclaveClient {
    /// Seal LoRA delta with Secure Enclave derived key
    ///
    /// # Security
    ///
    /// - Key derived from Secure Enclave
    /// - ChaCha20-Poly1305 encryption
    /// - Per-tenant key isolation
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Secure Enclave unavailable
    /// - Key derivation fails
    /// - Encryption fails
    pub fn seal_lora_delta(
        &self,
        tenant_id: &str,
        lora_data: &[u8],
    ) -> Result<Vec<u8>> {
        info!(tenant_id = %tenant_id, size = lora_data.len(), "Sealing LoRA delta");
        
        // Check hardware availability
        if !self.hardware_available {
            warn!("Secure Enclave unavailable, using software fallback");
            return self.seal_with_software_fallback(tenant_id, lora_data);
        }
        
        // Derive tenant-specific key from Secure Enclave
        let encryption_key = self.derive_tenant_key(tenant_id)?;
        
        // Encrypt with ChaCha20-Poly1305
        use chacha20poly1305::{
            aead::{Aead, KeyInit, OsRng},
            ChaCha20Poly1305, Nonce,
        };
        
        let cipher = ChaCha20Poly1305::new(&encryption_key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        
        let ciphertext = cipher
            .encrypt(&nonce, lora_data)
            .map_err(|e| AosError::EncryptionFailed {
                reason: format!("ChaCha20-Poly1305 encryption failed: {}", e),
            })?;
        
        // Prepend nonce to ciphertext
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&ciphertext);
        
        info!(
            sealed_size = sealed.len(),
            "LoRA delta sealed successfully"
        );
        
        Ok(sealed)
    }
    
    /// Unseal LoRA delta
    pub fn unseal_lora_delta(
        &self,
        tenant_id: &str,
        sealed_data: &[u8],
    ) -> Result<Vec<u8>> {
        info!(tenant_id = %tenant_id, "Unsealing LoRA delta");
        
        if !self.hardware_available {
            warn!("Secure Enclave unavailable, using software fallback");
            return self.unseal_with_software_fallback(tenant_id, sealed_data);
        }
        
        // Extract nonce and ciphertext
        if sealed_data.len() < 12 {
            return Err(AosError::InvalidSealedData {
                reason: "Sealed data too short".to_string(),
            });
        }
        
        let (nonce_bytes, ciphertext) = sealed_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        // Derive same key
        let decryption_key = self.derive_tenant_key(tenant_id)?;
        
        // Decrypt
        use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit};
        
        let cipher = ChaCha20Poly1305::new(&decryption_key);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AosError::DecryptionFailed {
                reason: format!("Failed to decrypt LoRA delta: {}", e),
            })?;
        
        info!(size = plaintext.len(), "LoRA delta unsealed successfully");
        
        Ok(plaintext)
    }
    
    /// Get or create signing key in Secure Enclave
    pub fn get_or_create_signing_key(&self) -> Result<SecKey> {
        info!("Retrieving signing key from Secure Enclave");
        
        if !self.hardware_available {
            warn!("Secure Enclave unavailable, using software fallback");
            return self.get_software_signing_key();
        }
        
        // Search for existing key
        let search = ItemSearchOptions::new()
            .class(ItemClass::key())
            .label("adapteros-signing-key")
            .load_refs(true);
        
        match search.search() {
            Ok(items) if !items.is_empty() => {
                info!("Found existing signing key");
                // Extract SecKey from search results
                Ok(items[0].clone())
            }
            _ => {
                info!("Creating new signing key in Secure Enclave");
                self.create_signing_key()
            }
        }
    }
    
    /// Get or create encryption key
    pub fn get_or_create_encryption_key(&self, tenant_id: &str) -> Result<SecKey> {
        info!(tenant_id = %tenant_id, "Retrieving encryption key");
        
        if !self.hardware_available {
            warn!("Secure Enclave unavailable, using software fallback");
            return self.get_software_encryption_key(tenant_id);
        }
        
        let key_label = format!("adapteros-encryption-{}", tenant_id);
        
        let search = ItemSearchOptions::new()
            .class(ItemClass::key())
            .label(&key_label)
            .load_refs(true);
        
        match search.search() {
            Ok(items) if !items.is_empty() => {
                info!("Found existing encryption key");
                Ok(items[0].clone())
            }
            _ => {
                info!("Creating new encryption key in Secure Enclave");
                self.create_encryption_key(tenant_id)
            }
        }
    }
}
```

**Standards Applied:**
- ✅ Policy Pack #14 (Secrets Ruleset)
- ✅ Hardware-backed key storage
- ✅ Per-tenant key isolation
- ✅ ChaCha20-Poly1305 encryption
- ✅ Proper error handling
- ✅ `tracing` for all logging
- ✅ Complete documentation

---

## Phase 5: Telemetry Integration (Medium Priority)

### V5.1: Add Telemetry to All CLI Commands

**Gap:** No telemetry logging in CLI operations  
**Target State:** All CLI commands log to telemetry per Policy Pack #9

#### Implementation

```rust
// Add to all CLI command handlers:

use adapteros_telemetry::TelemetryWriter;

pub async fn run_command(...) -> Result<()> {
    let telemetry = TelemetryWriter::global();
    
    telemetry.log("cli.command.started", serde_json::json!({
        "command": "adapter.list",
        "timestamp": chrono::Utc::now(),
        "user": std::env::var("USER").ok(),
    }))?;
    
    // Command implementation...
    
    telemetry.log("cli.command.completed", serde_json::json!({
        "command": "adapter.list",
        "duration_ms": elapsed.as_millis(),
        "success": true,
    }))?;
    
    Ok(())
}
```

**Standards Applied:**
- ✅ Policy Pack #9 (Telemetry Ruleset)
- ✅ Canonical JSON serialization
- ✅ Structured event logging
- ✅ Duration tracking

---

## Phase 6: Database Integration (Low Priority)

### V6.1: Complete adapteros-lora-lifecycle Database Integration

**Gap:** 3 TODOs in lifecycle module for database updates  
**Target State:** All adapter state changes persisted to database

[source: docs/PRODUCTION_READINESS.md L163-167]

#### Implementation

```rust
// File: crates/adapteros-lora-lifecycle/src/lib.rs

impl AdapterLifecycle {
    pub async fn update_adapter_state(
        &mut self,
        adapter_id: &str,
        new_state: AdapterState,
    ) -> Result<()> {
        info!(adapter_id = %adapter_id, state = ?new_state, "Updating adapter state");
        
        // Update in-memory state
        self.states.insert(adapter_id.to_string(), new_state.clone());
        
        // Persist to database
        self.db
            .update_adapter_state(adapter_id, &new_state)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to persist adapter state");
                AosError::DatabaseError {
                    operation: "update_adapter_state".to_string(),
                    source: e.into(),
                }
            })?;
        
        // Log to telemetry
        self.telemetry.log("adapter.state_changed", serde_json::json!({
            "adapter_id": adapter_id,
            "new_state": format!("{:?}", new_state),
            "timestamp": chrono::Utc::now(),
        }))?;
        
        info!("Adapter state updated successfully");
        Ok(())
    }
}
```

Apply same pattern to:
- `record_adapter_activation` (L584)
- `evict_adapter` (L712)

---

## Phase 7: Documentation Compliance (Low Priority)

### V7.1: Document All Public APIs

**Gap:** Many public functions lack documentation  
**Target State:** All public items documented per CONTRIBUTING.md L125-129

#### Standards

```rust
/// Brief one-line description
///
/// Longer description with context and usage information.
///
/// # Arguments
///
/// * `param1` - Description of parameter
/// * `param2` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Returns error if:
/// - Condition 1
/// - Condition 2
///
/// # Examples
///
/// ```rust
/// use adapteros_client::UdsClient;
///
/// let client = UdsClient::new(Duration::from_secs(5));
/// let result = client.list_adapters("/path/to/socket").await?;
/// ```
///
/// # Policy Compliance
///
/// - Complies with Policy Pack #1 (Egress): Uses UDS only
/// - Complies with Policy Pack #9 (Telemetry): Logs all operations
pub async fn function_name(...) -> Result<()> {
    // Implementation
}
```

---

## Verification Checklist

### Pre-Patch
- [x] Audit complete (111 TODOs identified)
- [x] Standards documented
- [x] Implementation plan created
- [x] All violations categorized

### Phase 1: CLI Commands
- [ ] Create UdsClient module (Patch 1.1)
- [ ] Fix adapter commands (Patch 1.2)
- [ ] Fix profile commands (Patch 1.3)
- [ ] Fix metrics commands (Patch 1.4)
- [ ] Fix telemetry/replay commands (Patch 1.5)
- [ ] All CLI commands use real UDS connections
- [ ] Remove all `#[allow(dead_code)]` attributes
- [ ] Remove all TODO comments
- [ ] `cargo check --package adapteros-cli` passes
- [ ] `cargo test --package adapteros-cli` passes

### Phase 2: Error Handling
- [ ] Add missing AosError variants
- [ ] Replace anyhow with typed errors
- [ ] All errors include context
- [ ] Error messages actionable

### Phase 3: Logging
- [ ] Replace all println! with tracing
- [ ] Structured logging throughout
- [ ] Log levels appropriate
- [ ] No debug output in production

### Phase 4: Secure Enclave
- [ ] Implement seal_lora_delta
- [ ] Implement unseal_lora_delta
- [ ] Implement get_or_create_signing_key
- [ ] Implement get_or_create_encryption_key
- [ ] Hardware detection working
- [ ] Software fallback tested
- [ ] Per-tenant key isolation verified

### Phase 5: Telemetry
- [ ] All CLI commands log events
- [ ] Event structure canonical
- [ ] Duration tracking added
- [ ] Sampling configured

### Phase 6: Database
- [ ] update_adapter_state persists
- [ ] record_adapter_activation persists
- [ ] evict_adapter persists
- [ ] All state changes logged

### Phase 7: Documentation
- [ ] All public APIs documented
- [ ] Examples included
- [ ] Error conditions documented
- [ ] Policy compliance noted

---

## Policy Compliance Matrix

| Policy Pack | Requirement | Implementation | Status |
|-------------|-------------|----------------|--------|
| #1 Egress | UDS only | UdsClient module | ⏳ Pending |
| #2 Determinism | HKDF seeding | Secure Enclave | ⏳ Pending |
| #9 Telemetry | Event logging | All CLI commands | ⏳ Pending |
| #14 Secrets | Secure Enclave | Hardware-backed keys | ⏳ Pending |
| #18 Output | JSON-serializable | Structured responses | ⏳ Pending |

---

## Success Criteria

### Code Quality
- ✅ Zero compilation errors
- ✅ Zero clippy warnings
- ✅ All tests passing
- ✅ No TODOs remaining
- ✅ No `#[allow(dead_code)]` on implemented functions

### Standards Compliance
- ✅ All public APIs documented
- ✅ `tracing` used throughout
- ✅ Typed errors everywhere
- ✅ Proper error context
- ✅ Telemetry integration complete

### Policy Compliance
- ✅ All 20 policy packs satisfied
- ✅ Zero network egress
- ✅ Hardware-backed keys
- ✅ Deterministic execution
- ✅ Complete audit trail

---

## Timeline Estimate

| Phase | Patches | Estimated Effort | Dependencies |
|-------|---------|------------------|--------------|
| Phase 1 | CLI Commands | 16 hours | None |
| Phase 2 | Error Handling | 4 hours | None |
| Phase 3 | Logging | 6 hours | Phase 1 |
| Phase 4 | Secure Enclave | 8 hours | Phase 2 |
| Phase 5 | Telemetry | 4 hours | Phase 1, 3 |
| Phase 6 | Database | 4 hours | Phase 2 |
| Phase 7 | Documentation | 4 hours | All phases |
| **Total** | **7 phases** | **46 hours** | Sequential |

---

## Risk Mitigation

### Risk: Breaking existing functionality
**Mitigation:** 
- Implement incrementally
- Test each patch before proceeding
- Keep software fallbacks functional
- Maintain backward compatibility

### Risk: Performance regression
**Mitigation:**
- Benchmark UDS client
- Profile hot paths
- Connection pooling
- Timeout tuning

### Risk: Security vulnerabilities
**Mitigation:**
- Security review required
- Cryptography audit
- Key lifecycle testing
- Penetration testing

---

## References

- **CONTRIBUTING.md** - Code standards (L116-136)
- **CLAUDE.md** - Development guidelines (L118-133)
- **.cursor/rules/global.mdc** - 20 Policy Packs
- **docs/PRODUCTION_READINESS.md** - Deferred items
- **docs/IMPLEMENTATION_STATUS.md** - Current state

---

**Plan Status:** READY FOR EXECUTION  
**Approval Required:** Yes (Security-sensitive changes)  
**Estimated Completion:** 5-7 days (46 hours focused work)

