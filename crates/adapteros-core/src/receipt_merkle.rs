//! Receipt Merkle Tree for Patent 3535886.0002 Claim 12 (Batch Verification)
//!
//! Aggregates multiple inference receipts into Merkle trees for efficient
//! batch verification. Enables third parties to verify receipt authenticity
//! without accessing the full receipt chain.
//!
//! ## Design
//!
//! - Receipts are batched by tenant and time window
//! - Each batch computes a Merkle root over sorted receipt digests
//! - Inclusion proofs allow verifying individual receipts against batch root
//! - Batch roots can be published/attested without exposing receipt details
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_core::receipt_merkle::{ReceiptBatch, batch_receipts, generate_inclusion_proof};
//!
//! // Create batch from receipt digests
//! let receipts = vec![receipt1_digest, receipt2_digest, receipt3_digest];
//! let batch = batch_receipts("tenant-1", &receipts)?;
//!
//! // Generate proof for specific receipt
//! let proof = generate_inclusion_proof(&batch, 1)?;
//!
//! // Verify receipt is in batch
//! assert!(verify_inclusion(&proof, &receipt2_digest));
//! ```

use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

// =============================================================================
// Batch Types
// =============================================================================

/// A batch of receipts aggregated into a Merkle tree.
///
/// Per Patent 3535886.0002 Claim 12: Batch verification enables efficient
/// auditing of multiple inference runs without accessing full trace data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptBatch {
    /// Unique batch identifier (UUID)
    pub batch_id: String,

    /// Tenant namespace for multi-tenant isolation
    pub tenant_id: String,

    /// Receipt digests included in this batch (sorted by trace_id)
    pub receipt_digests: Vec<B3Hash>,

    /// Merkle root computed over receipt digests
    pub merkle_root: B3Hash,

    /// ISO 8601 timestamp when batch was created
    pub created_at: String,

    /// Number of receipts in batch
    pub receipt_count: usize,
}

/// Inclusion proof for a receipt in a batch.
///
/// Allows verification that a specific receipt digest is included in a
/// batch without revealing other receipts in the batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptInclusionProof {
    /// Index of the receipt in the sorted batch
    pub index: usize,

    /// Sibling hashes along the path to root
    pub siblings: Vec<B3Hash>,

    /// Merkle root of the batch
    pub merkle_root: B3Hash,

    /// Trace ID of the receipt being proven
    pub trace_id: String,

    /// Batch ID containing this receipt
    pub batch_id: String,
}

/// Metadata for a receipt within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMember {
    /// Batch this receipt belongs to
    pub batch_id: String,

    /// Trace ID of the inference run
    pub trace_id: String,

    /// Receipt digest (BLAKE3)
    pub receipt_digest: B3Hash,

    /// Position in the sorted batch (0-indexed)
    pub position: usize,
}

// =============================================================================
// Batch Creation
// =============================================================================

/// Create a receipt batch from a list of receipt digests.
///
/// Receipts are sorted by their hex representation for deterministic ordering,
/// then aggregated into a Merkle tree.
///
/// # Arguments
/// * `tenant_id` - Tenant namespace for the batch
/// * `receipt_digests` - List of receipt digests to batch
///
/// # Returns
/// A `ReceiptBatch` with computed Merkle root.
///
/// # Errors
/// Returns error if batch is empty.
pub fn batch_receipts(tenant_id: &str, receipt_digests: &[B3Hash]) -> Result<ReceiptBatch> {
    if receipt_digests.is_empty() {
        return Err(AosError::Config(
            "Cannot create batch from empty receipt list".to_string(),
        ));
    }

    // Sort by hex for deterministic ordering
    let mut sorted_digests = receipt_digests.to_vec();
    sorted_digests.sort_by_key(|a| a.to_hex());

    // Compute Merkle root
    let merkle_root = compute_receipt_merkle_root(&sorted_digests);

    // Generate batch ID
    let batch_id = uuid::Uuid::new_v4().to_string();

    Ok(ReceiptBatch {
        batch_id,
        tenant_id: tenant_id.to_string(),
        receipt_digests: sorted_digests,
        merkle_root,
        created_at: chrono::Utc::now().to_rfc3339(),
        receipt_count: receipt_digests.len(),
    })
}

