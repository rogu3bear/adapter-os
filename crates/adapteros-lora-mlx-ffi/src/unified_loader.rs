//! Unified SafeTensors loader with MLX native path and Rust fallback
//!
//! Provides zero-copy GPU loading when MLX is available, falling back
//! to the Rust safetensors crate for compatibility.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::path::Path;

/// Loading strategy preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadStrategy {
    /// Try MLX native first, fallback to Rust crate
    #[default]
    MlxPreferred,
    /// Always use Rust crate
    RustOnly,
}

/// Tensor metadata
#[derive(Debug, Clone)]
pub struct TensorMetadata {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub size_bytes: usize,
}

/// Unified SafeTensors loader
pub struct UnifiedSafeTensorsLoader {
    /// MLX weights handle (if loaded via MLX)
    mlx_weights: Option<*mut crate::mlx_weights_t>,
    /// Rust safetensors data (if loaded via Rust)
    rust_data: Option<Vec<u8>>,
    /// Tensor metadata
    tensor_info: HashMap<String, TensorMetadata>,
    /// Strategy used
    strategy_used: LoadStrategy,
}

impl UnifiedSafeTensorsLoader {
    /// Load safetensors file with specified strategy
    pub fn load<P: AsRef<Path>>(path: P, strategy: LoadStrategy) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(AosError::NotFound(format!(
                "SafeTensors file not found: {}",
                path.display()
            )));
        }

        match strategy {
            LoadStrategy::MlxPreferred => {
                // Try MLX native first
                match Self::load_mlx_native(path) {
                    Ok(loader) => {
                        tracing::info!(path = %path.display(), "Loaded via MLX native");
                        return Ok(loader);
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "MLX native load failed, using Rust fallback"
                        );
                    }
                }
                // Fallback to Rust
                Self::load_rust_crate(path)
            }
            LoadStrategy::RustOnly => Self::load_rust_crate(path),
        }
    }

    /// Load using MLX C++ FFI
    fn load_mlx_native(path: &Path) -> Result<Self> {
        // Ensure MLX is initialized
        crate::mlx_ensure_initialized(true)?;

        let path_str = path.to_string_lossy();
        let path_cstr = std::ffi::CString::new(path_str.as_bytes())
            .map_err(|e| AosError::Internal(format!("Invalid path: {}", e)))?;

        unsafe {
            crate::mlx_clear_error();
            let weights = crate::mlx_load_safetensors(path_cstr.as_ptr());

            if weights.is_null() {
                let err = crate::mlx_get_last_error();
                let err_str = if err.is_null() {
                    "Unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned()
                };
                crate::mlx_clear_error();
                return Err(AosError::Mlx(format!("MLX load failed: {}", err_str)));
            }

            // Get tensor names
            let mut tensor_info = HashMap::new();
            let mut names_ptrs: Vec<*const std::os::raw::c_char> = vec![std::ptr::null(); 512];
            let num = crate::mlx_weights_list(weights, names_ptrs.as_mut_ptr(), 512);

            for i in 0..(num as usize).min(512) {
                if !names_ptrs[i].is_null() {
                    let name = std::ffi::CStr::from_ptr(names_ptrs[i])
                        .to_string_lossy()
                        .into_owned();

                    let name_cstr = std::ffi::CString::new(name.as_bytes()).ok();
                    if let Some(cstr) = name_cstr {
                        let tensor = crate::mlx_weights_get(weights, cstr.as_ptr());
                        if !tensor.is_null() {
                            let size = crate::mlx_array_size(tensor) as usize;
                            let ndim = crate::mlx_array_ndim(tensor) as usize;
                            let mut shape_buf = vec![0i32; 8];
                            crate::mlx_array_shape(tensor, shape_buf.as_mut_ptr(), 8);
                            let shape: Vec<usize> =
                                shape_buf[..ndim].iter().map(|&s| s as usize).collect();

                            tensor_info.insert(
                                name.clone(),
                                TensorMetadata {
                                    name,
                                    shape,
                                    dtype: "float32".to_string(),
                                    size_bytes: size * 4,
                                },
                            );
                        }
                    }
                }
            }

            Ok(Self {
                mlx_weights: Some(weights),
                rust_data: None,
                tensor_info,
                strategy_used: LoadStrategy::MlxPreferred,
            })
        }
    }

    /// Load using Rust safetensors crate
    fn load_rust_crate(path: &Path) -> Result<Self> {
        let data =
            std::fs::read(path).map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

        let tensors = safetensors::SafeTensors::deserialize(&data)
            .map_err(|e| AosError::Parse(format!("Failed to parse: {}", e)))?;

        let mut tensor_info = HashMap::new();
        for (name, _) in tensors.tensors() {
            if let Ok(view) = tensors.tensor(&name) {
                let shape: Vec<usize> = view.shape().to_vec();
                let size: usize = shape.iter().product();
                tensor_info.insert(
                    name.to_string(),
                    TensorMetadata {
                        name: name.to_string(),
                        shape,
                        dtype: format!("{:?}", view.dtype()),
                        size_bytes: size * 4,
                    },
                );
            }
        }

        tracing::info!(path = %path.display(), tensors = tensor_info.len(), "Loaded via Rust crate");

        Ok(Self {
            mlx_weights: None,
            rust_data: Some(data),
            tensor_info,
            strategy_used: LoadStrategy::RustOnly,
        })
    }

    /// Get tensor as f32 vec
    pub fn get_tensor_f32(&self, name: &str) -> Result<Vec<f32>> {
        if let Some(weights) = self.mlx_weights {
            // MLX path
            let name_cstr = std::ffi::CString::new(name)
                .map_err(|e| AosError::Internal(format!("Invalid name: {}", e)))?;

            unsafe {
                let array = crate::mlx_weights_get(weights, name_cstr.as_ptr());
                if array.is_null() {
                    return Err(AosError::NotFound(format!("Tensor not found: {}", name)));
                }

                crate::mlx_eval(array);
                crate::mlx_synchronize();

                let size = crate::mlx_array_size(array) as usize;
                let ptr = crate::mlx_array_data(array);
                if ptr.is_null() {
                    return Err(AosError::Mlx("Failed to get data".to_string()));
                }

                Ok(std::slice::from_raw_parts(ptr, size).to_vec())
            }
        } else if let Some(ref data) = self.rust_data {
            // Rust path
            let tensors = safetensors::SafeTensors::deserialize(data)
                .map_err(|e| AosError::Parse(format!("Parse error: {}", e)))?;

            let view = tensors
                .tensor(name)
                .map_err(|_| AosError::NotFound(format!("Tensor not found: {}", name)))?;

            let bytes = view.data();
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            Ok(floats)
        } else {
            Err(AosError::Internal("No data loaded".to_string()))
        }
    }

    /// List tensor names
    pub fn tensor_names(&self) -> Vec<String> {
        self.tensor_info.keys().cloned().collect()
    }

    /// Get tensor metadata
    pub fn get_metadata(&self, name: &str) -> Option<&TensorMetadata> {
        self.tensor_info.get(name)
    }

    /// Get strategy used
    pub fn strategy_used(&self) -> LoadStrategy {
        self.strategy_used
    }
}

impl Drop for UnifiedSafeTensorsLoader {
    fn drop(&mut self) {
        if let Some(weights) = self.mlx_weights.take() {
            unsafe {
                crate::mlx_weights_free(weights);
            }
        }
    }
}
