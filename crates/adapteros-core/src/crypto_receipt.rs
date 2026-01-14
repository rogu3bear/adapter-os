//! Cryptographic Receipt Generation for Inference Runs
//!
//! This module produces a single digest that cryptographically binds all inputs,
//! configuration, routing decisions, and outputs for an inference run. The receipt
//! serves as a verifiable proof that a specific output was produced under specific
//! conditions. Any modification to any bound value produces a different digest.
//!
//! This enables third-party verification without access to the inference system.
//!
//! # Receipt Structure
//!
//! The receipt binds together:
//! - **context_id**: Derived from model hash and adapter configuration
//! - **input_digest**: BLAKE3 hash of the input token sequence
//! - **routing_digest**: Finalized hash from accumulated per-token routing records
//! - **output_digest**: BLAKE3 hash of the generated token sequence
//! - **equipment_profile_digest**: Hash of processor ID and engine version
//!
//! # Final Digest Computation
//!
//! ```text
//! receipt_digest = BLAKE3(context_id || input_digest || routing_digest || output_digest || equipment_profile_digest)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use adapteros_core::crypto_receipt::{CryptographicReceipt, ReceiptGenerator};
//! use adapteros_core::B3Hash;
//!
//! // Initialize generator with context
//! let mut generator = ReceiptGenerator::new(
//!     model_hash,
//!     adapter_config_hash,
//! );
//!
//! // Set equipment profile
//! generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
//!
//! // Bind input tokens
//! generator.bind_input_tokens(&input_tokens);
//!
//! // Accumulate routing decisions during generation
//! for (step, decision) in routing_decisions.iter().enumerate() {
//!     generator.record_routing_decision(step as u32, decision);
//! }
//!
//! // Finalize with output tokens
//! let receipt = generator.finalize(&output_tokens)?;
//!
//! // Receipt can be verified by third parties
//! assert!(receipt.verify());
//! ```

use crate::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

// ============================================================================
// Schema Version
// ============================================================================

/// Current schema version for cryptographic receipts
pub const CRYPTO_RECEIPT_SCHEMA_VERSION: u8 = 1;

// ============================================================================
// Component Digests
// ============================================================================

/// Context identifier derived from model hash and adapter configuration.
///
/// This binds the receipt to the specific model and adapter stack used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextId {
    /// BLAKE3 hash of the base model weights
    pub model_hash: B3Hash,
    /// BLAKE3 hash of the adapter configuration (stack, ranks, alphas)
    pub adapter_config_hash: B3Hash,
    /// Combined context identifier
    pub digest: B3Hash,
}

impl ContextId {
    /// Compute context_id from model hash and adapter configuration.
    ///
    /// Formula: `context_id = BLAKE3(model_hash || adapter_config_hash)`
    pub fn compute(model_hash: B3Hash, adapter_config_hash: B3Hash) -> Self {
        let digest = B3Hash::hash_multi(&[model_hash.as_bytes(), adapter_config_hash.as_bytes()]);
        Self {
            model_hash,
            adapter_config_hash,
            digest,
        }
    }

    /// Get the combined context identifier
    pub fn as_hash(&self) -> &B3Hash {
        &self.digest
    }
}

/// Equipment profile identifying the processor and engine version.
///
/// This ensures receipts are bound to specific hardware/software configurations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentProfile {
    /// Processor identifier (e.g., "Apple M4 Max:stepping-1")
    pub processor_id: String,
    /// Inference engine version (e.g., "mlx-0.21.0")
    pub engine_version: String,
    /// Optional ANE version for Apple Neural Engine
    pub ane_version: Option<String>,
    /// Computed equipment profile digest
    pub digest: B3Hash,
}

impl EquipmentProfile {
    /// Compute equipment profile digest from processor ID and engine version.
    ///
    /// Formula: `equipment_digest = BLAKE3(processor_id || '\0' || engine_version || '\0' || ane_version)`
    pub fn compute(processor_id: &str, engine_version: &str, ane_version: Option<&str>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(processor_id.as_bytes());
        hasher.update(b"\x00"); // null separator
        hasher.update(engine_version.as_bytes());
        hasher.update(b"\x00");
        hasher.update(ane_version.unwrap_or("none").as_bytes());
        let digest = B3Hash::from_bytes(hasher.finalize().into());

        Self {
            processor_id: processor_id.to_string(),
            engine_version: engine_version.to_string(),
            ane_version: ane_version.map(String::from),
            digest,
        }
    }

    /// Get the equipment profile digest
    pub fn as_hash(&self) -> &B3Hash {
        &self.digest
    }
}

/// Single routing record for one token generation step.
///
/// Records the adapter selection decision at each step for reproducibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRecord {
    /// Token generation step index (0-based)
    pub step: u32,
    /// Input token ID that triggered this routing decision
    pub input_token_id: Option<u32>,
    /// Selected adapter indices
    pub adapter_indices: Vec<u16>,
    /// Selected adapter IDs (string identifiers)
    #[serde(default)]
    pub adapter_ids: Vec<String>,
    /// Quantized gate values (Q15 format)
    pub gates_q15: Vec<i16>,
    /// Routing entropy at this step
    pub entropy: f32,
    /// Policy mask digest (if policy was applied)
    pub policy_mask_digest: Option<B3Hash>,
    /// Backend identifier (e.g., "metal", "coreml", "mlx")
    #[serde(default)]
    pub backend_id: Option<String>,
    /// Kernel version identifier for the backend
    #[serde(default)]
    pub kernel_version_id: Option<String>,
    /// Allowed mask for policy filtering
    #[serde(default)]
    pub allowed_mask: Option<Vec<bool>>,
}

