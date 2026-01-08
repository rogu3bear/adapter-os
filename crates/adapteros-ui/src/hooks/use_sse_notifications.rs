//! SSE notification hook.

use crate::api::SseState;
use crate::signals::try_use_notifications;
use js_sys::Date;
use leptos::prelude::*;

/// Emit notifications when SSE connection state changes.
pub fn use_sse_notifications(state: ReadSignal<SseState>) {
    let Some(notifications) = try_use_notifications() else {
        return;
    };

    let last_state = StoredValue::new(state.get_untracked());
    let last_notify_at = StoredValue::new(0f64);
    let cooldown_ms = 2500.0;

    Effect::new(move || {
        let current = state.get();
        let previous = last_state.get_value();
        if current == previous {
            return;
        }

        let now = Date::now();
        if now - last_notify_at.get_value() < cooldown_ms {
            last_state.set_value(current);
            return;
        }

        let mut did_notify = false;
        match current {
            SseState::Connected => {
                notifications.success("Live updates", "Streaming connection established.");
                did_notify = true;
            }
            SseState::Error => {
                notifications.warning("Live updates", "Connection interrupted. Retrying...");
                did_notify = true;
            }
            SseState::CircuitOpen => {
                notifications.error(
                    "Live updates",
                    "Connection paused. Manual retry may be needed.",
                );
                did_notify = true;
            }
            _ => {}
        }

        if did_notify {
            last_notify_at.set_value(now);
        }
        last_state.set_value(current);
    });
}
