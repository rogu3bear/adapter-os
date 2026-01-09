//! MLX GPU Training Operations
//!
//! This module provides GPU-accelerated training primitives for LoRA fine-tuning,
//! including loss computation, gradient calculation via MLX autograd, and optimizers.
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_lora_mlx_ffi::training::{
//!     MlxOptimizer, MlxOptimizerType,
//!     mlx_lora_backward_gpu, mlx_clip_grad_norm_gpu,
//! };
//!
//! // Create Adam optimizer
//! let mut optimizer = MlxOptimizer::adam(0.001, 0.9, 0.999, 1e-8, 0.0)?;
//!
//! // Compute gradients
//! let (loss, grad_a, grad_b) =
//!     mlx_lora_backward_gpu(&hidden, &targets, &lora_a, &lora_b, 16.0, 16, 42)?;
//!
//! // Clip gradients
//! let grad_norm = mlx_clip_grad_norm_gpu(&mut [grad_a, grad_b], 1.0);
//!
//! // Apply optimizer step
//! optimizer.step(&mut [lora_a, lora_b], &[grad_a, grad_b])?;
//! ```

use crate::ffi_error;
use crate::tensor::MLXFFITensor;
use adapteros_core::{AosError, Result};
use std::ffi::c_void;
use std::ptr;

// ============================================================================
// FFI Declarations
// ============================================================================

// Loss functions
extern "C" {
    fn mlx_cross_entropy_loss(
        logits: *mut c_void,
        targets: *mut c_void,
        ignore_index: i32,
    ) -> *mut c_void;

    fn mlx_mse_loss(predictions: *mut c_void, targets: *mut c_void) -> *mut c_void;
}

// Gradient computation
extern "C" {
    fn mlx_lora_backward(
        hidden: *mut c_void,
        targets: *mut c_void,
        lora_a: *mut c_void,
        lora_b: *mut c_void,
        alpha: f32,
        rank: i32,
        seed: u64,
        out_loss: *mut f32,
        out_grad_a: *mut *mut c_void,
        out_grad_b: *mut *mut c_void,
    ) -> i32;

    fn mlx_lora_backward_ce(
        hidden: *mut c_void,
        output_proj: *mut c_void,
        targets: *mut c_void,
        lora_a: *mut c_void,
        lora_b: *mut c_void,
        alpha: f32,
        rank: i32,
        ignore_index: i32,
        seed: u64,
        out_loss: *mut f32,
        out_grad_a: *mut *mut c_void,
        out_grad_b: *mut *mut c_void,
    ) -> i32;
}

// Optimizer operations
extern "C" {
    fn mlx_optimizer_sgd(learning_rate: f32, momentum: f32, weight_decay: f32) -> *mut c_void;

    fn mlx_optimizer_adam(
        learning_rate: f32,
        beta1: f32,
        beta2: f32,
        eps: f32,
        weight_decay: f32,
    ) -> *mut c_void;

    fn mlx_optimizer_step(
        optimizer: *mut c_void,
        params: *mut *mut c_void,
        grads: *mut *mut c_void,
        num_params: i32,
    ) -> i32;

    fn mlx_optimizer_set_lr(optimizer: *mut c_void, lr: f32);
    fn mlx_optimizer_get_lr(optimizer: *mut c_void) -> f32;
    fn mlx_optimizer_reset(optimizer: *mut c_void);
    fn mlx_optimizer_free(optimizer: *mut c_void);
}

// Gradient utilities
extern "C" {
    fn mlx_clip_grad_norm(grads: *mut *mut c_void, num_grads: i32, max_norm: f32) -> f32;

    fn mlx_zero_grad(grads: *mut *mut c_void, num_grads: i32);
}

// ============================================================================
// Loss Functions
// ============================================================================

/// Compute cross-entropy loss on GPU.
///
/// # Arguments
/// * `logits` - Model output logits tensor
/// * `targets` - Target token IDs tensor
/// * `ignore_index` - Token ID to ignore (e.g., padding), use -1 to disable
///
/// # Returns
/// Scalar loss value as a tensor
pub fn mlx_cross_entropy_loss_gpu(
    logits: &MLXFFITensor,
    targets: &MLXFFITensor,
    ignore_index: i32,
) -> Result<MLXFFITensor> {
    ffi_error::clear_ffi_error();

    let result = unsafe {
        mlx_cross_entropy_loss(
            logits.as_ptr() as *mut c_void,
            targets.as_ptr() as *mut c_void,
            ignore_index,
        )
    };

    ffi_error::check_ffi_ptr(result, "cross entropy loss").map(|ptr| MLXFFITensor::from_raw(ptr))
}

/// Compute MSE (Mean Squared Error) loss on GPU.
///
/// # Arguments
/// * `predictions` - Model predictions tensor
/// * `targets` - Target values tensor (same shape as predictions)
///
/// # Returns
/// Scalar loss value as a tensor
pub fn mlx_mse_loss_gpu(predictions: &MLXFFITensor, targets: &MLXFFITensor) -> Result<MLXFFITensor> {
    ffi_error::clear_ffi_error();

    let result = unsafe {
        mlx_mse_loss(
            predictions.as_ptr() as *mut c_void,
            targets.as_ptr() as *mut c_void,
        )
    };

    ffi_error::check_ffi_ptr(result, "MSE loss").map(|ptr| MLXFFITensor::from_raw(ptr))
}

// ============================================================================
// Gradient Computation
// ============================================================================

/// LoRA backward pass result containing loss and gradients.
#[derive(Debug)]
pub struct LoraBackwardResult {
    /// Scalar loss value
    pub loss: f32,
    /// Gradient for LoRA A matrix
    pub grad_a: MLXFFITensor,
    /// Gradient for LoRA B matrix
    pub grad_b: MLXFFITensor,
}

/// Compute LoRA backward pass on GPU using MLX autograd.
///
/// This function computes the forward pass, loss, and gradients in a single call,
/// leveraging MLX's automatic differentiation capabilities.
///
/// # Arguments
/// * `hidden` - Input hidden states tensor
/// * `targets` - Target values for loss computation
/// * `lora_a` - LoRA A matrix (down-projection) [rank, hidden_dim]
/// * `lora_b` - LoRA B matrix (up-projection) [hidden_dim, rank]
/// * `alpha` - LoRA scaling factor
/// * `rank` - LoRA rank dimension
/// * `seed` - Deterministic seed for MLX RNG
///
/// # Returns
/// `LoraBackwardResult` containing loss value and gradients for A and B matrices
///
/// # Example
/// ```ignore
/// let result = mlx_lora_backward_gpu(&hidden, &targets, &lora_a, &lora_b, 16.0, 16, 42)?;
/// println!("Loss: {}", result.loss);
/// // Use result.grad_a and result.grad_b for optimizer step
/// ```
pub fn mlx_lora_backward_gpu(
    hidden: &MLXFFITensor,
    targets: &MLXFFITensor,
    lora_a: &MLXFFITensor,
    lora_b: &MLXFFITensor,
    alpha: f32,
    rank: usize,
    seed: u64,
) -> Result<LoraBackwardResult> {
    ffi_error::clear_ffi_error();

    let mut loss: f32 = 0.0;
    let mut grad_a: *mut c_void = ptr::null_mut();
    let mut grad_b: *mut c_void = ptr::null_mut();

    let result = unsafe {
        mlx_lora_backward(
            hidden.as_ptr() as *mut c_void,
            targets.as_ptr() as *mut c_void,
            lora_a.as_ptr() as *mut c_void,
            lora_b.as_ptr() as *mut c_void,
            alpha,
            rank as i32,
            seed,
            &mut loss,
            &mut grad_a,
            &mut grad_b,
        )
    };

    ffi_error::check_ffi_result(result, "LoRA backward")?;

    // Validate gradient pointers
    let grad_a_ptr = ffi_error::check_ffi_ptr(grad_a, "gradient A")?;
    let grad_b_ptr = ffi_error::check_ffi_ptr(grad_b, "gradient B")?;

    Ok(LoraBackwardResult {
        loss,
        grad_a: MLXFFITensor::from_raw(grad_a_ptr),
        grad_b: MLXFFITensor::from_raw(grad_b_ptr),
    })
}

/// Compute LoRA backward pass on GPU with cross-entropy loss for language model training.
///
/// This variant includes output projection to vocabulary space, enabling proper
/// cross-entropy loss computation against target token IDs. Use this for real
/// language model fine-tuning.
///
/// # Arguments
/// * `hidden` - Input hidden states tensor [batch, seq_len, hidden_dim] or [seq_len, hidden_dim]
/// * `output_proj` - Output projection matrix (lm_head weights) [vocab_size, hidden_dim]
/// * `targets` - Target token IDs tensor [batch, seq_len] or [seq_len]
/// * `lora_a` - LoRA A matrix (down-projection) [rank, hidden_dim]
/// * `lora_b` - LoRA B matrix (up-projection) [hidden_dim, rank]
/// * `alpha` - LoRA scaling factor
/// * `rank` - LoRA rank dimension
/// * `ignore_index` - Token ID to ignore in loss (e.g., padding token), use -1 to disable
/// * `seed` - Deterministic seed for MLX RNG
///
/// # Returns
/// `LoraBackwardResult` containing loss value and gradients for A and B matrices
///
/// # Example
/// ```ignore
/// // Load output projection from base model
/// let output_proj = model.get_lm_head_weights()?;
///
/// // Compute backward pass with cross-entropy loss
/// let result = mlx_lora_backward_ce_gpu(
///     &hidden, &output_proj, &target_tokens,
///     &lora_a, &lora_b, 16.0, 16, 0, 42 // ignore padding token 0
/// )?;
/// println!("CE Loss: {}", result.loss);
/// ```
pub fn mlx_lora_backward_ce_gpu(
    hidden: &MLXFFITensor,
    output_proj: &MLXFFITensor,
    targets: &MLXFFITensor,
    lora_a: &MLXFFITensor,
    lora_b: &MLXFFITensor,
    alpha: f32,
    rank: usize,
    ignore_index: i32,
    seed: u64,
) -> Result<LoraBackwardResult> {
    ffi_error::clear_ffi_error();

    let mut loss: f32 = 0.0;
    let mut grad_a: *mut c_void = ptr::null_mut();
    let mut grad_b: *mut c_void = ptr::null_mut();

    let result = unsafe {
        mlx_lora_backward_ce(
            hidden.as_ptr() as *mut c_void,
            output_proj.as_ptr() as *mut c_void,
            targets.as_ptr() as *mut c_void,
            lora_a.as_ptr() as *mut c_void,
            lora_b.as_ptr() as *mut c_void,
            alpha,
            rank as i32,
            ignore_index,
            seed,
            &mut loss,
            &mut grad_a,
            &mut grad_b,
        )
    };

    ffi_error::check_ffi_result(result, "LoRA backward (CE)")?;

    // Validate gradient pointers
    let grad_a_ptr = ffi_error::check_ffi_ptr(grad_a, "gradient A")?;
    let grad_b_ptr = ffi_error::check_ffi_ptr(grad_b, "gradient B")?;

    Ok(LoraBackwardResult {
        loss,
        grad_a: MLXFFITensor::from_raw(grad_a_ptr),
        grad_b: MLXFFITensor::from_raw(grad_b_ptr),
    })
}

// ============================================================================
// Optimizer Types
// ============================================================================

/// Optimizer type selection for GPU training.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MlxOptimizerType {
    /// SGD with optional momentum
    Sgd {
        /// Momentum factor (0.0 for vanilla SGD)
        momentum: f32,
    },
    /// Adam optimizer with bias correction
    Adam {
        /// First moment decay (typically 0.9)
        beta1: f32,
        /// Second moment decay (typically 0.999)
        beta2: f32,
        /// Numerical stability constant (typically 1e-8)
        eps: f32,
    },
}

