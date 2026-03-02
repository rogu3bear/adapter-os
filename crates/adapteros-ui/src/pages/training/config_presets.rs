//! Training configuration presets
//!
//! Provides preset configurations for common training scenarios:
//! - Identity: Minimal training to learn dataset identity
//! - QA: Question-answer pair training optimization
//!
//! These presets use existing TrainingConfig fields without adding new config logic.

use crate::components::Select;
use leptos::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_preset_config() {
        let preset = TrainingPreset::Identity;
        let config = preset.config();

        assert_eq!(config.epochs, 3, "Identity preset should have 3 epochs");
        assert!(
            (config.learning_rate - 0.0002).abs() < 0.00001,
            "Identity LR should be 0.0002"
        );
        assert!(
            (config.validation_split - 0.1).abs() < 0.001,
            "Identity val split should be 0.1"
        );
        assert!(
            !config.early_stopping,
            "Identity should not use early stopping"
        );
    }

    #[test]
    fn test_qa_preset_config() {
        let preset = TrainingPreset::Qa;
        let config = preset.config();

        assert_eq!(config.epochs, 10, "QA preset should have 10 epochs");
        assert!(
            (config.learning_rate - 0.0001).abs() < 0.00001,
            "QA LR should be 0.0001"
        );
        assert!(
            (config.validation_split - 0.15).abs() < 0.001,
            "QA val split should be 0.15"
        );
        assert!(config.early_stopping, "QA should use early stopping");
    }

    #[test]
    fn test_custom_preset_config() {
        let preset = TrainingPreset::Custom;
        let config = preset.config();

        assert_eq!(
            config.epochs, 10,
            "Custom preset should have 10 default epochs"
        );
        assert!(
            !config.early_stopping,
            "Custom should not use early stopping by default"
        );
    }

    #[test]
    fn test_preset_parse_str_roundtrip() {
        for preset in [
            TrainingPreset::Identity,
            TrainingPreset::Qa,
            TrainingPreset::Custom,
        ] {
            let as_str = preset.as_str();
            let back = TrainingPreset::parse_str(as_str);
            assert_eq!(preset, back, "Roundtrip failed for {:?}", preset);
        }
    }

    #[test]
    fn test_preset_labels() {
        assert_eq!(TrainingPreset::Identity.label(), "Identity");
        assert_eq!(TrainingPreset::Qa.label(), "Q&A");
        assert_eq!(TrainingPreset::Custom.label(), "Custom");
    }

    #[test]
    fn test_time_estimation() {
        let preset = TrainingPreset::Qa; // 10 epochs

        // No sample count = no estimate
        assert!(preset.estimate_time_minutes(None).is_none());

        // Zero samples = no estimate
        assert!(preset.estimate_time_minutes(Some(0)).is_none());

        // With samples, should get an estimate
        let minutes = preset
            .estimate_time_minutes(Some(100))
            .expect("estimate should be Some for non-zero sample count");
        // 100 samples * 0.5s/sample * 10 epochs = 500s = ~8.33 min
        assert!(
            minutes > 7.0 && minutes < 10.0,
            "Expected ~8.33 min, got {}",
            minutes
        );
    }
}

/// Training preset type
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TrainingPreset {
    /// Identity training - learns dataset style with minimal epochs
    Identity,
    /// QA training - optimized for question-answer pairs
    Qa,
    /// Custom configuration
    #[default]
    Custom,
}

impl TrainingPreset {
    pub fn label(&self) -> &'static str {
        match self {
            TrainingPreset::Identity => "Identity",
            TrainingPreset::Qa => "Q&A",
            TrainingPreset::Custom => "Custom",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            TrainingPreset::Identity => "Learn dataset style with minimal training",
            TrainingPreset::Qa => "Optimized for question-answer pairs",
            TrainingPreset::Custom => "Configure all parameters manually",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingPreset::Identity => "identity",
            TrainingPreset::Qa => "qa",
            TrainingPreset::Custom => "custom",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "identity" => TrainingPreset::Identity,
            "qa" => TrainingPreset::Qa,
            _ => TrainingPreset::Custom,
        }
    }
}

/// Preset configuration values
pub struct PresetConfig {
    pub epochs: u32,
    pub learning_rate: f32,
    pub validation_split: f32,
    pub early_stopping: bool,
}

impl TrainingPreset {
    /// Get the preset configuration values
    pub fn config(&self) -> PresetConfig {
        match self {
            TrainingPreset::Identity => PresetConfig {
                epochs: 3,
                learning_rate: 0.0002,
                validation_split: 0.1,
                early_stopping: false,
            },
            TrainingPreset::Qa => PresetConfig {
                epochs: 10,
                learning_rate: 0.0001,
                validation_split: 0.15,
                early_stopping: true,
            },
            TrainingPreset::Custom => PresetConfig {
                epochs: 10,
                learning_rate: 0.0001,
                validation_split: 0.0,
                early_stopping: false,
            },
        }
    }

    /// Estimate training time in minutes based on dataset size
    /// Returns None if estimation not available
    pub fn estimate_time_minutes(&self, sample_count: Option<usize>) -> Option<f32> {
        let samples = sample_count?;
        if samples == 0 {
            return None;
        }

        // Rough estimate: ~0.5 seconds per sample per epoch
        // This is a placeholder - real estimation would depend on hardware
        let seconds_per_sample = 0.5_f32;
        let config = self.config();
        let total_seconds = samples as f32 * seconds_per_sample * config.epochs as f32;

        Some(total_seconds / 60.0)
    }
}

/// Compact preset selector for inline use
#[component]
pub fn PresetSelector(
    #[prop(into)] value: RwSignal<String>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let options = vec![
        ("identity".to_string(), "Identity".to_string()),
        ("qa".to_string(), "Q&A".to_string()),
        ("custom".to_string(), "Custom".to_string()),
    ];

    view! {
        <Select
            value=value
            options=options
            label="Preset".to_string()
            class=class
        />
    }
}
