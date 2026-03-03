//! Shared onboarding primitives for first-run/setup experiences.

pub mod action_panel;
pub mod container;
pub mod progress_stepper;
pub mod readiness_checklist;

pub use action_panel::OnboardingActionPanel;
pub use container::{OnboardingContainer, OnboardingHeader};
pub use progress_stepper::{OnboardingProgressStep, OnboardingProgressStepper};
pub use readiness_checklist::{OnboardingReadinessChecklist, ReadinessCheckItem};
