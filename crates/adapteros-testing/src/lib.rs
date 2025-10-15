//! Unified testing framework for AdapterOS
//!
//! Provides a centralized testing framework that consolidates all testing
//! patterns across the system with consistent setup, teardown, and assertions.

pub mod unified_framework;

// Re-export unified framework types
pub use unified_framework::{
    TestingFramework, UnifiedTestingFramework, TestConfig, TestEnvironment, TestEnvironmentType,
    EnvironmentState, TestCase, TestType, TestPriority, TestStep, TestAction, TestAssertion,
    AssertionType, TestResult, TestStatus, AssertionResult, StepResult, TestSuite, TestSuiteResult,
    TestSummary, CoverageReport, FileCoverage, PerformanceMetrics,
};
