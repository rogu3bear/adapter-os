use leptos::prelude::*;

/// Shared onboarding card container.
#[component]
pub fn OnboardingContainer(children: Children) -> impl IntoView {
    view! {
        <div class="welcome-container">
            <div class="welcome-card">{children()}</div>
        </div>
    }
}

/// Shared onboarding header block.
#[component]
pub fn OnboardingHeader(
    #[prop(into)] title: String,
    #[prop(into)] subtitle: String,
) -> impl IntoView {
    view! {
        <div class="welcome-header">
            <h2 class="welcome-title">{title}</h2>
            <p class="welcome-subtitle">{subtitle}</p>
        </div>
    }
}
