//! FFI Error Handling Utilities
//!
//! Provides centralized error handling for MLX FFI operations including:
//! - Error extraction and clearing from C++ error state
//! - RAII guards for automatic resource cleanup
//! - Result wrappers for common FFI patterns

use adapteros_core::AosError;
use std::ffi::CStr;

use crate::{mlx_array_free, mlx_array_t, mlx_clear_error, mlx_get_last_error};

/// Get the last FFI error message and clear the error state.
///
/// This function retrieves any pending error from the C++ layer,
/// converts it to a Rust String, and clears the error state for
/// subsequent operations.
///
/// # Returns
/// - `Some(error_string)` if an error was pending
/// - `None` if no error was set
///
/// # Example
/// ```ignore
/// if let Some(err) = get_and_clear_ffi_error() {
///     return Err(AosError::Mlx(format!("Operation failed: {}", err)));
/// }
/// ```
#[inline]
pub fn get_and_clear_ffi_error() -> Option<String> {
    // SAFETY: FFI error handling contract:
    // 1. mlx_get_last_error() returns either null (no error) or a pointer to
    //    a static or thread-local C string that remains valid until the next
    //    FFI call or mlx_clear_error().
    // 2. We copy the string immediately via to_string_lossy().to_string()
    //    before any other FFI calls, so the pointer remains valid.
    // 3. CStr::from_ptr() is safe because we check for null first.
    // 4. mlx_clear_error() resets internal error state; safe to call anytime.
    unsafe {
        let error_msg = mlx_get_last_error();
        if error_msg.is_null() {
            return None;
        }

        let error_str = CStr::from_ptr(error_msg).to_string_lossy().to_string();

        mlx_clear_error();

        if error_str.is_empty() {
            None
        } else {
            Some(error_str)
        }
    }
}

/// Get the last FFI error or return a default message.
///
/// Convenience wrapper that always returns a string, using the provided
/// default if no error was set.
///
/// # Arguments
/// * `default` - Message to use if no error was set
///
/// # Example
/// ```ignore
/// let error = get_ffi_error_or("Unknown error");
/// return Err(AosError::Mlx(format!("Failed: {}", error)));
/// ```
#[inline]
pub fn get_ffi_error_or(default: &str) -> String {
    get_and_clear_ffi_error().unwrap_or_else(|| default.to_string())
}

/// Check if a pointer result is valid and return an appropriate error if not.
///
/// This is a common pattern for FFI functions that return pointers:
/// - Check if the result is null
/// - If null, retrieve the FFI error message
/// - Return a properly formatted AosError
///
/// # Arguments
/// * `ptr` - The pointer result from an FFI call
/// * `context` - Description of the operation for error messages
///
/// # Returns
/// * `Ok(ptr)` if the pointer is non-null
/// * `Err(AosError::Mlx(...))` if the pointer is null
///
/// # Example
/// ```ignore
/// let array = unsafe { mlx_array_from_ints(data.as_ptr(), data.len() as i32) };
/// let array = check_ffi_ptr(array, "create input array")?;
/// ```
#[inline]
pub fn check_ffi_ptr<T>(ptr: *mut T, context: &str) -> Result<*mut T, AosError> {
    if ptr.is_null() {
        let error = get_ffi_error_or("Unknown error");
        Err(AosError::Mlx(format!("Failed to {}: {}", context, error)))
    } else {
        Ok(ptr)
    }
}

/// Check an FFI result code and return an appropriate error if non-zero.
///
/// # Arguments
/// * `result` - The result code (0 = success, non-zero = failure)
/// * `context` - Description of the operation for error messages
///
/// # Returns
/// * `Ok(())` if result is 0
/// * `Err(AosError::Mlx(...))` if result is non-zero
#[inline]
pub fn check_ffi_result(result: i32, context: &str) -> Result<(), AosError> {
    if result != 0 {
        let error = get_ffi_error_or("Unknown error");
        Err(AosError::Mlx(format!("Failed to {}: {}", context, error)))
    } else {
        Ok(())
    }
}

/// Clear FFI error state before an operation.
///
/// Call this before FFI operations to ensure a clean error state.
#[inline]
pub fn clear_ffi_error() {
    // SAFETY: mlx_clear_error() resets internal error state in the C++ wrapper.
    // It has no return value and does not access any Rust state. This is a
    // simple state reset that is always safe to call.
    unsafe {
        mlx_clear_error();
    }
}

/// RAII guard for MLX arrays that automatically frees the array on drop.
///
/// This prevents memory leaks by ensuring `mlx_array_free` is called
/// even when errors occur or control flow exits early.
///
/// # Example
/// ```ignore
/// let input = MlxArrayGuard::new(unsafe {
///     mlx_array_from_ints(data.as_ptr(), data.len() as i32)
/// })?;
///
/// // Use input.as_ptr() for FFI calls
/// let output = unsafe { mlx_model_forward(model, input.as_ptr()) };
///
/// // input is automatically freed when it goes out of scope
/// ```
pub struct MlxArrayGuard {
    ptr: *mut mlx_array_t,
}

