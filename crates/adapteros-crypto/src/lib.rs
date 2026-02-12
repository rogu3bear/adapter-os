//! Cryptographic primitives for adapterOS

pub mod audit;
pub mod bundle_sign;
pub mod checkpoint_verify;
pub mod decision_chain;
pub mod envelope;
pub mod key_manager;
pub mod key_provider;
pub mod policy_enforcement;
pub mod provenance;
pub mod providers;
pub mod receipt_signing;
pub mod receipt_verifier;
pub mod rotation_daemon;
pub mod secret;
pub mod sep_attestation;
pub mod signature;

pub use audit::{CryptoAuditEntry, CryptoAuditLogger, CryptoOperation, OperationResult};
pub use bundle_sign::{
    compute_key_id, generate_signing_key, load_signing_key, sign_and_save_bundle, sign_bundle,
    verify_bundle_from_file, BundleSignature,
};
pub use envelope::{decrypt_envelope, encrypt_envelope};
pub use key_manager::{KeyManager, KeyManagerConfig};
pub use key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, KeyProviderMode, ProviderAttestation,
    RotationReceipt,
};
pub use policy_enforcement::{CryptoPolicy, CryptoPolicyEnforcer, PolicyViolation, ViolationType};
pub use providers::file::FileProvider;
pub use providers::keychain::KeychainProvider;
pub use rotation_daemon::{
    CryptoStore, EncryptedDekEntry, RotationDaemon, RotationHistoryEntry, RotationPolicy,
    RotationReason,
};
pub use secret::{KeyMaterial, SecretKey, SensitiveData};
pub use sep_attestation::{
    check_sep_availability, detect_chip_generation, generate_sep_key_with_attestation,
    get_cached_trusted_roots, get_key_creation_date, get_root_ca_path, load_root_ca_bundle,
    verify_attestation_chain, verify_attestation_chain_with_root_ca, verify_attestation_nonce,
    RootCaConfig, RootCaVerificationResult, SepAttestation, SepAvailability, SepChipGeneration,
    TrustedRootCa, DEFAULT_ROOT_CA_PATH, ROOT_CA_PATH_ENV,
};
pub use signature::{sign_bytes, verify_signature, Keypair, PublicKey, Signature};

pub use receipt_signing::{
    sign_receipt_digest, sign_receipt_digest_bytes, SignedReceipt, SigningMode,
};

pub use receipt_verifier::{
    verify_bundle_bytes, verify_receipt_payload_bytes, DigestComparison,
    ReceiptPayloadVerifyResult, ReceiptVerificationReport, VerifyOptions,
};

// Back-compat alias used by vector tests and callers.
pub use receipt_verifier::ReasonCode as ReceiptVerifyReasonCode;

pub use decision_chain::{
    verify_bundle_commits, DecisionChainBuilder, EnvironmentIdentity, MerkleBundleCommits,
    RouterEventDigest,
};

pub use checkpoint_verify::{
    verify_checkpoint_bytes, verify_checkpoint_file, CheckpointVerifyReport,
};

pub use provenance::{
    ChainCompleteness, ProvenanceCertificate, ProvenanceCertificateBuilder, ProvenanceVerifyReport,
    PROVENANCE_SCHEMA_VERSION,
};

// Re-export ed25519-dalek types for node agent
pub use ed25519_dalek::SigningKey;
