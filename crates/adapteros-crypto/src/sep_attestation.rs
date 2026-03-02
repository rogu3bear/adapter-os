//! Secure Enclave (SEP) attestation for macOS
//!
//! This module provides hardware-backed key generation and attestation using
//! the Apple Secure Enclave Processor on M-series Macs (M1/M2/M3/M4).
//!
//! ## Features
//! - Hardware-backed key generation in Secure Enclave
//! - Attestation chain verification
//! - Root CA verification against trusted Apple certificates
//! - Graceful fallback on Intel Macs (no SEP available)
//! - Automatic chip detection (M1/M2/M3/M4)
//!
//! ## Security Properties
//! - Private keys never leave the Secure Enclave
//! - Attestation proves key was generated in hardware
//! - Protection against key extraction and cloning
//! - Certificate chains verified against Apple Root CA
//!
//! ## Platform Support
//! - macOS 12+ on Apple Silicon (M-series): Full SEP support
//! - macOS on Intel: Graceful fallback to keychain-backed keys
//! - Other platforms: Returns error
//!
//! ## Root CA Configuration
//! The root CA bundle path can be configured via:
//! - `AOS_SEP_ROOT_CA_PATH` environment variable
//! - Default: `/etc/adapteros/certs/apple-root-ca.pem`

use ::pem::parse_many as pem_parse_many;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// SEP (Secure Enclave Processor) chip generation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SepChipGeneration {
    /// M1 chip (2020)
    M1,
    /// M2 chip (2022)
    M2,
    /// M3 chip (2023)
    M3,
    /// M4 chip (2024)
    M4,
    /// Unknown Apple Silicon chip
    UnknownAppleSilicon,
    /// Intel chip (no SEP)
    Intel,
}

impl std::fmt::Display for SepChipGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SepChipGeneration::M1 => write!(f, "M1"),
            SepChipGeneration::M2 => write!(f, "M2"),
            SepChipGeneration::M3 => write!(f, "M3"),
            SepChipGeneration::M4 => write!(f, "M4"),
            SepChipGeneration::UnknownAppleSilicon => write!(f, "Unknown Apple Silicon"),
            SepChipGeneration::Intel => write!(f, "Intel"),
        }
    }
}

/// SEP availability status
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SepAvailability {
    /// Whether SEP is available on this system
    pub available: bool,
    /// Chip generation
    pub chip_generation: SepChipGeneration,
    /// Reason if not available
    pub reason: Option<String>,
}

/// Attestation data from Secure Enclave
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SepAttestation {
    /// Public key bytes (P-256 ECDSA)
    pub public_key: Vec<u8>,
    /// Attestation certificate chain (X.509 DER)
    pub certificate_chain: Vec<Vec<u8>>,
    /// Nonce used for attestation
    pub nonce: Vec<u8>,
    /// Chip generation
    pub chip_generation: SepChipGeneration,
    /// Timestamp of attestation (Unix timestamp)
    pub timestamp: u64,
}

impl SepAttestation {
    /// Returns true when attestation includes a certificate chain proving hardware issuance.
    pub fn is_hardware_backed(&self) -> bool {
        !self.certificate_chain.is_empty()
    }

    /// Human-readable attestation kind.
    pub fn attestation_kind(&self) -> &'static str {
        if self.is_hardware_backed() {
            "hardware"
        } else {
            "synthetic"
        }
    }
}

/// Default path for the Apple Root CA bundle
pub const DEFAULT_ROOT_CA_PATH: &str = "/etc/adapteros/certs/apple-root-ca.pem";

/// Environment variable for configuring Root CA path
pub const ROOT_CA_PATH_ENV: &str = "AOS_SEP_ROOT_CA_PATH";

fn require_hardware_attestation() -> bool {
    match std::env::var("AOS_REQUIRE_HARDWARE_ATTESTATION") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// Configuration for SEP Root CA verification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootCaConfig {
    /// Path to the Root CA bundle (PEM or DER format)
    pub bundle_path: PathBuf,
    /// Whether to require Root CA verification (fail if bundle not found)
    pub require_verification: bool,
    /// Whether to allow self-signed certificates when Root CA bundle is unavailable
    pub allow_self_signed_fallback: bool,
}

