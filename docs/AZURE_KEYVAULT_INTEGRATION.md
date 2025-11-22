# Azure Key Vault Integration Guide

**Last Updated:** 2025-11-22
**Status:** Implemented (v1.0)
**Maintainer:** AdapterOS Crypto Team

---

## Overview

The Azure Key Vault backend provides cloud-based key management for AdapterOS cryptographic operations. It integrates with Microsoft Azure Key Vault to store, rotate, and manage cryptographic keys securely.

### Features

- **Asymmetric Key Support**: Ed25519 signing keys
- **Symmetric Encryption**: AES-256-GCM, ChaCha20-Poly1305
- **Key Rotation**: Automatic and manual key version management
- **Authentication**: Service principal and managed identity support
- **Retry Logic**: Exponential backoff with configurable max retries
- **Caching**: In-memory metadata caching for performance
- **Multi-region**: Support for Azure regions (eastus, westus, etc.)
- **Namespace Isolation**: Multi-tenant key isolation via key namespacing

---

## Architecture

### Backend Structure

```
AzureKeyVaultBackend
├── vault_url: String              // Vault endpoint
├── credential: DefaultAzureCredential  // Azure auth
├── config: KmsConfig              // Configuration
└── key_cache: Arc<RwLock<...>>   // Metadata cache
```

### Authentication Methods

1. **Service Principal** (Explicit Credentials)
   ```rust
   KmsCredentials::AzureServicePrincipal {
       tenant_id: "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
       client_id: "yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy".to_string(),
       client_secret: "client-secret-value".to_string(),
   }
   ```

2. **Managed Identity** (Implicit, Azure VM/App Service)
   ```rust
   KmsCredentials::None  // Uses DefaultAzureCredential
   ```

3. **Environment Variables**
   ```bash
   export AZURE_TENANT_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
   export AZURE_CLIENT_ID="yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy"
   export AZURE_CLIENT_SECRET="client-secret-value"
   ```

---

## Configuration

### Basic Setup

```rust
use adapteros_crypto::providers::kms::{
    KmsConfig, KmsCredentials, KmsBackendType, KmsProvider,
};

let config = KmsConfig {
    backend_type: KmsBackendType::AzureKeyVault,
    endpoint: "myvault.vault.azure.net".to_string(),
    region: Some("eastus".to_string()),
    credentials: KmsCredentials::AzureServicePrincipal {
        tenant_id: "tenant-id".to_string(),
        client_id: "client-id".to_string(),
        client_secret: "client-secret".to_string(),
    },
    timeout_secs: 30,
    max_retries: 3,
    key_namespace: Some("production".to_string()),
};

// Initialize async (required for Azure)
let provider = KmsProvider::with_kms_config_async(config).await?;
```

### Endpoint Format Variants

All of the following are valid endpoint formats:

```rust
// Format 1: Full FQDN
"myvault.vault.azure.net"

// Format 2: Full HTTPS URL without trailing slash
"https://myvault.vault.azure.net"

// Format 3: Full HTTPS URL with trailing slash
"https://myvault.vault.azure.net/"

// Format 4: Vault name only (auto-qualified)
"myvault"
```

The implementation automatically normalizes to:
```
https://myvault.vault.azure.net/
```

---

## Feature Gate

Enable Azure Key Vault support in `Cargo.toml`:

```toml
[dependencies]
adapteros-crypto = { path = "../adapteros-crypto", features = ["azure-kms"] }
```

Without the feature flag, the backend falls back to mock implementation with warnings.

---

## Key Operations

### Generate Key

```rust
use adapteros_crypto::key_provider::KeyAlgorithm;

// Generate Ed25519 signing key
let key_handle = backend
    .generate_key("my-signing-key", KeyAlgorithm::Ed25519)
    .await?;

// Generate AES-256-GCM encryption key
let key_handle = backend
    .generate_key("my-encrypt-key", KeyAlgorithm::Aes256Gcm)
    .await?;
```

