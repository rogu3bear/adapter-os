use adapteros_base_llm::{BaseLLM, BaseLLMConfig, BaseLLMFactory, BaseLLMMetadata, ModelType};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};

fn main() -> adapteros_core::Result<()> {
    // Simple stdout logging; enable tracing in app if needed

    let metadata = BaseLLMMetadata::default();
    let cfg = BaseLLMConfig {
        model_type: ModelType::Qwen,
        metadata,
        model_path: std::env::var("AOS_MLX_FFI_MODEL").ok(),
    };

    let mut model = BaseLLMFactory::from_config(cfg)?;
    let mut exec = DeterministicExecutor::new(ExecutorConfig::default());
    model.load(&mut exec)?;

    let logits = model.forward(&[1, 2, 3, 4])?;
    println!("FFI logits len = {} nonzero={}", logits.len(), logits.iter().filter(|v| **v != 0.0).count());
    Ok(())
}
