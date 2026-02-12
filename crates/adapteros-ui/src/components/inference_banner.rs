//! Inference readiness banner
//!
//! Shown globally when inference is not ready. Terse and action-oriented:
//! state the impact, name the cause, offer the fix. No mechanism explanations.

use crate::api::ApiClient;
use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn InferenceBanner() -> impl IntoView {
    let (status, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    let status_center = crate::components::status_center::use_status_center();

    let retry = StoredValue::new(refetch);

    view! {
        {move || match status.get() {
            LoadingState::Loaded(s) => {
                if matches!(s.inference_ready, InferenceReadyState::True) {
                    return view! {}.into_any();
                }

                let blocker = primary_blocker(&s.inference_blockers);
                let guidance = guidance_for(s.inference_ready, blocker);
                let action = guidance.action;
                let extra_count = s.inference_blockers.len().saturating_sub(1);

                view! {
                    <div class="inference-banner" role="status" aria-live="polite">
                        <div class="inference-banner-content">
                            <span class="inference-banner-title">"Chat unavailable"</span>
                            <span class="inference-banner-message">
                                {format!("{}.", guidance.reason)}
                                {(extra_count > 0).then(|| {
                                    let label = if extra_count == 1 {
                                        " +1 other issue.".to_string()
                                    } else {
                                        format!(" +{extra_count} other issues.")
                                    };
                                    view! {
                                        <span class="inference-banner-extra">{label}</span>
                                    }
                                })}
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
                                    "Details"
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
            _ => view! {}.into_any(),
        }}
    }
}
