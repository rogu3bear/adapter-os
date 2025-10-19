//! LoRA adapter implementation for MLX FFI

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// LoRA adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Target modules for LoRA adaptation
    pub target_modules: Vec<String>,
    /// Dropout rate (currently unused in MLX FFI; prefer 0.0 for inference)
    pub dropout: f32,
}

impl Default for LoRAConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
            dropout: 0.0,
        }
    }
}

/// LoRA adapter with weights
#[derive(Debug, Clone)]
pub struct LoRAAdapter {
    /// Adapter identifier
    pub id: String,
    /// LoRA configuration
    pub config: LoRAConfig,
    /// LoRA A matrices (down-projection) by module name
    pub lora_a: HashMap<String, Vec<Vec<f32>>>,
    /// LoRA B matrices (up-projection) by module name
    pub lora_b: HashMap<String, Vec<Vec<f32>>>,
    /// Weight shapes by module name
    pub shapes: HashMap<String, (usize, usize)>,
    /// Adapter hash for integrity checking
    pub hash: B3Hash,
    /// Cached flattened weights (row-major) per module to avoid repeated allocations
    flatten_cache: Arc<RwLock<HashMap<String, (Arc<Vec<f32>>, Arc<Vec<f32>>)>>>,
}

impl LoRAAdapter {
    /// Create a new LoRA adapter
    pub fn new(id: String, config: LoRAConfig) -> Self {
        let hash = B3Hash::hash(id.as_bytes());
        Self {
            id,
            config,
            lora_a: HashMap::new(),
            lora_b: HashMap::new(),
            shapes: HashMap::new(),
            hash,
            flatten_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add LoRA weights for a module
    pub fn add_module_weights(
        &mut self,
        module_name: &str,
        lora_a: Vec<Vec<f32>>,
        lora_b: Vec<Vec<f32>>,
    ) {
        self.lora_a.insert(module_name.to_string(), lora_a);
        self.lora_b.insert(module_name.to_string(), lora_b);

        // Store shape information
        if let Some(a_matrix) = self.lora_a.get(module_name) {
            if !a_matrix.is_empty() && !a_matrix[0].is_empty() {
                let rows = a_matrix.len();
                let cols = a_matrix[0].len();
                self.shapes.insert(module_name.to_string(), (rows, cols));
            }
        }

        // Invalidate any cached flattened weights for this module
        if let Ok(mut cache) = self.flatten_cache.write() {
            cache.remove(module_name);
        }
    }

    /// Get LoRA weights for a module
    pub fn get_module_weights(
        &self,
        module_name: &str,
    ) -> Option<(&Vec<Vec<f32>>, &Vec<Vec<f32>>)> {
        let lora_a = self.lora_a.get(module_name)?;
        let lora_b = self.lora_b.get(module_name)?;
        Some((lora_a, lora_b))
    }

    /// Get adapter identifier
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get adapter configuration
    pub fn config(&self) -> &LoRAConfig {
        &self.config
    }

    /// Get adapter hash
    pub fn hash(&self) -> B3Hash {
        self.hash
    }

    /// Check if adapter has weights for a module
    pub fn has_module(&self, module_name: &str) -> bool {
        self.lora_a.contains_key(module_name) && self.lora_b.contains_key(module_name)
    }

    /// Get total parameter count
    pub fn parameter_count(&self) -> usize {
        let mut total = 0usize;
        for module in self.lora_a.keys() {
            if let Some(a) = self.lora_a.get(module) {
                if !a.is_empty() {
                    total += a.len() * a[0].len();
                }
            }
            if let Some(b) = self.lora_b.get(module) {
                if !b.is_empty() {
                    total += b.len() * b[0].len();
                }
            }
        }
        total
    }

    /// Get memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        self.parameter_count() * 4 // f32 = 4 bytes
    }

    /// Get the stored module shape as (rank, hidden_dim)
    pub fn module_shape(&self, module: &str) -> Option<(usize, usize)> {
        self.shapes.get(module).copied()
    }

    /// Validate that all module weights have consistent shapes.
    ///
    /// Requirements per module `m`:
    /// - `lora_a[m]` is rank x hidden_dim
    /// - `lora_b[m]` is hidden_dim x rank
    /// - `rank == self.config.rank`
    /// - `self.shapes[m] == (rank, hidden_dim)`
    pub fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Collect union of module names seen in A and B
        let mut modules: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for k in self.lora_a.keys() {
            modules.insert(k.as_str());
        }
        for k in self.lora_b.keys() {
            modules.insert(k.as_str());
        }

        for m in modules {
            let a = self
                .lora_a
                .get(m)
                .ok_or_else(|| format!("missing lora_a.{}", m))?;
            let b = self
                .lora_b
                .get(m)
                .ok_or_else(|| format!("missing lora_b.{}", m))?;

            // Check rectangular shapes and derive dims
            if a.is_empty() || a[0].is_empty() {
                return Err(format!(
                    "invalid shape for lora_a.{}: expected 2D rank×hidden, got empty",
                    m
                )
                .into());
            }
            let rank = a.len();
            let hidden_dim = a[0].len();
            for (i, row) in a.iter().enumerate() {
                if row.len() != hidden_dim {
                    return Err(format!(
                        "non-rectangular lora_a.{}: row {} has {}, expected {}",
                        m,
                        i,
                        row.len(),
                        hidden_dim
                    )
                    .into());
                }
            }

            // B should be hidden_dim x rank
            if b.is_empty() || b[0].is_empty() {
                return Err(format!(
                    "invalid shape for lora_b.{}: expected 2D hidden×rank, got empty",
                    m
                )
                .into());
            }
            let b_rows = b.len();
            let b_cols = b[0].len();
            for (i, row) in b.iter().enumerate() {
                if row.len() != b_cols {
                    return Err(format!(
                        "non-rectangular lora_b.{}: row {} has {}, expected {}",
                        m,
                        i,
                        row.len(),
                        b_cols
                    )
                    .into());
                }
            }
            if rank != self.config.rank {
                return Err(format!(
                    "rank mismatch for {}: config.rank={} but lora_a rows={}",
                    m, self.config.rank, rank
                )
                .into());
            }
            if b_rows != hidden_dim || b_cols != rank {
                return Err(format!(
                    "invalid shape for lora_b.{}: expected {}×{}, got {}×{}",
                    m, hidden_dim, rank, b_rows, b_cols
                )
                .into());
            }

            // Check stored shape record
            match self.shapes.get(m) {
                Some(&(s_rank, s_hidden)) => {
                    if s_rank != rank || s_hidden != hidden_dim {
                        return Err(format!(
                            "shape metadata mismatch for {}: shapes map=({},{}) but actual=({},{})",
                            m, s_rank, s_hidden, rank, hidden_dim
                        )
                        .into());
                    }
                }
                None => {
                    return Err(format!(
                        "missing shape metadata for {}: expected ({},{})",
                        m, rank, hidden_dim
                    )
                    .into());
                }
            }
        }

        Ok(())
    }

