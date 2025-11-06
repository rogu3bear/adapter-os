# AdapterOS Keychain Integration

## Overview

AdapterOS provides secure cryptographic key storage across multiple platforms using native OS keychain facilities with advanced features including backend health monitoring, dynamic switching, and hardware security integration. This document describes the fully rectified keychain integration, including supported backends, schema definitions, access control policies, and key lifecycle management.

## Supported Backends

### macOS (Security Framework)

**Primary Backend**: macOS Security Framework via secure CLI commands
- **Storage**: Keychain database protected by user login credentials
- **Hardware Integration**: Secure Enclave support on Apple Silicon for receipt signing
- **Security**: Command injection prevention with input validation and base64 encoding

**Features**:
- Hardware-backed receipt signing via Secure Enclave (P-256 ECDSA)
- Automatic keychain unlocking with device unlock
- Fine-grained access control policies
- Integration with Keychain Access UI
- Secure CLI implementation with input sanitization

### Linux (Multiple Backends)

**Primary Backend**: freedesktop Secret Service (D-Bus)
- **Daemons**: GNOME Keyring, KDE KWallet, etc.
- **Storage**: Encrypted keyring files in `~/.local/share/keyrings/`
- **Fallback**: Linux kernel keyring (keyutils)

**Secondary Backend**: Linux kernel keyring
- **Storage**: In-kernel memory (not persisted to disk)
- **Headless Support**: Works without D-Bus or desktop session
- **Lifetime**: Keys expire after kernel-defined periods

### Password-Based Fallback

**Fallback Backend**: Encrypted JSON keystore
- **Storage**: AES-256-GCM encrypted file (`~/.adapteros-keys.enc` or `./.adapteros-keys.enc`)
- **KDF**: Argon2id with high iteration count (65536, 3, 4, 32)
- **Opt-in**: Requires `ADAPTEROS_KEYCHAIN_FALLBACK=pass:<password>` environment variable

## Schema Definitions

### macOS Keychain Attributes

All keychain items use these standard attributes:

| Attribute | Value | Description |
|-----------|-------|-------------|
| `kSecClass` | `kSecClassGenericPassword` | Item class for generic secrets |
| `kSecAttrService` | `"adapteros"` | Service namespace |
| `kSecAttrAccount` | `"<key_id>-<type>"` | Unique identifier (e.g., `"mykey-ed25519"`) |
| `kSecAttrLabel` | `"AdapterOS <Type> Key: <key_id>"` | Human-readable label |
| `kSecAttrAccessible` | `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` | Access policy |

**Examples**:
- Ed25519 private key: `service="adapteros"`, `account="mykey-ed25519"`
- AES-256 key: `service="adapteros"`, `account="mykey-symmetric"`

### Linux Secret Service Schema

Items stored via D-Bus Secret Service use these attributes:

| Attribute | Value | Description |
|-----------|-------|-------------|
| `service` | `"adapteros"` | Service namespace |
| `key-type` | `"ed25519" \| "symmetric"` | Algorithm type |
| `key-id` | `"<key_id>"` | Unique identifier |
| `label` | `"AdapterOS <Type> Key: <key_id>"` | Human-readable label |

### Linux Kernel Keyring Schema

Keys stored in kernel keyring use descriptive names:

| Component | Format | Example |
|-----------|--------|---------|
| Description | `"adapteros:<key_id>:<type>"` | `"adapteros:mykey:ed25519"` |
| Type | `"user"` | Kernel user key type |
| Keyring | Persistent user keyring | `keyctl_get_persistent(uid)` |

### Password Fallback Schema

JSON structure encrypted with AES-256-GCM:

```json
{
  "keys": {
    "<key_id>": {
      "algorithm": "ed25519" | "aes256gcm" | "chacha20poly1305",
      "private_key_b64": "<base64_encoded_key_bytes>",
      "public_key_b64": "<base64_encoded_public_key_bytes>", // if applicable
      "created_at": "<unix_timestamp>"
    }
  }
}
```

## Access Control Policies

### macOS Access Control

**Default Policy**: `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`
- Keys accessible when device is unlocked
- Keys never sync to iCloud or migrate to other devices
- Provides hardware-backed encryption at rest

**Secure Enclave Keys**:
- Additional access control: `kSecAttrTokenIDSecureEnclave`
- Private key material never leaves hardware
- Operations performed inside Secure Enclave

### Linux Access Control

**Secret Service**:
- Access controlled by D-Bus daemon permissions
- Keys encrypted with user login credentials
- Automatic unlocking on desktop session start

**Kernel Keyring**:
- Access limited to owning UID
- Keys stored in kernel memory, not accessible via `/proc/<pid>/mem`
- No persistence across reboots by default

### Password Fallback

**Access Control**:
- Encryption key derived from user-provided password
- File permissions: Owner read/write only (0600)
- No automatic unlocking - requires password each run

## Key Lifecycle

### Key Generation

1. **macOS**: Generate via `SecKey::generate()` or software crypto
2. **Linux**: Generate via software crypto (Ed25519/AES)
3. **Storage**: Store in appropriate backend with proper attributes
4. **Caching**: Cache `KeyHandle` in memory for performance

### Key Retrieval

