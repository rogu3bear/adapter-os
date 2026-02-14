//! Multi-step progress indicator for long-running operations.
//!
//! Shows step-by-step progress (e.g., "Preparing...", "Uploading...", "Registering...")
//! instead of a single spinner, improving perceived responsiveness for operations
//! like training job start, model load, or bulk actions.

use crate::components::Spinner;
use leptos::prelude::*;

/// A single stage in a multi-step operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressStage {
    /// Unique identifier for this stage.
    pub id: &'static str,
    /// Display label shown to user.
    pub label: &'static str,
    /// Optional detail message shown below the label.
    pub detail: Option<String>,
}

impl ProgressStage {
    /// Create a new progress stage with just a label.
    pub fn new(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            detail: None,
        }
    }

    /// Create a new progress stage with a detail message.
    pub fn with_detail(id: &'static str, label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            label,
            detail: Some(detail.into()),
        }
    }
}

/// Status of a progress stage.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum StageStatus {
    /// Stage has not started yet.
    #[default]
    Pending,
    /// Stage is currently in progress.
    Active,
    /// Stage completed successfully.
    Done,
    /// Stage encountered an error.
    Error,
}

/// Multi-step progress indicator component.
///
/// Displays a vertical list of stages with their current status, showing
/// users exactly what's happening during long operations.
///
/// # Example
/// ```rust,ignore
/// let stages = vec![
///     ProgressStage::new("prepare", "Preparing..."),
///     ProgressStage::new("upload", "Uploading data..."),
///     ProgressStage::new("register", "Registering adapter..."),
/// ];
/// let current_stage = RwSignal::new(Some("prepare".to_string()));
/// let completed = RwSignal::new(vec!["prepare".to_string()]);
///
/// view! {
///     <ProgressStages
///         stages=stages
///         current_stage=current_stage
///         completed_stages=completed
///     />
/// }
/// ```
#[component]
pub fn ProgressStages(
    /// List of stages to display.
    stages: Vec<ProgressStage>,
    /// ID of the currently active stage (None if not started or all done).
    current_stage: Signal<Option<String>>,
    /// IDs of completed stages.
    completed_stages: Signal<Vec<String>>,
    /// IDs of stages that encountered errors.
    #[prop(optional)]
    error_stages: Option<Signal<Vec<String>>>,
    /// Optional title shown above the stages.
    #[prop(optional, into)]
    title: Option<String>,
) -> impl IntoView {
    let stages = StoredValue::new(stages);

    view! {
        <div class="progress-stages">
            {title.map(|t| view! {
                <div class="progress-stages-header">
                    <Spinner />
                    <span class="progress-stages-title">{t}</span>
                </div>
            })}
            <div class="progress-stages-list">
                {move || {
                    let current = current_stage.try_get().flatten();
                    let completed = completed_stages.try_get().unwrap_or_default();
                    let errors = error_stages.and_then(|s| s.try_get()).unwrap_or_default();

                    stages.with_value(|stages| {
                        stages.iter().map(|stage| {
                            let status = if errors.contains(&stage.id.to_string()) {
                                StageStatus::Error
                            } else if completed.contains(&stage.id.to_string()) {
                                StageStatus::Done
                            } else if current.as_deref() == Some(stage.id) {
                                StageStatus::Active
                            } else {
                                StageStatus::Pending
                            };

                            let status_class = match status {
                                StageStatus::Pending => "progress-stage-pending",
                                StageStatus::Active => "progress-stage-active",
                                StageStatus::Done => "progress-stage-done",
                                StageStatus::Error => "progress-stage-error",
                            };

                            view! {
                                <div class=format!("progress-stage {}", status_class)>
                                    <div class="progress-stage-icon">
                                        {match status {
                                            StageStatus::Done => view! {
                                                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                                </svg>
                                            }.into_any(),
                                            StageStatus::Error => view! {
                                                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                                                </svg>
                                            }.into_any(),
                                            StageStatus::Active => view! {
                                                <div class="progress-stage-spinner">
                                                    <div class="progress-stage-spinner-dot"></div>
                                                </div>
                                            }.into_any(),
                                            StageStatus::Pending => view! {
                                                <div class="progress-stage-dot"></div>
                                            }.into_any(),
                                        }}
                                    </div>
                                    <div class="progress-stage-content">
                                        <span class="progress-stage-label">{stage.label}</span>
                                        {stage.detail.clone().map(|d| view! {
                                            <span class="progress-stage-detail">{d}</span>
                                        })}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()
                    })
                }}
            </div>
        </div>
    }
}

/// Inline progress indicator for simpler use cases.
///
/// Shows a single line with spinner and current stage label.
#[component]
pub fn InlineProgress(
    /// Current stage label.
    label: Signal<String>,
    /// Optional detail message.
    #[prop(optional)]
    detail: Option<Signal<String>>,
) -> impl IntoView {
    view! {
        <div class="inline-progress">
            <Spinner />
            <span class="inline-progress-label">{move || label.try_get().unwrap_or_default()}</span>
            {detail.map(|d| view! {
                <span class="inline-progress-detail">{move || d.try_get().unwrap_or_default()}</span>
            })}
        </div>
    }
}

/// Controller for managing progress stage state.
///
/// Provides a convenient API for driving ProgressStages from async operations.
#[derive(Clone)]
pub struct ProgressController {
    current: RwSignal<Option<String>>,
    completed: RwSignal<Vec<String>>,
    errors: RwSignal<Vec<String>>,
}

impl ProgressController {
    /// Create a new progress controller.
    pub fn new() -> Self {
        Self {
            current: RwSignal::new(None),
            completed: RwSignal::new(vec![]),
            errors: RwSignal::new(vec![]),
        }
    }

    /// Get the current stage signal for binding to ProgressStages.
    pub fn current_stage(&self) -> Signal<Option<String>> {
        self.current.into()
    }

    /// Get the completed stages signal for binding to ProgressStages.
    pub fn completed_stages(&self) -> Signal<Vec<String>> {
        self.completed.into()
    }

    /// Get the error stages signal for binding to ProgressStages.
    pub fn error_stages(&self) -> Signal<Vec<String>> {
        self.errors.into()
    }

    /// Start a stage (marks it as active).
    pub fn start(&self, stage_id: &str) {
        self.current.set(Some(stage_id.to_string()));
    }

    /// Complete a stage successfully.
    pub fn complete(&self, stage_id: &str) {
        self.completed.update(|c| {
            if !c.contains(&stage_id.to_string()) {
                c.push(stage_id.to_string());
            }
        });
        // Clear current if this was the active stage
        if self.current.get_untracked().as_deref() == Some(stage_id) {
            self.current.set(None);
        }
    }

    /// Mark a stage as having an error.
    pub fn error(&self, stage_id: &str) {
        self.errors.update(|e| {
            if !e.contains(&stage_id.to_string()) {
                e.push(stage_id.to_string());
            }
        });
        // Clear current if this was the active stage
        if self.current.get_untracked().as_deref() == Some(stage_id) {
            self.current.set(None);
        }
    }

    /// Reset all progress state.
    pub fn reset(&self) {
        self.current.set(None);
        self.completed.set(vec![]);
        self.errors.set(vec![]);
    }

    /// Advance through stages: completes current and starts next.
    pub fn advance(&self, completed_id: &str, next_id: &str) {
        self.complete(completed_id);
        self.start(next_id);
    }
}

impl Default for ProgressController {
    fn default() -> Self {
        Self::new()
    }
}
