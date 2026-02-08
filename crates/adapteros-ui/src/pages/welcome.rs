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

            <style>
                "
                .welcome-container {
                    display: flex;
                    justify-content: center;
                    align-items: flex-start;
                    padding-top: 4rem;
                }
                .welcome-card {
                    max-width: 32rem;
                    width: 100%;
                    padding: 2.5rem;
                    border-radius: 1rem;
                    text-align: center;

                    /* Liquid Glass Tier 2: cards, panels */
                    backdrop-filter: blur(12px);
                    -webkit-backdrop-filter: blur(12px);
                    background: hsla(var(--card-hsl, 0 0% 100%), 0.78);
                    border: 1px solid hsla(0, 0%, 100%, 0.30);
                }
                .welcome-title {
                    font-size: 1.5rem;
                    font-weight: 700;
                    color: var(--foreground);
                    margin: 0 0 0.5rem 0;
                }
                .welcome-subtitle {
                    font-size: 0.95rem;
                    color: var(--muted-foreground);
                    margin: 0 0 2rem 0;
                }
                .welcome-skip {
                    margin-top: 2rem;
                    padding-top: 1.5rem;
                    border-top: 1px solid hsla(0, 0%, 100%, 0.15);
                }
                .welcome-skip-link {
                    font-size: 0.8125rem;
                    color: var(--muted-foreground);
                    text-decoration: none;
                    transition: color 0.15s ease;
                }
                .welcome-skip-link:hover {
                    color: var(--foreground);
                    text-decoration: underline;
                }
                "
            </style>
        </PageScaffold>
    }
}
