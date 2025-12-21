// New/Extend for domain adapters
use adapteros_core::AosError;
use adapteros_lora_lifecycle::Executor;

#[derive(Debug)]
pub struct ExecutorLoadConfig {
    pub tenant_id: String,
    pub adapter_id: String,
    pub rank: u32,
    pub alpha: f32,
}

pub async fn load_adapter_to_executor(config: &ExecutorLoadConfig, executor: &Executor) -> Result<(), AosError> {
    // Deterministic load with seeding
    executor.load_domain_adapter(&config.adapter_id, config.rank, config.alpha)
        .await
        .map_err(|e| AosError::Kernel(format!("Load failed: {}", e)))
}

pub async fn unload_adapter_from_executor(adapter_id: &str, executor: &Executor) -> Result<(), AosError> {
    executor.unload(&adapter_id).await.map_err(|e| AosError::Kernel(e.to_string()))
}

pub async fn test_adapter_determinism(adapter_id: &str, executor: &Executor) -> Result<bool, AosError> {
    // Run seeded test
    let seed = blake3::hash(adapter_id.as_bytes());
    let output1 = executor.execute_test(&adapter_id, &seed[..]).await.map_err(|e| AosError::Kernel(e.to_string()))?;
    let output2 = executor.execute_test(&adapter_id, &seed[..]).await.map_err(|e| AosError::Kernel(e.to_string()))?;
    Ok(output1 == output2)
}