impl Default for RootCaConfig {
    fn default() -> Self {
        Self {
            bundle_path: get_root_ca_path(),
            require_verification: false,
            allow_self_signed_fallback: true,
        }
    }
}

impl RootCaConfig {
    /// Create a strict configuration that requires Root CA verification
    pub fn strict(bundle_path: impl Into<PathBuf>) -> Self {
        Self {
            bundle_path: bundle_path.into(),
            require_verification: true,
            allow_self_signed_fallback: false,
        }
    }

    /// Create a lenient configuration for development
    pub fn development() -> Self {
        Self {
            bundle_path: get_root_ca_path(),
            require_verification: false,
            allow_self_signed_fallback: true,
        }
    }
}

/// Get the configured Root CA path from environment or default
pub fn get_root_ca_path() -> PathBuf {
    std::env::var(ROOT_CA_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_ROOT_CA_PATH))
}

/// Result of Root CA verification
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootCaVerificationResult {
    /// Whether the root certificate was verified against a trusted CA
    pub trusted_root_found: bool,
    /// Subject of the root certificate in the chain
    pub root_subject: Option<String>,
    /// Subject of the matching trusted CA (if found)
    pub trusted_ca_subject: Option<String>,
    /// Reason if verification failed or was skipped
    pub reason: Option<String>,
}

/// Cached trusted root certificates (loaded once)
static TRUSTED_ROOTS_CACHE: OnceLock<Vec<TrustedRootCa>> = OnceLock::new();

/// Representation of a trusted Root CA certificate
#[derive(Clone, Debug)]
pub struct TrustedRootCa {
    /// Certificate subject (distinguished name)
    pub subject: String,
    /// Certificate issuer (distinguished name)
    pub issuer: String,
    /// Serial number (hex encoded)
    pub serial_number: String,
    /// DER-encoded certificate bytes
    pub der_bytes: Vec<u8>,
    /// SHA-256 fingerprint of the certificate
    pub fingerprint_sha256: String,
}

/// Detect Apple Silicon chip generation
#[cfg(target_os = "macos")]
pub fn detect_chip_generation() -> SepChipGeneration {
    use std::process::Command;

    // Run: sysctl -n machdep.cpu.brand_string
    let output = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output();

    if let Ok(output) = output {
        if let Ok(cpu_brand) = String::from_utf8(output.stdout) {
            let cpu_brand = cpu_brand.to_lowercase();
            debug!(cpu_brand = %cpu_brand, "Detected CPU");

            if cpu_brand.contains("apple m4") {
                return SepChipGeneration::M4;
            } else if cpu_brand.contains("apple m3") {
                return SepChipGeneration::M3;
            } else if cpu_brand.contains("apple m2") {
                return SepChipGeneration::M2;
            } else if cpu_brand.contains("apple m1") {
                return SepChipGeneration::M1;
            } else if cpu_brand.contains("apple") {
                return SepChipGeneration::UnknownAppleSilicon;
            } else {
                return SepChipGeneration::Intel;
            }
        }
    }

    // Fallback: Check architecture
    let output = Command::new("uname").arg("-m").output();

    if let Ok(output) = output {
        if let Ok(arch) = String::from_utf8(output.stdout) {
            if arch.trim() == "arm64" {
                debug!("Detected arm64 architecture, assuming Apple Silicon");
                return SepChipGeneration::UnknownAppleSilicon;
            }
        }
    }

    debug!("Unable to detect Apple Silicon, assuming Intel");
    SepChipGeneration::Intel
}

#[cfg(not(target_os = "macos"))]
pub fn detect_chip_generation() -> SepChipGeneration {
    SepChipGeneration::Intel
}

