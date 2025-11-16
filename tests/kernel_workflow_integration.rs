//! Integration tests for workflow execution with real Metal kernels
//!
//! These tests validate the KernelAdapterBackend implementation and
//! Worker integration with workflow execution.

use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_lora_lifecycle::{
    KernelAdapterBackend, WorkflowContext, WorkflowExecutor, WorkflowResult, WorkflowType,
};
use adapteros_lora_worker::Worker;
use adapteros_manifest::{AdapterEntry, ManifestV3, Policies, TrustLevel};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Helper to create a test manifest with multiple adapters
fn create_test_manifest(adapter_count: usize) -> ManifestV3 {
    let adapters: Vec<AdapterEntry> = (0..adapter_count)
        .map(|i| AdapterEntry {
            id: format!("test_adapter_{}", i),
            path: PathBuf::from(format!("/tmp/test_adapter_{}.aos", i)),
            trust_level: TrustLevel::Verified,
            hash_b3: format!("hash_{}", i),
            metadata: HashMap::new(),
        })
        .collect();

    ManifestV3 {
        version: "3".to_string(),
        adapters,
        policies: Policies::default(),
        metadata: HashMap::new(),
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_sequential_workflow() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064, // Qwen2.5 vocab size
    ));

    // Create workflow executor
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, adapter_names.clone(), backend);

    // Create execution context
    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow
    let result = executor
        .execute(context)
        .await
        .expect("Workflow execution failed");

    // Verify results
    assert_eq!(result.stats.adapters_executed, 3);
    assert_eq!(result.stats.phases.len(), 3);
    assert!(result.stats.total_time_ms > 0);

    // Each phase should execute one adapter
    for phase in &result.stats.phases {
        assert_eq!(phase.adapter_ids.len(), 1);
        assert!(phase.duration_ms > 0);
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_parallel_workflow() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec![
        "adapter_a".to_string(),
        "adapter_b".to_string(),
        "adapter_c".to_string(),
        "adapter_d".to_string(),
    ];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Create workflow executor
    let executor = WorkflowExecutor::new(WorkflowType::Parallel, adapter_names.clone(), backend);

    // Create execution context
    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3, 4, 5],
        model_state: HashMap::new(),
        metadata: HashMap::from([
            ("request_id".to_string(), "test_parallel".to_string()),
            ("workflow_type".to_string(), "parallel".to_string()),
        ]),
    };

    // Execute workflow
    let result = executor
        .execute(context)
        .await
        .expect("Workflow execution failed");

    // Verify results
    assert_eq!(result.stats.adapters_executed, 4);
    assert_eq!(result.stats.phases.len(), 1);
    assert_eq!(result.stats.phases[0].name, "parallel_all");
    assert_eq!(result.stats.phases[0].adapter_ids.len(), 4);
    assert!(result.stats.total_time_ms > 0);
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_upstream_downstream_workflow() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names (6 adapters: 3 upstream + 3 downstream)
    let adapter_names = vec![
        "upstream_0".to_string(),
        "upstream_1".to_string(),
        "upstream_2".to_string(),
        "downstream_0".to_string(),
        "downstream_1".to_string(),
        "downstream_2".to_string(),
    ];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Create workflow executor
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        adapter_names.clone(),
        backend,
    );

    // Create execution context
    let context = WorkflowContext {
        input_tokens: vec![10, 20, 30],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow
    let result = executor
        .execute(context)
        .await
        .expect("Workflow execution failed");

    // Verify results
    assert_eq!(result.stats.adapters_executed, 6);
    assert_eq!(result.stats.phases.len(), 2);

    // Verify upstream phase
    assert_eq!(result.stats.phases[0].name, "upstream");
    assert_eq!(result.stats.phases[0].adapter_ids.len(), 3);
    assert!(result.stats.phases[0].duration_ms > 0);

    // Verify downstream phase
    assert_eq!(result.stats.phases[1].name, "downstream");
    assert_eq!(result.stats.phases[1].adapter_ids.len(), 3);
    assert!(result.stats.phases[1].duration_ms > 0);
}

