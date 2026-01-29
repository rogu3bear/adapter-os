//! Link component
//!
//! Semantic text links with glass design system integration.

use leptos::prelude::*;

/// Link variants (Glass-Integrated design)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum LinkVariant {
    /// Primary color with underline on hover - for inline text links
    #[default]
    Default,
    /// Muted foreground, transitions to foreground on hover - for breadcrumbs/navigation
    Muted,
}

impl LinkVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Default => "link-default",
            Self::Muted => "link-muted",
        }
    }
}

/// Link component for semantic text links
///
/// Use this for navigation and inline text links. For action cards or button-styled
/// links, use the appropriate layout components instead.
#[component]
pub fn Link(
    /// Target URL
    #[prop(into)]
    href: String,
    /// Link variant
    #[prop(optional)]
    variant: LinkVariant,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// Link content
    children: Children,
) -> impl IntoView {
    let variant_class = variant.class();
    let full_class = if class.is_empty() {
        format!("link {variant_class}")
    } else {
        format!("link {variant_class} {class}")
    };

    view! {
        <a href=href class=full_class>
            {children()}
        </a>
    }
}