impl Default for MlxOptimizerType {
    fn default() -> Self {
        MlxOptimizerType::Adam {
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
        }
    }
}

// ============================================================================
// Optimizer
// ============================================================================

/// MLX GPU Optimizer for training.
///
/// Wraps MLX's optimizer implementations (SGD, Adam) for GPU-accelerated
/// parameter updates during training.
///
/// # Example
/// ```ignore
/// // Create Adam optimizer
/// let mut optimizer = MlxOptimizer::adam(0.001, 0.9, 0.999, 1e-8, 0.0)?;
///
/// // Training loop
/// for epoch in 0..num_epochs {
///     let (loss, grad_a, grad_b) = mlx_lora_backward_gpu(..., seed)?;
///     optimizer.step(&mut [lora_a, lora_b], &[grad_a, grad_b])?;
///     optimizer.set_learning_rate(new_lr); // Optional LR schedule
/// }
/// ```
pub struct MlxOptimizer {
    inner: *mut c_void,
    optimizer_type: MlxOptimizerType,
    learning_rate: f32,
    weight_decay: f32,
}

// Safety: MLX handles are thread-safe for read operations within a single thread.
// Cross-thread usage requires external synchronization.
unsafe impl Send for MlxOptimizer {}

impl MlxOptimizer {
    /// Create an SGD optimizer with optional momentum.
    ///
    /// # Arguments
    /// * `learning_rate` - Learning rate for parameter updates
    /// * `momentum` - Momentum factor (0.0 for vanilla SGD)
    /// * `weight_decay` - L2 regularization factor (0.0 to disable)
    pub fn sgd(learning_rate: f32, momentum: f32, weight_decay: f32) -> Result<Self> {
        ffi_error::clear_ffi_error();

        let inner = unsafe { mlx_optimizer_sgd(learning_rate, momentum, weight_decay) };

        let inner = ffi_error::check_ffi_ptr(inner, "create SGD optimizer")?;

        Ok(Self {
            inner,
            optimizer_type: MlxOptimizerType::Sgd { momentum },
            learning_rate,
            weight_decay,
        })
    }

