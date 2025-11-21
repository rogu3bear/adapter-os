//! Cryptographic primitives for AdapterOS

pub mod bundle_sign;
pub mod envelope;
pub mod key_provider;
pub mod providers;
pub mod secret;
pub mod signature;

pub use bundle_sign::{
    compute_key_id, generate_signing_key, load_signing_key, sign_and_save_bundle, sign_bundle,
    verify_bundle_from_file, BundleSignature,
};
pub use envelope::{decrypt_envelope, encrypt_envelope};
pub use key_provider::{
    KeyAlgorithm, KeyHandle, KeyProvider, KeyProviderConfig, KeyProviderMode, ProviderAttestation,
    RotationReceipt,
};
pub use providers::keychain::KeychainProvider;
pub use secret::{KeyMaterial, SecretKey, SensitiveData};
pub use signature::{sign_bytes, verify_signature, Keypair, PublicKey, Signature};

// Re-export ed25519-dalek types for node agent
pub use ed25519_dalek::SigningKey;
