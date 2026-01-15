//! Tab navigation components for adapterOS UI.
//!
//! Provides reusable tab navigation patterns for settings, admin, and other pages.
//!
//! # Usage
//!
//! ## With String tabs:
//! ```rust
//! let active_tab = RwSignal::new("profile".to_string());
//!
//! view! {
//!     <TabNav
//!         tabs=vec![
//!             ("profile", "Profile"),
//!             ("api", "API Keys"),
//!             ("preferences", "Preferences"),
//!         ]
//!         active=active_tab
//!     />
//!     <Show when=move || active_tab.get() == "profile">
//!         <ProfileSection />
//!     </Show>
//! }
//! ```
//!
//! ## With enum tabs:
//! ```rust
//! #[derive(Clone, Copy, PartialEq)]
//! enum SettingsTab { Profile, Api, Preferences }
//!
//! let active = RwSignal::new(SettingsTab::Profile);
//!
//! view! {
//!     <TabNavEnum
//!         tabs=vec![
//!             (SettingsTab::Profile, "Profile"),
//!             (SettingsTab::Api, "API Keys"),
//!             (SettingsTab::Preferences, "Preferences"),
//!         ]
//!         active=active
//!     />
//! }
//! ```

use leptos::prelude::*;

// =============================================================================
// String-based Tab Navigation
// =============================================================================

/// Tab navigation with string-based tab identifiers.
///
/// Best for simple cases where tabs are known at compile time.
#[component]
pub fn TabNav(
    /// List of (tab_id, label) pairs
    tabs: Vec<(&'static str, &'static str)>,
    /// Signal holding the currently active tab ID
    active: RwSignal<String>,
    /// Optional aria-label for accessibility
    #[prop(optional, into)]
    aria_label: Option<String>,
) -> impl IntoView {
    let aria = aria_label.unwrap_or_else(|| "Tab navigation".to_string());

    view! {
        <nav role="tablist" aria-label=aria class="tab-nav">
            {tabs
                .into_iter()
                .map(|(id, label)| {
                    let tab_id = id.to_string();
                    let tab_id_clone = tab_id.clone();
                    let tab_id_for_aria = tab_id.clone();
                    let tab_id_for_class = tab_id.clone();

                    view! {
                        <button
                            role="tab"
                            type="button"
                            aria-selected=move || (active.get() == tab_id_for_aria).to_string()
                            class=move || {
                                if active.get() == tab_id_for_class {
                                    "tab-button tab-button-active"
                                } else {
                                    "tab-button"
                                }
                            }
                            on:click=move |_| active.set(tab_id_clone.clone())
                        >
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

/// Individual tab button (for custom layouts)
#[component]
pub fn TabButton(
    /// Tab identifier
    #[prop(into)]
    tab: String,
    /// Display label
    #[prop(into)]
    label: String,
    /// Signal holding active tab
    active: RwSignal<String>,
) -> impl IntoView {
    let tab_for_aria = tab.clone();
    let tab_for_class = tab.clone();
    let tab_for_click = tab.clone();

    view! {
        <button
            role="tab"
            type="button"
            aria-selected=move || (active.get() == tab_for_aria).to_string()
            class=move || {
                if active.get() == tab_for_class {
                    "tab-button tab-button-active"
                } else {
                    "tab-button"
                }
            }
            on:click=move |_| active.set(tab_for_click.clone())
        >
            {label}
        </button>
    }
}

// =============================================================================
// Enum-based Tab Navigation
// =============================================================================

/// Tab navigation with enum-based tab identifiers.
///
/// Provides type safety when tabs are defined as enums.
#[component]
pub fn TabNavEnum<T>(
    /// List of (tab_variant, label) pairs
    tabs: Vec<(T, &'static str)>,
    /// Signal holding the currently active tab variant
    active: RwSignal<T>,
    /// Optional aria-label for accessibility
    #[prop(optional, into)]
    aria_label: Option<String>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let aria = aria_label.unwrap_or_else(|| "Tab navigation".to_string());

    view! {
        <nav role="tablist" aria-label=aria class="tab-nav">
            {tabs
                .into_iter()
                .map(|(tab_variant, label)| {
                    let variant_for_aria = tab_variant.clone();
                    let variant_for_class = tab_variant.clone();
                    let variant_for_click = tab_variant.clone();

                    view! {
                        <button
                            role="tab"
                            type="button"
                            aria-selected=move || (active.get() == variant_for_aria).to_string()
                            class=move || {
                                if active.get() == variant_for_class {
                                    "tab-button tab-button-active"
                                } else {
                                    "tab-button"
                                }
                            }
                            on:click=move |_| active.set(variant_for_click.clone())
                        >
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

/// Tab button for enum-based tabs (for custom layouts)
#[component]
pub fn TabButtonEnum<T>(
    /// Tab variant
    tab: T,
    /// Display label
    #[prop(into)]
    label: String,
    /// Signal holding active tab
    active: RwSignal<T>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let tab_for_aria = tab.clone();
    let tab_for_class = tab.clone();
    let tab_for_click = tab.clone();

    view! {
        <button
            role="tab"
            type="button"
            aria-selected=move || (active.get() == tab_for_aria).to_string()
            class=move || {
                if active.get() == tab_for_class {
                    "tab-button tab-button-active"
                } else {
                    "tab-button"
                }
            }
            on:click=move |_| active.set(tab_for_click.clone())
        >
            {label}
        </button>
    }
}

// =============================================================================
// Tab Panel (content container)
// =============================================================================

/// Container for tab content with proper ARIA attributes
#[component]
pub fn TabPanel(
    /// Tab identifier this panel belongs to
    #[prop(into)]
    tab: String,
    /// Currently active tab
    active: RwSignal<String>,
    /// Panel content
    children: Children,
) -> impl IntoView {
    let tab_for_hidden = tab.clone();
    let tab_for_class = tab.clone();

    view! {
        <div
            role="tabpanel"
            aria-hidden=move || (active.get() != tab_for_hidden).to_string()
            class=move || if active.get() == tab_for_class { "block" } else { "hidden" }
        >
            {children()}
        </div>
    }
}

/// Tab panel for enum-based tabs
#[component]
pub fn TabPanelEnum<T>(
    /// Tab variant this panel belongs to
    tab: T,
    /// Currently active tab
    active: RwSignal<T>,
    /// Panel content
    children: Children,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let tab_for_hidden = tab.clone();
    let tab_for_class = tab.clone();

    view! {
        <div
            role="tabpanel"
            aria-hidden=move || (active.get() != tab_for_hidden).to_string()
            class=move || if active.get() == tab_for_class { "block" } else { "hidden" }
        >
            {children()}
        </div>
    }
}
