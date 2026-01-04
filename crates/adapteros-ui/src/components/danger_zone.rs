//! DangerZone component for destructive actions
//!
//! Provides visual grouping and warnings for dangerous operations.
//! Follows the pattern of settings pages with a distinct "danger zone" section.

use leptos::prelude::*;

/// DangerZone container component
///
/// Groups destructive actions with clear visual warning.
/// Typically placed at the bottom of settings/detail pages.
///
/// # Example
/// ```rust
/// view! {
///     <DangerZone>
///         <DangerZoneItem
///             title="Delete Adapter"
///             description="Permanently remove this adapter and all its data."
///         >
///             <Button
///                 variant=ButtonVariant::Destructive
///                 on_click=Callback::new(move |_| show_delete_dialog.set(true))
///             >
///                 "Delete Adapter"
///             </Button>
///         </DangerZoneItem>
///     </DangerZone>
/// }
/// ```
#[component]
pub fn DangerZone(
    /// Title for the danger zone section
    #[prop(optional, into)]
    title: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// The danger zone items
    children: Children,
) -> impl IntoView {
    let zone_title = title.unwrap_or_else(|| "Danger Zone".to_string());

    view! {
        <div class=format!("mt-8 border-t border-destructive/30 pt-6 {}", class)>
            // Header with warning icon
            <div class="flex items-center gap-2 mb-4">
                <svg
                    aria-hidden="true"
                    xmlns="http://www.w3.org/2000/svg"
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="text-destructive"
                >
                    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>
                    <line x1="12" y1="9" x2="12" y2="13"/>
                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
                <h3 class="text-lg font-semibold text-destructive">{zone_title}</h3>
            </div>

            // Items container
            <div class="space-y-4 rounded-lg border border-destructive/30 bg-destructive/5 p-4">
                {children()}
            </div>
        </div>
    }
}

/// DangerZoneItem - individual destructive action within a DangerZone
#[component]
pub fn DangerZoneItem(
    /// Title of the action
    #[prop(into)]
    title: String,
    /// Description of what the action does
    #[prop(into)]
    description: String,
    /// The action button/controls
    children: Children,
) -> impl IntoView {
    view! {
        <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 py-3 border-b border-destructive/20 last:border-b-0 last:pb-0 first:pt-0">
            <div class="flex-1">
                <h4 class="font-medium text-foreground">{title}</h4>
                <p class="text-sm text-muted-foreground mt-1">{description}</p>
            </div>
            <div class="flex-shrink-0">
                {children()}
            </div>
        </div>
    }
}

/// Warning banner for inline warnings
/// Use when a warning needs to be shown but doesn't warrant a full DangerZone
#[component]
pub fn WarningBanner(
    /// The warning message
    #[prop(into)]
    message: String,
    /// Optional title
    #[prop(optional, into)]
    title: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    view! {
        <div class=format!("banner banner-warning {}", class)>
            <div class="flex gap-3">
                <svg
                    aria-hidden="true"
                    xmlns="http://www.w3.org/2000/svg"
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="text-warning shrink-0 mt-1"
                >
                    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>
                    <line x1="12" y1="9" x2="12" y2="13"/>
                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
                <div>
                    {title.map(|t| view! {
                        <h4 class="font-medium text-warning-strong mb-1">{t}</h4>
                    })}
                    <p class="text-sm text-warning-muted">{message}</p>
                </div>
            </div>
        </div>
    }
}

/// InfoBanner for informational messages
#[component]
pub fn InfoBanner(
    /// The info message
    #[prop(into)]
    message: String,
    /// Optional title
    #[prop(optional, into)]
    title: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    view! {
        <div class=format!("banner banner-info {}", class)>
            <div class="flex gap-3">
                <svg
                    aria-hidden="true"
                    xmlns="http://www.w3.org/2000/svg"
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="text-info shrink-0 mt-1"
                >
                    <circle cx="12" cy="12" r="10"/>
                    <path d="M12 16v-4"/>
                    <path d="M12 8h.01"/>
                </svg>
                <div>
                    {title.map(|t| view! {
                        <h4 class="font-medium text-info-strong mb-1">{t}</h4>
                    })}
                    <p class="text-sm text-info-muted">{message}</p>
                </div>
            </div>
        </div>
    }
}
