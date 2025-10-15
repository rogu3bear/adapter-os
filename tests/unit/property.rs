//! Property-Based Testing Infrastructure
//!
//! This module provides property-based testing utilities for AdapterOS components,
//! enabling generation of test cases from property specifications and automated
//! testing of component invariants.
//!
//! ## Key Features
//!
//! - **Arbitrary Data Generation**: Generate test inputs from specifications
//! - **Property Verification**: Test mathematical and logical properties
//! - **Invariant Checking**: Verify component invariants hold under various conditions
//! - **Shrinkage**: Minimize failing test cases for easier debugging
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::property::*;
//!
//! #[test]
//! fn test_hash_properties() {
//!     let property = hash_deterministic_property();
//!     let result = check_property(property, 1000);
//!     assert!(result.is_ok(), "Hash determinism property failed");
//! }
//! ```

use std::fmt;
use adapteros_core::{B3Hash, derive_seed, derive_seed_indexed};

/// Result of a property test
#[derive(Debug, Clone)]
pub enum PropertyResult {
    /// Property holds for all tested cases
    Passed { tests_run: usize },
    /// Property failed with a counterexample
    Failed { counterexample: Vec<u8>, tests_run: usize },
    /// Property test was exhausted (couldn't generate enough test cases)
    Exhausted { tests_run: usize },
}

impl PropertyResult {
    /// Check if the property test passed
    pub fn is_passed(&self) -> bool {
        matches!(self, PropertyResult::Passed { .. })
    }

    /// Check if the property test failed
    pub fn is_failed(&self) -> bool {
        matches!(self, PropertyResult::Failed { .. })
    }

    /// Get the number of tests run
    pub fn tests_run(&self) -> usize {
        match self {
            PropertyResult::Passed { tests_run } => *tests_run,
            PropertyResult::Failed { tests_run, .. } => *tests_run,
            PropertyResult::Exhausted { tests_run } => *tests_run,
        }
    }
}

impl fmt::Display for PropertyResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyResult::Passed { tests_run } => {
                write!(f, "Property passed after {} tests", tests_run)
            }
            PropertyResult::Failed { counterexample, tests_run } => {
                write!(f, "Property failed after {} tests with counterexample: {:?}",
                       tests_run, counterexample)
            }
            PropertyResult::Exhausted { tests_run } => {
                write!(f, "Property test exhausted after {} tests (couldn't generate more cases)",
                       tests_run)
            }
        }
    }
}

/// A property to test
pub trait Property {
    /// Test the property with the given input
    fn test(&self, input: &[u8]) -> bool;

    /// Get the name of this property
    fn name(&self) -> &str;

    /// Get the maximum size of test inputs for this property
    fn max_input_size(&self) -> usize {
        1024
    }
}

/// Generator for test inputs
pub trait Generator {
    /// Generate a test input of the given size
    fn generate(&mut self, size: usize, seed: &B3Hash) -> Vec<u8>;

    /// Shrink a failing input to a minimal counterexample
    fn shrink(&self, input: &[u8]) -> Vec<u8> {
        input.to_vec()
    }
}

/// Simple byte array generator
pub struct ByteArrayGenerator;

impl Generator for ByteArrayGenerator {
    fn generate(&mut self, size: usize, seed: &B3Hash) -> Vec<u8> {
        let mut result = Vec::with_capacity(size);
        for i in 0..size {
            let derived = derive_seed_indexed(seed, "byte", i as u64);
            result.push(derived[0]);
        }
        result
    }

    fn shrink(&self, input: &[u8]) -> Vec<u8> {
        // Simple shrinkage: try smaller sizes
        if input.len() > 1 {
            input[..input.len() / 2].to_vec()
        } else {
            input.to_vec()
        }
    }
}

/// String generator that produces valid UTF-8 strings
pub struct StringGenerator;

impl Generator for StringGenerator {
    fn generate(&mut self, size: usize, seed: &B3Hash) -> Vec<u8> {
        let mut result = Vec::with_capacity(size);
        for i in 0..size {
            let derived = derive_seed_indexed(seed, "char", i as u64);
            // Generate printable ASCII characters
            let char_code = 32 + (derived[0] % 95); // 32-126 printable ASCII
            result.push(char_code);
        }
        result
    }

    fn shrink(&self, input: &[u8]) -> Vec<u8> {
        // Try to shrink while maintaining valid UTF-8
        if input.len() > 1 {
            let new_len = input.len() / 2;
            match std::str::from_utf8(&input[..new_len]) {
                Ok(_) => input[..new_len].to_vec(),
                Err(_) => input.to_vec(),
            }
        } else {
            input.to_vec()
        }
    }
}

/// Property checker configuration
#[derive(Debug, Clone)]
pub struct PropertyConfig {
    pub max_tests: usize,
    pub max_input_size: usize,
    pub seed: B3Hash,
}