impl RoutingRecord {
    /// Compute hash of this routing record for chain accumulation.
    ///
    /// **IMPORTANT**: This method uses a standalone serialization format that is
    /// NOT compatible with the production trace system (`SqlTraceSink`). For
    /// replay verification compatibility, use [`compute_hash_canonical`] instead,
    /// which matches the `receipt_digest::hash_token_decision` algorithm.
    ///
    /// The hash includes all fields that affect routing determinism:
    /// - Step index and input token
    /// - Adapter indices and string IDs
    /// - Gate values (Q15)
    /// - Entropy value
    /// - Policy mask digest
    /// - Backend and kernel version
    /// - Allowed mask (if policy filtering applied)
    #[deprecated(
        since = "0.2.0",
        note = "Use compute_hash_canonical() for replay verification compatibility"
    )]
    pub fn compute_hash(&self) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Step index
        hasher.update(&self.step.to_le_bytes());

        // Input token (with presence marker)
        match self.input_token_id {
            Some(token) => {
                hasher.update(&[1u8]);
                hasher.update(&token.to_le_bytes());
            }
            None => {
                hasher.update(&[0u8]);
            }
        }

        // Adapter indices (length-prefixed)
        hasher.update(&(self.adapter_indices.len() as u32).to_le_bytes());
        for idx in &self.adapter_indices {
            hasher.update(&idx.to_le_bytes());
        }

        // Adapter IDs (length-prefixed strings)
        hasher.update(&(self.adapter_ids.len() as u32).to_le_bytes());
        for id in &self.adapter_ids {
            let bytes = id.as_bytes();
            hasher.update(&(bytes.len() as u32).to_le_bytes());
            hasher.update(bytes);
        }

        // Gates (length-prefixed Q15 values)
        hasher.update(&(self.gates_q15.len() as u32).to_le_bytes());
        for gate in &self.gates_q15 {
            hasher.update(&gate.to_le_bytes());
        }

        // Entropy (as raw f32 bytes for determinism)
        hasher.update(&self.entropy.to_le_bytes());

        // Policy mask digest (with presence marker)
        match &self.policy_mask_digest {
            Some(digest) => {
                hasher.update(&[1u8]);
                hasher.update(digest.as_bytes());
            }
            None => {
                hasher.update(&[0u8]);
            }
        }

        // Backend ID (with presence marker)
        match &self.backend_id {
            Some(id) => {
                hasher.update(&[1u8]);
                let bytes = id.as_bytes();
                hasher.update(&(bytes.len() as u32).to_le_bytes());
                hasher.update(bytes);
            }
            None => {
                hasher.update(&[0u8]);
            }
        }

        // Kernel version ID (with presence marker)
        match &self.kernel_version_id {
            Some(id) => {
                hasher.update(&[1u8]);
                let bytes = id.as_bytes();
                hasher.update(&(bytes.len() as u32).to_le_bytes());
                hasher.update(bytes);
            }
            None => {
                hasher.update(&[0u8]);
            }
        }

        // Allowed mask (with presence marker)
        match &self.allowed_mask {
            Some(mask) => {
                hasher.update(&[1u8]);
                hasher.update(&(mask.len() as u32).to_le_bytes());
                for &allowed in mask {
                    hasher.update(&[if allowed { 1u8 } else { 0u8 }]);
                }
            }
            None => {
                hasher.update(&[0u8]);
            }
        }

        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Compute hash of this routing record using the canonical algorithm.
    ///
    /// This method produces hashes compatible with the production trace system
    /// (`SqlTraceSink::hash_decision`) and replay verification. It uses the
    /// canonical `receipt_digest::hash_token_decision` function internally.
    ///
    /// # Arguments
    /// * `context_digest` - The context digest (model + adapter config hash) that
    ///   binds this routing decision to its inference context.
    ///
    /// # Compatibility
    ///
    /// This method produces identical hashes to `SqlTraceSink::hash_decision`
    /// when given the same inputs, enabling cross-system verification.
    pub fn compute_hash_canonical(&self, context_digest: &[u8; 32]) -> B3Hash {
        use crate::receipt_digest::{encode_adapter_ids, encode_allowed_mask, encode_gates_q15, hash_token_decision};

        // Encode fields to canonical blob format
        let adapter_ids_blob = encode_adapter_ids(&self.adapter_ids);
        let gates_blob = encode_gates_q15(&self.gates_q15);
        let allowed_mask_blob = self.allowed_mask.as_ref().map(|m| encode_allowed_mask(m));

        // Convert policy_mask_digest to [u8; 32] if present
        let policy_mask_digest = self.policy_mask_digest.map(|h| *h.as_bytes());

        // Note: policy_overrides_json is not present in RoutingRecord,
        // so we pass None for compatibility with traces that don't have it.
        hash_token_decision(
            context_digest,
            self.step,
            &adapter_ids_blob,
            &gates_blob,
            policy_mask_digest,
            allowed_mask_blob.as_deref(),
            None, // policy_overrides_json not in RoutingRecord
            self.backend_id.as_deref(),
            self.kernel_version_id.as_deref(),
        )
    }

    /// Create a new routing record with minimal required fields.
    pub fn new(step: u32, adapter_indices: Vec<u16>, gates_q15: Vec<i16>, entropy: f32) -> Self {
        Self {
            step,
            input_token_id: None,
            adapter_indices,
            adapter_ids: Vec::new(),
            gates_q15,
            entropy,
            policy_mask_digest: None,
            backend_id: None,
            kernel_version_id: None,
            allowed_mask: None,
        }
    }

    /// Builder method to set input token ID.
    pub fn with_input_token(mut self, token_id: u32) -> Self {
        self.input_token_id = Some(token_id);
        self
    }

    /// Builder method to set adapter IDs.
    pub fn with_adapter_ids(mut self, ids: Vec<String>) -> Self {
        self.adapter_ids = ids;
        self
    }

    /// Builder method to set policy mask digest.
    pub fn with_policy_mask(mut self, digest: B3Hash) -> Self {
        self.policy_mask_digest = Some(digest);
        self
    }

    /// Builder method to set backend information.
    pub fn with_backend(mut self, backend_id: &str, kernel_version_id: Option<&str>) -> Self {
        self.backend_id = Some(backend_id.to_string());
        self.kernel_version_id = kernel_version_id.map(String::from);
        self
    }

    /// Builder method to set allowed mask.
    pub fn with_allowed_mask(mut self, mask: Vec<bool>) -> Self {
        self.allowed_mask = Some(mask);
        self
    }
}

/// Accumulated routing digest from per-token routing records.
///
/// Uses a hash chain to accumulate routing decisions in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDigest {
    /// Number of routing decisions accumulated
    pub decision_count: u32,
    /// Final chained hash over all routing records
    pub digest: B3Hash,
}

impl RoutingDigest {
    /// Start a new routing digest accumulator with zero hash.
    pub fn new() -> Self {
        Self {
            decision_count: 0,
            digest: B3Hash::zero(),
        }
    }

