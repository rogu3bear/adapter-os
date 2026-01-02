//! Loading spinner component

use leptos::prelude::*;

/// Spinner sizes
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SpinnerSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl SpinnerSize {
    fn class(&self) -> &'static str {
        match self {
            Self::Sm => "h-4 w-4",
            Self::Md => "h-8 w-8",
            Self::Lg => "h-12 w-12",
        }
    }
}

/// Loading spinner
#[component]
pub fn Spinner(
    #[prop(optional)] size: SpinnerSize,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let full_class = format!("animate-spin {} {}", size.class(), class);

    view! {
        <svg
            class=full_class
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
        >
            <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
            ></circle>
            <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            ></path>
        </svg>
    }
}

/// Full page loading
#[component]
pub fn PageLoader() -> impl IntoView {
    view! {
        <div class="flex h-screen w-full items-center justify-center">
            <div class="flex flex-col items-center gap-4">
                <Spinner size=SpinnerSize::Lg/>
                <p class="text-muted-foreground">"Loading..."</p>
            </div>
        </div>
    }
}
