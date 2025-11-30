# Azure Key Vault KMS Backend - Quick Reference

**Last Updated:** 2025-11-22
**Implementation Status:** Complete ✅

---

## Quick Start

### 1. Enable Feature

```toml
# Cargo.toml
[dependencies]
adapteros-crypto = { features = ["azure-kms"] }
```

### 2. Create Configuration

```rust
use adapteros_crypto::providers::kms::{
    KmsConfig, KmsCredentials, KmsBackendType, KmsProvider,
};

let config = KmsConfig {
    backend_type: KmsBackendType::AzureKeyVault,
    endpoint: "myvault.vault.azure.net".to_string(),
    region: Some("eastus".to_string()),
    credentials: KmsCredentials::None, // Uses env vars or managed identity
    timeout_secs: 30,
    max_retries: 3,
    key_namespace: Some("production".to_string()),
};
```

### 3. Initialize Provider

```rust
let provider = KmsProvider::with_kms_config_async(config).await?;
```

### 4. Use Key Operations

```rust
use adapteros_crypto::key_provider::KeyAlgorithm;

// Generate key
let key = provider.generate_key("my-key", KeyAlgorithm::Ed25519).await?;

// Sign
let sig = provider.sign("my-key", b"data").await?;

// Encrypt
let ct = provider.encrypt("my-key", b"secret").await?;

// Decrypt
let pt = provider.decrypt("my-key", &ct).await?;

// Rotate
let rotated = provider.rotate_key("my-key").await?;
```

---

## Authentication Methods

### Method 1: Environment Variables

```bash
export AZURE_TENANT_ID="your-tenant-id"
export AZURE_CLIENT_ID="your-client-id"
export AZURE_CLIENT_SECRET="your-secret"
```

```rust
credentials: KmsCredentials::None
```

### Method 2: Service Principal (Explicit)

```rust
credentials: KmsCredentials::AzureServicePrincipal {
    tenant_id: "tenant-id".to_string(),
    client_id: "client-id".to_string(),
    client_secret: "secret".to_string(),
}
```

### Method 3: Managed Identity (Azure VM/App Service)

```rust
credentials: KmsCredentials::None // Auto-detected
```

---

## Endpoint Formats

All formats are supported (auto-normalized):

```
"myvault"                          ✅
"myvault.vault.azure.net"         ✅
"https://myvault.vault.azure.net" ✅
"https://myvault.vault.azure.net/"✅
```

---

## Supported Algorithms

| Algorithm | Use Case | Key Size |
|-----------|----------|----------|
| Ed25519 | Signing | 32 bytes |
| AES-256-GCM | Encryption | 256 bits |
| ChaCha20-Poly1305 | Encryption | 256 bits |

---

## Key Operations Reference

### Generate Key

```rust
backend.generate_key("key-id", KeyAlgorithm::Ed25519).await?
→ KeyHandle { key_id, algorithm, public_key }
```

### Sign

```rust
backend.sign("key-id", data).await?
→ Vec<u8> (signature bytes)
```

### Encrypt

```rust
backend.encrypt("key-id", plaintext).await?
→ Vec<u8> (ciphertext)
```

### Decrypt

```rust
backend.decrypt("key-id", ciphertext).await?
→ Vec<u8> (plaintext)
```

### Rotate Key

```rust
backend.rotate_key("key-id").await?
→ KeyHandle { ..new_version }
```

### Get Public Key

```rust
backend.get_public_key("key-id").await?
→ Vec<u8> (public key bytes)
```

### Check Existence

```rust
backend.key_exists("key-id").await?
→ bool
```

### Delete Key

```rust
backend.delete_key("key-id").await?
// 90-day recovery period
```

---

## Configuration Examples

### Development (Local Debugging)

```rust
KmsConfig {
    backend_type: KmsBackendType::AzureKeyVault,
    endpoint: "https://dev-vault.vault.azure.net/".to_string(),
    region: Some("eastus".to_string()),
    credentials: KmsCredentials::None,
    timeout_secs: 60,      // Longer timeout for local
    max_retries: 5,
    key_namespace: Some("dev".to_string()),
}
```

### Production (High Availability)

```rust
KmsConfig {
    backend_type: KmsBackendType::AzureKeyVault,
    endpoint: "https://prod-vault.vault.azure.net/".to_string(),
    region: Some("eastus".to_string()),
    credentials: KmsCredentials::AzureServicePrincipal {
        tenant_id: env::var("AZURE_TENANT_ID")?,
        client_id: env::var("AZURE_CLIENT_ID")?,
        client_secret: env::var("AZURE_CLIENT_SECRET")?,
    },
    timeout_secs: 30,
    max_retries: 3,
    key_namespace: Some("production".to_string()),
}
```

### Multi-Tenant Isolation

```rust
KmsConfig {
    // ... other fields
    key_namespace: Some(format!("tenant-{}", tenant_id)),
}
// Keys are namespaced: "tenant-123/key-id"
```

---

## Error Handling

