//! Progress rail signal context.
//!
//! Tracks background training progress from SSE events for the thin
//! fixed bar at the bottom of the HUD shell viewport.

use leptos::prelude::*;

/// State for the progress rail bar.
#[derive(Clone, Debug, Default)]
pub struct ProgressRailState {
    /// Progress fraction (0.0..=1.0). `None` hides the bar.
    pub progress: Option<f32>,
    /// Human-readable label, e.g. "Training epoch 3/10".
    pub label: Option<String>,
}

/// Provide the progress rail context at a shell-level scope.
///
/// Should be called once inside `HudShell` (or equivalent top-level component)
/// so that any descendant can read progress via [`use_progress_rail`].
pub fn provide_progress_rail_context() {
    let state = RwSignal::new(ProgressRailState::default());
    provide_context(state);
}

/// Read the current progress rail state.
pub fn use_progress_rail() -> ReadSignal<ProgressRailState> {
    expect_context::<RwSignal<ProgressRailState>>().read_only()
}

/// Get the writable signal for updating progress from SSE handlers.
///
/// Intended for internal use by event dispatchers (e.g. training SSE
/// handler). Components should use [`use_progress_rail`] instead.
pub fn use_progress_rail_writer() -> RwSignal<ProgressRailState> {
    expect_context::<RwSignal<ProgressRailState>>()
}