    /// Create an Adam optimizer with bias correction.
    ///
    /// # Arguments
    /// * `learning_rate` - Learning rate for parameter updates
    /// * `beta1` - First moment decay (typically 0.9)
    /// * `beta2` - Second moment decay (typically 0.999)
    /// * `eps` - Numerical stability constant (typically 1e-8)
    /// * `weight_decay` - L2 regularization factor (0.0 to disable)
    pub fn adam(
        learning_rate: f32,
        beta1: f32,
        beta2: f32,
        eps: f32,
        weight_decay: f32,
    ) -> Result<Self> {
        ffi_error::clear_ffi_error();

        let inner = unsafe { mlx_optimizer_adam(learning_rate, beta1, beta2, eps, weight_decay) };

        let inner = ffi_error::check_ffi_ptr(inner, "create Adam optimizer")?;

        Ok(Self {
            inner,
            optimizer_type: MlxOptimizerType::Adam { beta1, beta2, eps },
            learning_rate,
            weight_decay,
        })
    }

    /// Create an optimizer from type configuration.
    pub fn from_config(
        opt_type: MlxOptimizerType,
        learning_rate: f32,
        weight_decay: f32,
    ) -> Result<Self> {
        match opt_type {
            MlxOptimizerType::Sgd { momentum } => Self::sgd(learning_rate, momentum, weight_decay),
            MlxOptimizerType::Adam { beta1, beta2, eps } => {
                Self::adam(learning_rate, beta1, beta2, eps, weight_decay)
            }
        }
    }

