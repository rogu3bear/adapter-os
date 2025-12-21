//! Signing helpers for TraceBundle using adapteros-crypto

use adapteros_core::B3Hash;
use adapteros_crypto::{
    sign_bundle as crypto_sign_bundle, verify_bundle_from_file, BundleSignature, Keypair,
};

use crate::schema::{Event, TraceBundle};

/// Compute a simple Merkle-like root from event hashes (left-to-right),
/// hashing pairs until one root remains. For odd counts, the last hash is promoted.
pub fn compute_events_merkle_root(events: &[Event]) -> B3Hash {
    if events.is_empty() {
        return B3Hash::hash(&[]);
    }
    let mut layer: Vec<B3Hash> = events.iter().map(|e| e.blake3_hash).collect();
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        let mut i = 0;
        while i < layer.len() {
            let a = layer[i];
            let b = if i + 1 < layer.len() { layer[i + 1] } else { a };
            // Concatenate bytes of a and b and hash
            let mut bytes = Vec::with_capacity(64);
            bytes.extend_from_slice(a.as_bytes());
            bytes.extend_from_slice(b.as_bytes());
            next.push(B3Hash::hash(&bytes));
            i += 2;
        }
        layer = next;
    }
    layer[0]
}

/// Sign a TraceBundle's bundle_hash with Ed25519 and include merkle root of events.
pub fn sign_bundle(
    bundle: &TraceBundle,
    keypair: &Keypair,
) -> adapteros_core::Result<BundleSignature> {
    let merkle = compute_events_merkle_root(&bundle.events);
    crypto_sign_bundle(&bundle.bundle_hash, &merkle, keypair)
}

/// Verify a saved signature for the given bundle by looking up var/signatures/<hash>.sig
pub fn verify_bundle_signature_from_dir(
    bundle: &TraceBundle,
    signatures_dir: &std::path::Path,
) -> adapteros_core::Result<BundleSignature> {
    verify_bundle_from_file(&bundle.bundle_hash, signatures_dir)
}
