//! Danger zone components for destructive actions.

use leptos::prelude::*;

/// Warning banner with amber styling.
#[component]
pub fn WarningBanner(children: Children) -> impl IntoView {
    view! {
        <div class="banner banner-warning">{children()}</div>
    }
}

/// Info banner with blue styling.
#[component]
pub fn InfoBanner(children: Children) -> impl IntoView {
    view! {
        <div class="banner banner-info">{children()}</div>
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
