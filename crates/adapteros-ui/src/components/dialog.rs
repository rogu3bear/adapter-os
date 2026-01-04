//! Dialog/Modal component
//!
//! Uses semantic CSS classes from components.css.
//! No Tailwind arbitrary value syntax.

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
        // Backdrop - uses .dialog-overlay CSS class
        <div
            class=move || {
                if open.get() {
                    "dialog-overlay"
                } else {
                    "hidden"
                }
            }
            on:click=close
        />

        // Dialog content - uses .dialog-content CSS class
        <div
            class=move || {
                if open.get() {
                    "dialog-content"
                } else {
                    "hidden"
                }
            }
        >
            // Close button
            <button
                class="dialog-close"
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
                    <div class="dialog-header">
                        {has_title.then(|| view! {
                            <h2 class="dialog-title">{title.clone()}</h2>
                        })}
                        {has_description.then(|| view! {
                            <p class="dialog-description">{description.clone()}</p>
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
