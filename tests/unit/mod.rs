#![cfg(all(test, feature = "extended-tests"))]

//! Unit Testing Framework for AdapterOS Core Components
//!
//! This module provides comprehensive unit testing utilities designed specifically
//! for the unique requirements of a deterministic inference runtime with async
//! components, Metal kernels, and evidence-grounded responses.
//!
//! ## Features
//!
//! - **Deterministic Mocking**: Controlled, reproducible test doubles
//! - **Component Isolation**: Test components in isolation with minimal dependencies
//! - **Property-Based Testing**: Generate test cases from property specifications
//! - **Async Testing**: Utilities for testing async components and futures
//! - **Metal Kernel Testing**: GPU kernel validation and performance testing
//! - **Evidence Testing**: Validation of evidence-grounded response generation
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::*;
//!
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!
//!     #[test]
//!     fn test_component_with_mocking() {
//!         let mock = MockComponent::deterministic();
//!         // Test implementation
//!     }
//! }
//! ```

// Include security tests when testing
#[cfg(feature = "security_tests")]
pub mod security {
    include!("../../security/mod.rs");
}
pub mod mocks;
pub mod isolation;
pub mod property;
pub mod async_utils;
pub mod metal;
pub mod evidence;

// Re-export commonly used testing utilities
pub use mocks::*;
pub use isolation::*;
pub use property::*;
pub use async_utils::*;
pub use metal::*;
pub use evidence::*;</code>