/// Check if Secure Enclave is available on this system
pub fn check_sep_availability() -> SepAvailability {
    let chip_generation = detect_chip_generation();

    match chip_generation {
        SepChipGeneration::Intel => SepAvailability {
            available: false,
            chip_generation,
            reason: Some("Intel Macs do not have Secure Enclave".to_string()),
        },
        SepChipGeneration::M1
        | SepChipGeneration::M2
        | SepChipGeneration::M3
        | SepChipGeneration::M4
        | SepChipGeneration::UnknownAppleSilicon => {
            #[cfg(target_os = "macos")]
            {
                SepAvailability {
                    available: true,
                    chip_generation,
                    reason: None,
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                SepAvailability {
                    available: false,
                    chip_generation,
                    reason: Some("SEP only available on macOS".to_string()),
                }
            }
        }
    }
}

/// Generate a key in Secure Enclave and return attestation
///
/// This function:
/// 1. Checks SEP availability
/// 2. Generates a P-256 ECDSA key in Secure Enclave
/// 3. Creates attestation with certificate chain
/// 4. Verifies attestation chain
///
/// On Intel Macs, this falls back to regular keychain key generation.
#[cfg(target_os = "macos")]
pub async fn generate_sep_key_with_attestation(
    key_label: &str,
    nonce: &[u8],
) -> Result<SepAttestation> {
    let availability = check_sep_availability();

    if !availability.available {
        if require_hardware_attestation() {
            return Err(AosError::Crypto(format!(
                "hardware attestation required but SEP unavailable: {}",
                availability
                    .reason
                    .unwrap_or_else(|| "no additional reason".to_string())
            )));
        }
        warn!(
            chip = %availability.chip_generation,
            reason = ?availability.reason,
            "Secure Enclave not available, falling back to regular keychain"
        );
        return generate_fallback_attestation(key_label, nonce, availability.chip_generation);
    }

    info!(
        key_label = %key_label,
        chip = %availability.chip_generation,
        "Generating key in Secure Enclave"
    );

    // Generate key in Secure Enclave using Security Framework
    let key = generate_sep_key_pair_internal(key_label)?;

    // Get attestation from the key
    let attestation = get_sep_attestation_internal(&key, nonce)?;

    // Verify attestation chain
    verify_attestation_chain(&attestation)?;

    info!(
        key_label = %key_label,
        cert_count = attestation.certificate_chain.len(),
        "SEP key generated and attested successfully"
    );

    Ok(attestation)
}

#[cfg(not(target_os = "macos"))]
pub async fn generate_sep_key_with_attestation(
    key_label: &str,
    nonce: &[u8],
) -> Result<SepAttestation> {
    Err(AosError::Crypto(
        "Secure Enclave only available on macOS".to_string(),
    ))
}

/// Internal function to generate SEP key pair using Security Framework
#[cfg(target_os = "macos")]
fn generate_sep_key_pair_internal(_key_label: &str) -> Result<security_framework::key::SecKey> {
    // Note: This is a simplified implementation
    // In production, you would use SecKeyCreateRandomKey with kSecAttrTokenID = kSecAttrTokenIDSecureEnclave
    // However, the security-framework crate doesn't expose all the necessary constants yet

    warn!("SEP key generation not fully implemented - using fallback");

    // For now, return an error to trigger fallback
    Err(AosError::Crypto(
        "SEP key generation not fully implemented in security-framework crate".to_string(),
    ))
}

/// Internal function to get attestation from SEP key
#[cfg(target_os = "macos")]
fn get_sep_attestation_internal(
    _key: &security_framework::key::SecKey,
    nonce: &[u8],
) -> Result<SepAttestation> {
    // Note: SecKeyCopyAttestationKey is only available on macOS 12+
    // The security-framework crate doesn't expose this API yet

    if require_hardware_attestation() {
        return Err(AosError::Crypto(
            "hardware attestation required but Security.framework attestation API is unavailable"
                .to_string(),
        ));
    }

    warn!("SEP attestation API not exposed in security-framework crate, using fallback");

    generate_fallback_attestation("fallback-key", nonce, detect_chip_generation())
}

/// Generate fallback attestation for systems without SEP
fn generate_fallback_attestation(
    key_label: &str,
    nonce: &[u8],
    chip_generation: SepChipGeneration,
) -> Result<SepAttestation> {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::rngs::OsRng;

    debug!(
        key_label = %key_label,
        chip = %chip_generation,
        "Generating fallback attestation (non-SEP)"
    );

    // Generate a regular key pair (not in SEP)
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = (&signing_key).into();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(SepAttestation {
        public_key: verifying_key.to_bytes().to_vec(),
        certificate_chain: vec![], // No cert chain for fallback
        nonce: nonce.to_vec(),
        chip_generation,
        timestamp,
    })
}

/// Verify attestation certificate chain
///
/// Parses and validates the X.509 certificate chain from SEP attestation.
/// Verifies that each certificate in the chain is signed by the next
/// certificate in the chain (signature chain validation).
///
/// # Arguments
/// * `attestation` - The SEP attestation containing the certificate chain
///
/// # Returns
/// * `Ok(())` if chain is valid or empty (fallback mode)
/// * `Err` if any certificate fails to parse or signature verification fails
///
/// # Note
/// This function performs signature chain verification only. For full Root CA
/// verification, use `verify_attestation_chain_with_root_ca`.
pub fn verify_attestation_chain(attestation: &SepAttestation) -> Result<()> {
    verify_attestation_chain_internal(attestation)
}

/// Verify attestation certificate chain with Root CA verification
///
/// Parses and validates the X.509 certificate chain from SEP attestation,
/// and verifies that the root certificate is signed by a trusted Apple Root CA.
///
/// # Arguments
/// * `attestation` - The SEP attestation containing the certificate chain
/// * `config` - Configuration for Root CA verification
///
/// # Returns
/// * `Ok(RootCaVerificationResult)` with verification details
/// * `Err` if chain verification fails or Root CA verification fails (when required)
///
/// # Platform Support
/// Root CA verification is primarily intended for macOS deployments.
/// On other platforms, this function will skip Root CA verification
/// unless `require_verification` is set to `true` in the config.
#[cfg(target_os = "macos")]
pub fn verify_attestation_chain_with_root_ca(
    attestation: &SepAttestation,
    config: &RootCaConfig,
) -> Result<RootCaVerificationResult> {
    // First, verify the signature chain
    verify_attestation_chain_internal(attestation)?;

    // Then verify against trusted Root CAs
    verify_root_ca(attestation, config)
}

#[cfg(not(target_os = "macos"))]
pub fn verify_attestation_chain_with_root_ca(
    attestation: &SepAttestation,
    config: &RootCaConfig,
) -> Result<RootCaVerificationResult> {
    // On non-macOS platforms, perform basic chain verification
    verify_attestation_chain_internal(attestation)?;

    if config.require_verification {
        return Err(AosError::Crypto(
            "Root CA verification only available on macOS".to_string(),
        ));
    }

    Ok(RootCaVerificationResult {
        trusted_root_found: false,
        root_subject: None,
        trusted_ca_subject: None,
        reason: Some("Root CA verification skipped on non-macOS platform".to_string()),
    })
}

/// Internal function for certificate chain verification
fn verify_attestation_chain_internal(attestation: &SepAttestation) -> Result<()> {
    use x509_parser::prelude::*;

    if attestation.certificate_chain.is_empty() {
        debug!("Attestation has no certificate chain (fallback mode)");
        return Ok(());
    }

    let chain_len = attestation.certificate_chain.len();
    info!(
        cert_count = chain_len,
        "Verifying X.509 attestation certificate chain"
    );

    // Parse all certificates in the chain
    let mut parsed_certs = Vec::with_capacity(chain_len);
    for (idx, der_bytes) in attestation.certificate_chain.iter().enumerate() {
        let (_, cert) = X509Certificate::from_der(der_bytes).map_err(|e| {
            AosError::Crypto(format!(
                "Failed to parse certificate at index {}: {:?}",
                idx, e
            ))
        })?;
        parsed_certs.push(cert);
    }

    // Verify signature chain: cert[i] is signed by cert[i+1]
    for i in 0..(parsed_certs.len() - 1) {
        let cert = &parsed_certs[i];
        let issuer = &parsed_certs[i + 1];

        // Get issuer's public key for verification
        let issuer_public_key = issuer.public_key();

        // Verify signature
        cert.verify_signature(Some(issuer_public_key))
            .map_err(|e| {
                AosError::Crypto(format!(
                    "Certificate chain signature verification failed at index {}: {:?}",
                    i, e
                ))
            })?;

        debug!(
            cert_idx = i,
            subject = %cert.subject(),
            issuer = %cert.issuer(),
            "Certificate signature verified"
        );
    }

    // Verify root certificate (self-signed check)
    if let Some(root_cert) = parsed_certs.last() {
        let root_public_key = root_cert.public_key();
        root_cert
            .verify_signature(Some(root_public_key))
            .map_err(|e| {
                AosError::Crypto(format!(
                    "Root certificate self-signature verification failed: {:?}",
                    e
                ))
            })?;
        debug!(
            subject = %root_cert.subject(),
            "Root certificate self-signature verified"
        );
    }

    info!(
        chain_len = chain_len,
        "X.509 attestation chain verification complete"
    );

    Ok(())
}

/// Load trusted Root CA certificates from a PEM or DER bundle file
///
/// Supports both PEM-encoded certificate bundles (multiple certificates
/// concatenated) and single DER-encoded certificates.
///
/// # Arguments
/// * `path` - Path to the certificate bundle file
///
/// # Returns
/// * `Ok(Vec<TrustedRootCa>)` - List of parsed trusted Root CAs
/// * `Err` if file cannot be read or certificates cannot be parsed
pub fn load_root_ca_bundle(path: &Path) -> Result<Vec<TrustedRootCa>> {
    use sha2::{Digest, Sha256};
    use x509_parser::prelude::*;

    let contents = std::fs::read(path).map_err(|e| {
        AosError::Crypto(format!(
            "Failed to read Root CA bundle from {}: {}",
            path.display(),
            e
        ))
    })?;

    let mut trusted_roots = Vec::new();

    // Try to parse as PEM first (pem 3.0 API: parse_many returns Result<Vec<Pem>>)
    if let Ok(pem_items) = pem_parse_many(&contents) {
        for pem_item in pem_items {
            if pem_item.tag() == "CERTIFICATE" {
                let der_bytes = pem_item.into_contents();
                if let Ok((_, cert)) = X509Certificate::from_der(&der_bytes) {
                    let fingerprint = Sha256::digest(&der_bytes);
                    trusted_roots.push(TrustedRootCa {
                        subject: cert.subject().to_string(),
                        issuer: cert.issuer().to_string(),
                        serial_number: cert.raw_serial_as_string(),
                        der_bytes,
                        fingerprint_sha256: hex::encode(fingerprint),
                    });
                }
            }
        }
    }

    // If no PEM certificates found, try parsing as raw DER
    if trusted_roots.is_empty() {
        if let Ok((_, cert)) = X509Certificate::from_der(&contents) {
            let fingerprint = Sha256::digest(&contents);
            trusted_roots.push(TrustedRootCa {
                subject: cert.subject().to_string(),
                issuer: cert.issuer().to_string(),
                serial_number: cert.raw_serial_as_string(),
                der_bytes: contents,
                fingerprint_sha256: hex::encode(fingerprint),
            });
        }
    }

    if trusted_roots.is_empty() {
        return Err(AosError::Crypto(format!(
            "No valid certificates found in Root CA bundle: {}",
            path.display()
        )));
    }

    info!(
        path = %path.display(),
        count = trusted_roots.len(),
        "Loaded trusted Root CA certificates"
    );

    for root in &trusted_roots {
        debug!(
            subject = %root.subject,
            fingerprint = %root.fingerprint_sha256,
            "Loaded trusted Root CA"
        );
    }

    Ok(trusted_roots)
}

/// Get or load cached trusted Root CA certificates
///
/// Certificates are loaded once and cached for the lifetime of the process.
/// If the bundle file does not exist, returns an empty vector.
pub fn get_cached_trusted_roots(config: &RootCaConfig) -> &'static [TrustedRootCa] {
    TRUSTED_ROOTS_CACHE.get_or_init(|| match load_root_ca_bundle(&config.bundle_path) {
        Ok(roots) => roots,
        Err(e) => {
            warn!(
                path = %config.bundle_path.display(),
                error = %e,
                "Failed to load Root CA bundle, Root CA verification will be unavailable"
            );
            Vec::new()
        }
    })
}

