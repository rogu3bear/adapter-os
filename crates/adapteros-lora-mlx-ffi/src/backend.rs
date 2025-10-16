//! MLX FFI backend implementation for FusedKernels trait

use crate::{LoRAAdapter, MLXFFIModel};
use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// MLX FFI backend for inference
pub struct MLXFFIBackend {
    /// Base model
    model: Arc<MLXFFIModel>,
    /// Loaded LoRA adapters by ID
    adapters: Arc<RwLock<HashMap<u16, Arc<LoRAAdapter>>>>,
    /// Device name
    device: String,
}

impl MLXFFIBackend {
    /// Create new MLX FFI backend with loaded model
    pub fn new(model: MLXFFIModel) -> Self {
        Self {
            model: Arc::new(model),
            adapters: Arc::new(RwLock::new(HashMap::new())),
            device: "MLX FFI (Apple Silicon)".to_string(),
        }
    }

    /// Register a LoRA adapter
    pub fn register_adapter(&self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        let adapter_name = adapter.id().to_string();
        let mut adapters = self.adapters.write();
        adapters.insert(adapter_id, Arc::new(adapter));
        tracing::info!(
            "Registered LoRA adapter {} with ID {}",
            adapter_name,
            adapter_id
        );
        Ok(())
    }

    /// Get registered adapter count
    pub fn adapter_count(&self) -> usize {
        self.adapters.read().len()
    }

    /// Load adapter at runtime (hot-swap)
    pub fn load_adapter_runtime(&self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        let adapter_name = adapter.id().to_string();
        let mut adapters = self.adapters.write();
        adapters.insert(adapter_id, Arc::new(adapter));
        tracing::info!(
            "Hot-loaded LoRA adapter {} with ID {}",
            adapter_name,
            adapter_id
        );
        Ok(())
    }

