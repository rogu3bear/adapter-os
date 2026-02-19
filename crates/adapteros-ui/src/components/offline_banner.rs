//! Offline banner to indicate backend connectivity issues.

use crate::components::Button;
use crate::hooks::{use_health, LoadingState};
use crate::signals::notifications::try_use_notifications;
use leptos::prelude::*;

/// Banner displayed when the backend is unreachable.
///
/// Standardized wording:
/// - "Retry" for refetch operations
/// - Shows cached data availability message
///
/// Also tracks state transitions and shows a "Reconnected" toast when
/// recovering from an offline state.
#[component]
pub fn OfflineBanner() -> impl IntoView {
    let (health, refetch) = use_health();

    let retry = StoredValue::new(refetch);

    // Track previous offline state for transition detection
    let was_offline = RwSignal::new(false);

    // Watch for offline→online transitions
    Effect::new(move || {
        let Some(current_state) = health.try_get() else {
            return;
        };
        let is_error = matches!(current_state, LoadingState::Error(_));
        let prev_was_offline = was_offline.get_untracked();

        if is_error {
            // Mark as offline
            was_offline.set(true);
        } else if matches!(current_state, LoadingState::Loaded(_)) && prev_was_offline {
            // Transitioned from offline to online
            was_offline.set(false);
            if let Some(notifications) = try_use_notifications() {
                notifications.success("Reconnected", "Backend connection restored.");
            }
        }
    });

    view! {
        <Show when=move || matches!(health.get(), LoadingState::Error(_))>
            <div
                class="global-banner global-banner--error"
                role="alert"
                aria-live="polite"
            >
                <div class="global-banner-content">
                    <span class="global-banner-title">"Backend offline"</span>
                    <span class="global-banner-message">
                        "Unable to reach the adapterOS API. You can keep viewing cached data."
                    </span>
                </div>
                <div class="global-banner-actions">
                    <Button
                        on_click=Callback::new(move |_| retry.with_value(|f| f.run(())))
                    >
                        "Retry"
                    </Button>
                </div>
            </div>
        </Show>
    }
}
