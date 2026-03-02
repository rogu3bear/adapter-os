//! Inline error and warning banners for form/page-level feedback.
//!
//! Shared CSS: `.inline-error-banner`, `.inline-warning-banner`.

use crate::components::IconX;
use leptos::prelude::*;

/// Inline error banner for displaying error messages (e.g. form validation, action failures).
#[component]
pub fn InlineErrorBanner(
    /// Error message to display
    #[prop(into)]
    message: String,
    /// Optional dismiss callback; when provided, shows an X button
    #[prop(optional)]
    on_dismiss: Option<Callback<()>>,
    /// Optional data-testid for testing
    #[prop(optional)]
    data_testid: Option<String>,
) -> impl IntoView {
    let data_testid = data_testid.filter(|v| !v.is_empty());

    view! {
        <div
            class="inline-error-banner"
            data-testid=move || data_testid.clone()
        >
            <div class="flex items-center justify-between gap-2">
                <p class="font-medium flex-1 min-w-0">{message}</p>
                {on_dismiss.map(|cb| view! {
                    <button
                        type="button"
                        class="shrink-0 p-1 rounded hover:bg-destructive/20 transition-colors"
                        aria-label="Dismiss"
                        on:click=move |_| cb.run(())
                    >
                        <IconX/>
                    </button>
                })}
            </div>
        </div>
    }
}

/// Inline warning banner for non-blocking warnings (e.g. backend status unknown).
#[component]
pub fn InlineWarningBanner(
    /// Optional title (e.g. "Backend Status Unknown")
    #[prop(optional)]
    title: Option<String>,
    /// Warning message
    #[prop(into)]
    message: String,
    /// Optional children for additional content (e.g. details/summary)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="inline-warning-banner">
            <div class="flex items-start gap-3">
                <div class="inline-warning-banner__icon">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
                        <path d="M12 9v4"/>
                        <path d="M12 17h.01"/>
                    </svg>
                </div>
                <div class="flex-1 space-y-2 min-w-0">
                    {title.map(|t| view! {
                        <p class="inline-warning-banner__title">{t}</p>
                    })}
                    <p class="text-sm text-muted-foreground">{message}</p>
                    {children.map(|c| c())}
                </div>
            </div>
        </div>
    }
}