/// Compute Merkle root over receipt digests.
///
/// Uses BLAKE3 for all hash computations. The tree is built bottom-up:
/// - Leaves are the receipt digests (already BLAKE3 hashes)
/// - Parent = BLAKE3(left || right)
/// - Odd leaves are duplicated
fn compute_receipt_merkle_root(digests: &[B3Hash]) -> B3Hash {
    if digests.is_empty() {
        return B3Hash::hash(b"empty_receipt_batch");
    }

    if digests.len() == 1 {
        return digests[0];
    }

    let mut level = digests.to_vec();

    while level.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..level.len()).step_by(2) {
            let left = &level[i];
            let right = if i + 1 < level.len() {
                &level[i + 1]
            } else {
                // Odd number: duplicate last
                left
            };

            // Parent = BLAKE3(left || right)
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(left.as_bytes());
            combined.extend_from_slice(right.as_bytes());
            next_level.push(B3Hash::hash(&combined));
        }

        level = next_level;
    }

    level[0]
}

// =============================================================================
// Inclusion Proofs
// =============================================================================

/// Generate an inclusion proof for a receipt at the given index.
///
/// The proof contains sibling hashes needed to recompute the Merkle root
/// from the receipt digest.
///
/// # Arguments
/// * `batch` - The receipt batch
/// * `index` - Index of the receipt in the batch (0-indexed)
///
/// # Returns
/// An inclusion proof that can be verified independently.
///
/// # Errors
/// Returns error if index is out of bounds.
pub fn generate_inclusion_proof(batch: &ReceiptBatch, index: usize) -> Result<ReceiptInclusionProof> {
    generate_inclusion_proof_with_trace_id(batch, index, &format!("receipt-{}", index))
}

/// Generate an inclusion proof with a specific trace ID.
pub fn generate_inclusion_proof_with_trace_id(
    batch: &ReceiptBatch,
    index: usize,
    trace_id: &str,
) -> Result<ReceiptInclusionProof> {
    if index >= batch.receipt_digests.len() {
        return Err(AosError::Config(format!(
            "Receipt index {} out of bounds (batch has {} receipts)",
            index,
            batch.receipt_digests.len()
        )));
    }

    let siblings = collect_siblings(&batch.receipt_digests, index);

    Ok(ReceiptInclusionProof {
        index,
        siblings,
        merkle_root: batch.merkle_root,
        trace_id: trace_id.to_string(),
        batch_id: batch.batch_id.clone(),
    })
}

/// Collect sibling hashes for inclusion proof.
fn collect_siblings(digests: &[B3Hash], target_index: usize) -> Vec<B3Hash> {
    let mut siblings = Vec::new();
    let mut level = digests.to_vec();
    let mut index = target_index;

    while level.len() > 1 {
        // Find sibling at current level
        let sibling_index = if index.is_multiple_of(2) {
            // Target is left child, sibling is right
            if index + 1 < level.len() {
                index + 1
            } else {
                // Odd case: sibling is self (duplicated)
                index
            }
        } else {
            // Target is right child, sibling is left
            index - 1
        };

        siblings.push(level[sibling_index]);

        // Build next level
        let mut next_level = Vec::new();
        for i in (0..level.len()).step_by(2) {
            let left = &level[i];
            let right = if i + 1 < level.len() {
                &level[i + 1]
            } else {
                left
            };

            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(left.as_bytes());
            combined.extend_from_slice(right.as_bytes());
            next_level.push(B3Hash::hash(&combined));
        }

        level = next_level;
        index /= 2;
    }

    siblings
}

// =============================================================================
// Verification
// =============================================================================

/// Verify that a receipt digest is included in a batch.
///
/// Recomputes the Merkle root from the receipt digest and proof siblings,
/// then compares against the claimed root.
///
/// # Arguments
/// * `proof` - The inclusion proof
/// * `receipt_digest` - The receipt digest to verify
///
/// # Returns
/// `true` if the receipt is included in the batch, `false` otherwise.
pub fn verify_inclusion(proof: &ReceiptInclusionProof, receipt_digest: &B3Hash) -> bool {
    let mut current = *receipt_digest;
    let mut index = proof.index;

    for sibling in &proof.siblings {
        let mut combined = Vec::with_capacity(64);

        if index.is_multiple_of(2) {
            // Current is left child
            combined.extend_from_slice(current.as_bytes());
            combined.extend_from_slice(sibling.as_bytes());
        } else {
            // Current is right child
            combined.extend_from_slice(sibling.as_bytes());
            combined.extend_from_slice(current.as_bytes());
        }

        current = B3Hash::hash(&combined);
        index /= 2;
    }

    current == proof.merkle_root
}