    /// Accumulate a routing record into the chain (legacy format).
    ///
    /// **IMPORTANT**: This method uses a non-canonical chain order that is
    /// NOT compatible with the production trace system. For replay verification
    /// compatibility, use [`accumulate_canonical`] instead.
    ///
    /// Formula: `new_digest = BLAKE3(prev_digest || step || record_hash)`
    #[deprecated(
        since = "0.2.0",
        note = "Use accumulate_canonical() for replay verification compatibility"
    )]
    #[allow(deprecated)]
    pub fn accumulate(&mut self, record: &RoutingRecord) {
        let record_hash = record.compute_hash();
        self.digest = B3Hash::hash_multi(&[
            self.digest.as_bytes(),
            &record.step.to_le_bytes(),
            record_hash.as_bytes(),
        ]);
        self.decision_count += 1;
    }

    /// Accumulate a routing record into the chain using the canonical algorithm.
    ///
    /// This method produces chain hashes compatible with the production trace
    /// system (`SqlTraceSink::update_head`) and replay verification. It uses
    /// the canonical `receipt_digest::update_run_head` function internally.
    ///
    /// Formula: `new_digest = BLAKE3(prev_digest || decision_hash || token_index)`
    ///
    /// # Arguments
    /// * `record` - The routing record for this token
    /// * `context_digest` - The context digest that binds this to its inference context
    ///
    /// # Compatibility
    ///
    /// This method produces identical chain hashes to `SqlTraceSink::update_head`
    /// when given the same inputs, enabling cross-system verification.
    pub fn accumulate_canonical(&mut self, record: &RoutingRecord, context_digest: &[u8; 32]) {
        use crate::receipt_digest::update_run_head;

        let record_hash = record.compute_hash_canonical(context_digest);
        self.digest = update_run_head(&self.digest, record.step, &record_hash);
        self.decision_count += 1;
    }

    /// Get the finalized routing digest
    pub fn as_hash(&self) -> &B3Hash {
        &self.digest
    }
}

impl Default for RoutingDigest {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Input/Output Token Digests
// ============================================================================

/// Compute input digest over input token sequence using BLAKE3.
///
/// Format: length-prefixed array of u32 little-endian token IDs.
pub fn compute_input_digest(input_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + input_tokens.len() * 4);
    buf.extend_from_slice(&(input_tokens.len() as u32).to_le_bytes());
    for token in input_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

/// Compute output digest over generated token sequence using BLAKE3.
///
/// Format: length-prefixed array of u32 little-endian token IDs.
pub fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for token in output_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

// ============================================================================
// Cryptographic Receipt
// ============================================================================

/// Complete cryptographic receipt binding all inference run components.
///
/// The receipt serves as a verifiable proof that a specific output was produced
/// under specific conditions. Any modification to any bound value produces a
/// different digest, enabling third-party verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptographicReceipt {
    /// Schema version for forward compatibility
    pub schema_version: u8,

    /// Context identifier (model + adapter configuration)
    pub context_id: ContextId,

    /// BLAKE3 hash of input token sequence
    pub input_digest: B3Hash,

    /// Accumulated hash of per-token routing decisions
    pub routing_digest: RoutingDigest,

    /// BLAKE3 hash of generated token sequence
    pub output_digest: B3Hash,

    /// Equipment profile (processor + engine version)
    pub equipment_profile: EquipmentProfile,

    /// Final receipt digest binding all components
    pub receipt_digest: B3Hash,

    /// Optional metadata for audit purposes
    #[serde(default)]
    pub metadata: ReceiptMetadata,
}

/// Metadata attached to a receipt for audit and debugging.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptMetadata {
    /// Tenant ID for multi-tenant isolation
    pub tenant_id: Option<String>,
    /// Trace ID for correlation
    pub trace_id: Option<String>,
    /// Timestamp when receipt was generated (RFC3339)
    pub created_at: Option<String>,
    /// Number of input tokens
    pub input_token_count: u32,
    /// Number of output tokens generated
    pub output_token_count: u32,
    /// Stop reason code
    pub stop_reason: Option<String>,
}

impl CryptographicReceipt {
    /// Compute the final receipt digest from all component digests.
    ///
    /// Formula:
    /// ```text
    /// receipt_digest = BLAKE3(
    ///     schema_version ||
    ///     context_id ||
    ///     input_digest ||
    ///     routing_digest ||
    ///     output_digest ||
    ///     equipment_profile_digest
    /// )
    /// ```
    fn compute_receipt_digest(
        context_id: &ContextId,
        input_digest: &B3Hash,
        routing_digest: &RoutingDigest,
        output_digest: &B3Hash,
        equipment_profile: &EquipmentProfile,
    ) -> B3Hash {
        B3Hash::hash_multi(&[
            &[CRYPTO_RECEIPT_SCHEMA_VERSION],
            context_id.digest.as_bytes(),
            input_digest.as_bytes(),
            routing_digest.digest.as_bytes(),
            output_digest.as_bytes(),
            equipment_profile.digest.as_bytes(),
        ])
    }

    /// Verify the receipt digest matches the component digests.
    ///
    /// Returns true if the receipt is internally consistent.
    pub fn verify(&self) -> bool {
        let expected = Self::compute_receipt_digest(
            &self.context_id,
            &self.input_digest,
            &self.routing_digest,
            &self.output_digest,
            &self.equipment_profile,
        );
        self.receipt_digest == expected
    }

    /// Get the final receipt digest for external verification.
    pub fn digest(&self) -> &B3Hash {
        &self.receipt_digest
    }

    /// Export receipt as canonical bytes for signing or storage.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);

        // Schema version
        buf.push(self.schema_version);

        // Context ID components
        buf.extend_from_slice(self.context_id.model_hash.as_bytes());
        buf.extend_from_slice(self.context_id.adapter_config_hash.as_bytes());
        buf.extend_from_slice(self.context_id.digest.as_bytes());

        // Input digest
        buf.extend_from_slice(self.input_digest.as_bytes());

        // Routing digest
        buf.extend_from_slice(&self.routing_digest.decision_count.to_le_bytes());
        buf.extend_from_slice(self.routing_digest.digest.as_bytes());

        // Output digest
        buf.extend_from_slice(self.output_digest.as_bytes());

        // Equipment profile
        encode_str(&mut buf, &self.equipment_profile.processor_id);
        encode_str(&mut buf, &self.equipment_profile.engine_version);
        encode_optional_str(&mut buf, self.equipment_profile.ane_version.as_deref());
        buf.extend_from_slice(self.equipment_profile.digest.as_bytes());

        // Final receipt digest
        buf.extend_from_slice(self.receipt_digest.as_bytes());

        buf
    }
}

// ============================================================================
// Receipt Generator (Builder Pattern)
// ============================================================================

