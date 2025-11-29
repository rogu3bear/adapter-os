//! Unified testing framework for AdapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.

pub mod fixtures;
pub mod kernel_testing;
pub mod unified_framework;

// Re-export unified framework types
pub use unified_framework::{
    AssertionResult, AssertionType, CoverageReport, EnvironmentState, FileCoverage,
    PerformanceMetrics, StepResult, TestAction, TestAssertion, TestCase, TestConfig,
    TestEnvironment, TestEnvironmentType, TestPriority, TestResult, TestStatus, TestStep,
    TestSuite, TestSuiteResult, TestSummary, TestType, TestingFramework, UnifiedTestingFramework,
};

// Re-export kernel testing utilities
pub use kernel_testing::{
    approx_eq, assert_bit_exact, assert_vectors_eq, compare_vectors, cpu_add, cpu_lora_forward,
    cpu_matmul, cpu_rms_norm, cpu_scale, cpu_softmax, cpu_softmax_2d, cpu_swiglu,
    deterministic_weights, float_to_q15, mock_model_plan, q15_to_float, test_attention_mask,
    test_input_sequence, test_position_ids, test_q15_gates, uniform_weights, xavier_weights,
    BenchTimer, ComparisonResult, MockAdapter, MockMlpWeights, MockQkvWeights, SeededLcg,
    Tolerance,
};

// Re-export fixture types
pub use fixtures::{
    TestAdapterFactory, TestAppStateBuilder, TestAssertions, TestAuth, TestDatasetFactory,
    TestDbBuilder, TestDbConfig, TestTrainingJobFactory, TestUser,
};
