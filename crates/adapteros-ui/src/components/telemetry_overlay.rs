//! Telemetry Overlay Component
//!
//! A minimal corner overlay displaying real-time system telemetry:
//! - Backend indicator (MLX/CoreML)
//! - Adapter count loaded
//! - Streaming status (idle/active/error)
//! - Last receipt digest (short form)
//!
//! Follows Liquid Glass Tier 1 design (blur: 9.6px, alpha: 70%).
//! Toggleable via user settings (off by default for clean UI).

use crate::components::status::{StatusColor, StatusIndicator};
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{use_chat, use_settings};
use adapteros_api_types::DataAvailability;
use leptos::prelude::*;

/// Streaming status for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingStatus {
    /// No active inference
    Idle,
    /// Currently streaming tokens
    Active,
    /// SSE connection error
    Error,
}

impl StreamingStatus {
    fn color(&self) -> StatusColor {
        match self {
            Self::Idle => StatusColor::Gray,
            Self::Active => StatusColor::Green,
            Self::Error => StatusColor::Red,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Active => "Streaming",
            Self::Error => "Error",
        }
    }
}

/// Telemetry overlay component
///
/// A small corner badge showing:
/// - Backend: MLX/CoreML/Metal indicator
/// - Adapters: count of adapters in active stacks
/// - Stream: idle/active/error status
/// - Digest: short form of last trace ID
///
/// Toggleable via Ctrl+Shift+T or settings.
#[component]
pub fn TelemetryOverlay() -> impl IntoView {
    let settings = use_settings();
    let (chat_state, _) = use_chat();

    // Fetch system status for backend and adapter info
    let (system_status, _refetch) = use_system_status();

    // Derive streaming status from chat state
    let streaming_status = Memo::new(move |_| {
        let Some(state) = chat_state.try_get() else {
            return StreamingStatus::Idle;
        };
        if state.streaming {
            StreamingStatus::Active
        } else if state.error.is_some() {
            StreamingStatus::Error
        } else {
            StreamingStatus::Idle
        }
    });

    // Get last trace ID from most recent assistant message
    let last_digest = Memo::new(move |_| {
        let state = chat_state.try_get()?;
        state
            .messages
            .iter()
            .rev()
            .find_map(|m| m.trace_id.clone())
            .map(|id| adapteros_id::short_id(&id))
    });

    // Extract backend and adapter count from system status
    let backend_info =
        Memo::new(
            move |_| match system_status.try_get().unwrap_or(LoadingState::Loading) {
                LoadingState::Loaded(status) => {
                    // Determine backend from kernel status
                    let backend = status
                        .kernel
                        .as_ref()
                        .and_then(|k| k.model.as_ref())
                        .map(|m| {
                            // Parse status field for backend indicator
                            let status_lower = m.status.to_lowercase();
                            if status_lower.contains("coreml") {
                                "CoreML"
                            } else if status_lower.contains("mlx") {
                                "MLX"
                            } else if status_lower.contains("metal") {
                                "Metal"
                            } else {
                                "Auto"
                            }
                        })
                        .unwrap_or("--");

                    // Get adapter count from kernel.adapters
                    let adapter_count = status
                        .kernel
                        .as_ref()
                        .and_then(|k| k.adapters.as_ref())
                        .and_then(|a| a.total_active)
                        .unwrap_or(0);

                    // Get model count from kernel.models
                    let model_count = status
                        .kernel
                        .as_ref()
                        .and_then(|k| k.models.as_ref())
                        .and_then(|m| m.loaded)
                        .unwrap_or(0);

                    // Check memory availability for status indicator
                    let memory_ok = status
                        .kernel
                        .as_ref()
                        .and_then(|k| k.memory.as_ref())
                        .and_then(|m| m.uma.as_ref())
                        .map(|uma| uma.availability == DataAvailability::Available)
                        .unwrap_or(false);

                    (backend.to_string(), adapter_count, model_count, memory_ok)
                }
                LoadingState::Loading | LoadingState::Idle => ("...".to_string(), 0, 0, false),
                LoadingState::Error(_) => ("Err".to_string(), 0, 0, false),
            },
        );

    // Keyboard shortcut for toggle (Ctrl+Shift+T)
    let shortcut_count = use_keyboard_shortcut_t();
    Effect::new(move || {
        let Some(count) = shortcut_count.try_get() else {
            return;
        };
        if count > 0 {
            let _ = settings.try_update(|s| {
                s.show_telemetry_overlay = !s.show_telemetry_overlay;
                s.save();
            });
        }
    });

    view! {
        {move || {
            let Some(s) = settings.try_get() else {
                return view! {}.into_any();
            };
            if !s.show_telemetry_overlay {
                return view! {}.into_any();
            }

            let (backend, adapters, models, memory_ok) = backend_info.try_get().unwrap_or_else(|| ("...".to_string(), 0, 0, false));
            let stream_status = streaming_status.try_get().unwrap_or(StreamingStatus::Idle);
            let digest = last_digest.try_get().flatten();

            view! {
                <div class="telemetry-overlay" role="status" aria-label="System telemetry">
                    // Backend indicator
                    <div class="telemetry-row" title="Inference backend">
                        <span class="telemetry-label">"Backend"</span>
                        <span class="telemetry-value font-mono">{backend}</span>
                    </div>

                    // Adapter count
                    <div class="telemetry-row" title="Adapters loaded in active stacks">
                        <span class="telemetry-label">"Adapters"</span>
                        <span class="telemetry-value font-mono">{adapters}</span>
                    </div>

                    // Model count
                    <div class="telemetry-row" title="Models loaded and ready">
                        <span class="telemetry-label">"Models"</span>
                        <span class="telemetry-value font-mono">{models}</span>
                    </div>

                    // Streaming status with indicator
                    <div class="telemetry-row" title="Streaming inference status">
                        <span class="telemetry-label">"Stream"</span>
                        <div class="telemetry-value flex items-center gap-1.5">
                            <StatusIndicator
                                color=stream_status.color()
                                pulsing=matches!(stream_status, StreamingStatus::Active)
                            />
                            <span class="font-mono text-xs">{stream_status.label()}</span>
                        </div>
                    </div>

                    // Last trace digest (if available)
                    {move || digest.clone().map(|d| view! {
                        <div class="telemetry-row" title="Last inference trace ID">
                            <span class="telemetry-label">"Digest"</span>
                            <span class="telemetry-value font-mono text-xs">{d}</span>
                        </div>
                    })}

                    // Memory status indicator
                    <div class="telemetry-row" title="Memory telemetry availability">
                        <span class="telemetry-label">"Memory"</span>
                        <div class="telemetry-value flex items-center gap-1.5">
                            <StatusIndicator
                                color=if memory_ok { StatusColor::Green } else { StatusColor::Yellow }
                            />
                            <span class="font-mono text-xs">
                                {if memory_ok { "OK" } else { "--" }}
                            </span>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

/// Hook for detecting Ctrl+Shift+T shortcut
fn use_keyboard_shortcut_t() -> ReadSignal<u32> {
    let (count, set_count) = signal(0u32);

    Effect::new(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let closure =
            Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
                let key_matches = event.key().to_lowercase() == "t";
                let ctrl_matches = event.ctrl_key() || event.meta_key();
                let shift_matches = event.shift_key();

                if key_matches && ctrl_matches && shift_matches {
                    event.prevent_default();
                    set_count.update(|c| *c = c.wrapping_add(1));
                }
            });

        if let Err(e) =
            document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
        {
            web_sys::console::warn_1(
                &format!("Failed to add telemetry overlay shortcut listener: {:?}", e).into(),
            );
            // Shortcut unavailable is non-critical; overlay is still accessible via settings
            return;
        }

        // Store closure to prevent it from being dropped
        closure.forget();
    });

    count
}
