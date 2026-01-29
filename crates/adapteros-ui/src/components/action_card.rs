//! ActionCard component
//!
//! Clickable card links for navigation actions, used in dashboards and tab navigation.

use leptos::prelude::*;

/// ActionCard variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ActionCardVariant {
    /// Default style - bordered with accent hover (dashboard quick actions)
    #[default]
    Default,
    /// Subtle style - muted hover (tab navigation)
    Subtle,
}

impl ActionCardVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Default => "action-card-default",
            Self::Subtle => "action-card-subtle",
        }
    }
}

/// ActionCard component for navigation action cards
///
/// Used for dashboard quick actions and tab navigation links.
#[component]
pub fn ActionCard(
    /// Target URL
    #[prop(into)]
    href: String,
    /// Card title
    #[prop(into)]
    title: String,
    /// Card description
    #[prop(into)]
    description: String,
    /// Optional icon (emoji or text)
    #[prop(optional, into)]
    icon: Option<String>,
    /// Card variant
    #[prop(optional)]
    variant: ActionCardVariant,
    /// Whether to center content
    #[prop(optional)]
    centered: bool,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let variant_class = variant.class();
    let center_class = if centered { " text-center" } else { "" };
    let full_class = if class.is_empty() {
        format!("action-card {variant_class}{center_class}")
    } else {
        format!("action-card {variant_class}{center_class} {class}")
    };

    let title_class = "font-medium";

    let desc_class = if centered {
        "text-xs text-muted-foreground"
    } else {
        "text-sm text-muted-foreground"
    };

    view! {
        <a href=href class=full_class>
            {icon.map(|i| view! {
                <div class="text-2xl mb-1">{i}</div>
            })}
            <div class=title_class>{title}</div>
            <div class=desc_class>{description}</div>
        </a>
    }
}
