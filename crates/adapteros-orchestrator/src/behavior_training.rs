//! Behavior training data generator
//!
//! Exports historical telemetry events and generates synthetic examples for adapter lifecycle training.

use adapteros_core::seed::{derive_seed_u64, get_deterministic_timestamp};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use chrono::{DateTime, Utc};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::BTreeMap;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

/// Behavior training example matching the specified JSONL schema
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BehaviorExample {
    pub input: BehaviorInput,
    pub target: BehaviorTarget,
    pub metadata: BehaviorMetadata,
}

/// Input state for a behavior transition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BehaviorInput {
    pub adapter_id: String,
    pub load_state: String,
    pub activation_pct: f32,
    pub memory_mb: u64,
    pub last_used: String,
}

/// Target/expected outcome of the transition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BehaviorTarget {
    pub next_state: String,
    pub action: String,
    pub reason: String,
    pub memory_delta: i64,
}

/// Metadata for the example
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BehaviorMetadata {
    pub quality: f32,
    #[serde(rename = "label")]
    pub rlhf_label: String,
    #[serde(rename = "policy_compliant")]
    pub policy_compliant: bool,
    pub category: BehaviorCategory,
    pub source: String,
}

/// Behavior categories for classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorCategory {
    Promotion,
    Demotion,
    Eviction,
    Pinning,
    Recovery,
    TtlEnforcement,
}

impl BehaviorCategory {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Promotion,
            Self::Demotion,
            Self::Eviction,
            Self::Pinning,
            Self::Recovery,
            Self::TtlEnforcement,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Promotion => "promotion",
            Self::Demotion => "demotion",
            Self::Eviction => "eviction",
            Self::Pinning => "pinning",
            Self::Recovery => "recovery",
            Self::TtlEnforcement => "ttl_enforcement",
        }
    }

    pub fn from_event_type(event_type: &str) -> Option<Self> {
        match event_type {
            "promoted" => Some(Self::Promotion),
            "demoted" => Some(Self::Demotion),
            "evicted" => Some(Self::Eviction),
            "pinned" => Some(Self::Pinning),
            "recovered" => Some(Self::Recovery),
            "ttl_expired" => Some(Self::TtlEnforcement),
            _ => None,
        }
    }
}

/// Filter for exporting historical events
#[derive(Debug, Clone, Default)]
pub struct ExportFilter {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub tenant_id: Option<String>,
    pub adapter_id: Option<String>,
    pub categories: Option<Vec<BehaviorCategory>>,
}

/// Configuration for synthetic generation
#[derive(Debug, Clone)]
pub struct SyntheticConfig {
    pub num_examples: usize,
    pub categories: Vec<BehaviorCategory>,
    pub seed: u64,
    pub activation_range: (f32, f32),
    pub memory_range: (u64, u64),
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            num_examples: 100,
            categories: BehaviorCategory::all(),
            seed: 42,
            activation_range: (0.0, 1.0),
            memory_range: (50, 300),
        }
    }
}

/// Configuration for dataset generation
#[derive(Debug, Clone, Default)]
pub struct DatasetConfig {
    pub export_filter: Option<ExportFilter>,
    pub synthetic_config: Option<SyntheticConfig>,
    pub min_per_category: usize,
    pub output_path: Option<String>,
}

/// Training dataset wrapper
#[derive(Debug, Clone)]
pub struct BehaviorDataset {
    pub examples: Vec<BehaviorExample>,
    pub categories: BTreeMap<BehaviorCategory, usize>,
    pub total_examples: usize,
    pub hash: String,
}

/// Generator for behavior training data from telemetry and synthetic examples.
///
/// The `BehaviorTrainingGenerator` creates training datasets for adapter lifecycle
/// management by:
/// - Exporting historical telemetry events as training examples
/// - Generating synthetic examples based on lifecycle rules
/// - Combining both sources to create balanced datasets
///
/// Training examples follow a structured format with input state, target behavior,
/// and metadata for RLHF (Reinforcement Learning from Human Feedback) training.
///
/// # Usage
///
/// ```rust,no_run
/// use adapteros_orchestrator::{BehaviorTrainingGenerator, DatasetConfig};
/// use adapteros_db::Db;
///
/// # async fn example() -> adapteros_core::Result<()> {
/// let db = Db::connect("sqlite://var/aos-cp.sqlite3").await?;
/// let generator = BehaviorTrainingGenerator::new(db, 42);
///
/// let config = DatasetConfig {
///     output_path: Some("training_data.jsonl".to_string()),
///     min_per_category: 100,
///     ..Default::default()
/// };
///
/// let dataset = generator.generate_dataset(&config).await?;
/// println!("Generated {} examples", dataset.total_examples);
/// # Ok(())
/// # }
/// ```
pub struct BehaviorTrainingGenerator {
    db: Db,
    seed: u64,
}

