//! Status section components
//!
//! Collapsible sections for grouping status items.

use leptos::prelude::*;

/// Collapsible status section component
///
/// Groups related status items with a collapsible header and badge count.
#[component]
pub fn StatusSection(
    /// Section title
    #[prop(into)]
    title: String,
    /// Badge count to show (e.g., number of items, warnings, etc.)
    /// Use usize::MAX to hide the badge
    #[prop(optional, default = usize::MAX)]
    badge_count: usize,
    /// Badge variant for styling
    #[prop(optional)]
    badge_variant: StatusSectionBadgeVariant,
    /// Whether section is initially expanded
    #[prop(optional)]
    initially_expanded: bool,
    /// Section content (status items)
    children: Children,
) -> impl IntoView {
    let (expanded, set_expanded) = signal(initially_expanded);
    let show_badge = badge_count < usize::MAX;

    view! {
        <div class="status-section">
            // Header (clickable to toggle)
            <button
                class="status-section-header"
                on:click=move |_| set_expanded.update(|e| *e = !*e)
                aria-expanded=move || expanded.get().to_string()
            >
                // Expand/collapse chevron
                <svg
                    class=move || format!(
                        "status-section-chevron {}",
                        if expanded.get() { "status-section-chevron-expanded" } else { "" }
                    )
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                </svg>

                // Title
                <span class="status-section-title">{title}</span>

                // Optional badge
                {if show_badge {
                    let badge_class = format!("status-section-badge {}", badge_variant.class());
                    view! {
                        <span class=badge_class>
                            {badge_count.to_string()}
                        </span>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }}
            </button>

            // Content (conditionally rendered)
            <div
                class=move || {
                    if expanded.get() {
                        "status-section-content status-section-content-expanded"
                    } else {
                        "status-section-content status-section-content-collapsed"
                    }
                }
            >
                {children()}
            </div>
        </div>
    }
}

/// Badge variant for section badges
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusSectionBadgeVariant {
    /// Default/neutral badge
    #[default]
    Default,
    /// Success/green badge
    Success,
    /// Warning/yellow badge
    Warning,
    /// Error/red badge
    Error,
    /// Info/blue badge
    Info,
}

impl StatusSectionBadgeVariant {
    /// Get CSS class for this badge variant
    pub fn class(&self) -> &'static str {
        match self {
            Self::Default => "status-section-badge-default",
            Self::Success => "status-section-badge-success",
            Self::Warning => "status-section-badge-warning",
            Self::Error => "status-section-badge-error",
            Self::Info => "status-section-badge-info",
        }
    }
}

/// Status divider for visual separation between sections
#[component]
pub fn StatusDivider() -> impl IntoView {
    view! {
        <div class="status-divider"></div>
    }
}

/// Status section header without collapse functionality
/// For use as a simple label above a group of items
#[component]
pub fn StatusSectionLabel(
    /// Section title
    #[prop(into)]
    title: String,
) -> impl IntoView {
    view! {
        <div class="status-section-label">
            {title}
        </div>
    }
}