    /// Return contiguous row-major views of weights for a module.
    /// Returns (A_row_major, B_row_major) if the module exists.
    pub fn flatten_module_weights(&self, module: &str) -> Option<(Vec<f32>, Vec<f32>)> {
        self.flatten_module_weights_cached(module)
            .map(|(a, b)| ((*a).clone(), (*b).clone()))
    }

    /// Cached contiguous row-major views using Arc to avoid reallocations.
    /// Returns (A_row_major, B_row_major) as Arc<Vec<_>> if the module exists.
    pub fn flatten_module_weights_cached(
        &self,
        module: &str,
    ) -> Option<(Arc<Vec<f32>>, Arc<Vec<f32>>)> {
        // First try cache
        if let Ok(cache) = self.flatten_cache.read() {
            if let Some((a, b)) = cache.get(module) {
                // Validate lengths against recorded shape for safety
                if let Some((rank, hidden)) = self.shapes.get(module).copied() {
                    if a.len() == rank * hidden && b.len() == hidden * rank {
                        return Some((a.clone(), b.clone()));
                    }
                }
            }
        }

        // Compute and insert
        let (a, b) = self.get_module_weights(module)?;
        let (rank, hidden) = self
            .shapes
            .get(module)
            .copied()
            .unwrap_or((a.len(), a.get(0).map(|r| r.len()).unwrap_or(0)));

        let mut a_flat = Vec::with_capacity(rank * hidden);
        for row in a.iter() {
            a_flat.extend_from_slice(row);
        }
        let mut b_flat = Vec::with_capacity(hidden * rank);
        for row in b.iter() {
            b_flat.extend_from_slice(row);
        }

        let a_arc = Arc::new(a_flat);
        let b_arc = Arc::new(b_flat);

        if let Ok(mut cache) = self.flatten_cache.write() {
            cache.insert(module.to_string(), (a_arc.clone(), b_arc.clone()));
        }

        Some((a_arc, b_arc))
    }