1. **Lookup**: Search by service/account or attributes
2. **Decryption**: Backend handles decryption automatically
3. **Validation**: Verify key format and length
4. **Caching**: Return cached handle if available

### Key Rotation

1. **Generation**: Create new key with same ID
2. **Storage**: Store new key (overwrites existing)
3. **Cleanup**: Delete old key from backend
4. **Receipt**: Generate cryptographically signed rotation receipt

### Key Deletion

1. **Lookup**: Find key by ID
2. **Removal**: Delete from backend storage
3. **Cleanup**: Remove from memory cache
4. **Audit**: Log deletion operation

## Backend Selection Logic

### macOS

Always uses Security Framework - no fallback logic required.

### Linux

1. **Try Secret Service**: Attempt D-Bus connection and collection access
2. **Fallback to Kernel**: If D-Bus fails, use keyutils kernel keyring
3. **Password Fallback**: If `ADAPTEROS_KEYCHAIN_FALLBACK` env var set

### Password Fallback

Only used when explicitly requested via environment variable:
```bash
export ADAPTEROS_KEYCHAIN_FALLBACK=pass:mysecurepassword
```

## Error Handling

### Common Errors

**macOS Keychain Locked**:
- Symptom: `errSecAuthFailed` or permission errors
- Solution: Unlock Keychain Access application

**Linux D-Bus Unavailable**:
- Symptom: Secret service connection fails
- Solution: Start desktop session or install keyring daemon

**Password Fallback Wrong Password**:
- Symptom: Decryption fails with authentication error
- Solution: Verify `ADAPTEROS_KEYCHAIN_FALLBACK` value

**Kernel Keyring Access Denied**:
- Symptom: `keyctl_get_persistent` fails with EPERM
- Solution: Check user permissions or kernel configuration

### Error Messages

All error messages include:
- Specific operation that failed
- Likely cause of failure
- Actionable remediation steps
- Underlying error details

## Security Considerations

### Key Material Protection

- **Never in Plaintext**: Keys never stored unencrypted
- **Memory Zeroization**: Sensitive data zeroized after use
- **Hardware Security**: Secure Enclave when available
- **Access Control**: Fine-grained permission policies

### Backend Security Properties

| Backend | At-Rest Encryption | Hardware Security | Headless Support |
|---------|-------------------|-------------------|------------------|
| macOS Keychain | ✅ User credentials | ✅ Secure Enclave | ❌ |
| Linux Secret Service | ✅ User credentials | ❌ | ❌ |
| Linux Kernel Keyring | ✅ Kernel protection | ❌ | ✅ |
| Password Fallback | ✅ AES-256-GCM | ❌ | ✅ |

### Threat Model

**Assumptions**:
- OS keychain backends are trustworthy
- Hardware (Secure Enclave) is not compromised
- User credentials are not compromised

**Protections Against**:
- Memory disclosure attacks
- Unauthorized file access
- Network-based attacks (keys never transmitted)
- Cross-VM attacks (Secure Enclave isolation)

## Configuration

### Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ADAPTEROS_KEYCHAIN_FALLBACK` | Enable password fallback | `pass:mysecret123` |

### Configuration File

```toml
[key_provider]
mode = "keychain"
keychain_service = "adapteros"  # Optional service name
rotation_interval_secs = 86400  # 24 hours
```

## Testing

### Backend Detection

Test that correct backend is selected based on environment:
- macOS: Always Security Framework
- Linux with D-Bus: Secret Service
- Linux headless: Kernel keyring
- With env var: Password fallback

### Key Operations

Test complete key lifecycle:
- Generate → Store → Retrieve → Sign/Seal → Rotate → Delete
- Verify cryptographic correctness
- Check proper cleanup and zeroization

### Error Scenarios

Test error handling for:
- Locked keychains
- Missing permissions
- Corrupted storage
- Network/D-Bus failures

## Maintenance

### Key Rotation

- Automatic rotation based on `rotation_interval_secs`
- Manual rotation via API calls
- Signed rotation receipts for audit

### Storage Cleanup

- Remove expired keys
- Clean up failed operations
- Monitor storage usage

### Security Updates

- Update cryptographic algorithms as needed
- Monitor for security vulnerabilities in backends
- Update access control policies

## Troubleshooting

### macOS Issues

**Keychain not accessible**:
```bash
# Unlock keychain
security unlock-keychain
```

**Secure Enclave unavailable**:
- Check if running on Apple Silicon
- Verify macOS version (13.0+ for attestation)

### Linux Issues

**Secret service not running**:
```bash
# Start GNOME keyring
gnome-keyring-daemon --start
```

**Kernel keyring not available**:
```bash
# Check kernel support
zgrep KEYCTL /proc/config.gz
```

### Password Fallback Issues

**Wrong password**:
```bash
# Check environment variable
echo $ADAPTEROS_KEYCHAIN_FALLBACK
# Reset keystore if needed
rm ~/.adapteros-keys.enc
```

## References

- [macOS Security Framework](https://developer.apple.com/documentation/security)
- [Freedesktop Secret Service](https://specifications.freedesktop.org/secret-service/)
- [Linux Kernel Keyring](https://man7.org/linux/man-pages/man7/keyrings.7.html)
- [AdapterOS Security Ruleset #14](../docs/SECURITY_RULESET.md#rule-14)
