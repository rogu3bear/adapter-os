//! Metal buffer purgeable state management
//!
//! Provides abstraction over MTLPurgeableState for KV cache memory management.
//! HOT entries are marked non-purgeable to prevent OS reclamation.
//!
//! Reference: https://developer.apple.com/documentation/metal/mtlpurgeablestate

use adapteros_core::{AosError, Result};

/// Metal purgeable state abstraction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeableState {
    /// Keep current state (query-only)
    KeepCurrent,
    /// Contents are non-volatile and cannot be purged
    NonVolatile,
    /// Contents are volatile and may be purged under memory pressure
    Volatile,
    /// Contents were purged (read-only status after setPurgeableState call)
    Empty,
}

/// Result of setting purgeable state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurgeableResult {
    /// Previous state before change
    pub previous: PurgeableState,
    /// Whether contents were purged before state change
    pub was_purged: bool,
}

/// Trait for buffers supporting purgeable state management
pub trait PurgeableBuffer {
    /// Set the purgeable state of this buffer
    fn set_purgeable_state(&self, state: PurgeableState) -> Result<PurgeableResult>;

    /// Make buffer non-purgeable (HOT protection)
    fn make_non_purgeable(&self) -> Result<PurgeableResult> {
        self.set_purgeable_state(PurgeableState::NonVolatile)
    }

    /// Make buffer purgeable (COLD/evictable)
    fn make_purgeable(&self) -> Result<PurgeableResult> {
        self.set_purgeable_state(PurgeableState::Volatile)
    }

    /// Check if purgeable state management is supported
    fn supports_purgeable_state(&self) -> bool;
}

#[cfg(target_os = "macos")]
mod metal_impl {
    use super::*;
    use metal::foreign_types::ForeignType;
    use metal::Buffer;
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    /// MTLPurgeableState values from Metal framework
    #[repr(u64)]
    #[derive(Debug, Clone, Copy)]
    pub enum MTLPurgeableState {
        KeepCurrent = 1,
        NonVolatile = 2,
        Volatile = 3,
        Empty = 4,
    }

    impl From<PurgeableState> for MTLPurgeableState {
        fn from(state: PurgeableState) -> Self {
            match state {
                PurgeableState::KeepCurrent => MTLPurgeableState::KeepCurrent,
                PurgeableState::NonVolatile => MTLPurgeableState::NonVolatile,
                PurgeableState::Volatile => MTLPurgeableState::Volatile,
                PurgeableState::Empty => MTLPurgeableState::Empty,
            }
        }
    }

    impl From<u64> for PurgeableState {
        fn from(value: u64) -> Self {
            match value {
                1 => PurgeableState::KeepCurrent,
                2 => PurgeableState::NonVolatile,
                3 => PurgeableState::Volatile,
                4 => PurgeableState::Empty,
                _ => PurgeableState::NonVolatile, // Safe default
            }
        }
    }

    impl PurgeableBuffer for Buffer {
        #[allow(unexpected_cfgs)]
        fn set_purgeable_state(&self, state: PurgeableState) -> Result<PurgeableResult> {
            let mtl_state: MTLPurgeableState = state.into();

            // SAFETY: Metal Buffer implements MTLResource which has setPurgeableState:
            let previous: u64 = unsafe {
                let raw: *mut Object = self.as_ptr() as *mut Object;
                objc::msg_send![raw, setPurgeableState: mtl_state as u64]
            };

            let previous_state = PurgeableState::from(previous);

            Ok(PurgeableResult {
                previous: previous_state,
                was_purged: matches!(previous_state, PurgeableState::Empty),
            })
        }

        fn supports_purgeable_state(&self) -> bool {
            true
        }
    }
}

#[cfg(target_os = "macos")]
pub use metal_impl::*;

/// No-op implementation for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub struct NoOpPurgeableBuffer;

#[cfg(not(target_os = "macos"))]
impl PurgeableBuffer for NoOpPurgeableBuffer {
    fn set_purgeable_state(&self, _state: PurgeableState) -> Result<PurgeableResult> {
        Ok(PurgeableResult {
            previous: PurgeableState::NonVolatile,
            was_purged: false,
        })
    }

    fn supports_purgeable_state(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_purgeable_state_display() {
        assert_eq!(format!("{:?}", PurgeableState::NonVolatile), "NonVolatile");
        assert_eq!(format!("{:?}", PurgeableState::Volatile), "Volatile");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_metal_buffer_purgeable_state() {
        use metal::Device;

        let device = Device::system_default().expect("Metal device should be available");
        let buffer = device.new_buffer(1024, metal::MTLResourceOptions::StorageModeShared);

        // Test make_non_purgeable
        let result = buffer.make_non_purgeable().expect("Should succeed");
        assert!(!result.was_purged);

        // Test make_purgeable
        let result = buffer.make_purgeable().expect("Should succeed");
        assert_eq!(result.previous, PurgeableState::NonVolatile);
    }
}