#[tokio::test]
#[ignore] // Requires Metal runtime and adapter files
async fn test_worker_execute_workflow_integration() {
    // Create test manifest
    let manifest = create_test_manifest(4);
    let adapter_names: Vec<String> = manifest.adapters.iter().map(|a| a.id.clone()).collect();

    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");

    // Create Worker
    let worker = Worker::new(
        manifest,
        kernels,
        PathBuf::from("/tmp/adapteros"),
        None, // No RAG for this test
    )
    .await
    .expect("Failed to create Worker");

    // Create workflow context
    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::from([("test_id".to_string(), "worker_integration".to_string())]),
    };

    // Test Sequential workflow
    let result = worker
        .execute_workflow(
            WorkflowType::Sequential,
            adapter_names.clone(),
            context.clone(),
        )
        .await
        .expect("Sequential workflow failed");

    assert_eq!(result.stats.adapters_executed, 4);
    assert_eq!(result.stats.phases.len(), 4);

    // Test Parallel workflow
    let result = worker
        .execute_workflow(
            WorkflowType::Parallel,
            adapter_names.clone(),
            context.clone(),
        )
        .await
        .expect("Parallel workflow failed");

    assert_eq!(result.stats.adapters_executed, 4);
    assert_eq!(result.stats.phases.len(), 1);

    // Test UpstreamDownstream workflow (only if we have even number of adapters)
    if adapter_names.len() >= 4 && adapter_names.len() % 2 == 0 {
        let result = worker
            .execute_workflow(
                WorkflowType::UpstreamDownstream,
                adapter_names.clone(),
                context.clone(),
            )
            .await
            .expect("UpstreamDownstream workflow failed");

        assert_eq!(result.stats.adapters_executed, adapter_names.len());
        assert_eq!(result.stats.phases.len(), 2);
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_error_handling_invalid_adapter() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Create workflow executor with an adapter that doesn't exist in the backend
    let invalid_adapters = vec!["nonexistent_adapter".to_string()];
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, invalid_adapters, backend);

    // Create execution context
    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow - should fail
    let result = executor.execute(context).await;
    assert!(result.is_err(), "Expected error for nonexistent adapter");
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_empty_input_tokens() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec!["adapter_0".to_string()];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Create workflow executor
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, adapter_names.clone(), backend);

    // Create execution context with empty tokens
    let context = WorkflowContext {
        input_tokens: vec![], // Empty input
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow - should handle gracefully
    let result = executor.execute(context).await;

    // Depending on implementation, this might succeed or fail gracefully
    // The key is that it shouldn't panic
    match result {
        Ok(workflow_result) => {
            assert_eq!(workflow_result.stats.adapters_executed, 1);
        }
        Err(_) => {
            // Acceptable to error on empty input
        }
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_performance_characteristics() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Test Sequential workflow timing
    let sequential_executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        adapter_names.clone(),
        backend.clone(),
    );

    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let sequential_result = sequential_executor
        .execute(context.clone())
        .await
        .expect("Sequential workflow failed");

    // Test Parallel workflow timing
    let parallel_executor = WorkflowExecutor::new(
        WorkflowType::Parallel,
        adapter_names.clone(),
        backend.clone(),
    );

    let parallel_result = parallel_executor
        .execute(context.clone())
        .await
        .expect("Parallel workflow failed");

    // Verify timing characteristics
    assert!(sequential_result.stats.total_time_ms > 0);
    assert!(parallel_result.stats.total_time_ms > 0);

    // Sequential should take longer than parallel for multiple adapters
    // (though with mutex locking, parallel might not be significantly faster)
    println!(
        "Sequential: {}ms, Parallel: {}ms",
        sequential_result.stats.total_time_ms, parallel_result.stats.total_time_ms
    );

    // Verify phase timings sum approximately to total time
    let sequential_phase_sum: u64 = sequential_result
        .stats
        .phases
        .iter()
        .map(|p| p.duration_ms)
        .sum();

    assert!(
        sequential_phase_sum <= sequential_result.stats.total_time_ms + 10,
        "Phase timings should sum to approximately total time"
    );
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_concurrent_workflows() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec!["adapter_0".to_string(), "adapter_1".to_string()];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Spawn multiple concurrent workflows
    let mut handles = vec![];

    for i in 0..3 {
        let backend_clone = backend.clone();
        let adapter_names_clone = adapter_names.clone();

        let handle = tokio::spawn(async move {
            let executor =
                WorkflowExecutor::new(WorkflowType::Sequential, adapter_names_clone, backend_clone);

            let context = WorkflowContext {
                input_tokens: vec![100 + i, 200 + i, 300 + i],
                model_state: HashMap::new(),
                metadata: HashMap::from([("workflow_id".to_string(), format!("workflow_{}", i))]),
            };

            executor.execute(context).await
        });

        handles.push(handle);
    }

    // Wait for all workflows to complete
    let results: Vec<WorkflowResult> = futures::future::try_join_all(handles)
        .await
        .expect("Failed to join tasks")
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Some workflows failed");

    // Verify all workflows succeeded
    assert_eq!(results.len(), 3);
    for result in results {
        assert_eq!(result.stats.adapters_executed, 2);
        assert!(result.stats.total_time_ms > 0);
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_kernel_backend_large_token_sequence() {
    // Initialize Metal kernels
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // Create adapter names
    let adapter_names = vec!["adapter_0".to_string()];

    // Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064,
    ));

    // Create workflow executor
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, adapter_names.clone(), backend);

    // Create large token sequence
    let large_tokens: Vec<u32> = (0..1024).collect();

    let context = WorkflowContext {
        input_tokens: large_tokens,
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow
    let result = executor
        .execute(context)
        .await
        .expect("Workflow with large tokens failed");

    assert_eq!(result.stats.adapters_executed, 1);
    assert!(result.stats.total_time_ms > 0);
}
