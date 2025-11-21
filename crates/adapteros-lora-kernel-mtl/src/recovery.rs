//! Panic recovery for Metal kernel dispatch
//!
//! Wraps Metal kernel dispatch calls in catch_unwind boundaries to prevent
//! GPU panics from taking down the entire worker. When a panic is caught,
//! the device is marked as degraded and requires explicit recovery.

use adapteros_core::{AosError, Result};
use std::panic::{catch_unwind, AssertUnwindSafe};
use tracing::{error, info};

/// Recovery wrapper for Metal kernel dispatch
///
/// Catches panics during kernel execution and marks the device
/// as degraded. Requires explicit recovery before further use.
pub struct RecoveryWrapper {
    /// True if a panic was caught and device is degraded
    pub degraded: bool,
    /// Counter for total panics caught
    panic_count: usize,
}

impl RecoveryWrapper {
    /// Create a new recovery wrapper
    pub fn new() -> Self {
        Self {
            degraded: false,
            panic_count: 0,
        }
    }

    /// Execute a function with panic recovery
    ///
    /// Catches panics and marks the wrapper as degraded. The function
    /// should contain only the Metal dispatch call, not application logic,
    /// to avoid masking bugs in business logic.
    ///
    /// # Arguments
    /// * `f` - Function to execute (typically a Metal kernel dispatch)
    ///
    /// # Returns
    /// - `Ok(T)` if function succeeds
    /// - `Err(AosError::Kernel)` if panic is caught
    pub fn safe_dispatch<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T> + std::panic::UnwindSafe,
    {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(result) => result,
            Err(panic_err) => {
                self.degraded = true;
                self.panic_count += 1;

                let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                error!(
                    panic_count = self.panic_count,
                    panic_message = %panic_msg,
                    "Metal kernel panic caught - device marked as degraded"
                );

                Err(AosError::Kernel(format!(
                    "Kernel panic - device marked degraded: {}",
                    panic_msg
                )))
            }
        }
    }

    /// Check if device is degraded
    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// Get total panic count
    pub fn panic_count(&self) -> usize {
        self.panic_count
    }

    /// Attempt recovery
    ///
    /// Clears the degraded flag. In a real implementation, this should
    /// recreate Metal command queues and pipelines. The device reference
    /// is provided for future use.
    ///
    /// # Safety
    /// Caller must ensure Metal resources are in a clean state before
    /// calling this method. Typically involves recreating the MetalKernels
    /// instance entirely.
    pub fn attempt_recovery(&mut self, _device: &metal::Device) -> Result<()> {
        if !self.degraded {
            return Ok(());
        }

        // In production, this would:
        // 1. Destroy old command queue
        // 2. Release any held buffers
        // 3. Create new command queue
        // 4. Verify with a test dispatch

        self.degraded = false;
        info!("Recovery attempted - device unmarked as degraded");
        Ok(())
    }

    /// Require health check before allowing dispatch
    ///
    /// Returns error if device is degraded
    pub fn health_check(&self) -> Result<()> {
        if self.degraded {
            Err(AosError::Kernel(
                "Device degraded - recovery required before dispatch".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for RecoveryWrapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_wrapper_starts_healthy() {
        let wrapper = RecoveryWrapper::new();
        assert!(!wrapper.is_degraded());
        assert_eq!(wrapper.panic_count(), 0);
        assert!(wrapper.health_check().is_ok());
    }

    #[test]
    fn test_successful_dispatch() {
        let mut wrapper = RecoveryWrapper::new();

        let result = wrapper.safe_dispatch(|| Ok(42));

        assert!(result.is_ok());
        assert_eq!(result.expect("Test dispatch should succeed"), 42);
        assert!(!wrapper.is_degraded());
    }

    #[test]
    fn test_panic_caught() {
        let mut wrapper = RecoveryWrapper::new();

        let result = wrapper.safe_dispatch(|| {
            panic!("Test panic");
            #[allow(unreachable_code)]
            Ok(())
        });

        assert!(result.is_err());
        assert!(wrapper.is_degraded());
        assert_eq!(wrapper.panic_count(), 1);
        assert!(wrapper.health_check().is_err());
    }

    #[test]
    fn test_recovery_clears_degraded() {
        let mut wrapper = RecoveryWrapper::new();

        // Trigger panic
        let _ = wrapper.safe_dispatch(|| {
            panic!("Test panic");
            #[allow(unreachable_code)]
            Ok(())
        });

        assert!(wrapper.is_degraded());

        // Attempt recovery (we need a mock device, but for unit test just use None pattern)
        #[cfg(target_os = "macos")]
        {
            let device =
                metal::Device::system_default().expect("Metal device should be available for test");
            wrapper
                .attempt_recovery(&device)
                .expect("Test recovery should succeed");
            assert!(!wrapper.is_degraded());
            assert!(wrapper.health_check().is_ok());
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Can't test recovery without Metal, but at least test the API
            assert!(wrapper.is_degraded());
        }
    }

    #[test]
    fn test_multiple_panics_increment_counter() {
        let mut wrapper = RecoveryWrapper::new();

        // First panic
        let _ = wrapper.safe_dispatch(|| {
            panic!("Panic 1");
            #[allow(unreachable_code)]
            Ok(())
        });
        assert_eq!(wrapper.panic_count(), 1);

        // Second panic (even though degraded)
        let _ = wrapper.safe_dispatch(|| {
            panic!("Panic 2");
            #[allow(unreachable_code)]
            Ok(())
        });
        assert_eq!(wrapper.panic_count(), 2);
    }
}