impl MlxArrayGuard {
    /// Create a new guard from an FFI array pointer.
    ///
    /// # Arguments
    /// * `ptr` - The array pointer to manage
    ///
    /// # Returns
    /// * `Ok(guard)` if pointer is non-null
    /// * `Err(...)` if pointer is null (with FFI error message)
    #[inline]
    pub fn new(ptr: *mut mlx_array_t) -> Result<Self, AosError> {
        if ptr.is_null() {
            let error = get_ffi_error_or("Failed to create array");
            Err(AosError::Mlx(error))
        } else {
            Ok(Self { ptr })
        }
    }

    /// Create a guard from a pointer, with a custom error context.
    #[inline]
    pub fn new_with_context(ptr: *mut mlx_array_t, context: &str) -> Result<Self, AosError> {
        if ptr.is_null() {
            let error = get_ffi_error_or("Unknown error");
            Err(AosError::Mlx(format!("Failed to {}: {}", context, error)))
        } else {
            Ok(Self { ptr })
        }
    }

    /// Create a guard from a pointer that may be null.
    ///
    /// Returns `None` for null pointers instead of an error.
    #[inline]
    pub fn new_optional(ptr: *mut mlx_array_t) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    /// Get the raw pointer for FFI calls.
    #[inline]
    pub fn as_ptr(&self) -> *mut mlx_array_t {
        self.ptr
    }

    /// Check if the guard holds a null pointer.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    /// Take ownership of the pointer without freeing it.
    ///
    /// Use this when transferring ownership to another FFI function
    /// or when the pointer will be freed by other means.
    #[inline]
    pub fn into_raw(self) -> *mut mlx_array_t {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for MlxArrayGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: The pointer was obtained from an MLX FFI function during
            // construction (new/new_with_context). The null check ensures we
            // don't double-free. mlx_array_free() is idempotent for valid
            // pointers and handles cleanup of the underlying MLX array.
            unsafe {
                mlx_array_free(self.ptr);
            }
        }
    }
}

/// Guard for multiple MLX arrays.
///
/// Useful when an operation produces multiple arrays that all need cleanup.
pub struct MlxArrayVecGuard {
    arrays: Vec<*mut mlx_array_t>,
}

impl MlxArrayVecGuard {
    /// Create a new empty guard.
    pub fn new() -> Self {
        Self { arrays: Vec::new() }
    }

    /// Add an array to be managed.
    pub fn push(&mut self, ptr: *mut mlx_array_t) {
        if !ptr.is_null() {
            self.arrays.push(ptr);
        }
    }

    /// Get all managed pointers.
    pub fn as_slice(&self) -> &[*mut mlx_array_t] {
        &self.arrays
    }

    /// Take ownership of all pointers without freeing them.
    pub fn into_raw(self) -> Vec<*mut mlx_array_t> {
        let arrays = self.arrays.clone();
        std::mem::forget(self);
        arrays
    }
}

impl Default for MlxArrayVecGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MlxArrayVecGuard {
    fn drop(&mut self) {
        for ptr in &self.arrays {
            if !ptr.is_null() {
                // SAFETY: Each pointer was added via push() which rejects null
                // pointers. The pointers were obtained from MLX FFI functions.
                // mlx_array_free() is safe to call for any valid MLX array pointer.
                unsafe {
                    mlx_array_free(*ptr);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ffi_error_or_default() {
        // When no error is set, should return default
        clear_ffi_error();
        let result = get_ffi_error_or("default message");
        assert_eq!(result, "default message");
    }

    #[test]
    fn test_check_ffi_ptr_null() {
        let null_ptr: *mut u8 = std::ptr::null_mut();
        clear_ffi_error();
        let result = check_ffi_ptr(null_ptr, "test operation");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("test operation"));
    }

    #[test]
    fn test_check_ffi_ptr_valid() {
        let mut value: u8 = 42;
        let ptr: *mut u8 = &mut value;
        let result = check_ffi_ptr(ptr, "test operation");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ptr);
    }

    #[test]
    fn test_check_ffi_result_success() {
        let result = check_ffi_result(0, "test operation");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_ffi_result_failure() {
        clear_ffi_error();
        let result = check_ffi_result(-1, "test operation");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("test operation"));
    }

    #[test]
    fn test_array_guard_null() {
        let result = MlxArrayGuard::new(std::ptr::null_mut());
        assert!(result.is_err());
    }

    #[test]
    fn test_array_guard_optional_null() {
        let result = MlxArrayGuard::new_optional(std::ptr::null_mut());
        assert!(result.is_none());
    }

    #[test]
    fn test_array_vec_guard_empty() {
        let guard = MlxArrayVecGuard::new();
        assert!(guard.as_slice().is_empty());
        // Should not panic on drop
    }
}