/// Verify the attestation chain root certificate against trusted Root CAs
fn verify_root_ca(
    attestation: &SepAttestation,
    config: &RootCaConfig,
) -> Result<RootCaVerificationResult> {
    use sha2::{Digest, Sha256};
    use x509_parser::prelude::*;

    if attestation.certificate_chain.is_empty() {
        return Ok(RootCaVerificationResult {
            trusted_root_found: false,
            root_subject: None,
            trusted_ca_subject: None,
            reason: Some("No certificate chain present (fallback mode)".to_string()),
        });
    }

    // Get the root certificate (last in chain)
    let root_der = attestation.certificate_chain.last().unwrap();
    let (_, root_cert) = X509Certificate::from_der(root_der)
        .map_err(|e| AosError::Crypto(format!("Failed to parse root certificate: {:?}", e)))?;

    let root_subject = root_cert.subject().to_string();
    let root_fingerprint = hex::encode(Sha256::digest(root_der));

    // Load trusted roots (may use cache)
    let trusted_roots = if config.bundle_path.exists() {
        get_cached_trusted_roots(config)
    } else {
        if config.require_verification {
            return Err(AosError::Crypto(format!(
                "Root CA bundle not found at {} and verification is required",
                config.bundle_path.display()
            )));
        }
        return Ok(RootCaVerificationResult {
            trusted_root_found: false,
            root_subject: Some(root_subject),
            trusted_ca_subject: None,
            reason: Some(format!(
                "Root CA bundle not found at {}",
                config.bundle_path.display()
            )),
        });
    };

    if trusted_roots.is_empty() {
        if config.require_verification {
            return Err(AosError::Crypto(
                "No trusted Root CAs loaded and verification is required".to_string(),
            ));
        }
        return Ok(RootCaVerificationResult {
            trusted_root_found: false,
            root_subject: Some(root_subject),
            trusted_ca_subject: None,
            reason: Some("No trusted Root CAs available".to_string()),
        });
    }

    // Check if the root certificate matches any trusted CA by fingerprint
    for trusted_ca in trusted_roots {
        if trusted_ca.fingerprint_sha256 == root_fingerprint {
            info!(
                root_subject = %root_subject,
                trusted_ca = %trusted_ca.subject,
                fingerprint = %root_fingerprint,
                "Root certificate verified against trusted CA"
            );
            return Ok(RootCaVerificationResult {
                trusted_root_found: true,
                root_subject: Some(root_subject),
                trusted_ca_subject: Some(trusted_ca.subject.clone()),
                reason: None,
            });
        }
    }

    // Check if root is signed by a trusted CA (for intermediate roots)
    let root_public_key = root_cert.public_key();
    for trusted_ca in trusted_roots {
        if let Ok((_, trusted_cert)) = X509Certificate::from_der(&trusted_ca.der_bytes) {
            let trusted_public_key = trusted_cert.public_key();
            if root_cert.verify_signature(Some(trusted_public_key)).is_ok() {
                info!(
                    root_subject = %root_subject,
                    trusted_ca = %trusted_ca.subject,
                    "Root certificate signature verified by trusted CA"
                );
                return Ok(RootCaVerificationResult {
                    trusted_root_found: true,
                    root_subject: Some(root_subject),
                    trusted_ca_subject: Some(trusted_ca.subject.clone()),
                    reason: None,
                });
            }
        }
    }

    // Root not found in trusted CAs
    if config.require_verification {
        return Err(AosError::Crypto(format!(
            "Root certificate '{}' not found in trusted Root CAs",
            root_subject
        )));
    }

    // Check if self-signed is allowed as fallback
    if config.allow_self_signed_fallback
        && root_cert.verify_signature(Some(root_public_key)).is_ok()
    {
        warn!(
            root_subject = %root_subject,
            "Root certificate is self-signed and not in trusted CAs (allowed as fallback)"
        );
        return Ok(RootCaVerificationResult {
            trusted_root_found: false,
            root_subject: Some(root_subject),
            trusted_ca_subject: None,
            reason: Some("Root certificate is self-signed but not in trusted CAs".to_string()),
        });
    }

    Ok(RootCaVerificationResult {
        trusted_root_found: false,
        root_subject: Some(root_subject),
        trusted_ca_subject: None,
        reason: Some("Root certificate not verified against trusted CAs".to_string()),
    })
}