/// Builder for generating cryptographic receipts during inference.
///
/// Accumulates routing decisions as tokens are generated, then finalizes
/// the receipt with output tokens when generation completes.
#[derive(Debug)]
pub struct ReceiptGenerator {
    /// Context ID from model and adapter configuration
    context_id: ContextId,
    /// Equipment profile
    equipment_profile: Option<EquipmentProfile>,
    /// Accumulated routing digest
    routing_digest: RoutingDigest,
    /// Input digest (set when input tokens are bound)
    input_digest: Option<B3Hash>,
    /// Metadata for the receipt
    metadata: ReceiptMetadata,
    /// Whether generation has completed
    finalized: bool,
}

impl ReceiptGenerator {
    /// Create a new receipt generator with model and adapter configuration.
    ///
    /// # Arguments
    /// * `model_hash` - BLAKE3 hash of the base model weights
    /// * `adapter_config_hash` - BLAKE3 hash of the adapter configuration
    pub fn new(model_hash: B3Hash, adapter_config_hash: B3Hash) -> Self {
        let context_id = ContextId::compute(model_hash, adapter_config_hash);
        Self {
            context_id,
            equipment_profile: None,
            routing_digest: RoutingDigest::new(),
            input_digest: None,
            metadata: ReceiptMetadata::default(),
            finalized: false,
        }
    }

    /// Set the equipment profile for this receipt.
    ///
    /// # Arguments
    /// * `processor_id` - Processor identifier (e.g., "Apple M4 Max")
    /// * `engine_version` - Inference engine version (e.g., "mlx-0.21.0")
    pub fn set_equipment_profile(&mut self, processor_id: &str, engine_version: &str) {
        self.equipment_profile = Some(EquipmentProfile::compute(
            processor_id,
            engine_version,
            None,
        ));
    }

    /// Set the equipment profile with ANE version.
    pub fn set_equipment_profile_with_ane(
        &mut self,
        processor_id: &str,
        engine_version: &str,
        ane_version: &str,
    ) {
        self.equipment_profile = Some(EquipmentProfile::compute(
            processor_id,
            engine_version,
            Some(ane_version),
        ));
    }

    /// Set equipment profile from an existing EquipmentProfile.
    pub fn with_equipment_profile(mut self, profile: EquipmentProfile) -> Self {
        self.equipment_profile = Some(profile);
        self
    }

    /// Bind input tokens to this receipt.
    ///
    /// This computes the input_digest over the input token sequence using BLAKE3.
    /// Must be called before recording routing decisions.
    pub fn bind_input_tokens(&mut self, input_tokens: &[u32]) {
        self.input_digest = Some(compute_input_digest(input_tokens));
        self.metadata.input_token_count = input_tokens.len() as u32;
    }

    /// Record a routing decision for a single token generation step.
    ///
    /// This accumulates the routing record into the routing_digest chain
    /// using the canonical algorithm that matches `SqlTraceSink`.
    /// Records should be added in step order (0, 1, 2, ...).
    pub fn record_routing_decision(&mut self, record: RoutingRecord) {
        let context_digest = self.context_id.digest.as_bytes();
        self.routing_digest.accumulate_canonical(&record, context_digest);
    }

    /// Record a routing decision with direct parameters.
    ///
    /// Convenience method that constructs a RoutingRecord internally.
    #[allow(clippy::too_many_arguments)]
    pub fn record_routing_step(
        &mut self,
        step: u32,
        input_token_id: Option<u32>,
        adapter_indices: Vec<u16>,
        gates_q15: Vec<i16>,
        entropy: f32,
        policy_mask_digest: Option<B3Hash>,
    ) {
        let record = RoutingRecord {
            step,
            input_token_id,
            adapter_indices,
            adapter_ids: Vec::new(),
            gates_q15,
            entropy,
            policy_mask_digest,
            backend_id: None,
            kernel_version_id: None,
            allowed_mask: None,
        };
        self.record_routing_decision(record);
    }

    /// Record a routing decision with full details including backend info.
    ///
    /// This method matches the fields available in `SqlTraceSink.record_token()`.
    #[allow(clippy::too_many_arguments)]
    pub fn record_routing_step_full(
        &mut self,
        step: u32,
        input_token_id: Option<u32>,
        adapter_indices: Vec<u16>,
        adapter_ids: Vec<String>,
        gates_q15: Vec<i16>,
        entropy: f32,
        policy_mask_digest: Option<B3Hash>,
        backend_id: Option<String>,
        kernel_version_id: Option<String>,
        allowed_mask: Option<Vec<bool>>,
    ) {
        let record = RoutingRecord {
            step,
            input_token_id,
            adapter_indices,
            adapter_ids,
            gates_q15,
            entropy,
            policy_mask_digest,
            backend_id,
            kernel_version_id,
            allowed_mask,
        };
        self.record_routing_decision(record);
    }

    /// Set metadata fields for the receipt.
    pub fn set_metadata(&mut self, metadata: ReceiptMetadata) {
        self.metadata = metadata;
    }

    /// Set tenant ID for multi-tenant isolation.
    pub fn set_tenant_id(&mut self, tenant_id: &str) {
        self.metadata.tenant_id = Some(tenant_id.to_string());
    }

    /// Set trace ID for correlation.
    pub fn set_trace_id(&mut self, trace_id: &str) {
        self.metadata.trace_id = Some(trace_id.to_string());
    }

    /// Set stop reason code.
    pub fn set_stop_reason(&mut self, stop_reason: &str) {
        self.metadata.stop_reason = Some(stop_reason.to_string());
    }