impl Default for PropertyConfig {
    fn default() -> Self {
        Self {
            max_tests: 100,
            max_input_size: 256,
            seed: B3Hash::hash(b"default_property_seed"),
        }
    }
}

/// Check a property with the given configuration
pub fn check_property_with_config<P, G>(
    property: &P,
    generator: &mut G,
    config: &PropertyConfig,
) -> PropertyResult
where
    P: Property,
    G: Generator,
{
    let mut tests_run = 0;

    for test_index in 0..config.max_tests {
        let input_size = (test_index % config.max_input_size) + 1;
        let test_seed = derive_seed_indexed(&config.seed, "test", test_index as u64);
        let input = generator.generate(input_size, &B3Hash::from(test_seed));

        if !property.test(&input) {
            // Property failed, try to shrink the counterexample
            let shrunk = generator.shrink(&input);
            return PropertyResult::Failed {
                counterexample: if property.test(&shrunk) { input } else { shrunk },
                tests_run: tests_run + 1,
            };
        }

        tests_run += 1;
    }

    PropertyResult::Passed { tests_run }
}

/// Check a property with default configuration
pub fn check_property<P>(property: P, max_tests: usize) -> PropertyResult
where
    P: Property,
{
    let mut generator = ByteArrayGenerator;
    let mut config = PropertyConfig::default();
    config.max_tests = max_tests;

    check_property_with_config(&property, &mut generator, &config)
}

/// Hash determinism property: identical inputs produce identical outputs
pub struct HashDeterminismProperty;

impl Property for HashDeterminismProperty {
    fn test(&self, input: &[u8]) -> bool {
        let hash1 = B3Hash::hash(input);
        let hash2 = B3Hash::hash(input);
        hash1 == hash2
    }

    fn name(&self) -> &str {
        "hash_determinism"
    }
}

/// Hash uniqueness property: different inputs rarely produce identical outputs
pub struct HashUniquenessProperty {
    pub acceptable_collision_rate: f64,
}

impl Property for HashUniquenessProperty {
    fn test(&self, input: &[u8]) -> bool {
        // This is a probabilistic property - we can't test it with a single input
        // In practice, this would be tested across many inputs
        true // Placeholder - real implementation would track collision statistics
    }

    fn name(&self) -> &str {
        "hash_uniqueness"
    }
}

/// Seed derivation determinism property
pub struct SeedDerivationDeterminismProperty;

impl Property for SeedDerivationDeterminismProperty {
    fn test(&self, input: &[u8]) -> bool {
        if input.is_empty() {
            return true;
        }

        let global = B3Hash::hash(input);
        let label = if input.len() > 1 { &input[..1] } else { input };

        let seed1 = derive_seed(&global, std::str::from_utf8(label).unwrap_or("default"));
        let seed2 = derive_seed(&global, std::str::from_utf8(label).unwrap_or("default"));

        seed1 == seed2
    }

    fn name(&self) -> &str {
        "seed_derivation_determinism"
    }
}

/// Commutativity property for operations that should be commutative
pub struct CommutativityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    pub operation: F,
    pub generator: Box<dyn Fn() -> (Vec<u8>, Vec<u8>) + Send + Sync>,
}

impl<F> Property for CommutativityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    fn test(&self, input: &[u8]) -> bool {
        let (a, b) = (self.generator)();
        let result1 = (self.operation)(&a, &b);
        let result2 = (self.operation)(&b, &a);
        result1 == result2
    }

    fn name(&self) -> &str {
        "commutativity"
    }
}

/// Associativity property for operations that should be associative
pub struct AssociativityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    pub operation: F,
    pub generator: Box<dyn Fn() -> (Vec<u8>, Vec<u8>, Vec<u8>) + Send + Sync>,
}

impl<F> Property for AssociativityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    fn test(&self, input: &[u8]) -> bool {
        let (a, b, c) = (self.generator)();
        let result1 = (self.operation)((self.operation)(&a, &b).as_slice(), &c);
        let result2 = (self.operation)(&a, (self.operation)(&b, &c).as_slice());
        result1 == result2
    }

    fn name(&self) -> &str {
        "associativity"
    }
}

/// Identity property for operations with identity elements
pub struct IdentityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    pub operation: F,
    pub identity: Vec<u8>,
    pub generator: Box<dyn Fn() -> Vec<u8> + Send + Sync>,
}

impl<F> Property for IdentityProperty<F>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
{
    fn test(&self, input: &[u8]) -> bool {
        let value = (self.generator)();
        let result1 = (self.operation)(&value, &self.identity);
        let result2 = (self.operation)(&self.identity, &value);
        result1 == value && result2 == value
    }

    fn name(&self) -> &str {
        "identity"
    }
}