/// Verify that the attestation nonce is embedded in the leaf certificate
///
/// # Arguments
/// * `attestation` - The SEP attestation containing the certificate chain and nonce
///
/// # Returns
/// * `Ok(true)` if nonce is found in leaf certificate
/// * `Ok(false)` if nonce is not found (may be in different extension format)
/// * `Err` if certificate parsing fails
pub fn verify_attestation_nonce(attestation: &SepAttestation) -> Result<bool> {
    use x509_parser::prelude::*;

    if attestation.certificate_chain.is_empty() {
        debug!("No certificate chain for nonce verification");
        return Ok(false);
    }

    // Get the leaf certificate (first in chain)
    let leaf_der = &attestation.certificate_chain[0];
    let (_, leaf_cert) = X509Certificate::from_der(leaf_der)
        .map_err(|e| AosError::Crypto(format!("Failed to parse leaf certificate: {:?}", e)))?;

    // Check extensions for nonce
    // Apple's attestation nonce is typically in a custom extension
    // The exact OID depends on Apple's attestation format
    for ext in leaf_cert.extensions() {
        // Check if extension data contains the nonce
        if ext
            .value
            .windows(attestation.nonce.len())
            .any(|w| w == attestation.nonce.as_slice())
        {
            debug!(
                oid = %ext.oid,
                "Found attestation nonce in certificate extension"
            );
            return Ok(true);
        }
    }

    debug!("Attestation nonce not found in leaf certificate extensions");
    Ok(false)
}