```rust
use adapteros_core::AosError;

match provider.sign("key", data).await {
    Ok(sig) => { /* use signature */ }
    Err(AosError::Auth(msg)) => {
        // Authentication/authorization failure
        eprintln!("Auth error: {}", msg);
    }
    Err(AosError::Crypto(msg)) => {
        // Cryptographic operation or key error
        eprintln!("Crypto error: {}", msg);
    }
    Err(AosError::Network(msg)) => {
        // Network/timeout error
        eprintln!("Network error: {}", msg);
    }
    Err(e) => {
        // Other errors
        eprintln!("Error: {}", e);
    }
}
```

---

## Testing

### Run Tests

```bash
# With Azure feature
cargo test --test azure_kms_tests --features azure-kms

# All tests
cargo test -p adapteros-crypto

# Single test
cargo test --test azure_kms_tests test_azure_kms_sign_data --features azure-kms
```

### Test Scenarios Covered

✅ Backend initialization
✅ Key generation (all algorithms)
✅ Signing/encryption/decryption
✅ Key rotation
✅ Public key retrieval
✅ Key existence checks
✅ Key deletion
✅ Error handling
✅ Retry logic
✅ URL normalization
✅ Multi-tenancy
✅ Feature gate fallback

---

## Performance Tips

### 1. Use Caching

Public keys are cached after first retrieval:
```rust
// First call: API call to Azure (50-100ms)
backend.get_public_key("key").await?

// Subsequent calls: Cache hit (<1ms)
backend.get_public_key("key").await?
```

### 2. Batch Operations

Group key operations when possible:
```rust
// Better: Generate multiple keys in one loop
for key_id in keys {
    backend.generate_key(&key_id, alg).await?;
}
```

### 3. Reuse Provider

Create provider once, reuse:
```rust
let provider = KmsProvider::with_kms_config_async(config).await?;
// Reuse provider for all operations
```

### 4. Tune Timeouts

Adjust based on network conditions:
```rust
// Slow network
timeout_secs: 60,
max_retries: 5,

// Fast network
timeout_secs: 30,
max_retries: 3,
```

---

## Common Issues

### Issue: "Authentication failed"

**Solution:** Verify Azure credentials
```bash
# List role assignments
az role assignment list --assignee <client-id>

# Check vault access
az keyvault list --resource-group <rg>
```

### Issue: "Key not found"

**Solution:** Verify key exists
```bash
# List keys in vault
az keyvault key list --vault-name <vault-name>

# Check specific key
az keyvault key show --vault-name <vault-name> --name <key-id>
```

### Issue: "Operation timeout"

**Solution:** Increase timeout
```rust
KmsConfig {
    timeout_secs: 60,  // Increase from 30
    max_retries: 5,    // Increase retries
    // ...
}
```

### Issue: Feature disabled warning

**Solution:** Enable in Cargo.toml
```toml
[dependencies]
adapteros-crypto = { features = ["azure-kms"] }
```

---

## Required Permissions (Azure RBAC)

### Minimum Required Role

```
Key Vault Crypto User
- Sign
- Verify
- Get Key
- List Keys
```

### Full Permission Role

```
Key Vault Administrator
- Create
- Import
- Rotate
- Delete
- Backup
- Restore
- Purge
```

---

## Production Checklist

- [ ] Azure subscription with Key Vault
- [ ] Service principal created and configured
- [ ] Required RBAC roles assigned
- [ ] Vault soft delete enabled (default)
- [ ] Purge protection enabled for critical keys
- [ ] TLS 1.2+ enforced
- [ ] Network access restricted
- [ ] Audit logging enabled
- [ ] Backup strategy documented
- [ ] Disaster recovery plan
- [ ] Cost monitoring configured
- [ ] Documentation updated

---

## Architecture Diagram

```
┌─────────────────────┐
│   Application       │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│   KmsProvider       │
│  (Factory)          │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────────────────────────┐
│ AzureKeyVaultBackend                    │
├─────────────────────────────────────────┤
│ • vault_url                             │
│ • credential (DefaultAzureCredential)   │
│ • key_cache (RwLock<HashMap>)           │
└──────────┬──────────────────────────────┘
           │
           ▼
┌─────────────────────┐
│ Azure Key Vault     │
│ (Cloud Service)     │
└─────────────────────┘
```

---

## File Locations

| Component | File |
|-----------|------|
| Implementation | `crates/adapteros-crypto/src/providers/kms.rs` (lines 1530-1956) |
| Tests | `crates/adapteros-crypto/tests/azure_kms_tests.rs` |
| Documentation | `docs/AZURE_KEYVAULT_INTEGRATION.md` |
| Dependencies | `crates/adapteros-crypto/Cargo.toml` |

---

## Related Documentation

- [Full Integration Guide](./docs/AZURE_KEYVAULT_INTEGRATION.md)
- [Implementation Summary](./AZURE_KMS_IMPLEMENTATION.md)
- [KMS Architecture](./docs/ARCHITECTURE_PATTERNS.md)
- [Error Handling Standards](./CLAUDE.md#error-handling)
- [Crypto Module](./crates/adapteros-crypto/src/providers/kms.rs)

---

## Support

For issues or questions:

1. Check [troubleshooting section](./docs/AZURE_KEYVAULT_INTEGRATION.md#troubleshooting)
2. Review test examples in `azure_kms_tests.rs`
3. Check Azure Key Vault documentation
4. Verify credentials and RBAC roles
5. Check application logs for detailed error messages

---

**Ready for Production Use** ✅

All components implemented, tested, and documented.
