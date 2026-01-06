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
            Self::Sm => "spinner-sm",
            Self::Md => "spinner-md",
            Self::Lg => "spinner-lg",
        }
    }
}

/// Loading spinner
#[component]
pub fn Spinner(
    #[prop(optional)] size: SpinnerSize,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let full_class = format!("spinner {} {}", size.class(), class);

    view! {
        <div class=full_class role="status" aria-live="polite"></div>
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
