# Secure Enclave Integration Guide

## Overview

This guide covers integrating AdapterOS with macOS Secure Enclave for hardware-backed signing keys and host identity attestation.

## Prerequisites

- macOS 10.15+ (Secure Enclave support)
- Xcode Command Line Tools
- Rust toolchain with macOS target
- Admin privileges for keychain access

## Implementation

### 1. Update Cargo.toml Dependencies

```toml
[dependencies]
# Secure Enclave integration
security-framework = "2.11"
core-foundation = "0.9"
ed25519-dalek = "2.1"
chacha20poly1305 = "0.10"

# Keychain integration
keychain-services = "0.1"
```

### 2. Implement Secure Enclave Connection

```rust:crates/adapteros-secd/src/secure_enclave.rs
use security_framework::{
    access::{Access, AccessControl, AccessFlags},
    certificate::SecCertificate,
    identity::SecIdentity,
    key::SecKey,
    keychain::{CreateOptions, Keychain},
    os::macos::keychain::SecKeychainExt,
    secure_transport::SslContext,
};

pub struct SecureEnclaveConnection {
    keychain: Keychain,
    key_alias: String,
}

impl SecureEnclaveConnection {
    pub fn new() -> Result<Self> {
        // Open or create keychain
        let keychain = Keychain::create(
            "AdapterOS-SecureEnclave",
            Some("AdapterOS Secure Enclave Keychain"),
            CreateOptions::default(),
        )?;
        
        Ok(Self {
            keychain,
            key_alias: "aos-host-signing".to_string(),
        })
    }
    
    pub fn generate_keypair(&self, alias: &str) -> Result<PublicKey> {
        // Generate Ed25519 keypair in Secure Enclave
        let key_attributes = [
            (kSecAttrKeyType, kSecAttrKeyTypeEd25519),
            (kSecAttrKeySizeInBits, 256),
            (kSecAttrTokenID, kSecAttrTokenIDSecureEnclave),
            (kSecAttrIsPermanent, true),
            (kSecAttrLabel, alias),
            (kSecAttrApplicationTag, b"aos-host-signing"),
        ];
        
        let key = SecKey::generate(&key_attributes)?;
        let pubkey = key.public_key()?;
        
        // Store in keychain
        self.keychain.add_item(&key)?;
        
        Ok(PublicKey::from_bytes(pubkey.bytes()))
    }
    
    pub fn sign(&self, alias: &str, data: &[u8]) -> Result<Signature> {
        // Retrieve key from keychain
        let key = self.keychain.find_item(alias)?;
        
        // Sign data using Secure Enclave
        let signature = key.sign(data, kSecPaddingPKCS1)?;
        
        Ok(Signature::from_bytes(signature))
    }
    
    pub fn attest_key(&self, alias: &str) -> Result<Vec<u8>> {
        // Request hardware attestation
        let key = self.keychain.find_item(alias)?;
        
        // Get attestation data from Secure Enclave
        let attestation = key.attestation_data()?;
        
        Ok(attestation)
    }
}
```

### 3. Update Host Identity Manager

```rust:crates/adapteros-secd/src/host_identity.rs
impl HostIdentityManager {
    pub fn new(key_alias: String) -> Result<Self> {
        let connection = SecureEnclaveConnection::new()?;
        
        Ok(Self {
            connection,
            key_alias,
        })
    }
    
    pub fn generate_host_key(&self, alias: &str) -> Result<PublicKey> {
        info!("Generating host key in Secure Enclave: {}", alias);
        
        let pubkey = self.connection.generate_keypair(alias)?;
        
        debug!("Generated host key: {}", hex::encode(pubkey.to_bytes()));
        
        Ok(pubkey)
    }
    
    pub fn sign_with_host_key(&self, data: &[u8]) -> Result<Signature> {
        self.connection.sign(&self.key_alias, data)
    }
    
    pub fn attest_host_identity(&self) -> Result<AttestationReport> {
        let pubkey = self.get_host_public_key()?;
        let attestation_data = self.connection.attest_key(&self.key_alias)?;
        
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        
        let attestation_metadata = AttestationMetadata {
            pubkey: pubkey.to_bytes().to_vec(),
            attestation_data,
            timestamp_us,
            hardware_model: self.get_hardware_model()?,
            secure_enclave_version: self.get_secure_enclave_version()?,
        };
        
        Ok(AttestationReport {
            pubkey: pubkey.to_bytes().to_vec(),
            attestation_metadata,
            timestamp_us,
        })
    }
    
    fn get_hardware_model(&self) -> Result<String> {
        // Get hardware model from system
        let output = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.model")
            .output()?;
        
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
    
    fn get_secure_enclave_version(&self) -> Result<String> {
        // Get Secure Enclave version
        let output = std::process::Command::new("system_profiler")
            .arg("SPHardwareDataType")
            .output()?;
        
        let output_str = String::from_utf8(output.stdout)?;
        
        // Extract Secure Enclave version (simplified)
        if output_str.contains("Apple T2") {
            Ok("T2".to_string())
        } else if output_str.contains("Apple M1") || output_str.contains("Apple M2") {
            Ok("M1/M2".to_string())
        } else {
            Ok("Unknown".to_string())
        }
    }
}
```

