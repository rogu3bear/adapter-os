//! Inference readiness banner
//!
//! Shown globally when inference is not ready. Terse and action-oriented:
//! state the impact, name the cause, offer the fix. No mechanism explanations.
//!
//! Status surface strategy:
//! - Primary: this banner for immediate readiness blockers.
//! - Primary deep-dive: `/system` for full diagnostics and remediation context.
//! - Secondary: Status Center and System Tray remain available for compact/power usage.

use crate::components::inference_guidance::{guidance_for, primary_blocker};
use crate::components::{Button, ButtonLink, ButtonSize, ButtonVariant, IconX};
use crate::constants::ui_language;
use crate::hooks::{use_system_status, LoadingState};
use adapteros_api_types::InferenceReadyState;
use leptos::prelude::*;

#[component]
pub fn InferenceBanner() -> impl IntoView {
    let (status, refetch) = use_system_status();

    let status_center = crate::components::status_center::use_status_center();

    let retry = StoredValue::new(refetch);
    let (dismissed, set_dismissed) = signal(false);

    view! {
        {move || match status.get() {
            LoadingState::Loaded(s) => {
                if matches!(s.inference_ready, InferenceReadyState::True) || dismissed.get() {
                    return view! {}.into_any();
                }

                let blocker = primary_blocker(&s.inference_blockers);
                let guidance = guidance_for(s.inference_ready, blocker);
                let action = guidance.action;
                let extra_count = s.inference_blockers.len().saturating_sub(1);

                view! {
                    <div class="global-banner global-banner--warning relative" role="status" aria-live="polite">
                        // Dismiss button
                        <button
                            class="absolute top-2 right-2 text-current opacity-60 hover:opacity-100 transition-opacity p-1"
                            on:click=move |_| set_dismissed.set(true)
                            aria-label="Dismiss Safety Shield"
                            title="Dismiss Safety Shield"
                        >
                            <IconX class="w-4 h-4"/>
                        </button>
                        <div class="global-banner-content pr-8">
                            <span class="global-banner-title">{ui_language::SAFETY_SHIELD}</span>
                            <span class="global-banner-message">
                                {format!("{}.", guidance.reason)}
                                {(extra_count > 0).then(|| {
                                    let label = if extra_count == 1 {
                                        " +1 other issue.".to_string()
                                    } else {
                                        format!(" +{extra_count} other issues.")
                                    };
                                    view! {
                                        <span class="text-muted-foreground">{label}</span>
                                    }
                                })}
                            </span>
                        </div>
                        <div class="global-banner-actions">
                            <ButtonLink href=action.href variant=ButtonVariant::Outline size=ButtonSize::Sm>
                                {action.label}
                            </ButtonLink>
                            {status_center.map(|ctx| view! {
                                <button
                                    class="global-banner-why"
                                    on:click=move |_| ctx.open()
                                    type="button"
                                    title="Open Status Center"
                                >
                                    "Review details"
                                </button>
                            })}
                            <Button
                                variant=ButtonVariant::Ghost
                                size=ButtonSize::Sm
                                on_click=Callback::new(move |_| retry.with_value(|f| f.run(())))
                            >
                                "Recover automatically"
                            </Button>
                        </div>
                    </div>
                }.into_any()
            }
            _ => view! {}.into_any(),
        }}
    }
}
