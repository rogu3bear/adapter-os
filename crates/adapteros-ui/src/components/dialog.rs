//! Dialog/Modal component

use leptos::prelude::*;

/// Dialog component
///
/// This is a simple dialog/modal that shows content when `open` is true.
/// The dialog content is always rendered (for simplicity) but hidden with CSS.
#[component]
pub fn Dialog(
    #[prop(into)] open: RwSignal<bool>,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] description: String,
    children: Children,
) -> impl IntoView {
    let close = move |_| open.set(false);
    let has_title = !title.is_empty();
    let has_description = !description.is_empty();

    view! {
        // Backdrop - conditionally visible
        <div
            class=move || {
                if open.get() {
                    "fixed inset-0 z-50 bg-black/80"
                } else {
                    "hidden"
                }
            }
            on:click=close
        />

        // Dialog content - conditionally visible
        <div
            class=move || {
                if open.get() {
                    "fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg duration-200 sm:rounded-lg"
                } else {
                    "hidden"
                }
            }
        >
            // Close button
            <button
                class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                on:click=close
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="24"
                    height="24"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="h-4 w-4"
                >
                    <path d="M18 6 6 18"/>
                    <path d="m6 6 12 12"/>
                </svg>
                <span class="sr-only">"Close"</span>
            </button>

            // Header
            {(has_title || has_description).then(|| {
                view! {
                    <div class="flex flex-col space-y-1.5 text-center sm:text-left">
                        {has_title.then(|| view! {
                            <h2 class="text-lg font-semibold leading-none tracking-tight">{title.clone()}</h2>
                        })}
                        {has_description.then(|| view! {
                            <p class="text-sm text-muted-foreground">{description.clone()}</p>
                        })}
                    </div>
                }
            })}

            // Content
            <div class="py-2">
                {children()}
            </div>
        </div>
    }
}