### 4. Build Configuration

```toml:Cargo.toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "2.11"
core-foundation = "0.9"
keychain-services = "0.1"

[features]
default = ["deterministic-only"]
multi-backend = []
secure-enclave = ["security-framework", "core-foundation"]
```

### 5. Compilation Flags

```bash
# Build with Secure Enclave support
cargo build --release --features secure-enclave

# Or for development
cargo build --features secure-enclave
```

### 6. Runtime Configuration

```toml:configs/cp.toml
[secd]
# Secure Enclave settings
enable_secure_enclave = true
key_alias = "aos-host-signing"
keychain_name = "AdapterOS-SecureEnclave"

# Attestation settings
require_hardware_attestation = true
attestation_timeout_secs = 30

# Key lifecycle
key_rotation_interval_days = 365
backup_enabled = false  # Keys never leave Secure Enclave
```

### 7. Testing

```rust:tests/secure_enclave_integration.rs
#[cfg(target_os = "macos")]
#[tokio::test]
async fn test_secure_enclave_integration() -> Result<()> {
    let manager = HostIdentityManager::new("test-key".to_string())?;
    
    // Generate key in Secure Enclave
    let pubkey = manager.generate_host_key("test-key")?;
    assert!(!pubkey.to_bytes().is_empty());
    
    // Sign data
    let data = b"test message";
    let signature = manager.sign_with_host_key(data)?;
    
    // Verify signature
    assert!(pubkey.verify(data, &signature).is_ok());
    
    // Get attestation
    let attestation = manager.attest_host_identity()?;
    assert!(!attestation.attestation_metadata.attestation_data.is_empty());
    
    Ok(())
}
```

## Security Considerations

### 1. Key Protection
- Keys never leave Secure Enclave
- No key export capability
- Hardware-backed attestation

### 2. Access Control
- Keychain access restricted to AdapterOS
- No network access from Secure Enclave
- Local signing only

### 3. Attestation
- Hardware-rooted identity
- Tamper-evident attestation
- Version tracking

## Troubleshooting

### Common Issues

1. **Keychain Access Denied**
   ```bash
   # Grant keychain access
   security add-generic-password -a "aos" -s "AdapterOS-SecureEnclave" -w ""
   ```

2. **Secure Enclave Not Available**
   ```bash
   # Check Secure Enclave status
   system_profiler SPHardwareDataType | grep -i "secure enclave"
   ```

3. **Compilation Errors**
   ```bash
   # Install required frameworks
   xcode-select --install
   ```

### Debug Mode

```bash
# Enable debug logging
export RUST_LOG=adapteros_secd=debug
export RUST_LOG=security_framework=debug

# Run with debug output
cargo run --features secure-enclave --bin aos-secd
```

## Production Deployment

### 1. Service Configuration

```ini:/etc/systemd/system/aos-secd.service
[Unit]
Description=AdapterOS Secure Enclave Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/aos-secd --config /etc/aos/secd.toml
User=aos
Group=aos
Restart=always

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict

[Install]
WantedBy=multi-user.target
```

### 2. Keychain Permissions

```bash
# Create keychain
security create-keychain -p "secure_password" "AdapterOS-SecureEnclave"

# Set keychain as default
security default-keychain -s "AdapterOS-SecureEnclave"

# Grant access to aos user
security add-generic-password -a "aos" -s "AdapterOS-SecureEnclave" -w "secure_password"
```

### 3. Monitoring

```bash
# Monitor keychain access
sudo fs_usage -w -f filesys | grep keychain

# Monitor Secure Enclave usage
sudo fs_usage -w -f filesys | grep secureenclave
```

## References

- [Apple Secure Enclave Documentation](https://developer.apple.com/documentation/security/certificate_key_and_trust_services/keys/storing_keys_in_the_secure_enclave)
- [Security Framework Reference](https://developer.apple.com/documentation/security)
- [Keychain Services Programming Guide](https://developer.apple.com/library/archive/documentation/Security/Conceptual/keychainServConcepts/)