    /// Finalize the receipt with output tokens.
    ///
    /// This computes the output_digest and final receipt_digest, returning
    /// the complete CryptographicReceipt.
    ///
    /// # Stop Conditions
    /// - All component digests have been computed
    /// - Generation has terminated (any stop condition)
    ///
    /// # Next Conditions
    /// - Return receipt with inference response
    /// - Store receipt in audit log with tenant binding
    ///
    /// # Errors
    /// Returns an error if:
    /// - Input tokens were not bound (call `bind_input_tokens` first)
    /// - Equipment profile was not set (call `set_equipment_profile` first)
    /// - Generator was already finalized
    pub fn finalize(mut self, output_tokens: &[u32]) -> Result<CryptographicReceipt> {
        if self.finalized {
            return Err(AosError::Validation(
                "Receipt generator already finalized".to_string(),
            ));
        }

        let input_digest = self.input_digest.ok_or_else(|| {
            AosError::Validation(
                "Input tokens not bound. Call bind_input_tokens first.".to_string(),
            )
        })?;

        let equipment_profile = self.equipment_profile.ok_or_else(|| {
            AosError::Validation(
                "Equipment profile not set. Call set_equipment_profile first.".to_string(),
            )
        })?;

        // Compute output digest
        let output_digest = compute_output_digest(output_tokens);
        self.metadata.output_token_count = output_tokens.len() as u32;

        // Note: created_at is left as-is (caller can set via metadata if needed)
        // This avoids coupling to chrono and maintains testability

        // Compute final receipt digest
        let receipt_digest = CryptographicReceipt::compute_receipt_digest(
            &self.context_id,
            &input_digest,
            &self.routing_digest,
            &output_digest,
            &equipment_profile,
        );

        self.finalized = true;

        Ok(CryptographicReceipt {
            schema_version: CRYPTO_RECEIPT_SCHEMA_VERSION,
            context_id: self.context_id,
            input_digest,
            routing_digest: self.routing_digest,
            output_digest,
            equipment_profile,
            receipt_digest,
            metadata: self.metadata,
        })
    }

    /// Get the current routing digest (for inspection before finalization).
    pub fn current_routing_digest(&self) -> &RoutingDigest {
        &self.routing_digest
    }

    /// Get the context ID.
    pub fn context_id(&self) -> &ContextId {
        &self.context_id
    }
}

// ============================================================================
// Encoding Helpers
// ============================================================================

/// Encode length-prefixed UTF-8 string into buffer.
fn encode_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