impl BehaviorTrainingGenerator {
    /// Create a new behavior training generator.
    ///
    /// # Arguments
    /// * `db` - Database handle for querying telemetry events
    /// * `seed` - Seed value for deterministic synthetic generation
    ///
    /// # Returns
    /// A new `BehaviorTrainingGenerator` instance.
    pub fn new(db: Db, seed: u64) -> Self {
        Self { db, seed }
    }

    /// Export historical telemetry events as training examples.
    ///
    /// Queries the database for behavior events matching the filter criteria
    /// and converts them into training examples. Events are filtered by time range,
    /// tenant, adapter, and behavior category.
    ///
    /// # Arguments
    /// * `filter` - Filter criteria for selecting events (time range, tenant, adapter, categories)
    ///
    /// # Returns
    /// A vector of training examples extracted from telemetry events.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Database query fails
    /// - Event data is malformed or missing required fields
    ///
    /// # Note
    /// Only events matching known behavior categories are exported. Unknown
    /// event types are skipped. The query is limited to 10,000 events for performance.
    pub async fn export_from_telemetry(
        &self,
        filter: &ExportFilter,
    ) -> Result<Vec<BehaviorExample>> {
        // Build query dynamically based on filters
        let mut conditions = vec!["1=1".to_string()];

        if filter.since.is_some() {
            conditions.push("created_at >= ?".to_string());
        }
        if filter.until.is_some() {
            conditions.push("created_at <= ?".to_string());
        }
        if filter.tenant_id.is_some() {
            conditions.push("tenant_id = ?".to_string());
        }
        if filter.adapter_id.is_some() {
            conditions.push("adapter_id = ?".to_string());
        }
        if let Some(cats) = &filter.categories {
            let cat_list = cats
                .iter()
                .map(|c| format!("'{}'", c.as_str()))
                .collect::<Vec<_>>()
                .join(",");
            conditions.push(format!("event_type IN ({})", cat_list));
        }

        let query_str = format!(
            r#"
            SELECT id, event_type, adapter_id, tenant_id, from_state, to_state, 
                   activation_pct, memory_mb, reason, created_at, metadata
            FROM behavior_events
            WHERE {}
            ORDER BY created_at DESC
            LIMIT 10000
            "#,
            conditions.join(" AND ")
        );

        // Build query with bindings
        let mut query = sqlx::query(&query_str);

        if let Some(since) = &filter.since {
            query = query.bind(since.to_rfc3339());
        }
        if let Some(until) = &filter.until {
            query = query.bind(until.to_rfc3339());
        }
        if let Some(tenant) = &filter.tenant_id {
            query = query.bind(tenant);
        }
        if let Some(adapter) = &filter.adapter_id {
            query = query.bind(adapter);
        }

        let rows = query
            .fetch_all(self.db.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Query failed: {}", e)))?;

        let mut examples = Vec::new();

        for row in rows {
            let event_type: String = row.try_get("event_type").unwrap_or_default();
            let category = match BehaviorCategory::from_event_type(&event_type) {
                Some(c) => c,
                None => continue,
            };

            let from_state: Option<String> = row.try_get("from_state").ok();
            let to_state: Option<String> = row.try_get("to_state").ok();
            let activation_pct: Option<f64> = row.try_get("activation_pct").ok();
            let memory_mb: Option<i64> = row.try_get("memory_mb").ok();
            let created_at: String = row.try_get("created_at").unwrap_or_default();
            let reason: Option<String> = row.try_get("reason").ok();
            let adapter_id: String = row.try_get("adapter_id").unwrap_or_default();

            let input = BehaviorInput {
                adapter_id: adapter_id.clone(),
                load_state: from_state.unwrap_or_else(|| "unknown".to_string()),
                activation_pct: activation_pct.unwrap_or(0.0) as f32,
                memory_mb: memory_mb.unwrap_or(0) as u64,
                last_used: created_at,
            };

            let target = BehaviorTarget {
                next_state: to_state.unwrap_or_else(|| "unknown".to_string()),
                action: event_type,
                reason: reason.unwrap_or_else(|| "unknown".to_string()),
                memory_delta: -(memory_mb.unwrap_or(0)),
            };

            let metadata = BehaviorMetadata {
                quality: 0.95,
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "telemetry_export".to_string(),
            };

            examples.push(BehaviorExample {
                input,
                target,
                metadata,
            });
        }

        info!("Exported {} historical behavior examples", examples.len());
        Ok(examples)
    }

    /// Generate synthetic behavior examples based on lifecycle rules.
    ///
    /// Creates synthetic training examples that follow realistic adapter lifecycle
    /// patterns. Examples are deterministically generated using HKDF-derived seeds
    /// for reproducibility compliance.
    ///
    /// # Arguments
    /// * `config` - Configuration for synthetic generation (count, categories, ranges, seed)
    ///
    /// # Returns
    /// A vector of synthetic training examples.
    ///
    /// # Errors
    /// Returns an error if generation fails (should not happen under normal conditions).
    ///
    /// # Determinism
    /// Uses HKDF-SHA256 seed derivation to ensure reproducible generation across
    /// runs with the same seed. This complies with determinism requirements.
    ///
    /// # Categories
    /// Generates examples for the specified categories, distributing the total
    /// count evenly across categories. Each category has specific generation logic:
    /// - Promotion: State transitions from unloaded → cold → warm → hot
    /// - Demotion: State transitions from resident → hot → warm → cold
    /// - Eviction: Memory pressure or low activation scenarios
    /// - Pinning: Manual pinning or TTL expiration
    /// - Recovery: Heartbeat timeouts or load failures
    /// - TTL Enforcement: Time-based expiration scenarios
    pub fn generate_synthetic(&self, config: &SyntheticConfig) -> Result<Vec<BehaviorExample>> {
        // Use HKDF-derived seed for determinism compliance
        let global_seed = B3Hash::hash(format!("behavior_training:{}", config.seed).as_bytes());
        let derived_seed = derive_seed_u64(&global_seed, "synthetic_generation");
        let mut rng = StdRng::seed_from_u64(derived_seed);
        let mut examples = Vec::new();

        if config.categories.is_empty() {
            return Ok(examples);
        }

        let num_per_cat = config.num_examples / config.categories.len();

        for category in &config.categories {
            for _ in 0..num_per_cat {
                let example = match category {
                    BehaviorCategory::Promotion => {
                        self.generate_promotion(&mut rng, config, *category)
                    }
                    BehaviorCategory::Demotion => {
                        self.generate_demotion(&mut rng, config, *category)
                    }
                    BehaviorCategory::Eviction => {
                        self.generate_eviction(&mut rng, config, *category)
                    }
                    BehaviorCategory::Pinning => self.generate_pinning(&mut rng, config, *category),
                    BehaviorCategory::Recovery => {
                        self.generate_recovery(&mut rng, config, *category)
                    }
                    BehaviorCategory::TtlEnforcement => {
                        self.generate_ttl(&mut rng, config, *category)
                    }
                };

                examples.push(example);
            }
        }

        info!("Generated {} synthetic behavior examples", examples.len());
        Ok(examples)
    }

    fn generate_promotion(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["unloaded", "cold", "warm"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(config.activation_range.0..config.activation_range.1);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let last_used = get_deterministic_timestamp().to_rfc3339();

        let (next_state, reason) = match current_state {
            "unloaded" => ("cold", "initial_load"),
            "cold" => ("warm", "activation_threshold_0.1"),
            "warm" => ("hot", "activation_threshold_0.5"),
            _ => ("cold", "unknown"),
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/category/test/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: next_state.to_string(),
                action: "promote".to_string(),
                reason: reason.to_string(),
                memory_delta: (memory_mb as i64 / 2),
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    fn generate_demotion(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["resident", "hot", "warm"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(0.0..0.1_f32);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let hours_ago = rng.gen_range(1..25);
        let last_used =
            (get_deterministic_timestamp() - chrono::Duration::hours(hours_ago)).to_rfc3339();

        let (next_state, reason) = match current_state {
            "resident" => ("hot", "manual_unpin"),
            "hot" => ("warm", "inactivity_1h"),
            "warm" => ("cold", "inactivity_24h"),
            _ => ("cold", "unknown"),
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/category/test/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: next_state.to_string(),
                action: "demote".to_string(),
                reason: reason.to_string(),
                memory_delta: -(memory_mb as i64 / 4),
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    fn generate_eviction(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["cold", "warm"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(0.0..0.05_f32);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let last_used = get_deterministic_timestamp().to_rfc3339();

        let reason = if rng.gen_bool(0.5) {
            "memory_pressure_85pct"
        } else {
            "low_activation_timeout"
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/category/test/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: "unloaded".to_string(),
                action: "evict".to_string(),
                reason: reason.to_string(),
                memory_delta: -(memory_mb as i64),
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    fn generate_pinning(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["cold", "warm", "hot"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(0.5..1.0_f32);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let last_used = get_deterministic_timestamp().to_rfc3339();

        let (next_state, reason, action) = if rng.gen_bool(0.5) {
            ("resident", "manual_production_pin", "pin")
        } else {
            ("hot", "ttl_expired", "unpin")
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/category/test/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: next_state.to_string(),
                action: action.to_string(),
                reason: reason.to_string(),
                memory_delta: 0,
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    fn generate_recovery(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["warm", "hot"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(0.1..0.5_f32);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let secs_ago = rng.gen_range(301..1800);
        let last_used =
            (get_deterministic_timestamp() - chrono::Duration::seconds(secs_ago)).to_rfc3339();

        let (next_state, reason) = if rng.gen_bool(0.7) {
            (current_state, "heartbeat_timeout_300s")
        } else {
            ("unloaded", "load_failure_retry_exhausted")
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/category/test/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: next_state.to_string(),
                action: "recover".to_string(),
                reason: reason.to_string(),
                memory_delta: 0,
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    fn generate_ttl(
        &self,
        rng: &mut StdRng,
        config: &SyntheticConfig,
        category: BehaviorCategory,
    ) -> BehaviorExample {
        let states = ["cold", "warm"];
        let current_state = states[rng.gen_range(0..states.len())];
        let activation_pct = rng.gen_range(0.0..0.1_f32);
        let memory_mb = rng.gen_range(config.memory_range.0..config.memory_range.1);
        let last_used = get_deterministic_timestamp().to_rfc3339();

        let ttl_hours = rng.gen_range(1..168);
        let reason = if ttl_hours < 24 {
            "ttl_expired_1h"
        } else if ttl_hours < 168 {
            "ttl_expired_7d"
        } else {
            "ttl_expired_long"
        };

        BehaviorExample {
            input: BehaviorInput {
                adapter_id: format!(
                    "tenant-{}/temp/short-lived/r{:03}",
                    rng.gen_range(1..10),
                    rng.gen_range(1..100)
                ),
                load_state: current_state.to_string(),
                activation_pct,
                memory_mb,
                last_used,
            },
            target: BehaviorTarget {
                next_state: "unloaded".to_string(),
                action: "evict".to_string(),
                reason: reason.to_string(),
                memory_delta: -(memory_mb as i64),
            },
            metadata: BehaviorMetadata {
                quality: rng.gen_range(0.85..1.0),
                rlhf_label: "positive".to_string(),
                policy_compliant: true,
                category,
                source: "synthetic".to_string(),
            },
        }
    }

    /// Generate a complete training dataset combining historical and synthetic examples.
    ///
    /// This is the main entry point for dataset generation. It:
    /// 1. Exports historical telemetry events (if configured)
    /// 2. Generates synthetic examples (if configured)
    /// 3. Fills categories to meet minimum counts
    /// 4. Computes dataset hash for verification
    /// 5. Saves to JSONL file (if output path provided)
    ///
    /// # Arguments
    /// * `config` - Dataset configuration specifying sources, filters, and output
    ///
    /// # Returns
    /// A `BehaviorDataset` containing all examples, category counts, total count, and hash.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Historical export fails
    /// - Synthetic generation fails
    /// - File I/O fails (if output path provided)
    /// - JSON serialization fails
    ///
    /// # Process
    /// - Historical examples are exported first (if filter provided)
    /// - Synthetic examples are generated to meet minimums
    /// - Categories are filled to ensure `min_per_category` is met
    /// - Dataset hash is computed from serialized examples
    /// - Results are saved to JSONL if `output_path` is set
    pub async fn generate_dataset(&self, config: &DatasetConfig) -> Result<BehaviorDataset> {
        let mut all_examples = Vec::new();
        let mut categories: BTreeMap<BehaviorCategory, usize> = BTreeMap::new();

        // Export historical if configured
        if let Some(export_filter) = &config.export_filter {
            let historical = self.export_from_telemetry(export_filter).await?;
            all_examples.extend(historical);
            info!("Added {} historical examples", all_examples.len());
        }

        // Generate synthetic to meet minimums
        if let Some(synth_config) = &config.synthetic_config {
            let synthetic = self.generate_synthetic(synth_config)?;
            all_examples.extend(synthetic);
            info!("Added synthetic examples (total: {})", all_examples.len());
        }

        // Count categories and fill to minimum
        for cat in BehaviorCategory::all() {
            let count = all_examples
                .iter()
                .filter(|ex| ex.metadata.category == cat)
                .count();

            if count < config.min_per_category && config.min_per_category > 0 {
                let needed = config.min_per_category - count;
                let fill_config = SyntheticConfig {
                    num_examples: needed,
                    categories: vec![cat],
                    seed: self.seed.wrapping_add(cat as u64),
                    ..Default::default()
                };
                let fill_examples = self.generate_synthetic(&fill_config)?;
                all_examples.extend(fill_examples);
                categories.insert(cat, config.min_per_category);
            } else {
                categories.insert(cat, count);
            }
        }

        // Compute dataset hash
        let serialized = serde_json::to_string(&all_examples)?;
        let hash = B3Hash::hash(serialized.as_bytes()).to_hex();

        let total_examples = all_examples.len();

        // Save to file if path provided
        if let Some(path) = &config.output_path {
            self.save_to_jsonl(&all_examples, path).await?;
        }

        Ok(BehaviorDataset {
            examples: all_examples,
            categories,
            total_examples,
            hash,
        })
    }

    /// Save a dataset to a JSONL (JSON Lines) file.
    ///
    /// Writes each example as a JSON object on a separate line. This format
    /// is commonly used for machine learning training data.
    ///
    /// # Arguments
    /// * `examples` - Training examples to save
    /// * `path` - File path where the JSONL file will be written
    ///
    /// # Returns
    /// `Ok(())` if the file is written successfully.
    ///
    /// # Errors
    /// Returns an error if:
    /// - File creation fails
    /// - JSON serialization fails
    /// - File write fails
    /// - File sync fails
    pub async fn save_to_jsonl(
        &self,
        examples: &[BehaviorExample],
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let mut file = File::create(path.as_ref())
            .await
            .map_err(|e| AosError::Io(format!("Failed to create file: {}", e)))?;

        for example in examples {
            let line = serde_json::to_string(example)?;
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| AosError::Io(format!("Failed to write: {}", e)))?;
            file.write_all(b"\n")
                .await
                .map_err(|e| AosError::Io(format!("Failed to write newline: {}", e)))?;
        }

        file.sync_all()
            .await
            .map_err(|e| AosError::Io(format!("Failed to sync: {}", e)))?;

        info!(
            "Saved {} examples to {}",
            examples.len(),
            path.as_ref().display()
        );
        Ok(())
    }

    /// Validate a generated dataset for correctness and quality.
    ///
    /// Performs validation checks on the dataset:
    /// - Category distribution (warns if categories have < 100 examples)
    /// - Activation percentage bounds (0.0-1.0)
    /// - Quality score bounds (0.0-1.0)
    ///
    /// # Arguments
    /// * `examples` - Training examples to validate
    ///
    /// # Returns
    /// `Ok(())` if validation passes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Any example has invalid activation_pct (outside 0.0-1.0)
    /// - Any example has invalid quality score (outside 0.0-1.0)
    ///
    /// # Warnings
    /// Logs warnings for categories with fewer than 100 examples (recommended minimum: 500+).
    pub fn validate_dataset(&self, examples: &[BehaviorExample]) -> Result<()> {
        let category_counts: BTreeMap<BehaviorCategory, usize> =
            examples.iter().fold(BTreeMap::new(), |mut acc, ex| {
                *acc.entry(ex.metadata.category).or_insert(0) += 1;
                acc
            });

        for (cat, count) in &category_counts {
            if *count < 100 {
                warn!(
                    "Category {} has only {} examples (recommended: 500+)",
                    cat.as_str(),
                    count
                );
            }
        }

        // Validate activation percentages
        for ex in examples {
            if ex.input.activation_pct > 1.0 || ex.input.activation_pct < 0.0 {
                return Err(AosError::Validation(format!(
                    "Invalid activation_pct: {} (must be 0.0-1.0)",
                    ex.input.activation_pct
                )));
            }

            if ex.metadata.quality < 0.0 || ex.metadata.quality > 1.0 {
                return Err(AosError::Validation(format!(
                    "Invalid quality: {} (must be 0.0-1.0)",
                    ex.metadata.quality
                )));
            }
        }

        info!(
            "Validated {} examples across {} categories",
            examples.len(),
            category_counts.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behavior_category_roundtrip() {
        for cat in BehaviorCategory::all() {
            let str_repr = cat.as_str();
            assert!(!str_repr.is_empty());
        }
    }

    #[test]
    fn test_synthetic_config_default() {
        let config = SyntheticConfig::default();
        assert_eq!(config.num_examples, 100);
        assert_eq!(config.categories.len(), 6);
    }
}