    /// Load a LoRA adapter from safetensors bytes
    pub fn from_safetensors_bytes(
        id: String,
        bytes: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let tensors = safetensors::SafeTensors::deserialize(bytes)
            .map_err(|e| format!("failed to deserialize safetensors: {}", e))?;

        // Discover module names by scanning keys lora_a.<mod>
        let mut modules: Vec<String> = Vec::new();
        for name in tensors.names() {
            if let Some(rest) = name.strip_prefix("lora_a.") {
                modules.push(rest.to_string());
            }
        }
        if modules.is_empty() {
            // Try legacy keys
            modules = vec![
                "q_proj".into(),
                "k_proj".into(),
                "v_proj".into(),
                "o_proj".into(),
            ];
        }

        // Infer rank/hidden_dim from first module
        let first = format!("lora_a.{}", modules[0]);
        let a_view = tensors
            .tensor(&first)
            .map_err(|e| format!("missing or invalid tensor {}: {}", first, e))?;
        let shape = a_view.shape();
        if shape.len() != 2 {
            return Err(format!(
                "invalid shape for {}: expected 2D rank×hidden, got {:?}",
                first, shape
            )
            .into());
        }
        let rank = shape[0];
        let hidden_dim = shape[1];

        let mut adapter = Self::new(
            id,
            LoRAConfig {
                rank,
                alpha: 16.0,
                target_modules: modules.clone(),
                dropout: 0.0,
            },
        );

        // Helper to convert a f32 slice from tensor bytes
        fn to_f32_vec(data: &[u8]) -> Vec<f32> {
            data.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        }

        for m in modules.iter() {
            let a_key = format!("lora_a.{}", m);
            let b_key = format!("lora_b.{}", m);
            let a_t = tensors
                .tensor(&a_key)
                .map_err(|e| format!("missing or invalid tensor {}: {}", a_key, e))?;
            let b_t = tensors
                .tensor(&b_key)
                .map_err(|e| format!("missing or invalid tensor {}: {}", b_key, e))?;

            // Validate shapes match expected per inferred dims
            let a_shape = a_t.shape();
            if a_shape.len() != 2 || a_shape[0] != rank || a_shape[1] != hidden_dim {
                return Err(format!(
                    "invalid shape for {}: expected {}×{}, got {:?}",
                    a_key, rank, hidden_dim, a_shape
                )
                .into());
            }
            let b_shape = b_t.shape();
            if b_shape.len() != 2 || b_shape[0] != hidden_dim || b_shape[1] != rank {
                return Err(format!(
                    "invalid shape for {}: expected {}×{}, got {:?}",
                    b_key, hidden_dim, rank, b_shape
                )
                .into());
            }
            let a_raw = to_f32_vec(a_t.data());
            let b_raw = to_f32_vec(b_t.data());

            if a_raw.len() != rank * hidden_dim {
                return Err(format!(
                    "data length mismatch for {}: expected {}, got {}",
                    a_key,
                    rank * hidden_dim,
                    a_raw.len()
                )
                .into());
            }
            if b_raw.len() != hidden_dim * rank {
                return Err(format!(
                    "data length mismatch for {}: expected {}, got {}",
                    b_key,
                    hidden_dim * rank,
                    b_raw.len()
                )
                .into());
            }

            // reshape
            let mut a_rows: Vec<Vec<f32>> = Vec::with_capacity(rank);
            for r in 0..rank {
                let start = r * hidden_dim;
                let end = start + hidden_dim;
                a_rows.push(a_raw[start..end].to_vec());
            }

            let mut b_rows: Vec<Vec<f32>> = Vec::with_capacity(hidden_dim);
            for h in 0..hidden_dim {
                let start = h * rank;
                let end = start + rank;
                b_rows.push(b_raw[start..end].to_vec());
            }

            adapter.add_module_weights(m, a_rows, b_rows);
        }
        // Final validation to ensure consistent state
        adapter.validate()?;

        Ok(adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::tensor::TensorView;

    #[test]
    fn test_lora_config_default() {
        let config = LoRAConfig::default();
        assert_eq!(config.rank, 4);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(config.target_modules.len(), 4);
        assert_eq!(config.dropout, 0.0);
    }

    #[test]
    fn test_lora_adapter_creation() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        assert_eq!(adapter.id(), "test_adapter");
        assert_eq!(adapter.config().rank, 4);
        assert_eq!(adapter.parameter_count(), 0);
    }

    #[test]
    fn test_lora_adapter_weights() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        // Add weights for a module
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        assert!(adapter.has_module("q_proj"));
        assert_eq!(adapter.parameter_count(), 8); // 2x2 + 2x2 = 8 parameters
        assert_eq!(adapter.memory_usage(), 32); // 8 * 4 bytes
    }

