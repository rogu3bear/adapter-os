use std::ffi::CStr;

use adapteros_core::{AosError, Result};
use tokenizers::Tokenizer;

use crate::{mlx_array_data, mlx_array_from_uints, mlx_array_size, mlx_get_last_error};

pub const KNOWN_EOS_TOKENS: [&str; 5] = ["<|im_end|>", "</s>", "<eos>", "<|endoftext|>", "<EOS>"];

pub const HIDDEN_STATE_MODULES: [&str; 6] = [
    "q_proj",
    "k_proj",
    "v_proj",
    "o_proj",
    "gate_proj",
    "up_proj",
];

pub fn detect_eos_token(tokenizer: &Tokenizer) -> Result<u32> {
    let vocab = tokenizer.get_vocab(true);
    for token in KNOWN_EOS_TOKENS {
        if let Some(id) = vocab.get(token) {
            return Ok(*id as u32);
        }
        if let Some(id) = tokenizer.token_to_id(token) {
            return Ok(id);
        }
    }

    vocab.values().max().map(|id| *id as u32).ok_or_else(|| {
        AosError::Config("Unable to determine EOS token id from tokenizer".to_string())
    })
}

pub fn last_mlx_error(context: &str) -> AosError {
    let error_ptr = unsafe { mlx_get_last_error() };
    let detail = if error_ptr.is_null() {
        "Unknown MLX error".to_string()
    } else {
        unsafe { CStr::from_ptr(error_ptr).to_string_lossy().to_string() }
    };
    AosError::Mlx(format!("{}: {}", context, detail))
}

pub fn create_token_array(token_ids: &[u32]) -> Result<*mut crate::mlx_array_t> {
    if token_ids.len() > i32::MAX as usize {
        return Err(AosError::Validation(
            "Token sequence exceeds FFI limits".to_string(),
        ));
    }
    let array = unsafe { mlx_array_from_uints(token_ids.as_ptr(), token_ids.len() as i32) };
    if array.is_null() {
        Err(last_mlx_error("Failed to create input array"))
    } else {
        Ok(array)
    }
}

pub fn extract_array(array: *mut crate::mlx_array_t) -> Result<Vec<f32>> {
    if array.is_null() {
        return Err(AosError::Mlx("Received null MLX array".to_string()));
    }
    let size = unsafe { mlx_array_size(array) };
    if size <= 0 {
        return Err(AosError::Mlx("MLX array has no elements".to_string()));
    }
    let data_ptr = unsafe { mlx_array_data(array) };
    if data_ptr.is_null() {
        return Err(AosError::Mlx("Failed to access MLX array data".to_string()));
    }
    let slice = unsafe { std::slice::from_raw_parts(data_ptr, size as usize) };
    Ok(slice.to_vec())
}

pub fn normalize_logits(mut logits: Vec<f32>, expected: usize) -> Vec<f32> {
    if expected > 0 {
        if logits.len() < expected {
            logits.resize(expected, f32::MIN);
        } else if logits.len() > expected {
            logits.truncate(expected);
        }
    }
    logits
}

pub fn sanitize_logits(logits: &mut [f32]) {
    for value in logits {
        if !value.is_finite() {
            *value = f32::MIN;
        }
    }
}

pub fn select_next_token(logits: &[f32]) -> Result<u32> {
    let mut best_idx: Option<usize> = None;
    let mut best_val = f32::NEG_INFINITY;
    for (idx, &value) in logits.iter().enumerate() {
        if !value.is_finite() {
            continue;
        }
        if value > best_val || best_idx.is_none() {
            best_val = value;
            best_idx = Some(idx);
        }
    }

    best_idx
        .map(|idx| idx as u32)
        .ok_or_else(|| AosError::Mlx("Model returned no valid logits".to_string()))
}
