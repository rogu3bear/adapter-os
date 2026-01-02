//! Status indicator components

use leptos::prelude::*;

/// Badge variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Success,
    Warning,
    Destructive,
    Outline,
}

impl BadgeVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground",
            Self::Secondary => "bg-secondary text-secondary-foreground",
            Self::Success => "bg-green-500 text-white",
            Self::Warning => "bg-yellow-500 text-white",
            Self::Destructive => "bg-destructive text-destructive-foreground",
            Self::Outline => "border border-input bg-background",
        }
    }
}

/// Badge component
#[component]
pub fn Badge(
    #[prop(optional)] variant: BadgeVariant,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let base_class = "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors";
    let full_class = format!("{} {} {}", base_class, variant.class(), class);

    view! {
        <span class=full_class>
            {children()}
        </span>
    }
}

/// Status indicator dot
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum StatusColor {
    #[default]
    Gray,
    Green,
    Yellow,
    Red,
    Blue,
}

impl StatusColor {
    fn dot_class(&self) -> &'static str {
        match self {
            Self::Gray => "bg-gray-500",
            Self::Green => "bg-green-500",
            Self::Yellow => "bg-yellow-500",
            Self::Red => "bg-red-500",
            Self::Blue => "bg-blue-500",
        }
    }

    fn pulse_class(&self) -> &'static str {
        match self {
            Self::Gray => "bg-gray-400",
            Self::Green => "bg-green-400",
            Self::Yellow => "bg-yellow-400",
            Self::Red => "bg-red-400",
            Self::Blue => "bg-blue-400",
        }
    }
}

/// Status indicator with pulsing dot
#[component]
pub fn StatusIndicator(
    #[prop(optional)] color: StatusColor,
    #[prop(optional)] pulsing: bool,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    view! {
        <div class="flex items-center gap-2">
            <span class="relative flex h-3 w-3">
                {move || {
                    if pulsing {
                        view! {
                            <span class=format!("animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 {}", color.pulse_class())></span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
                <span class=format!("relative inline-flex rounded-full h-3 w-3 {}", color.dot_class())></span>
            </span>
            {label.map(|l| view! {
                <span class="text-sm text-muted-foreground">{l}</span>
            })}
        </div>
    }
}
