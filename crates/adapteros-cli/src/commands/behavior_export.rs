//! Behavior export command
//!
//! Exports adapter lifecycle telemetry and generates synthetic behavior data for training.

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_orchestrator::{
    BehaviorCategory, BehaviorTrainingGenerator, DatasetConfig, ExportFilter, SyntheticConfig,
};
use chrono::{NaiveDate, TimeZone, Utc};
use clap::Args;
use std::path::PathBuf;
use tracing::info;

/// Export behavior training data from lifecycle events
#[derive(Args, Debug)]
pub struct BehaviorExportArgs {
    /// Output path for generated JSONL
    #[arg(short, long, required = true)]
    pub output: PathBuf,

    /// Categories to include (comma-separated: promotion,demotion,eviction,pinning,recovery,ttl_enforcement)
    #[arg(long)]
    pub categories: Option<String>,

    /// Start date for historical export (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// End date for historical export (YYYY-MM-DD)
    #[arg(long)]
    pub until: Option<String>,

    /// Tenant ID filter for export
    #[arg(long)]
    pub tenant: Option<String>,

    /// Adapter ID filter for export
    #[arg(long)]
    pub adapter: Option<String>,

    /// Number of synthetic examples to generate
    #[arg(long, default_value = "0")]
    pub synthetic_count: usize,

    /// Minimum examples per category (generates synthetic if needed)
    #[arg(long, default_value = "0")]
    pub min_per_category: usize,

    /// Seed for reproducible synthetic generation
    #[arg(long, default_value = "42")]
    pub seed: u64,

    /// Database path (defaults to DATABASE_URL env var)
    #[arg(long)]
    pub db_path: Option<String>,
}

impl BehaviorExportArgs {
    pub async fn execute(&self) -> Result<()> {
        info!("Starting behavior data export to {}", self.output.display());

        // Connect to database
        let db = if let Some(path) = &self.db_path {
            Db::connect(path).await?
        } else {
            Db::connect_env().await?
        };

        let generator = BehaviorTrainingGenerator::new(db, self.seed);

        // Parse categories
        let categories = self.parse_categories();

        // Build export filter if any time/tenant/adapter filters are specified
        let export_filter = if self.since.is_some()
            || self.until.is_some()
            || self.tenant.is_some()
            || self.adapter.is_some()
        {
            Some(ExportFilter {
                since: self.parse_date(&self.since, false)?,
                until: self.parse_date(&self.until, true)?,
                tenant_id: self.tenant.clone(),
                adapter_id: self.adapter.clone(),
                categories: if categories.is_empty() {
                    None
                } else {
                    Some(categories.clone())
                },
            })
        } else {
            None
        };

        // Build synthetic config if synthetic count > 0
        let synthetic_config = if self.synthetic_count > 0 {
            Some(SyntheticConfig {
                num_examples: self.synthetic_count,
                categories: if categories.is_empty() {
                    BehaviorCategory::all()
                } else {
                    categories.clone()
                },
                seed: self.seed,
                activation_range: (0.0, 1.0),
                memory_range: (50, 300),
            })
        } else {
            None
        };

        let config = DatasetConfig {
            export_filter,
            synthetic_config,
            min_per_category: self.min_per_category,
            output_path: Some(self.output.to_string_lossy().to_string()),
        };

        let dataset = generator.generate_dataset(&config).await?;

        info!(
            "Exported {} behavior examples to {}",
            dataset.total_examples,
            self.output.display()
        );

        for (cat, count) in &dataset.categories {
            info!("  Category {}: {} examples", cat.as_str(), count);
        }

        info!("Dataset hash: {}", dataset.hash);

        Ok(())
    }

    fn parse_categories(&self) -> Vec<BehaviorCategory> {
        if let Some(cat_str) = &self.categories {
            cat_str
                .split(',')
                .map(|s| s.trim())
                .filter_map(|s| match s {
                    "promotion" => Some(BehaviorCategory::Promotion),
                    "demotion" => Some(BehaviorCategory::Demotion),
                    "eviction" => Some(BehaviorCategory::Eviction),
                    "pinning" => Some(BehaviorCategory::Pinning),
                    "recovery" => Some(BehaviorCategory::Recovery),
                    "ttl_enforcement" => Some(BehaviorCategory::TtlEnforcement),
                    "all" => None,
                    _ => {
                        tracing::warn!("Unknown category: {}, ignoring", s);
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    fn parse_date(
        &self,
        date_str: &Option<String>,
        end_of_day: bool,
    ) -> Result<Option<chrono::DateTime<Utc>>> {
        match date_str {
            Some(s) => {
                let naive = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
                    adapteros_core::AosError::Parse(format!("Invalid date format: {}", e))
                })?;

                let datetime = if end_of_day {
                    naive.and_hms_opt(23, 59, 59).ok_or_else(|| {
                        adapteros_core::AosError::Parse("Invalid time".to_string())
                    })?
                } else {
                    naive.and_hms_opt(0, 0, 0).ok_or_else(|| {
                        adapteros_core::AosError::Parse("Invalid time".to_string())
                    })?
                };

                Ok(Some(Utc.from_utc_datetime(&datetime)))
            }
            None => Ok(None),
        }
    }
}