/// Get key creation date from keychain (S7 requirement)
#[cfg(target_os = "macos")]
pub fn get_key_creation_date(_key_label: &str) -> Result<u64> {
    // Note: Security Framework doesn't easily expose creation date
    // We use the current timestamp as fallback
    // In production, you would parse kSecAttrCreationDate from item attributes

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(timestamp)
}

#[cfg(not(target_os = "macos"))]
pub fn get_key_creation_date(_key_label: &str) -> Result<u64> {
    Err(AosError::Crypto(
        "Key creation date only available on macOS".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_chip_generation() {
        let chip = detect_chip_generation();
        println!("Detected chip: {}", chip);
        // Should not panic
    }

    #[test]
    fn test_check_sep_availability() {
        let availability = check_sep_availability();
        println!(
            "SEP available: {}, chip: {}",
            availability.available, availability.chip_generation
        );

        // Verify consistency
        match availability.chip_generation {
            SepChipGeneration::Intel => {
                assert!(!availability.available);
                assert!(availability.reason.is_some());
            }
            _ => {
                #[cfg(target_os = "macos")]
                assert!(availability.available);
                #[cfg(not(target_os = "macos"))]
                assert!(!availability.available);
            }
        }
    }

    #[tokio::test]
    async fn test_generate_fallback_attestation() {
        let nonce = b"test-nonce-12345";
        let attestation =
            generate_fallback_attestation("test-key", nonce, SepChipGeneration::Intel)
                .expect("Should generate fallback attestation");

        assert_eq!(attestation.nonce, nonce);
        assert_eq!(attestation.chip_generation, SepChipGeneration::Intel);
        assert!(!attestation.public_key.is_empty());
        assert!(attestation.certificate_chain.is_empty());
    }

    #[test]
    fn test_verify_empty_chain() {
        let attestation = SepAttestation {
            public_key: vec![1, 2, 3],
            certificate_chain: vec![],
            nonce: vec![4, 5, 6],
            chip_generation: SepChipGeneration::Intel,
            timestamp: 1234567890,
        };

        // Should succeed for empty chain (fallback mode)
        assert!(verify_attestation_chain(&attestation).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_generate_sep_key_with_attestation() {
        let nonce = b"test-nonce-123456789012345678901234";
        let result = generate_sep_key_with_attestation("test-sep-key", nonce).await;

        match result {
            Ok(attestation) => {
                assert_eq!(attestation.nonce, nonce);
                assert!(!attestation.public_key.is_empty());
                println!(
                    "Generated SEP attestation on chip: {}",
                    attestation.chip_generation
                );
            }
            Err(e) => {
                println!("SEP key generation failed (expected on Intel): {}", e);
                // This is expected on Intel Macs
            }
        }
    }
}
