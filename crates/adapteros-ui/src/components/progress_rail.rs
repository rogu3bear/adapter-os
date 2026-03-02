//! Progress rail — thin bar at the bottom of the viewport showing
//! background training progress.

use crate::signals::progress_rail::use_progress_rail;
use leptos::prelude::*;

/// A 3 px fixed bar at the bottom of the viewport.
///
/// Hidden when no progress is active (`progress == None`). Width tracks
/// the `progress` fraction (0.0..=1.0). The human-readable label is
/// exposed via a `title` attribute for hover tooltip.
#[component]
pub fn ProgressRail() -> impl IntoView {
    let state = use_progress_rail();

    let visible = Memo::new(move |_| state.get().progress.is_some());

    let width_pct = Memo::new(move |_| {
        state
            .get()
            .progress
            .map(|p| format!("{}%", (p.clamp(0.0, 1.0) * 100.0)))
            .unwrap_or_default()
    });

    let label = Memo::new(move |_| state.get().label.clone().unwrap_or_default());

    move || {
        if !visible.get() {
            return None;
        }

        let style = format!("width: {}", width_pct.get());
        let title = label.get();

        Some(view! {
            <div class="progress-rail" title=title.clone()>
                <div class="progress-rail-fill" style=style/>
            </div>
        })
    }
}
