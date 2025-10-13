//! Cryptographic primitives for AdapterOS

pub mod bundle_sign;
pub mod envelope;
pub mod signature;

pub use bundle_sign::{
    compute_key_id, generate_signing_key, load_signing_key, sign_and_save_bundle, sign_bundle,
    verify_bundle_from_file, BundleSignature,
};
pub use envelope::{decrypt_envelope, encrypt_envelope};
pub use signature::{sign_bytes, verify_signature, Keypair, PublicKey, Signature};
