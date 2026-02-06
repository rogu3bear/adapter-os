//! Inference readiness banner
//!
//! Shown globally when inference is not ready (e.g., no workers or no model loaded).
//! This is intentionally terse and action-oriented: users should immediately know
//! what "load a model" means and where to go next.

use crate::api::ApiClient;
use crate::components::inference_guidance::guidance_for;
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{InferenceBlocker, InferenceReadyState};
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn InferenceBanner() -> impl IntoView {
    let (status, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    // Optional context for opening the Status Center panel (Ctrl+Shift+S).
    // If absent, we fall back to linking to /system via the primary action.
    let status_center = crate::components::status_center::use_status_center();

    let retry = StoredValue::new(refetch);

    view! {
        {move || match status.get() {
            LoadingState::Loaded(s) => {
                if matches!(s.inference_ready, InferenceReadyState::True) {
                    view! {}.into_any()
                } else {
                    let first_blocker = s.inference_blockers.first();
                    let guidance = guidance_for(s.inference_ready, first_blocker);
                    let action = guidance.action;
                    let extra = extra_context(first_blocker);

                    view! {
                        <div class="inference-banner" role="status" aria-live="polite">
                            <div class="inference-banner-content">
                                <span class="inference-banner-title">"Inference not ready"</span>
                                <span class="inference-banner-message">
                                    {format!("{}.", guidance.reason)}
                                    {extra.map(|t| view! { <span class="inference-banner-extra">" " {t}</span> })}
                                </span>
                            </div>
                            <div class="inference-banner-actions">
                                <a href=action.href class="btn btn-outline btn-sm">
                                    {action.label}
                                </a>
                                {status_center.map(|ctx| view! {
                                    <button
                                        class="inference-banner-why"
                                        on:click=move |_| ctx.open()
                                        type="button"
                                        title="Open Status Center"
                                    >
                                        "Why?"
                                    </button>
                                })}
                                <button
                                    class="btn btn-ghost btn-sm"
                                    on:click=move |_| retry.with_value(|f| f.run(()))
                                    type="button"
                                >
                                    "Retry"
                                </button>
                            </div>
                        </div>
                    }.into_any()
                }
            }
            _ => view! {}.into_any(),
        }}
    }
}

fn extra_context(blocker: Option<&InferenceBlocker>) -> Option<&'static str> {
    match blocker {
        Some(InferenceBlocker::NoModelLoaded) => {
            Some("Loading a model makes it active in memory on a worker so chat/inference can run.")
        }
        Some(InferenceBlocker::WorkerMissing) => {
            Some("A worker is the process that hosts models and runs inference for requests.")
        }
        _ => None,
    }
}
