#![cfg(all(test, feature = "extended-tests"))]

//! # AdapterOS Unit Testing Framework
//!
//! A comprehensive unit testing framework designed specifically for the unique
//! requirements of a deterministic inference runtime with async components,
//! Metal kernels, and evidence-grounded responses.
//!
//! ## Architecture
//!
//! The framework is organized into specialized modules:
//!
//! - **`mocks`**: Deterministic mocking utilities for controlled test doubles
//! - **`isolation`**: Component isolation helpers for testing in minimal environments
//! - **`property`**: Property-based testing infrastructure for mathematical properties
//! - **`async_utils`**: Async component testing utilities with timeout and determinism
//! - **`metal`**: Metal kernel testing helpers for GPU operations
//! - **`evidence`**: Evidence-grounded response testing utilities
//!
//! ## Key Design Principles
//!
//! 1. **Determinism**: All test utilities produce reproducible results
//! 2. **Isolation**: Components can be tested with minimal external dependencies
//! 3. **Composability**: Utilities can be combined for complex test scenarios
//! 4. **Performance**: Minimal overhead compared to real implementations
//! 5. **Cross-Crate**: Framework can be reused across all AdapterOS crates
//!
//! ## Usage Examples
//!
//! ### Basic Mocking
//! ```rust
//! use adapteros_unit_testing::mocks::*;
//!
//! let mock = DeterministicRng::from_seed(42);
//! let value = mock.gen_range(0..100); // Always returns the same value
//! ```
//!
//! ### Component Isolation
//! ```rust
//! use adapteros_unit_testing::isolation::*;
//!
//! let sandbox = TestSandbox::new();
//! let isolated = IsolatedComponent::new(my_component);
//! // Test with controlled file system and dependencies
//! ```
//!
//! ### Property-Based Testing
//! ```rust
//! use adapteros_unit_testing::property::*;
//!
//! let property = hash_deterministic_property();
//! let result = check_property(property, 1000);
//! assert!(result.is_passed());
//! ```
//!
//! ### Async Testing
//! ```rust
//! use adapteros_unit_testing::async_utils::*;
//!
//! let timeout = Timeout::new(Duration::from_secs(5));
//! let result = timeout.run(my_async_function()).await;
//! ```
//!
//! ### Metal Kernel Testing
//! ```rust
//! use adapteros_unit_testing::metal::*;
//!
//! let tester = MetalKernelTester::new();
//! let result = tester.test_kernel_compilation("my_kernel");
//! ```
//!
//! ### Evidence Validation
//! ```rust
//! use adapteros_unit_testing::evidence::*;
//!
//! let validator = EvidenceValidator::new();
//! let result = validator.validate_response(&my_response);
//! assert!(result.is_valid);
//! ```

// Re-export all modules for easy access
pub mod mocks;
pub mod isolation;
pub mod property;
pub mod async_utils;
pub mod metal;
pub mod evidence;

// Re-export commonly used types and functions
pub use mocks::*;
pub use isolation::*;
pub use property::*;
pub use async_utils::*;
pub use metal::*;
pub use evidence::*;

/// Version information for the testing framework
pub const FRAMEWORK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Get framework information
pub fn framework_info() -> FrameworkInfo {
    FrameworkInfo {
        version: FRAMEWORK_VERSION.to_string(),
        modules: vec![
            "mocks".to_string(),
            "isolation".to_string(),
            "property".to_string(),
            "async_utils".to_string(),
            "metal".to_string(),
            "evidence".to_string(),
        ],
        description: "Comprehensive unit testing framework for AdapterOS".to_string(),
    }
}

/// Framework information structure
#[derive(Debug, Clone)]
pub struct FrameworkInfo {
    pub version: String,
    pub modules: Vec<String>,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framework_info() {
        let info = framework_info();
        assert!(!info.version.is_empty());
        assert_eq!(info.modules.len(), 6);
        assert!(info.modules.contains(&"mocks".to_string()));
        assert!(info.modules.contains(&"evidence".to_string()));
    }

    #[test]
    fn test_module_imports() {
        // Test that all modules can be imported
        let _mock = DeterministicRng::from_seed(42);
        let _sandbox = TestSandbox::new();
        let _property = hash_deterministic_property();
        let _timeout = Timeout::new(std::time::Duration::from_secs(1));
        let _tester = MetalKernelTester::new();
        let _validator = EvidenceValidator::new();
    }
}</code>
