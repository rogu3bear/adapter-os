//! Cryptographic primitives for AdapterOS

pub mod envelope;
pub mod signature;

pub use envelope::{decrypt_envelope, encrypt_envelope};
pub use signature::{sign_bytes, verify_signature, Keypair, PublicKey, Signature};
