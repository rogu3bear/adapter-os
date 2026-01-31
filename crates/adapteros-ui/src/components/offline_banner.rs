//! Offline banner to indicate backend connectivity issues.

use crate::api::ApiClient;
use crate::components::Button;
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::notifications::try_use_notifications;
use leptos::prelude::*;
use std::sync::Arc;

/// Banner displayed when the backend is unreachable.
///
/// Also tracks state transitions and shows a "Reconnected" toast when
/// recovering from an offline state.
#[component]
pub fn OfflineBanner() -> impl IntoView {
    let (health, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.health().await });

    let retry = StoredValue::new(refetch);

    // Track previous offline state for transition detection
    let was_offline = RwSignal::new(false);

    // Watch for offline→online transitions
    Effect::new(move || {
        let current_state = health.get();
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
            <div class="offline-banner">
                <div class="offline-banner-content">
                    <span class="offline-banner-title">"Backend Offline"</span>
                    <span class="offline-banner-message">
                        "Unable to reach the adapterOS API. Some data may be stale."
                    </span>
                </div>
                <Button
                    class="offline-banner-action".to_string()
                    on_click=Callback::new(move |_| retry.with_value(|f| f.run(())))
                >
                    "Retry"
                </Button>
            </div>
        </Show>
    }
}