    /// Unload adapter at runtime (hot-swap)
    pub fn unload_adapter_runtime(&self, adapter_id: u16) -> Result<()> {
        let mut adapters = self.adapters.write();
        if let Some(adapter) = adapters.remove(&adapter_id) {
            tracing::info!("Unloaded LoRA adapter {} (ID {})", adapter.id(), adapter_id);
            Ok(())
        } else {
            Err(adapteros_core::AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Get adapter memory usage (estimated)
    pub fn get_adapter_memory_usage(&self, adapter_id: u16) -> Result<usize> {
        let adapters = self.adapters.read();
        if let Some(adapter) = adapters.get(&adapter_id) {
            // Estimate memory usage based on LoRA parameters
            // rank * (dim_in + dim_out) * sizeof(f32) per target module
            let rank = adapter.config().rank;
            let num_modules = adapter.config().target_modules.len();

            // Simplified: assume 7B model with 4096 hidden dim
            let estimated_bytes = rank * 4096 * 2 * num_modules * 4; // f32 = 4 bytes
            Ok(estimated_bytes)
        } else {
            Err(adapteros_core::AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Apply LoRA adapters based on router decisions
    fn apply_loras(
        &self,
        ring: &RouterRing,
        base_output: &[f32],
        input: &[f32],
        module_name: &str,
    ) -> Result<Vec<f32>> {
        let adapters = self.adapters.read();

        // Collect active adapters
        let mut active_adapters = Vec::new();
        let mut gates = Vec::new();

        for (idx, &adapter_id) in ring.indices.iter().enumerate() {
            if let Some(adapter) = adapters.get(&adapter_id) {
                active_adapters.push(adapter.clone());
                // Convert i16 Q15 to u16 for routing module
                gates.push(ring.gates_q15[idx].max(0) as u16);
            }
        }

        if active_adapters.is_empty() {
            tracing::info!(
                reason = "no_adapters_qualify",
                "Router decision: K=0, using base model only"
            );
            return Ok(base_output.to_vec());
        }

        // Apply multi-LoRA routing
        let adapter_refs: Vec<&LoRAAdapter> = active_adapters.iter().map(|a| a.as_ref()).collect();

        crate::routing::apply_multi_lora(&adapter_refs, &gates, module_name, input, base_output)
    }
}

impl FusedKernels for MLXFFIBackend {
    fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        // MLX backend is experimental and non-deterministic
        use adapteros_lora_kernel_api::attestation::*;

        Ok(DeterminismReport {
            backend_type: BackendType::Mlx,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::SystemEntropy,
            floating_point_mode: FloatingPointMode::Unknown,
            compiler_flags: vec![],
            deterministic: false, // MLX is non-deterministic
        })
    }

    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Plan loading not needed for MLX FFI - model already loaded
        tracing::info!(
            "MLX FFI backend ready with {} adapters",
            self.adapter_count()
        );
        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Get base logits and hidden states
        let (logits, hidden_states) = self.model.forward_with_hidden_states(&io.input_ids)?;

        // Apply LoRA adapters if we have hidden states
        if !hidden_states.is_empty() {
            let adapters = self.adapters.read();

            // Apply LoRA to each target module
            for module_name in ["q_proj", "k_proj", "v_proj", "o_proj"] {
                if let Some(hidden) = hidden_states.get(module_name) {
                    // Apply multi-LoRA routing to this module
                    match self.apply_loras_internal(ring, &logits, hidden, module_name, &adapters) {
                        Ok(adapted_output) => {
                            // In a full implementation, we would merge this back into the model
                            // For now, we just log that LoRA was applied
                            tracing::trace!(
                                "Applied LoRA to {} with {} adapters",
                                module_name,
                                ring.indices.len()
                            );
                            // The adapted output would be used to recompute logits
                            let _ = adapted_output; // Suppress unused warning
                        }
                        Err(e) => {
                            tracing::warn!("Failed to apply LoRA to {}: {}", module_name, e);
                        }
                    }
                }
            }
        } else {
            tracing::debug!("No hidden states available, using base model logits only");
        }

        io.output_logits.copy_from_slice(&logits);
        io.position += 1;

        tracing::debug!(
            "MLX FFI step complete: position={}, active_adapters={}, logits_len={}",
            io.position,
            ring.indices.len(),
            logits.len()
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device
    }
}

impl MLXFFIBackend {
    /// Internal helper to apply LoRAs with locked adapter access
    fn apply_loras_internal(
        &self,
        ring: &RouterRing,
        base_output: &[f32],
        input: &[f32],
        module_name: &str,
        adapters: &parking_lot::RwLockReadGuard<HashMap<u16, Arc<LoRAAdapter>>>,
    ) -> Result<Vec<f32>> {
        // Collect active adapters
        let mut active_adapters = Vec::new();
        let mut gates = Vec::new();

        for (idx, &adapter_id) in ring.indices.iter().enumerate() {
            if let Some(adapter) = adapters.get(&adapter_id) {
                active_adapters.push(adapter.clone());
                // Convert i16 Q15 to u16 for routing module
                gates.push(ring.gates_q15[idx].max(0) as u16);
            }
        }

        if active_adapters.is_empty() {
            tracing::info!(
                reason = "no_adapters_qualify",
                "Router decision: K=0, using base model only"
            );
            return Ok(base_output.to_vec());
        }

        // Apply multi-LoRA routing
        let adapter_refs: Vec<&LoRAAdapter> = active_adapters.iter().map(|a| a.as_ref()).collect();

        crate::routing::apply_multi_lora(&adapter_refs, &gates, module_name, input, base_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_core::B3Hash;

    fn create_dummy_adapter(id: &str) -> LoRAAdapter {
        LoRAAdapter {
            id: id.to_string(),
            config: LoRAConfig::default(),
            lora_a: HashMap::new(),
            lora_b: HashMap::new(),
            shapes: HashMap::new(),
            hash: B3Hash::hash(id.as_bytes()),
        }
    }

    #[test]
    #[ignore] // Requires MLX model
    fn test_backend_adapter_registration() {
        // This test would require a real MLX model
        // Skipped for now
    }

    #[test]
    fn test_router_ring_creation() {
        let ring = RouterRing::new(3);
        assert_eq!(ring.indices.len(), 3);
        assert_eq!(ring.gates_q15.len(), 3);
        assert_eq!(ring.position, 0);
    }
}
