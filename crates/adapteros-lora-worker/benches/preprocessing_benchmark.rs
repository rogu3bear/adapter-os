//! CoreML preprocessing benchmark for tokenized datasets.
//!
//! Run with:
//!   AOS_MODEL_PATH=./var/models/<model> cargo bench --bench preprocessing_benchmark
//! Optional CoreML model:
//!   AOS_COREML_PREPROCESS_MODEL=./var/models/<model>.mlpackage

use adapteros_config::ModelConfig;
use adapteros_lora_worker::training::preprocessing::preprocess_examples;
use adapteros_lora_worker::training::{
    PreprocessCompression, PreprocessOutputFeature, PreprocessingConfig, TrainingExample,
};
use adapteros_types::training::{ExampleMetadataV1, TrainingDataContractConfig};
use std::path::PathBuf;
use std::time::Instant;

fn create_examples(count: usize, seq_len: usize, vocab_size: usize) -> Vec<TrainingExample> {
    (0..count)
        .map(|i| {
            let input = (0..seq_len)
                .map(|j| ((i + j) % vocab_size) as u32)
                .collect::<Vec<_>>();
            let target = (0..seq_len)
                .map(|j| ((i + j + 1) % vocab_size) as u32)
                .collect::<Vec<_>>();
            let attention_mask = TrainingExample::attention_mask_from_tokens(&input, 0);
            let metadata = ExampleMetadataV1::new("bench", i as u64, "{}", 0);
            TrainingExample::new(input, target, attention_mask, metadata)
        })
        .collect()
}

fn main() {
    let model_path = match std::env::var("AOS_MODEL_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            eprintln!("AOS_MODEL_PATH not set; skipping preprocessing benchmark");
            return;
        }
    };

    let model_config = match ModelConfig::from_config_json(&model_path) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed to load model config: {err}");
            return;
        }
    };

    let mut cfg = PreprocessingConfig::default();
    cfg.enabled = true;
    cfg.coreml_model_path = std::env::var("AOS_COREML_PREPROCESS_MODEL")
        .ok()
        .map(PathBuf::from);
    cfg.compression = Some(PreprocessCompression::Q15);
    cfg.output_feature = PreprocessOutputFeature::HiddenStateLast;

    let contract = TrainingDataContractConfig::new(0, -1);

    let examples = create_examples(256, 128, model_config.vocab_size);

    let start = Instant::now();
    let result = preprocess_examples(
        &examples,
        &contract,
        &cfg,
        model_config.hidden_size,
        model_config.vocab_size,
        &model_path,
        None,
        None,
        None,
        0,
    );
    let elapsed = start.elapsed();

    match result {
        Ok(result) => {
            let throughput = examples.len() as f64 / elapsed.as_secs_f64();
            println!(
                "Preprocessing backend={} examples={} seq_len={} hidden_dim={} time_ms={} throughput={:.2} ex/s cache_hit={}",
                result.stats.backend,
                examples.len(),
                128,
                model_config.hidden_size,
                elapsed.as_millis(),
                throughput,
                result.stats.cache_hit
            );
        }
        Err(err) => {
            eprintln!("Preprocessing benchmark failed: {err}");
        }
    }
}
