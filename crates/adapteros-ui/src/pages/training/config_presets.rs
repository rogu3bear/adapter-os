//! Training configuration presets
//!
//! Provides preset configurations for common training scenarios:
//! - Identity: Minimal training to learn dataset identity
//! - QA: Question-answer pair training optimization
//!
//! These presets use existing TrainingConfig fields without adding new config logic.

use crate::components::{FormField, Input, Select, Toggle};
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
        let estimate = preset.estimate_time_minutes(Some(100));
        assert!(estimate.is_some());
        let minutes = estimate.unwrap();
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

/// Training configuration presets panel
///
/// Provides preset selection and parameter configuration with:
/// - Preset selector (Identity, QA, Custom)
/// - Epochs input
/// - Learning rate input
/// - Validation split input
/// - Early stopping toggle
/// - Optional training time estimate
#[component]
pub fn TrainingConfigPresets(
    /// Current preset selection
    #[prop(into)]
    preset: RwSignal<String>,
    /// Epochs value (string for input binding)
    #[prop(into)]
    epochs: RwSignal<String>,
    /// Learning rate value (string for input binding)
    #[prop(into)]
    learning_rate: RwSignal<String>,
    /// Validation split value (string for input binding)
    #[prop(into)]
    validation_split: RwSignal<String>,
    /// Early stopping enabled
    #[prop(into)]
    early_stopping: RwSignal<bool>,
    /// Optional sample count for time estimation
    #[prop(optional)]
    sample_count: Option<usize>,
) -> impl IntoView {
    // Derive current preset from signal
    let current_preset = Signal::derive(move || TrainingPreset::parse_str(&preset.get()));

    // Apply preset when selection changes
    let apply_preset = move |new_preset: TrainingPreset| {
        preset.set(new_preset.as_str().to_string());
        if new_preset != TrainingPreset::Custom {
            let config = new_preset.config();
            epochs.set(config.epochs.to_string());
            learning_rate.set(config.learning_rate.to_string());
            validation_split.set(config.validation_split.to_string());
            early_stopping.set(config.early_stopping);
        }
    };

    // Time estimate signal
    let time_estimate = Signal::derive(move || {
        let p = current_preset.get();
        p.estimate_time_minutes(sample_count)
    });

    view! {
        <div class="space-y-6">
            // Preset selector
            <div>
                <label class="text-sm font-medium mb-3 block">"Training Preset"</label>
                <div class="grid grid-cols-3 gap-3">
                    <PresetCard
                        preset=TrainingPreset::Identity
                        selected=Signal::derive(move || current_preset.get() == TrainingPreset::Identity)
                        on_select=move |_| apply_preset(TrainingPreset::Identity)
                    />
                    <PresetCard
                        preset=TrainingPreset::Qa
                        selected=Signal::derive(move || current_preset.get() == TrainingPreset::Qa)
                        on_select=move |_| apply_preset(TrainingPreset::Qa)
                    />
                    <PresetCard
                        preset=TrainingPreset::Custom
                        selected=Signal::derive(move || current_preset.get() == TrainingPreset::Custom)
                        on_select=move |_| apply_preset(TrainingPreset::Custom)
                    />
                </div>
            </div>

            // Time estimate (when available)
            {move || match time_estimate.get() {
                Some(minutes) => {
                    let formatted = if minutes < 1.0 {
                        "< 1 min".to_string()
                    } else if minutes < 60.0 {
                        format!("~{:.0} min", minutes)
                    } else {
                        let hours = minutes / 60.0;
                        format!("~{:.1} hrs", hours)
                    };

                    view! {
                        <div class="rounded-lg border border-primary/20 bg-primary/5 p-3">
                            <div class="flex items-center gap-2">
                                <svg class="w-4 h-4 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                </svg>
                                <span class="text-sm">
                                    "Estimated training time (rough): "
                                    <span class="font-medium">{formatted}</span>
                                </span>
                            </div>
                        </div>
                    }.into_any()
                }
                None => view! {
                    <div class="rounded-lg border border-muted/60 bg-muted/30 p-3">
                        <div class="flex items-center gap-2">
                            <svg class="w-4 h-4 text-muted-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
                            </svg>
                            <span class="text-sm text-muted-foreground">
                                "Estimated training time: unavailable (select a dataset)"
                            </span>
                        </div>
                    </div>
                }.into_any(),
            }}

            // Configuration parameters
            <div class="space-y-4">
                <div class="grid gap-4 sm:grid-cols-2">
                    <FormField
                        label="Epochs"
                        name="epochs"
                        required=true
                        help="Number of complete passes through the dataset"
                    >
                        <Input
                            value=epochs
                            input_type="number".to_string()
                        />
                    </FormField>

                    <FormField
                        label="Learning Rate"
                        name="learning_rate"
                        required=true
                        help="Step size for weight updates (0.00001 - 0.01)"
                    >
                        <Input
                            value=learning_rate
                            input_type="number".to_string()
                        />
                    </FormField>
                </div>

                <div class="grid gap-4 sm:grid-cols-2">
                    <FormField
                        label="Validation Split"
                        name="validation_split"
                        help="Fraction of data for validation (0 - 0.5)"
                    >
                        <Input
                            value=validation_split
                            input_type="number".to_string()
                        />
                    </FormField>

                    <div class="flex items-end pb-1">
                        <Toggle
                            checked=early_stopping
                            label="Early Stopping".to_string()
                            description="Stop training when validation loss stops improving".to_string()
                        />
                    </div>
                </div>
            </div>

            // Preset description
            <div class="text-xs text-muted-foreground">
                {move || {
                    let p = current_preset.get();
                    view! {
                        <p>
                            <span class="font-medium">{p.label()}</span>
                            ": "
                            {p.description()}
                        </p>
                    }
                }}
            </div>
        </div>
    }
}

/// Preset selection card
#[component]
fn PresetCard(
    preset: TrainingPreset,
    selected: Signal<bool>,
    on_select: impl Fn(()) + Clone + 'static,
) -> impl IntoView {
    let config = preset.config();

    view! {
        <button
            type="button"
            class=move || {
                let base = "relative rounded-lg border p-4 text-left transition-all hover:border-primary/50";
                if selected.get() {
                    format!("{} border-primary bg-primary/5 ring-1 ring-primary", base)
                } else {
                    format!("{} border-border bg-card hover:bg-muted/50", base)
                }
            }
            on:click=move |_| on_select(())
        >
            // Selection indicator
            {move || selected.get().then(|| view! {
                <div class="absolute top-2 right-2">
                    <svg class="w-4 h-4 text-primary" fill="currentColor" viewBox="0 0 20 20">
                        <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/>
                    </svg>
                </div>
            })}

            <div class="space-y-2">
                <div class="font-medium">{preset.label()}</div>
                <div class="text-xs text-muted-foreground space-y-1">
                    <div class="flex justify-between">
                        <span>"Epochs"</span>
                        <span class="font-mono">{config.epochs}</span>
                    </div>
                    <div class="flex justify-between">
                        <span>"LR"</span>
                        <span class="font-mono">{format!("{:.4}", config.learning_rate)}</span>
                    </div>
                    <div class="flex justify-between">
                        <span>"Val Split"</span>
                        <span class="font-mono">{format!("{:.0}%", config.validation_split * 100.0)}</span>
                    </div>
                    {config.early_stopping.then(|| view! {
                        <div class="flex items-center gap-1 text-primary">
                            <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                            </svg>
                            <span>"Early stop"</span>
                        </div>
                    })}
                </div>
            </div>
        </button>
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
