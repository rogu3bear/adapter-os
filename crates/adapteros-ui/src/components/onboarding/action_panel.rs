use leptos::prelude::*;

/// Shared action panel used by onboarding setup steps.
#[component]
pub fn OnboardingActionPanel(#[prop(into)] title: String, children: Children) -> impl IntoView {
    view! {
        <div class="wizard-action-area">
            <h3 class="wizard-step-title">{title}</h3>
            {children()}
        </div>
    }
}