/// Encode optional string with presence marker.
fn encode_optional_str(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(val) => {
            buf.push(1u8);
            encode_str(buf, val);
        }
        None => buf.push(0u8),
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Create a receipt generator from a context manifest digest.
///
/// This is a convenience function for integration with existing manifest types.
pub fn receipt_generator_from_manifest(
    model_hash: B3Hash,
    manifest_digest: B3Hash,
) -> ReceiptGenerator {
    ReceiptGenerator::new(model_hash, manifest_digest)
}

/// Compute adapter configuration hash from adapter stack.
///
/// This creates a deterministic hash over the adapter configuration
/// suitable for binding in the context_id.
pub fn compute_adapter_config_hash(
    adapters: &[(String, B3Hash, u32, f32)], // (id, hash, rank, alpha)
) -> B3Hash {
    let mut hasher = blake3::Hasher::new();

    // Sort adapters by ID for determinism
    let mut sorted: Vec<_> = adapters.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    hasher.update(&(sorted.len() as u32).to_le_bytes());
    for (id, hash, rank, alpha) in &sorted {
        hasher.update(&(id.len() as u32).to_le_bytes());
        hasher.update(id.as_bytes());
        hasher.update(hash.as_bytes());
        hasher.update(&rank.to_le_bytes());
        hasher.update(&alpha.to_le_bytes());
    }

    B3Hash::from_bytes(hasher.finalize().into())
}

// ============================================================================
// Integration Helpers
// ============================================================================

/// Convert a `CryptographicReceipt` to `ReceiptDigestInput` for V5 schema compatibility.
///
/// This allows using the new crypto_receipt module alongside the existing
/// receipt_digest infrastructure.
impl CryptographicReceipt {
    /// Convert to ReceiptDigestInput for V5 schema computation.
    ///
    /// Note: This conversion loses billing information since `CryptographicReceipt`
    /// doesn't include billing fields. Use this for verification purposes only.
    pub fn to_receipt_digest_input(&self) -> crate::receipt_digest::ReceiptDigestInput {
        crate::receipt_digest::ReceiptDigestInput {
            context_digest: *self.context_id.digest.as_bytes(),
            run_head_hash: *self.routing_digest.digest.as_bytes(),
            output_digest: *self.output_digest.as_bytes(),
            // Billing fields not tracked in CryptographicReceipt
            logical_prompt_tokens: self.metadata.input_token_count,
            prefix_cached_token_count: 0,
            billed_input_tokens: self.metadata.input_token_count,
            logical_output_tokens: self.metadata.output_token_count,
            billed_output_tokens: self.metadata.output_token_count,
            // Backend fields
            backend_used: None,
            backend_attestation_b3: None,
            // Seed fields
            root_seed_digest: None,
            seed_mode: None,
            has_manifest_binding: None,
            // Stop controller
            stop_reason_code: self.metadata.stop_reason.clone(),
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            // KV quota (not tracked)
            tenant_kv_quota_bytes: 0,
            tenant_kv_bytes_used: 0,
            kv_evictions: 0,
            kv_residency_policy_id: None,
            kv_quota_enforced: false,
            // Prefix cache (not tracked)
            prefix_kv_key_b3: None,
            prefix_cache_hit: false,
            prefix_kv_bytes: 0,
            // Model cache (not tracked)
            model_cache_identity_v2_digest_b3: None,
            // V5 equipment profile
            equipment_profile_digest_b3: Some(*self.equipment_profile.digest.as_bytes()),
            processor_id: Some(self.equipment_profile.processor_id.clone()),
            mlx_version: Some(self.equipment_profile.engine_version.clone()),
            ane_version: self.equipment_profile.ane_version.clone(),
            // Citations (not tracked)
            citations_merkle_root_b3: None,
            citation_count: 0,
        }
    }
}

/// Create a `RoutingRecord` from router decision event data.
///
/// This is a convenience function for converting from telemetry event format.
#[allow(clippy::too_many_arguments)]
pub fn routing_record_from_decision(
    step: u32,
    input_token_id: Option<u32>,
    adapter_indices: &[u16],
    adapter_ids: &[String],
    gates_q15: &[i16],
    entropy: f32,
    policy_mask_digest: Option<B3Hash>,
    backend_id: Option<&str>,
    kernel_version_id: Option<&str>,
) -> RoutingRecord {
    RoutingRecord {
        step,
        input_token_id,
        adapter_indices: adapter_indices.to_vec(),
        adapter_ids: adapter_ids.to_vec(),
        gates_q15: gates_q15.to_vec(),
        entropy,
        policy_mask_digest,
        backend_id: backend_id.map(String::from),
        kernel_version_id: kernel_version_id.map(String::from),
        allowed_mask: None,
    }
}

/// Create an `EquipmentProfile` from a device fingerprint's equipment fields.
///
/// This bridges the `DeviceFingerprint` type from `adapteros-verify` to
/// the `EquipmentProfile` used in crypto receipts.
pub fn equipment_profile_from_fingerprint(
    processor_id: Option<&str>,
    mlx_version: Option<&str>,
    ane_version: Option<&str>,
) -> EquipmentProfile {
    EquipmentProfile::compute(
        processor_id.unwrap_or("unknown"),
        mlx_version.unwrap_or("unknown"),
        ane_version,
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model_hash() -> B3Hash {
        B3Hash::hash(b"qwen2.5-7b-base-model")
    }

    fn sample_adapter_config_hash() -> B3Hash {
        B3Hash::hash(b"adapter-stack-config")
    }

    fn sample_input_tokens() -> Vec<u32> {
        vec![1, 2, 3, 4, 5, 100, 200, 300]
    }

    fn sample_output_tokens() -> Vec<u32> {
        vec![500, 501, 502, 503, 504]
    }

    fn sample_routing_record(step: u32) -> RoutingRecord {
        RoutingRecord {
            step,
            input_token_id: Some(step + 100),
            adapter_indices: vec![0, 2, 5],
            adapter_ids: vec![
                "adapter_0".to_string(),
                "adapter_2".to_string(),
                "adapter_5".to_string(),
            ],
            gates_q15: vec![16384, 8192, 8191], // Q15 values
            entropy: 0.75,
            policy_mask_digest: None,
            backend_id: Some("mlx".to_string()),
            kernel_version_id: Some("mlx-v1.0".to_string()),
            allowed_mask: None,
        }
    }

    #[test]
    fn test_context_id_computation() {
        let model_hash = sample_model_hash();
        let adapter_hash = sample_adapter_config_hash();

        let ctx1 = ContextId::compute(model_hash, adapter_hash);
        let ctx2 = ContextId::compute(model_hash, adapter_hash);

        // Same inputs produce same context_id
        assert_eq!(ctx1.digest, ctx2.digest);
        assert_eq!(ctx1, ctx2);
    }

    #[test]
    fn test_context_id_differs_with_different_inputs() {
        let model_hash = sample_model_hash();
        let adapter_hash1 = B3Hash::hash(b"config-a");
        let adapter_hash2 = B3Hash::hash(b"config-b");

        let ctx1 = ContextId::compute(model_hash, adapter_hash1);
        let ctx2 = ContextId::compute(model_hash, adapter_hash2);

        assert_ne!(ctx1.digest, ctx2.digest);
    }

    #[test]
    fn test_equipment_profile_computation() {
        let profile1 = EquipmentProfile::compute("Apple M4 Max", "mlx-0.21.0", None);
        let profile2 = EquipmentProfile::compute("Apple M4 Max", "mlx-0.21.0", None);

        // Same inputs produce same digest
        assert_eq!(profile1.digest, profile2.digest);
    }

    #[test]
    fn test_equipment_profile_differs_with_ane() {
        let profile1 = EquipmentProfile::compute("Apple M4 Max", "mlx-0.21.0", None);
        let profile2 =
            EquipmentProfile::compute("Apple M4 Max", "mlx-0.21.0", Some("ANEv4-38core"));

        assert_ne!(profile1.digest, profile2.digest);
    }

    #[test]
    fn test_input_digest_deterministic() {
        let tokens = sample_input_tokens();

        let d1 = compute_input_digest(&tokens);
        let d2 = compute_input_digest(&tokens);

        assert_eq!(d1, d2);
    }

    #[test]
    fn test_output_digest_deterministic() {
        let tokens = sample_output_tokens();

        let d1 = compute_output_digest(&tokens);
        let d2 = compute_output_digest(&tokens);

        assert_eq!(d1, d2);
    }

    #[test]
    #[allow(deprecated)] // Testing deprecated API for backward compatibility
    fn test_routing_record_hash_deterministic() {
        let record = sample_routing_record(0);

        let h1 = record.compute_hash();
        let h2 = record.compute_hash();

        assert_eq!(h1, h2);
    }

    #[test]
    #[allow(deprecated)] // Testing deprecated API for backward compatibility
    fn test_routing_digest_accumulation() {
        let mut digest1 = RoutingDigest::new();
        let mut digest2 = RoutingDigest::new();

        for step in 0..5 {
            let record = sample_routing_record(step);
            digest1.accumulate(&record);
            digest2.accumulate(&record);
        }

        // Same records in same order produce same digest
        assert_eq!(digest1.digest, digest2.digest);
        assert_eq!(digest1.decision_count, 5);
    }

    #[test]
    #[allow(deprecated)] // Testing deprecated API for backward compatibility
    fn test_routing_digest_order_matters() {
        let record0 = sample_routing_record(0);
        let record1 = sample_routing_record(1);

        let mut digest1 = RoutingDigest::new();
        digest1.accumulate(&record0);
        digest1.accumulate(&record1);

        let mut digest2 = RoutingDigest::new();
        digest2.accumulate(&record1);
        digest2.accumulate(&record0);

        // Different order produces different digest
        assert_ne!(digest1.digest, digest2.digest);
    }

    #[test]
    fn test_receipt_generator_complete_flow() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());

        // Record routing decisions
        for step in 0..5 {
            generator.record_routing_decision(sample_routing_record(step));
        }

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();

        // Receipt should verify
        assert!(receipt.verify());
        assert_eq!(receipt.schema_version, CRYPTO_RECEIPT_SCHEMA_VERSION);
        assert_eq!(receipt.routing_digest.decision_count, 5);
    }

    #[test]
    fn test_receipt_verification_fails_on_tampering() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());

        for step in 0..3 {
            generator.record_routing_decision(sample_routing_record(step));
        }

        let mut receipt = generator.finalize(&sample_output_tokens()).unwrap();

        // Tamper with output digest
        receipt.output_digest = B3Hash::hash(b"tampered");

        // Verification should fail
        assert!(!receipt.verify());
    }

    #[test]
    fn test_receipt_generator_requires_input_tokens() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        // Don't bind input tokens

        let result = generator.finalize(&sample_output_tokens());
        assert!(result.is_err());
    }

    #[test]
    fn test_receipt_generator_requires_equipment_profile() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.bind_input_tokens(&sample_input_tokens());
        // Don't set equipment profile

        let result = generator.finalize(&sample_output_tokens());
        assert!(result.is_err());
    }

    #[test]
    fn test_receipt_deterministic_across_runs() {
        // Create two receipts with identical inputs
        let create_receipt = || {
            let mut generator =
                ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

            generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
            generator.bind_input_tokens(&sample_input_tokens());

            for step in 0..3 {
                generator.record_routing_decision(sample_routing_record(step));
            }

            let mut receipt = generator.finalize(&sample_output_tokens()).unwrap();
            // Clear timestamp for comparison
            receipt.metadata.created_at = None;
            receipt
        };

        let receipt1 = create_receipt();
        let receipt2 = create_receipt();

        // Same inputs produce same receipt digest
        assert_eq!(receipt1.receipt_digest, receipt2.receipt_digest);
    }

    #[test]
    fn test_canonical_bytes_deterministic() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());
        generator.record_routing_decision(sample_routing_record(0));

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();

        let bytes1 = receipt.to_canonical_bytes();
        let bytes2 = receipt.to_canonical_bytes();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_adapter_config_hash_sorted() {
        let adapters1 = vec![
            ("b".to_string(), B3Hash::hash(b"b"), 16, 1.0f32),
            ("a".to_string(), B3Hash::hash(b"a"), 8, 0.5f32),
        ];
        let adapters2 = vec![
            ("a".to_string(), B3Hash::hash(b"a"), 8, 0.5f32),
            ("b".to_string(), B3Hash::hash(b"b"), 16, 1.0f32),
        ];

        // Different order but same content should produce same hash
        let h1 = compute_adapter_config_hash(&adapters1);
        let h2 = compute_adapter_config_hash(&adapters2);

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_metadata_fields() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());
        generator.set_tenant_id("tenant-001");
        generator.set_trace_id("trace-abc123");
        generator.set_stop_reason("EOS");

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();

        assert_eq!(receipt.metadata.tenant_id, Some("tenant-001".to_string()));
        assert_eq!(receipt.metadata.trace_id, Some("trace-abc123".to_string()));
        assert_eq!(receipt.metadata.stop_reason, Some("EOS".to_string()));
        assert_eq!(
            receipt.metadata.input_token_count,
            sample_input_tokens().len() as u32
        );
        assert_eq!(
            receipt.metadata.output_token_count,
            sample_output_tokens().len() as u32
        );
    }

    #[test]
    fn test_empty_routing_decisions() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());
        // No routing decisions recorded

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();

        assert!(receipt.verify());
        assert_eq!(receipt.routing_digest.decision_count, 0);
        assert_eq!(receipt.routing_digest.digest, B3Hash::zero());
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());
        generator.record_routing_decision(sample_routing_record(0));
        generator.set_tenant_id("tenant-001");

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();

        let json = serde_json::to_string(&receipt).expect("serialize");
        let parsed: CryptographicReceipt = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(receipt.receipt_digest, parsed.receipt_digest);
        assert!(parsed.verify());
    }

    #[test]
    fn test_routing_record_builder() {
        let record = RoutingRecord::new(0, vec![0, 1, 2], vec![16384, 8192, 8191], 0.75)
            .with_input_token(100)
            .with_adapter_ids(vec!["a".to_string(), "b".to_string(), "c".to_string()])
            .with_backend("mlx", Some("mlx-v1.0"))
            .with_allowed_mask(vec![true, true, false]);

        assert_eq!(record.step, 0);
        assert_eq!(record.input_token_id, Some(100));
        assert_eq!(record.adapter_ids.len(), 3);
        assert_eq!(record.backend_id, Some("mlx".to_string()));
        assert_eq!(record.kernel_version_id, Some("mlx-v1.0".to_string()));
        assert_eq!(record.allowed_mask, Some(vec![true, true, false]));
    }

    #[test]
    #[allow(deprecated)] // Testing deprecated API for backward compatibility
    fn test_routing_record_backend_changes_hash() {
        let record1 = RoutingRecord::new(0, vec![0], vec![16384], 0.5);
        let record2 = record1.clone().with_backend("mlx", Some("v1"));
        let record3 = record1.clone().with_backend("metal", Some("v1"));

        let h1 = record1.compute_hash();
        let h2 = record2.compute_hash();
        let h3 = record3.compute_hash();

        // Different backend settings produce different hashes
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
    }

    #[test]
    #[allow(deprecated)] // Testing deprecated API for backward compatibility
    fn test_routing_record_allowed_mask_changes_hash() {
        let record1 = RoutingRecord::new(0, vec![0, 1], vec![16384, 16383], 0.5);
        let record2 = record1.clone().with_allowed_mask(vec![true, false]);
        let record3 = record1.clone().with_allowed_mask(vec![false, true]);

        let h1 = record1.compute_hash();
        let h2 = record2.compute_hash();
        let h3 = record3.compute_hash();

        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
    }

    #[test]
    fn test_record_routing_step_full() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());

        // Use full recording method
        generator.record_routing_step_full(
            0,
            Some(100),
            vec![0, 1],
            vec!["adapter_0".to_string(), "adapter_1".to_string()],
            vec![16384, 16383],
            0.75,
            None,
            Some("mlx".to_string()),
            Some("mlx-v1.0".to_string()),
            Some(vec![true, true]),
        );

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();
        assert!(receipt.verify());
        assert_eq!(receipt.routing_digest.decision_count, 1);
    }

    #[test]
    fn test_to_receipt_digest_input_conversion() {
        let mut generator =
            ReceiptGenerator::new(sample_model_hash(), sample_adapter_config_hash());

        generator.set_equipment_profile("Apple M4 Max", "mlx-0.21.0");
        generator.bind_input_tokens(&sample_input_tokens());
        generator.record_routing_decision(sample_routing_record(0));
        generator.set_stop_reason("EOS");

        let receipt = generator.finalize(&sample_output_tokens()).unwrap();
        let input = receipt.to_receipt_digest_input();

        // Verify conversion preserves key fields
        assert_eq!(input.context_digest, *receipt.context_id.digest.as_bytes());
        assert_eq!(
            input.run_head_hash,
            *receipt.routing_digest.digest.as_bytes()
        );
        assert_eq!(input.output_digest, *receipt.output_digest.as_bytes());
        assert_eq!(
            input.equipment_profile_digest_b3,
            Some(*receipt.equipment_profile.digest.as_bytes())
        );
        assert_eq!(input.processor_id, Some("Apple M4 Max".to_string()));
        assert_eq!(input.mlx_version, Some("mlx-0.21.0".to_string()));
        assert_eq!(input.stop_reason_code, Some("EOS".to_string()));
    }

    #[test]
    fn test_routing_record_from_decision() {
        let record = super::routing_record_from_decision(
            5,
            Some(100),
            &[0, 1, 2],
            &["a".to_string(), "b".to_string(), "c".to_string()],
            &[16384, 8192, 8191],
            0.8,
            Some(B3Hash::hash(b"policy")),
            Some("mlx"),
            Some("v1.0"),
        );

        assert_eq!(record.step, 5);
        assert_eq!(record.input_token_id, Some(100));
        assert_eq!(record.adapter_indices, vec![0, 1, 2]);
        assert_eq!(record.adapter_ids.len(), 3);
        assert_eq!(record.gates_q15, vec![16384, 8192, 8191]);
        assert!((record.entropy - 0.8).abs() < 0.001);
        assert!(record.policy_mask_digest.is_some());
        assert_eq!(record.backend_id, Some("mlx".to_string()));
        assert_eq!(record.kernel_version_id, Some("v1.0".to_string()));
    }

    #[test]
    fn test_equipment_profile_from_fingerprint() {
        let profile = super::equipment_profile_from_fingerprint(
            Some("Apple M4 Max"),
            Some("mlx-0.21.0"),
            Some("ANEv4-38core"),
        );

        assert_eq!(profile.processor_id, "Apple M4 Max");
        assert_eq!(profile.engine_version, "mlx-0.21.0");
        assert_eq!(profile.ane_version, Some("ANEv4-38core".to_string()));
    }

    #[test]
    fn test_equipment_profile_from_fingerprint_defaults() {
        let profile = super::equipment_profile_from_fingerprint(None, None, None);

        assert_eq!(profile.processor_id, "unknown");
        assert_eq!(profile.engine_version, "unknown");
        assert_eq!(profile.ane_version, None);
    }

    // ========================================================================
    // Canonical Hash Parity Tests (Gap 3.2 Fix)
    // ========================================================================

    /// Verifies that `RoutingRecord::compute_hash_canonical` produces the same
    /// hash as `receipt_digest::hash_token_decision` for identical inputs.
    ///
    /// This is a critical test for replay verification: if these hashes diverge,
    /// receipts generated via `crypto_receipt` will not match receipts stored
    /// by `SqlTraceSink`, breaking offline verification.
    #[test]
    fn test_canonical_hash_parity_with_receipt_digest() {
        use crate::receipt_digest::{encode_adapter_ids, encode_allowed_mask, encode_gates_q15, hash_token_decision};

        let context_digest = [0x42u8; 32];
        let step = 5u32;
        let adapter_ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let gates_q15 = vec![16384i16, 16383];
        let policy_mask_digest = Some(B3Hash::hash(b"policy-mask"));
        let backend_id = Some("mlx".to_string());
        let kernel_version_id = Some("mlx-v1.0".to_string());
        let allowed_mask = Some(vec![true, false, true]);

        // Create a RoutingRecord with these fields
        let record = RoutingRecord {
            step,
            input_token_id: None, // Not included in canonical hash
            adapter_indices: vec![0, 1], // Not included in canonical hash
            adapter_ids: adapter_ids.clone(),
            gates_q15: gates_q15.clone(),
            entropy: 0.5, // Not included in canonical hash
            policy_mask_digest,
            backend_id: backend_id.clone(),
            kernel_version_id: kernel_version_id.clone(),
            allowed_mask: allowed_mask.clone(),
        };

        // Compute using the canonical method
        let record_hash = record.compute_hash_canonical(&context_digest);

        // Compute directly using receipt_digest functions (as SqlTraceSink does)
        let adapter_blob = encode_adapter_ids(&adapter_ids);
        let gates_blob = encode_gates_q15(&gates_q15);
        let allowed_mask_blob = allowed_mask.as_ref().map(|m| encode_allowed_mask(m));
        let policy_bytes = policy_mask_digest.map(|h| *h.as_bytes());

        let direct_hash = hash_token_decision(
            &context_digest,
            step,
            &adapter_blob,
            &gates_blob,
            policy_bytes,
            allowed_mask_blob.as_deref(),
            None, // policy_overrides_json not in RoutingRecord
            backend_id.as_deref(),
            kernel_version_id.as_deref(),
        );

        assert_eq!(
            record_hash, direct_hash,
            "RoutingRecord::compute_hash_canonical must produce identical hash to hash_token_decision.\n\
            Record hash: {}\n\
            Direct hash: {}",
            record_hash.to_hex(),
            direct_hash.to_hex()
        );
    }

    /// Verifies that `RoutingDigest::accumulate_canonical` produces the same
    /// chain hash as `receipt_digest::update_run_head` for identical inputs.
    ///
    /// This ensures the chaining order matches the production trace system.
    #[test]
    fn test_canonical_chain_parity_with_receipt_digest() {
        use crate::receipt_digest::update_run_head;

        let context_digest = [0x42u8; 32];

        // Create two routing records
        let record0 = RoutingRecord {
            step: 0,
            input_token_id: None,
            adapter_indices: vec![0],
            adapter_ids: vec!["adapter-a".to_string()],
            gates_q15: vec![16384],
            entropy: 0.5,
            policy_mask_digest: None,
            backend_id: Some("mlx".to_string()),
            kernel_version_id: None,
            allowed_mask: None,
        };
        let record1 = RoutingRecord {
            step: 1,
            input_token_id: None,
            adapter_indices: vec![1],
            adapter_ids: vec!["adapter-b".to_string()],
            gates_q15: vec![16383],
            entropy: 0.6,
            policy_mask_digest: None,
            backend_id: Some("mlx".to_string()),
            kernel_version_id: None,
            allowed_mask: None,
        };

        // Accumulate using canonical method
        let mut digest = RoutingDigest::new();
        digest.accumulate_canonical(&record0, &context_digest);
        digest.accumulate_canonical(&record1, &context_digest);

        // Compute manually using receipt_digest functions
        let hash0 = record0.compute_hash_canonical(&context_digest);
        let expected_head0 = update_run_head(&B3Hash::zero(), 0, &hash0);

        let hash1 = record1.compute_hash_canonical(&context_digest);
        let expected_head1 = update_run_head(&expected_head0, 1, &hash1);

        assert_eq!(
            digest.digest, expected_head1,
            "RoutingDigest::accumulate_canonical must produce identical chain to update_run_head.\n\
            Accumulated: {}\n\
            Expected: {}",
            digest.digest.to_hex(),
            expected_head1.to_hex()
        );
        assert_eq!(digest.decision_count, 2);
    }
}
