//! SSE notification hook.

use crate::api::SseState;
use crate::signals::try_use_notifications;
use leptos::prelude::*;

/// Emit notifications when SSE connection state changes.
pub fn use_sse_notifications(state: ReadSignal<SseState>) {
    let Some(notifications) = try_use_notifications() else {
        return;
    };

    let last_state = StoredValue::new(state.get_untracked());

    Effect::new(move || {
        let current = state.get();
        let previous = last_state.get_value();
        if current == previous {
            return;
        }

        match current {
            SseState::Connected => {
                notifications.success("Live updates", "Streaming connection established.");
            }
            SseState::Error => {
                notifications.warning("Live updates", "Connection interrupted. Retrying...");
            }
            SseState::CircuitOpen => {
                notifications.error(
                    "Live updates",
                    "Connection paused. Manual retry may be needed.",
                );
            }
            _ => {}
        }

        last_state.set_value(current);
    });
}