/// Inverse property for operations with inverses
pub struct InverseProperty<F, G>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
    G: Fn(&[u8]) -> Vec<u8>,
{
    pub operation: F,
    pub inverse: G,
    pub generator: Box<dyn Fn() -> Vec<u8> + Send + Sync>,
}

impl<F, G> Property for InverseProperty<F, G>
where
    F: Fn(&[u8], &[u8]) -> Vec<u8>,
    G: Fn(&[u8]) -> Vec<u8>,
{
    fn test(&self, input: &[u8]) -> bool {
        let value = (self.generator)();
        let inv = (self.inverse)(&value);
        let result = (self.operation)(&value, &inv);
        // Check if applying operation with inverse returns to original
        result == value
    }

    fn name(&self) -> &str {
        "inverse"
    }
}

/// Property test runner for running multiple properties
pub struct PropertyTestRunner {
    properties: Vec<Box<dyn Property + Send + Sync>>,
    config: PropertyConfig,
}

impl PropertyTestRunner {
    /// Create a new property test runner
    pub fn new() -> Self {
        Self {
            properties: Vec::new(),
            config: PropertyConfig::default(),
        }
    }

    /// Add a property to test
    pub fn add_property<P>(&mut self, property: P)
    where
        P: Property + Send + Sync + 'static,
    {
        self.properties.push(Box::new(property));
    }

    /// Set the configuration
    pub fn with_config(mut self, config: PropertyConfig) -> Self {
        self.config = config;
        self
    }

    /// Run all properties
    pub fn run(&self) -> Vec<(String, PropertyResult)> {
        let mut results = Vec::new();
        let mut generator = ByteArrayGenerator;

        for property in &self.properties {
            let result = check_property_with_config(
                property.as_ref(),
                &mut generator,
                &self.config,
            );
            results.push((property.name().to_string(), result));
        }

        results
    }
}

/// Convenience functions for common properties

/// Test hash determinism property
pub fn hash_deterministic_property() -> HashDeterminismProperty {
    HashDeterminismProperty
}

/// Test seed derivation determinism property
pub fn seed_derivation_deterministic_property() -> SeedDerivationDeterminismProperty {
    SeedDerivationDeterminismProperty
}

/// Create a property test runner with common AdapterOS properties
pub fn adapteros_property_runner() -> PropertyTestRunner {
    let mut runner = PropertyTestRunner::new();
    runner.add_property(hash_deterministic_property());
    runner.add_property(seed_derivation_deterministic_property());
    runner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_determinism_property() {
        let property = HashDeterminismProperty;
        let result = check_property(property, 100);
        assert!(result.is_passed(), "Hash determinism should always pass");
    }

    #[test]
    fn test_seed_derivation_determinism_property() {
        let property = SeedDerivationDeterminismProperty;
        let result = check_property(property, 100);
        assert!(result.is_passed(), "Seed derivation determinism should always pass");
    }

    #[test]
    fn test_byte_array_generator() {
        let mut generator = ByteArrayGenerator;
        let seed = B3Hash::hash(b"test_seed");

        let data = generator.generate(10, &seed);
        assert_eq!(data.len(), 10);

        // Same seed should produce same output
        let data2 = generator.generate(10, &seed);
        assert_eq!(data, data2);
    }

    #[test]
    fn test_string_generator() {
        let mut generator = StringGenerator;
        let seed = B3Hash::hash(b"test_seed");

        let data = generator.generate(10, &seed);
        assert_eq!(data.len(), 10);

        // Should be valid UTF-8
        let _ = std::str::from_utf8(&data).expect("Generated data should be valid UTF-8");
    }

    #[test]
    fn test_property_runner() {
        let runner = adapteros_property_runner();
        let results = runner.run();

        for (name, result) in results {
            assert!(result.is_passed(), "Property {} should pass", name);
        }
    }

    #[test]
    fn test_commutativity_property() {
        // Test with a simple concatenation operation
        let property = CommutativityProperty {
            operation: |a: &[u8], b: &[u8]| {
                let mut result = a.to_vec();
                result.extend_from_slice(b);
                result
            },
            generator: Box::new(|| {
                (b"hello".to_vec(), b"world".to_vec())
            }),
        };

        // Concatenation is not commutative, so this should fail
        let result = check_property(property, 10);
        assert!(result.is_failed(), "Concatenation should not be commutative");
    }

    #[test]
    fn test_identity_property() {
        // Test with addition-like operation
        let property = IdentityProperty {
            operation: |a: &[u8], b: &[u8]| {
                // Simple "addition" of first bytes
                vec![a.get(0).unwrap_or(&0).wrapping_add(b.get(0).unwrap_or(&0))]
            },
            identity: vec![0],
            generator: Box::new(|| vec![42]),
        };

        let result = check_property(property, 10);
        assert!(result.is_passed(), "Addition with identity should work");
    }
}</code>
