//! Danger zone components for destructive actions.

use leptos::prelude::*;

/// Banner variants for different alert levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BannerVariant {
    /// Warning level - amber/yellow
    Warning,
    /// Info level - blue
    Info,
    /// Success level - green
    Success,
    /// Error level - red
    Error,
}

impl BannerVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Warning => "banner-warning",
            Self::Info => "banner-info",
            Self::Success => "banner-success",
            Self::Error => "banner-error",
        }
    }
}

/// Generic alert banner for displaying feedback/status.
#[component]
pub fn AlertBanner(
    #[prop(into)] title: String,
    #[prop(into)] message: String,
    #[prop(default = BannerVariant::Info)] variant: BannerVariant,
) -> impl IntoView {
    view! {
        <div class=format!("banner {}", variant.class())>
            <strong>{title}</strong>
            <span>{message}</span>
        </div>
    }
}

/// Container for destructive actions.
#[component]
pub fn DangerZone(children: Children) -> impl IntoView {
    view! {
        <div class="card border border-destructive bg-destructive/10">
            <div class="card-header">
                <h3 class="card-title">"Danger Zone"</h3>
                <p class="card-description">
                    "Actions in this section are irreversible. Proceed with care."
                </p>
            </div>
            <div class="card-content card-content--full">
                <div class="space-y-3">
                    {children()}
                </div>
            </div>
        </div>
    }
}

/// Individual danger zone item.
#[component]
pub fn DangerZoneItem(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="danger-zone-item">
            <div class="danger-zone-item-content">
                <h4 class="danger-zone-item-title">{title}</h4>
                <p class="danger-zone-item-description">{description}</p>
            </div>
            <div class="danger-zone-item-actions">
                {children()}
            </div>
        </div>
    }
}
