//! Content-addressed artifact store with signing and SBOM

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

pub mod bundle;
pub mod cas;
pub mod replication;
pub mod sbom;
pub mod secd_client;
pub mod secureenclave;

pub use bundle::{create_bundle, extract_bundle};
pub use cas::CasStore;
pub use replication::{
    create_manifest, export_air_gap, import_air_gap, replicate_artifacts, verify_replication,
    ArtifactDescriptor, ChunkDescriptor, ReplicationManifest, ReplicationResult,
};
pub use secd_client::{default_secd_client, SecdClient};
pub use secureenclave::EnclaveError;

/// Signature metadata for a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureMetadata {
    pub bundle_hash: B3Hash,
    pub public_key: PublicKey,
    pub signature: Signature,
    pub signed_at: String,
}

impl SignatureMetadata {
    /// Verify the signature
    pub fn verify(&self, bundle_bytes: &[u8]) -> Result<()> {
        let hash = B3Hash::hash(bundle_bytes);
        if hash != self.bundle_hash {
            return Err(AosError::Crypto("Bundle hash mismatch".to_string()));
        }

        self.public_key.verify(bundle_bytes, &self.signature)
    }
}