### Sign Data

```rust
let data = b"data to sign";
let signature = backend.sign("my-signing-key", data).await?;
```

### Encrypt Data

```rust
let plaintext = b"sensitive data";
let ciphertext = backend.encrypt("my-encrypt-key", plaintext).await?;
```

### Decrypt Data

```rust
let plaintext = backend.decrypt("my-encrypt-key", &ciphertext).await?;
```

### Rotate Key

```rust
let rotated_key = backend.rotate_key("my-signing-key").await?;
```

### Get Public Key

```rust
let public_key = backend.get_public_key("my-signing-key").await?;
```

### Check Key Exists

```rust
let exists = backend.key_exists("my-signing-key").await?;
```

### Delete Key

```rust
backend.delete_key("my-signing-key").await?;
```

---

## Error Handling

### Error Mapping

Azure-specific errors are mapped to AdapterOS error types:

| Azure Error | Maps To | Description |
|------------|---------|-------------|
| Not Found (404) | `AosError::Crypto` | Key does not exist in vault |
| Unauthorized (401) | `AosError::Auth` | Invalid credentials or permissions |
| Forbidden (403) | `AosError::Auth` | User lacks required permissions |
| Invalid Request (400) | `AosError::Crypto` | Invalid key ID or operation |
| Timeout | `AosError::Network` | Operation exceeded timeout |
| Conflict (409) | `AosError::Crypto` | Key already exists |
| Other | `AosError::Crypto` | Generic Azure KMS error |

### Common Errors

```rust
// Key not found
Err(AosError::Crypto("Azure Key Vault: Key not found"))

// Authentication failed
Err(AosError::Auth("Azure Key Vault: Authentication failed"))

// Network timeout
Err(AosError::Network("Azure Key Vault: Operation timeout"))
```

---

## Retry Logic

All operations implement exponential backoff:

```
Attempt 1: Immediate
Attempt 2: 100ms delay
Attempt 3: 200ms delay
Attempt 4: 400ms delay
...
```

Configuration:

```rust
let config = KmsConfig {
    max_retries: 3,        // Max number of retries
    timeout_secs: 30,      // Per-operation timeout
    // ... other fields
};
```

---

## Caching

Key metadata is cached in-memory to reduce API calls:

```
┌─────────────────────┐
│ generate_key()      │ → Cache write (key_id, algorithm, public_key, version)
│ rotate_key()        │ → Cache update (new version)
│ get_public_key()    │ → Cache read (fast path)
│ delete_key()        │ → Cache eviction
└─────────────────────┘
```

Cache structure:
```rust
struct AzureKeyMetadata {
    key_id: String,
    algorithm: KeyAlgorithm,
    public_key: Option<Vec<u8>>,
    created_at: u64,
    version: String,
}
```

---

## Multi-Tenancy

Use key namespacing to isolate keys per tenant:

```rust
let config = KmsConfig {
    key_namespace: Some("tenant-a".to_string()),
    // ... other fields
};

// Key ID "signing-key" becomes "tenant-a/signing-key"
backend.generate_key("signing-key", KeyAlgorithm::Ed25519).await?;
```

---

## Compliance & Security

### Key Management Policies

1. **Automatic Rotation**: Azure Key Vault supports automatic key rotation policies
2. **Access Control**: RBAC via Azure IAM
3. **Audit Logging**: All operations logged to Azure Activity Log
4. **Soft Delete**: 90-day recovery period for deleted keys (configurable)
5. **Purge Protection**: Prevent permanent deletion

### Production Checklist

- [ ] Service principal credentials stored securely (e.g., Azure Key Vault)
- [ ] RBAC roles configured (minimum: `Key Vault Crypto User`)
- [ ] Soft delete enabled on vault
- [ ] Purge protection enabled for critical keys
- [ ] Audit logging enabled
- [ ] Network access restricted via firewall/VNet
- [ ] TLS 1.2+ enforced