    /// Apply optimizer step to update parameters based on gradients.
    ///
    /// Parameters are updated in-place. The number of parameters and gradients
    /// must match.
    ///
    /// # Arguments
    /// * `params` - Mutable slice of parameter tensors to update
    /// * `grads` - Slice of gradient tensors (same order as params)
    pub fn step(&mut self, params: &mut [MLXFFITensor], grads: &[MLXFFITensor]) -> Result<()> {
        if params.len() != grads.len() {
            return Err(AosError::Validation(
                "params and grads must have same length".to_string(),
            ));
        }

        if params.is_empty() {
            return Ok(());
        }

        // Collect raw pointers
        let mut param_ptrs: Vec<*mut c_void> =
            params.iter().map(|p| p.as_ptr() as *mut c_void).collect();
        let mut grad_ptrs: Vec<*mut c_void> =
            grads.iter().map(|g| g.as_ptr() as *mut c_void).collect();

        ffi_error::clear_ffi_error();

        let result = unsafe {
            mlx_optimizer_step(
                self.inner,
                param_ptrs.as_mut_ptr(),
                grad_ptrs.as_mut_ptr(),
                params.len() as i32,
            )
        };

        ffi_error::check_ffi_result(result, "optimizer step")
    }

    /// Set the learning rate.
    pub fn set_learning_rate(&mut self, lr: f32) {
        self.learning_rate = lr;
        unsafe { mlx_optimizer_set_lr(self.inner, lr) };
    }