/// Verify a batch's Merkle root is correctly computed.
///
/// Recomputes the root from the receipt digests and compares.
pub fn verify_batch_root(batch: &ReceiptBatch) -> bool {
    let computed = compute_receipt_merkle_root(&batch.receipt_digests);
    computed == batch.merkle_root
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_digests(count: usize) -> Vec<B3Hash> {
        (0..count)
            .map(|i| B3Hash::hash(format!("receipt-{}", i).as_bytes()))
            .collect()
    }

    #[test]
    fn test_batch_creation() {
        let digests = sample_digests(5);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        assert_eq!(batch.tenant_id, "tenant-1");
        assert_eq!(batch.receipt_count, 5);
        assert_eq!(batch.receipt_digests.len(), 5);
        assert!(!batch.batch_id.is_empty());
        assert_ne!(batch.merkle_root, B3Hash::zero());
    }

    #[test]
    fn test_empty_batch_error() {
        let result = batch_receipts("tenant-1", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_single_receipt_batch() {
        let digests = sample_digests(1);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        // Single receipt batch: root equals the receipt digest
        assert_eq!(batch.merkle_root, digests[0]);
    }

    #[test]
    fn test_batch_determinism() {
        let digests = sample_digests(4);

        let batch1 = batch_receipts("tenant-1", &digests).unwrap();
        let batch2 = batch_receipts("tenant-1", &digests).unwrap();

        // Different batch IDs but same root
        assert_ne!(batch1.batch_id, batch2.batch_id);
        assert_eq!(batch1.merkle_root, batch2.merkle_root);
    }

    #[test]
    fn test_batch_sorted_determinism() {
        let mut digests = sample_digests(4);
        let batch1 = batch_receipts("tenant-1", &digests).unwrap();

        // Reverse order
        digests.reverse();
        let batch2 = batch_receipts("tenant-1", &digests).unwrap();

        // Should produce same root due to sorting
        assert_eq!(batch1.merkle_root, batch2.merkle_root);
    }

    #[test]
    fn test_inclusion_proof_valid() {
        let digests = sample_digests(4);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        for i in 0..4 {
            let proof = generate_inclusion_proof(&batch, i).unwrap();
            let receipt_digest = &batch.receipt_digests[i];

            assert!(
                verify_inclusion(&proof, receipt_digest),
                "Proof for index {} should be valid",
                i
            );
        }
    }

    #[test]
    fn test_inclusion_proof_invalid_digest() {
        let digests = sample_digests(4);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        let proof = generate_inclusion_proof(&batch, 0).unwrap();
        let wrong_digest = B3Hash::hash(b"wrong-receipt");

        assert!(!verify_inclusion(&proof, &wrong_digest));
    }

    #[test]
    fn test_inclusion_proof_out_of_bounds() {
        let digests = sample_digests(4);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        let result = generate_inclusion_proof(&batch, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_odd_number_of_receipts() {
        let digests = sample_digests(5);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        // All proofs should still be valid
        for i in 0..5 {
            let proof = generate_inclusion_proof(&batch, i).unwrap();
            assert!(verify_inclusion(&proof, &batch.receipt_digests[i]));
        }
    }

    #[test]
    fn test_verify_batch_root() {
        let digests = sample_digests(4);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        assert!(verify_batch_root(&batch));

        // Tamper with root
        let mut tampered = batch.clone();
        tampered.merkle_root = B3Hash::hash(b"tampered");
        assert!(!verify_batch_root(&tampered));
    }

    #[test]
    fn test_large_batch() {
        let digests = sample_digests(100);
        let batch = batch_receipts("tenant-1", &digests).unwrap();

        assert_eq!(batch.receipt_count, 100);

        // Verify a few random proofs
        for i in [0, 50, 99] {
            let proof = generate_inclusion_proof(&batch, i).unwrap();
            assert!(verify_inclusion(&proof, &batch.receipt_digests[i]));
        }
    }

    #[test]
    fn test_proof_serialization() {
        let digests = sample_digests(4);
        let batch = batch_receipts("tenant-1", &digests).unwrap();
        let proof = generate_inclusion_proof(&batch, 1).unwrap();

        // Serialize and deserialize
        let json = serde_json::to_string(&proof).unwrap();
        let parsed: ReceiptInclusionProof = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.index, proof.index);
        assert_eq!(parsed.merkle_root, proof.merkle_root);
        assert_eq!(parsed.siblings.len(), proof.siblings.len());
    }
}
