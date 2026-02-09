//! Welcome / first-run page
//!
//! Shown when AdapterOS detects a fresh installation (no models loaded,
//! no workers registered). Guides the operator through initial setup.

use crate::components::PageScaffold;
use leptos::prelude::*;

/// Welcome page for first-run setup guidance.
#[component]
pub fn Welcome() -> impl IntoView {
    view! {
        <PageScaffold
            title="Welcome"
            subtitle="First-run setup"
        >
            <div class="welcome-container">
                <div class="welcome-card">
                    <h2 class="welcome-title">"Welcome to AdapterOS"</h2>
                    <p class="welcome-subtitle">
                        "Let\u{2019}s get your system ready for inference."
                    </p>

                    // B2: Setup checklist will be rendered here

                    <div class="welcome-skip">
                        <a href="/" class="welcome-skip-link">
                            "Go to Dashboard"
                        </a>
                    </div>
                </div>
            </div>
        </PageScaffold>
    }
}
