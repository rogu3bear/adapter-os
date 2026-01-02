//! 404 Not Found page

use leptos::prelude::*;

/// Not found page
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="flex min-h-screen flex-col items-center justify-center">
            <h1 class="text-6xl font-bold">"404"</h1>
            <p class="mt-4 text-xl text-muted-foreground">"Page not found"</p>
            <a
                href="/"
                class="mt-8 inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
            >
                "Go Home"
            </a>
        </div>
    }
}
