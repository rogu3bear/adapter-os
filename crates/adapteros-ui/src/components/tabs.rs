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
// Generic Tab Navigation
// =============================================================================

/// Tab navigation with generic tab identifiers.
#[component]
pub fn TabNav<T>(
    /// List of (tab_id, label) pairs
    tabs: Vec<(T, &'static str)>,
    /// Signal holding the currently active tab ID
    active: RwSignal<T>,
    /// Optional aria-label for accessibility
    #[prop(optional, into)]
    aria_label: Option<String>,
) -> impl IntoView
where
    T: Clone + PartialEq + ToString + Send + Sync + 'static,
{
    let aria = aria_label.unwrap_or_else(|| "Tab navigation".to_string());

    view! {
        <nav role="tablist" aria-label=aria class="tab-nav">
            {tabs
                .into_iter()
                .enumerate()
                .map(|(index, (id, label))| {
                    let tab_id = id.clone();
                    let tab_id_for_aria = id.clone();
                    let tab_id_for_class = id.clone();
                    // Generate unique IDs for ARIA relationships using tab's string representation
                    let id_str = id.to_string();
                    let button_id = if id_str.is_empty() {
                        format!("tab-{}", index)
                    } else {
                        format!("tab-{}", id_str)
                    };
                    let panel_id = if id_str.is_empty() {
                        format!("panel-{}", index)
                    } else {
                        format!("panel-{}", id_str)
                    };

                    view! {
                        <button
                            role="tab"
                            type="button"
                            id=button_id
                            aria-controls=panel_id
                            aria-selected=move || (active.get() == tab_id_for_aria).to_string()
                            class=move || {
                                if active.get() == tab_id_for_class {
                                    "tab-button tab-button-active"
                                } else {
                                    "tab-button"
                                }
                            }
                            on:click=move |_| active.set(tab_id.clone())
                        >
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

/// Generic tab button (for custom layouts)
#[component]
pub fn TabButton<T>(
    /// Tab identifier
    tab: T,
    /// Display label
    #[prop(into)]
    label: String,
    /// Signal holding active tab
    active: RwSignal<T>,
    /// Optional string ID for ARIA attributes (required for accessibility)
    #[prop(optional, into)]
    tab_id: Option<String>,
    /// Optional additional classes
    #[prop(optional, into)]
    class: String,
    /// Optional badge count to display next to label
    #[prop(optional, into)]
    badge_count: Option<Signal<usize>>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let tab_for_aria = tab.clone();
    let tab_for_class = tab.clone();
    let tab_for_click = tab.clone();
    // Generate unique IDs for ARIA relationships
    let button_id = tab_id.as_ref().map(|id| format!("tab-{}", id));
    let panel_id = tab_id.as_ref().map(|id| format!("panel-{}", id));

    view! {
        <button
            role="tab"
            type="button"
            id=button_id
            aria-controls=panel_id
            aria-selected=move || (active.get() == tab_for_aria).to_string()
            class=move || {
                let is_active = active.get() == tab_for_class;
                let base = if is_active {
                    "tab-button tab-button-active"
                } else {
                    "tab-button"
                };

                if class.is_empty() {
                    base.to_string()
                } else {
                    format!("{} {}", base, class)
                }
            }
            on:click=move |_| active.set(tab_for_click.clone())
        >
            {label}
            {move || {
                badge_count.and_then(|count| {
                    let c = count.get();
                    if c > 0 {
                        let aria_label = if c == 1 {
                            "1 item".to_string()
                        } else {
                            format!("{} items", c)
                        };
                        Some(view! {
                            <span class="tab-badge" aria-label=aria_label>
                                {c.to_string()}
                            </span>
                        })
                    } else {
                        None
                    }
                })
            }}
        </button>
    }
}

// =============================================================================
// Tab Panel (content container)
// =============================================================================

/// Container for tab content with proper ARIA attributes.
/// Works with any type T that implements PartialEq.
#[component]
pub fn TabPanel<T>(
    /// Tab identifier this panel belongs to
    tab: T,
    /// Currently active tab
    active: RwSignal<T>,
    /// Panel content
    children: Children,
    /// Optional string ID for ARIA attributes (required for accessibility)
    #[prop(optional, into)]
    tab_id: Option<String>,
    /// Optional additional classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let tab_for_hidden = tab.clone();
    let tab_for_class = tab.clone();
    // Generate unique IDs for ARIA relationships
    let panel_id = tab_id.as_ref().map(|id| format!("panel-{}", id));
    let labelledby_id = tab_id.as_ref().map(|id| format!("tab-{}", id));

    view! {
        <div
            role="tabpanel"
            id=panel_id
            aria-labelledby=labelledby_id
            aria-hidden=move || (active.get() != tab_for_hidden).to_string()
            class=move || {
                let display = if active.get() == tab_for_class { "block" } else { "hidden" };
                if class.is_empty() {
                    display.to_string()
                } else {
                    format!("{} {}", display, class)
                }
            }
        >
            {children()}
        </div>
    }
}
