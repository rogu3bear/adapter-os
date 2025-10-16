//! Unified testing framework for AdapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.

pub mod unified_framework;

// Re-export unified framework types
pub use unified_framework::{
    AssertionResult, AssertionType, CoverageReport, EnvironmentState, FileCoverage,
    PerformanceMetrics, StepResult, TestAction, TestAssertion, TestCase, TestConfig,
    TestEnvironment, TestEnvironmentType, TestPriority, TestResult, TestStatus, TestStep,
    TestSuite, TestSuiteResult, TestSummary, TestType, TestingFramework, UnifiedTestingFramework,
};
