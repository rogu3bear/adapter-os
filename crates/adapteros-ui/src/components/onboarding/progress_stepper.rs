use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct OnboardingProgressStep {
    pub label: String,
    pub index: usize,
    pub is_complete: bool,
    pub is_active: bool,
}

/// Shared progress stepper for onboarding/checklist flows.
#[component]
pub fn OnboardingProgressStepper(steps: Vec<OnboardingProgressStep>) -> impl IntoView {
    view! {
        <div class="wizard-progress">
            {steps
                .into_iter()
                .map(|step| {
                    let class = if step.is_complete {
                        "wizard-step wizard-step-complete"
                    } else if step.is_active {
                        "wizard-step wizard-step-active"
                    } else {
                        "wizard-step"
                    };
                    let step_num = step.index + 1;
                    view! {
                        <div class=class>
                            <div class="wizard-step-circle">
                                {if step.is_complete {
                                    view! {
                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M5 13l4 4L19 7"/>
                                        </svg>
                                    }
                                        .into_any()
                                } else {
                                    view! { <span>{step_num}</span> }.into_any()
                                }}
                            </div>
                            <span class="wizard-step-label">{step.label}</span>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}
