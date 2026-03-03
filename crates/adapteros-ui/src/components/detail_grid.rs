//! Detail Grid component
//!
//! Provides a standardized layout for presenting key-value read-only metadata.

use crate::components::CopyableId;
use leptos::prelude::*;

/// A standardized grid container for detail items.
#[component]
pub fn DetailGrid(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let full_class = format!("grid grid-cols-2 gap-4 {}", class);
    view! {
        <div class=full_class>
            {children()}
        </div>
    }
}

/// A standardized key-value item within a detail grid.
#[component]
pub fn DetailItem(
    /// The label for the item
    #[prop(into)]
    label: String,
    /// The value to display
    #[prop(into)]
    value: String,
    /// Whether the value is an ID that should use the CopyableId component
    #[prop(optional)]
    is_id: bool,
    /// Whether the value should be displayed in a monospace font
    #[prop(optional)]
    mono: bool,
) -> impl IntoView {
    view! {
        <div>
            <p class="text-xs text-muted-foreground">{label.clone()}</p>
            {if is_id {
                view! { <CopyableId id=value.clone() truncate=24 /> }.into_any()
            } else {
                let val_class = if mono { "text-sm font-mono truncate" } else { "text-sm font-medium truncate" };
                view! { <p class=val_class title=value.clone()>{value.clone()}</p> }.into_any()
            }}
        </div>
    }
}

/// A horizontal key-value row (used in places like profile settings).
#[component]
pub fn DetailGridRow(
    #[prop(into)] label: String,
    #[prop(optional)] mono: bool,
    #[prop(optional)] items_start: bool,
    children: Children,
) -> impl IntoView {
    let align_class = if items_start {
        "items-start"
    } else {
        "items-center"
    };

    view! {
        <div class=format!("flex justify-between {}", align_class)>
            <p class="text-sm font-medium text-muted-foreground w-1/3">{label}</p>
            <div class=format!("w-2/3 text-right {}", if mono { "font-mono" } else { "" })>
                {children()}
            </div>
        </div>
    }
}
