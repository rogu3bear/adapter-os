//! FormField component - wrapper for form inputs with validation, errors, and help text
//!
//! Provides consistent UX for form fields with:
//! - Label with optional required indicator
//! - Input slot (children)
//! - Error display (maps to field)
//! - Inline help/hints
//! - Accessibility attributes

use leptos::prelude::*;

/// FormField wrapper component
///
/// Wraps form inputs with consistent styling, labels, error display, and help text.
///
/// # Example
/// ```rust
/// view! {
///     <FormField
///         label="Email"
///         name="email"
///         required=true
///         error=move || errors.get().get("email").cloned()
///         help="We'll never share your email"
///     >
///         <Input value=email placeholder="you@example.com" />
///     </FormField>
/// }
/// ```
#[component]
pub fn FormField(
    /// The label text for the field
    #[prop(into)]
    label: String,
    /// The field name (used for error mapping and accessibility)
    #[prop(into)]
    name: String,
    /// Whether the field is required
    #[prop(optional)]
    required: bool,
    /// Error message (reactive - returns Option<String>)
    #[prop(optional, into)]
    error: Option<Signal<Option<String>>>,
    /// Help text shown below the input
    #[prop(optional, into)]
    help: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// The input element(s) to wrap
    children: Children,
) -> impl IntoView {
    let field_id = format!("field-{}", name);
    let error_id = format!("error-{}", name);
    let help_id = format!("help-{}", name);

    // Compute if there's an error
    let has_error = move || error.map(|e| e.get().is_some()).unwrap_or(false);

    // Get the error message
    let error_message = move || error.and_then(|e| e.get());

    view! {
        <div class=format!("space-y-1.5 {}", class)>
            // Label with required indicator
            <label
                for=field_id.clone()
                class="label"
            >
                {label}
                {required.then(|| view! {
                    <span class="text-destructive ml-0.5">"*"</span>
                })}
            </label>

            // Input slot
            <div
                class=move || {
                    if has_error() {
                        "form-field-error"
                    } else {
                        ""
                    }
                }
            >
                {children()}
            </div>

            // Error message
            {move || {
                error_message().map(|msg| view! {
                    <p
                        id=error_id.clone()
                        class="text-sm text-destructive flex items-center gap-1"
                        role="alert"
                    >
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            class="flex-shrink-0"
                        >
                            <circle cx="12" cy="12" r="10"/>
                            <line x1="12" y1="8" x2="12" y2="12"/>
                            <line x1="12" y1="16" x2="12.01" y2="16"/>
                        </svg>
                        {msg}
                    </p>
                })
            }}

            // Help text
            {help.map(|h| view! {
                <p
                    id=help_id
                    class="text-sm text-muted-foreground"
                >
                    {h}
                </p>
            })}
        </div>
    }
}

/// HelpTooltip component - inline help icon with tooltip
#[component]
pub fn HelpTooltip(
    /// The tooltip content
    #[prop(into)]
    text: String,
) -> impl IntoView {
    let show = RwSignal::new(false);

    view! {
        <span
            class="inline-flex items-center cursor-help relative"
            on:mouseenter=move |_| show.set(true)
            on:mouseleave=move |_| show.set(false)
            on:focus=move |_| show.set(true)
            on:blur=move |_| show.set(false)
            tabindex="0"
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="text-muted-foreground"
            >
                <circle cx="12" cy="12" r="10"/>
                <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
                <line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
            <div
                class=move || {
                    if show.get() {
                        "absolute z-50 bottom-full left-1/2 -translate-x-1/2 mb-2 px-3 py-2 text-sm bg-popover text-popover-foreground rounded-md shadow-md border max-w-xs whitespace-normal"
                    } else {
                        "hidden"
                    }
                }
                role="tooltip"
            >
                {text.clone()}
                // Arrow
                <div class="absolute top-full left-1/2 -translate-x-1/2 -mt-px border-4 border-transparent border-t-popover" />
            </div>
        </span>
    }
}

/// LabelWithHelp - label text with an inline help tooltip
#[component]
pub fn LabelWithHelp(
    /// The label text
    #[prop(into)]
    label: String,
    /// The help tooltip content
    #[prop(into)]
    help: String,
    /// Whether the field is required
    #[prop(optional)]
    required: bool,
) -> impl IntoView {
    view! {
        <span class="inline-flex items-center gap-1.5">
            {label}
            {required.then(|| view! {
                <span class="text-destructive">"*"</span>
            })}
            <HelpTooltip text=help />
        </span>
    }
}