    /// Get the current learning rate.
    pub fn learning_rate(&self) -> f32 {
        self.learning_rate
    }

    /// Get the optimizer type.
    pub fn optimizer_type(&self) -> MlxOptimizerType {
        self.optimizer_type
    }

    /// Get the weight decay factor.
    pub fn weight_decay(&self) -> f32 {
        self.weight_decay
    }

    /// Reset optimizer state (momentum/moment estimates).
    ///
    /// Call this when starting a new training run or after significant
    /// changes to the training setup.
    pub fn reset(&mut self) {
        unsafe { mlx_optimizer_reset(self.inner) };
    }
}

impl Drop for MlxOptimizer {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { mlx_optimizer_free(self.inner) };
        }
    }
}

// ============================================================================
// Gradient Utilities
// ============================================================================

/// Clip gradient norm (in-place modification).
///
/// If the total gradient norm exceeds `max_norm`, all gradients are scaled
/// down proportionally to meet the constraint.
///
/// # Arguments
/// * `grads` - Mutable slice of gradient tensors (modified in-place if clipped)
/// * `max_norm` - Maximum allowed gradient norm
///
/// # Returns
/// The actual gradient norm before clipping (useful for monitoring)
///
/// # Example
/// ```ignore
/// let mut grads = [grad_a, grad_b];
/// let norm = mlx_clip_grad_norm_gpu(&mut grads, 1.0);
/// if norm > 1.0 {
///     println!("Gradients clipped from {} to 1.0", norm);
/// }
/// ```
pub fn mlx_clip_grad_norm_gpu(grads: &mut [MLXFFITensor], max_norm: f32) -> f32 {
    if grads.is_empty() || max_norm <= 0.0 {
        return 0.0;
    }

    let mut grad_ptrs: Vec<*mut c_void> =
        grads.iter().map(|g| g.as_ptr() as *mut c_void).collect();

    unsafe { mlx_clip_grad_norm(grad_ptrs.as_mut_ptr(), grads.len() as i32, max_norm) }
}

/// Zero out gradients (in-place).
///
/// Useful for gradient accumulation reset between batches.
///
/// # Arguments
/// * `grads` - Mutable slice of gradient tensors to zero
pub fn mlx_zero_grad_gpu(grads: &mut [MLXFFITensor]) {
    if grads.is_empty() {
        return;
    }

    let mut grad_ptrs: Vec<*mut c_void> =
        grads.iter().map(|g| g.as_ptr() as *mut c_void).collect();

    unsafe { mlx_zero_grad(grad_ptrs.as_mut_ptr(), grads.len() as i32) };
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimizer_type_default() {
        let default = MlxOptimizerType::default();
        match default {
            MlxOptimizerType::Adam { beta1, beta2, eps } => {
                assert!((beta1 - 0.9).abs() < 1e-6);
                assert!((beta2 - 0.999).abs() < 1e-6);
                assert!((eps - 1e-8).abs() < 1e-10);
            }
            _ => panic!("Expected Adam default"),
        }
    }

    #[test]
    fn test_optimizer_type_sgd() {
        let sgd = MlxOptimizerType::Sgd { momentum: 0.9 };
        match sgd {
            MlxOptimizerType::Sgd { momentum } => {
                assert!((momentum - 0.9).abs() < 1e-6);
            }
            _ => panic!("Expected SGD"),
        }
    }
}
