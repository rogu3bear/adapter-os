//! Dataset size guardrails for training ingestion.

pub const DEFAULT_MAX_FILES: usize = 1000;
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
pub const DEFAULT_MAX_SAMPLES: usize = 100_000;
pub const DEFAULT_MAX_TOKENS: u64 = 100_000_000;

#[derive(Debug, Clone, Copy)]
pub struct DatasetSizeLimits {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_samples: usize,
    pub max_tokens: u64,
}

impl DatasetSizeLimits {
    pub fn from_env() -> Self {
        Self {
            max_files: parse_env_usize("AOS_DATASET_MAX_FILES", DEFAULT_MAX_FILES),
            max_total_bytes: parse_env_u64("AOS_DATASET_MAX_TOTAL_BYTES", DEFAULT_MAX_TOTAL_BYTES),
            max_samples: parse_env_usize("AOS_DATASET_MAX_SAMPLES", DEFAULT_MAX_SAMPLES),
            max_tokens: parse_env_u64("AOS_DATASET_MAX_TOKENS", DEFAULT_MAX_TOKENS),
        }
    }
}

pub(crate) fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