---

## Testing

### Run Tests

```bash
# With Azure KMS feature
cargo test --test azure_kms_tests --features azure-kms

# All tests
cargo test --workspace --features azure-kms
```

### Test Coverage

- Backend initialization (sync/async)
- Endpoint URL parsing and normalization
- Key generation (all algorithms)
- Sign/encrypt/decrypt operations
- Key rotation
- Public key retrieval
- Key existence checks
- Key deletion
- Error handling
- Retry logic
- Caching behavior
- Multi-tenancy isolation

### Mock Backend

When `azure-kms` feature is disabled, tests use a mock backend:

```rust
#[cfg(not(feature = "azure-kms"))]
mod azure_kms_feature_disabled_tests {
    // Tests that feature fallback works correctly
}
```

---

## Implementation Details

### Algorithm Mapping

```rust
KeyAlgorithm::Ed25519 → Azure "Ed25519" (signing)
KeyAlgorithm::Aes256Gcm → Azure "RSA2048" (encryption)
KeyAlgorithm::ChaCha20Poly1305 → Azure "RSA2048" (encryption)
```

Note: Azure Key Vault uses RSA-based encryption instead of symmetric algorithms for vault-stored keys.

### Vault URL Normalization

```
Input: "myvault"
→ Output: "https://myvault.vault.azure.net/"

Input: "myvault.vault.azure.net"
→ Output: "https://myvault.vault.azure.net/"

Input: "https://myvault.vault.azure.net"
→ Output: "https://myvault.vault.azure.net/"
```

### Fingerprint Format

```
"azure-keyvault-{vault-name}-v1.0"

Example: "azure-keyvault-myvault-v1.0"
```

Used for attestation and backend identification.

---

## Performance Considerations

### Latency

Typical operation latencies:

| Operation | Latency | Notes |
|-----------|---------|-------|
| generate_key | 100-500ms | Network call to Azure |
| sign | 50-200ms | Vault operation |
| encrypt | 50-200ms | Vault operation |
| decrypt | 50-200ms | Vault operation |
| rotate_key | 100-500ms | Version creation |
| get_public_key | 50-100ms | With caching: <1ms |

### Throughput

- Recommend 10-20 concurrent operations max
- Use client-side caching to reduce calls
- Batch operations when possible

### Cost

Azure Key Vault pricing (as of 2025):

- **Vault fee**: $0.60/month per vault
- **Key operations**: $0.03-0.06 per 10,000 operations
- **Soft deleted keys**: $0.25/month per key

---

## Troubleshooting

### Authentication Failures

```rust
// Error: "Authentication failed"
// Solution: Verify credentials and RBAC roles
az role assignment list --assignee <client-id> --scope <vault-id>
```

### Key Not Found

```rust
// Error: "Key not found"
// Solution: Check key exists and vault access
az keyvault key list --vault-name myvault
```

### Timeout Issues

```rust
// Error: "Operation timeout"
// Solution: Increase timeout or check network
let config = KmsConfig {
    timeout_secs: 60,  // Increase from default 30
    max_retries: 5,    // Increase retry attempts
    // ...
};
```

### Connection Issues

```bash
# Test vault connectivity
curl -I https://myvault.vault.azure.net/

# Check firewall rules
az keyvault network-rule list --vault-name myvault
```

---

## References

- [Azure Key Vault Documentation](https://docs.microsoft.com/azure/key-vault/)
- [Azure Rust SDK](https://github.com/Azure/azure-sdk-for-rust)
- [Key Management Best Practices](https://docs.microsoft.com/azure/key-vault/general/best-practices)
- [CLAUDE.md - Error Handling](../CLAUDE.md#error-handling)
- [adapteros-crypto/Cargo.toml](../crates/adapteros-crypto/Cargo.toml)
- [adapteros-crypto/src/providers/kms.rs](../crates/adapteros-crypto/src/providers/kms.rs)