    #[test]
    fn test_lora_adapter_serialization() {
        let config = LoRAConfig::default();
        let adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        // Test serialization
        let serialized = serde_json::to_string(&adapter.config()).unwrap();
        let deserialized: LoRAConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(adapter.config().rank, deserialized.rank);
        assert_eq!(adapter.config().alpha, deserialized.alpha);
    }

    #[test]
    fn test_from_safetensors_bytes_roundtrip_shapes() {
        let rank = 4usize;
        let hidden = 8usize;
        let modules = ["q_proj", "k_proj"];

        fn flat_f32_to_le_bytes(vals: &[f32]) -> Vec<u8> {
            let mut out = Vec::with_capacity(vals.len() * 4);
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out
        }

        // Phase 1: materialize byte blobs for all tensors
        let mut blobs: Vec<Vec<u8>> = Vec::with_capacity(modules.len() * 2);
        let mut map: Vec<(usize, usize, &str)> = Vec::with_capacity(modules.len());
        for m in modules.iter() {
            let mut a_vals: Vec<f32> = Vec::with_capacity(rank * hidden);
            for r in 0..rank {
                for h in 0..hidden {
                    a_vals.push((r * hidden + h) as f32 * 0.01);
                }
            }
            let mut b_vals: Vec<f32> = Vec::with_capacity(hidden * rank);
            for h in 0..hidden {
                for r in 0..rank {
                    b_vals.push((h * rank + r) as f32 * 0.02);
                }
            }
            let a_idx = blobs.len();
            blobs.push(flat_f32_to_le_bytes(&a_vals));
            let b_idx = blobs.len();
            blobs.push(flat_f32_to_le_bytes(&b_vals));
            map.push((a_idx, b_idx, *m));
        }

        // Phase 2: build TensorViews that borrow from stable blobs
        let mut tensors: Vec<(String, TensorView)> = Vec::with_capacity(modules.len() * 2);
        for (a_idx, b_idx, m) in map.into_iter() {
            let a_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![rank, hidden],
                blobs[a_idx].as_slice(),
            )
            .unwrap();
            let b_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![hidden, rank],
                blobs[b_idx].as_slice(),
            )
            .unwrap();
            tensors.push((format!("lora_a.{}", m), a_view));
            tensors.push((format!("lora_b.{}", m), b_view));
        }

        let bytes = safetensors::serialize(tensors, &Default::default()).unwrap();
        let parsed = LoRAAdapter::from_safetensors_bytes("test".into(), &bytes).unwrap();

        // Validate shapes and presence
        for m in modules.iter() {
            assert!(parsed.has_module(m));
            let (a, b) = parsed.get_module_weights(m).unwrap();
            assert_eq!(a.len(), rank);
            assert_eq!(a[0].len(), hidden);
            assert_eq!(b.len(), hidden);
            assert_eq!(b[0].len(), rank);
        }
    }

    #[test]
    fn test_apply_after_loading_safetensors() {
        let rank = 2usize;
        let hidden = 4usize;
        let module = "q_proj";

        fn flat_f32_to_le_bytes(vals: &[f32]) -> Vec<u8> {
            let mut out = Vec::with_capacity(vals.len() * 4);
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out
        }

        // Build a single-module safetensors with simple values
        let mut tensors: Vec<(String, TensorView)> = Vec::new();
        let mut a_vals: Vec<f32> = Vec::with_capacity(rank * hidden);
        for r in 0..rank {
            for h in 0..hidden {
                a_vals.push(((r * hidden + h) as f32 + 1.0) * 0.1);
            }
        }
        let a_blob = flat_f32_to_le_bytes(&a_vals);
        let a_view = TensorView::new(
            safetensors::Dtype::F32,
            vec![rank, hidden],
            a_blob.as_slice(),
        )
        .unwrap();

        let mut b_vals: Vec<f32> = Vec::with_capacity(hidden * rank);
        for h in 0..hidden {
            for r in 0..rank {
                b_vals.push(((h * rank + r) as f32 + 1.0) * 0.2);
            }
        }
        let b_blob = flat_f32_to_le_bytes(&b_vals);
        let b_view = TensorView::new(
            safetensors::Dtype::F32,
            vec![hidden, rank],
            b_blob.as_slice(),
        )
        .unwrap();

        tensors.push((format!("lora_a.{}", module), a_view));
        tensors.push((format!("lora_b.{}", module), b_view));

        let bytes = safetensors::serialize(tensors, &Default::default()).unwrap();
        let adapter = LoRAAdapter::from_safetensors_bytes("test".into(), &bytes).unwrap();

        // Build input and base_output and apply routing
        let input: Vec<f32> = vec![0.3, -0.2, 0.5, 0.1];
        let base_output: Vec<f32> = vec![0.0; hidden];
        let adapters = vec![&adapter];
        let gates = vec![32767u16]; // full weight

        let result =
            crate::routing::apply_multi_lora(&adapters, &gates, module, &input, &base_output)
                .unwrap();
        assert_eq!(result.len(), hidden);
        // Non-zero effect expected
        assert!(result.iter().any(|&x| x.abs() > 1e-6));
    }

    #[test]
    fn test_validate_success() {
        let mut adapter = LoRAAdapter::new(
            "ok".into(),
            LoRAConfig {
                rank: 2,
                alpha: 1.0,
                target_modules: vec!["q_proj".into()],
                dropout: 0.0,
            },
        );
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]]; // 2 x 2
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]]; // 2 x 2
        adapter.add_module_weights("q_proj", lora_a, lora_b);
        assert!(adapter.validate().is_ok());
        // module_shape accessor
        assert_eq!(adapter.module_shape("q_proj"), Some((2, 2)));
        // flattener lengths
        let (a_flat, b_flat) = adapter.flatten_module_weights("q_proj").unwrap();
        assert_eq!(a_flat.len(), 4);
        assert_eq!(b_flat.len(), 4);
    }

    #[test]
    fn test_validate_mismatch() {
        let mut adapter = LoRAAdapter::new(
            "bad".into(),
            LoRAConfig {
                rank: 2,
                alpha: 1.0,
                target_modules: vec!["q_proj".into()],
                dropout: 0.0,
            },
        );
        // A is 2 x 3 (rank=2, hidden=3)
        let lora_a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        // B is 2 x 2 (should be 3 x 2) -> mismatch
        let lora_b = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        adapter.add_module_weights("q_proj", lora_a, lora_b);
        let err = adapter.validate().expect_err("expected validation error");
        let msg = format!("{}", err);
        assert!(
            msg.contains("invalid shape for lora_b.q_proj")
                || msg.contains("shape metadata mismatch")
                || msg.contains("missing shape metadata")
        );
    }

    #[test]
    fn test_flatten_module_weights() {
        let mut adapter = LoRAAdapter::new(
            "flat".into(),
            LoRAConfig {
                rank: 2,
                alpha: 1.0,
                target_modules: vec!["m".into()],
                dropout: 0.0,
            },
        );
        let a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]; // 2x3
        let b = vec![vec![7.0, 8.0], vec![9.0, 10.0], vec![11.0, 12.0]]; // 3x2
        adapter.add_module_weights("m", a.clone(), b.clone());
        let (a_flat, b_flat) = adapter.flatten_module_weights("m").unwrap();
        assert_eq!(a_flat, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(b_flat, vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
    }

    #[test]
    fn test_flatten_cache_reuse_and_invalidate() {
        let mut adapter = LoRAAdapter::new(
            "cache".into(),
            LoRAConfig {
                rank: 2,
                alpha: 1.0,
                target_modules: vec!["m".into()],
                dropout: 0.0,
            },
        );
        let a = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]; // 2x3
        let b = vec![vec![7.0, 8.0], vec![9.0, 10.0], vec![11.0, 12.0]]; // 3x2
        adapter.add_module_weights("m", a.clone(), b.clone());

        let (a1, b1) = adapter.flatten_module_weights_cached("m").unwrap();
        let (a2, b2) = adapter.flatten_module_weights_cached("m").unwrap();
        assert!(Arc::ptr_eq(&a1, &a2));
        assert!(Arc::ptr_eq(&b1, &b2));

        // Update weights -> cache should be invalidated and rebuilt
        let a_new = vec![vec![0.0, 0.0, 0.0], vec![1.0, 1.0, 1.0]]; // different contents
        let b_new = vec![vec![2.0, 2.0], vec![3.0, 3.0], vec![4.0, 4.0]];
        adapter.add_module_weights("m", a_new, b_new);

        let (a3, b3) = adapter.flatten_module_weights_cached("m").unwrap();
        assert!(!Arc::ptr_eq(&a1, &a3) || !Arc::ptr_eq(&b1, &b3));
    }

    #[test]
    fn test_safetensors_missing_b_keys() {
        // Build a safetensors buffer with only lora_a.q_proj present
        let rank = 2usize;
        let hidden = 3usize;
        let mut a_vals: Vec<f32> = Vec::with_capacity(rank * hidden);
        for r in 0..rank {
            for h in 0..hidden {
                a_vals.push((1 + r * hidden + h) as f32 * 0.1);
            }
        }

        let a_blob: Vec<u8> = a_vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let a_view = TensorView::new(
            safetensors::Dtype::F32,
            vec![rank, hidden],
            a_blob.as_slice(),
        )
        .unwrap();
        let tensors = vec![("lora_a.q_proj".to_string(), a_view)];
        let bytes = safetensors::serialize(tensors, &Default::default()).unwrap();

        let err = LoRAAdapter::from_safetensors_bytes("test".into(), &bytes)
            .expect_err("expected error for missing lora_b.*");
        let msg = format!("{}", err);
        assert!(msg.contains("missing or invalid tensor lora_b.q_proj"));
    }

    #[test]
    fn test_safetensors_truncated_buffer() {
        // Create a valid buffer first
        let rank = 2usize;
        let hidden = 2usize;
        let a_vals: Vec<f32> = (0..rank * hidden).map(|i| i as f32 * 0.01).collect();
        let b_vals: Vec<f32> = (0..hidden * rank).map(|i| i as f32 * 0.02).collect();
        let a_blob: Vec<u8> = a_vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let b_blob: Vec<u8> = b_vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let a_view = TensorView::new(
            safetensors::Dtype::F32,
            vec![rank, hidden],
            a_blob.as_slice(),
        )
        .unwrap();
        let b_view = TensorView::new(
            safetensors::Dtype::F32,
            vec![hidden, rank],
            b_blob.as_slice(),
        )
        .unwrap();
        let tensors = vec![
            ("lora_a.q_proj".to_string(), a_view),
            ("lora_b.q_proj".to_string(), b_view),
        ];
        let mut bytes = safetensors::serialize(tensors, &Default::default()).unwrap();

        // Truncate the buffer to simulate a partial write/corruption
        bytes.truncate(bytes.len().saturating_sub(7));

        let err = LoRAAdapter::from_safetensors_bytes("test".into(), &bytes)
            .expect_err("expected error for truncated safetensors");
        let msg = format!("{}", err);
        assert!(msg.contains("failed to deserialize safetensors"));
    }

    #[test]
    fn test_roundtrip_rank1_hidden_odd() {
        // Explicit coverage for rank=1, hidden=5
        let rank = 1usize;
        let hidden = 5usize;
        let modules = ["q_proj", "o_proj"];

        fn as_le_bytes(vals: &[f32]) -> Vec<u8> {
            let mut out = Vec::with_capacity(vals.len() * 4);
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out
        }

        // Keep blobs alive while views borrow from them
        let mut blobs: Vec<Vec<u8>> = Vec::with_capacity(modules.len() * 2);
        let mut map: Vec<(usize, usize, &str)> = Vec::with_capacity(modules.len());
        for m in modules.iter() {
            let a_vals: Vec<f32> = (0..rank * hidden).map(|i| 0.1 + i as f32 * 0.01).collect();
            let b_vals: Vec<f32> = (0..hidden * rank).map(|i| -0.2 + i as f32 * 0.02).collect();
            let a_idx = blobs.len();
            blobs.push(as_le_bytes(&a_vals));
            let b_idx = blobs.len();
            blobs.push(as_le_bytes(&b_vals));
            map.push((a_idx, b_idx, *m));
        }
        let mut entries: Vec<(String, TensorView)> = Vec::with_capacity(modules.len() * 2);
        for (a_idx, b_idx, m) in map.into_iter() {
            let a_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![rank, hidden],
                blobs[a_idx].as_slice(),
            )
            .unwrap();
            let b_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![hidden, rank],
                blobs[b_idx].as_slice(),
            )
            .unwrap();
            entries.push((format!("lora_a.{}", m), a_view));
            entries.push((format!("lora_b.{}", m), b_view));
        }

        let bytes = safetensors::serialize(entries, &Default::default()).unwrap();
        let parsed = LoRAAdapter::from_safetensors_bytes("odd".into(), &bytes).unwrap();
        for m in modules.iter() {
            assert!(parsed.has_module(m));
            assert_eq!(parsed.module_shape(m), Some((rank, hidden)));
        }
    }
}
