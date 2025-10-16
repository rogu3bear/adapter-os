use crate::{
    types::*,
    unified_framework::{TestingFramework, UnifiedTestingFramework},
};
use serde_json::Value;
use std::collections::HashMap;

fn base_config() -> TestConfig {
    TestConfig {
        environment_type: TestEnvironmentType::Unit,
        timeout_seconds: 30,
        max_concurrent_tests: 1,
        enable_isolation: true,
        enable_parallelization: false,
        test_data_dir: None,
        fixtures_dir: None,
        additional_config: HashMap::new(),
    }
}

fn make_step(id: &str, action: TestAction) -> TestStep {
    TestStep {
        id: id.into(),
        name: id.into(),
        description: None,
        action,
        parameters: HashMap::new(),
        timeout_seconds: None,
        retries: None,
        dependencies: Vec::new(),
    }
}

fn make_assertion(id: &str, left: Value, right: Value, equals: bool) -> TestAssertion {
    TestAssertion {
        id: id.into(),
        name: id.into(),
        assertion_type: if equals {
            AssertionType::Equals
        } else {
            AssertionType::NotEquals
        },
        parameters: HashMap::from([(String::from("left"), left), (String::from("right"), right)]),
        message: None,
    }
}

#[tokio::test]
async fn run_test_step_executes_command() {
    let framework = UnifiedTestingFramework::new(base_config());
    let step = make_step(
        "echo",
        TestAction::ExecuteCommand {
            command: "echo".into(),
            args: vec!["hello".into()],
        },
    );
    let result = framework.run_test_step(&step).await.unwrap();
    assert_eq!(result.status, TestStatus::Passed);
    assert_eq!(result.output, Some("hello".into()));
}

#[tokio::test]
async fn run_assertion_handles_equality() {
    let framework = UnifiedTestingFramework::new(base_config());
    let assertion = make_assertion("eq", Value::from(1), Value::from(1), true);
    let result = framework.run_assertion(&assertion).await.unwrap();
    assert_eq!(result.status, TestStatus::Passed);
    let failing_assertion = make_assertion("neq", Value::from("a"), Value::from("a"), false);
    let failing = framework.run_assertion(&failing_assertion).await.unwrap();
    assert_eq!(failing.status, TestStatus::Failed);
}

#[tokio::test]
async fn run_suite_aggregates_results_and_metrics() {
    let config = base_config();
    let framework = UnifiedTestingFramework::new(config.clone());
    let step = make_step(
        "echo",
        TestAction::ExecuteCommand {
            command: "echo".into(),
            args: vec!["suite".into()],
        },
    );
    let assertion = TestAssertion {
        id: "contains".into(),
        name: "contains".into(),
        assertion_type: AssertionType::Contains,
        parameters: HashMap::from([
            (String::from("container"), Value::from("adapteros suite")),
            (String::from("item"), Value::from("suite")),
        ]),
        message: None,
    };
    let case = TestCase {
        id: "case1".into(),
        name: "case1".into(),
        description: None,
        test_type: TestType::Unit,
        priority: TestPriority::Medium,
        tags: Vec::new(),
        setup: None,
        steps: vec![step],
        teardown: None,
        assertions: vec![assertion],
        timeout_seconds: None,
        dependencies: Vec::new(),
        metadata: HashMap::new(),
    };
    let suite = TestSuite {
        id: "suite".into(),
        name: "suite".into(),
        description: None,
        test_cases: vec![case],
        config,
        metadata: HashMap::new(),
    };
    let suite_result = framework.run_suite(&suite).await.unwrap();
    assert_eq!(suite_result.summary.total_tests, 1);
    assert_eq!(suite_result.summary.passed_tests, 1);
    let metrics = framework.get_performance_metrics().await.unwrap();
    assert!(metrics.total_execution_time_ms >= suite_result.execution_time_ms);
    let history = framework.test_results_history.lock().unwrap();
    assert_eq!(history.len(), 1);
}
