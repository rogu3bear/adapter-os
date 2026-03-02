//! Sidebar - Collapsible workflow navigation
//!
//! Persistent left sidebar with workflow groups from nav_registry.
//! Supports expanded (label + icon) and collapsed (icon-only rail) modes.
//! Replaces the Start Menu popup as the primary navigation surface.
//!
//! Glass tier: 1 (nav surface - 9.6px blur, 70% alpha)
//! Keyboard: Alt+1..8 shortcuts remain functional via Shell handler

use super::nav_registry::{nav_groups, NavGroup, NavItem, DASHBOARD_ITEM};
use crate::signals::settings::{update_setting, use_settings};
use crate::signals::use_ui_profile;
use adapteros_api_types::UiProfile;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Sidebar collapsed/expanded state — stored as a context so Taskbar
/// toggle and Shell layout can react.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarState {
    Expanded,
    Collapsed,
}

impl SidebarState {
    pub fn is_expanded(self) -> bool {
        self == Self::Expanded
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Expanded => Self::Collapsed,
            Self::Collapsed => Self::Expanded,
        }
    }
}

/// Provide sidebar context (call once in Shell)
pub fn provide_sidebar_context() {
    let state = RwSignal::new(SidebarState::Expanded);
    provide_context(state);
}

/// Use sidebar state from context
pub fn use_sidebar() -> RwSignal<SidebarState> {
    expect_context::<RwSignal<SidebarState>>()
}

/// Toggle the sidebar open/closed
pub fn toggle_sidebar() {
    let state = use_sidebar();
    state.update(|s| *s = s.toggle());
}

/// Collapsible workflow sidebar.
///
/// Renders the 8 workflow groups as collapsible sections with icons.
/// In collapsed mode, shows only icons with tooltips.
#[component]
pub fn SidebarNav() -> impl IntoView {
    let sidebar = use_sidebar();
    let ui_profile = use_ui_profile();
    let location = use_location();
    let groups = Signal::derive(move || ui_profile.try_get().map(nav_groups).unwrap_or_default());

    view! {
        <aside
            class=move || {
                if sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false) {
                    "sidebar sidebar--expanded"
                } else {
                    "sidebar sidebar--collapsed"
                }
            }
            aria-label="Main navigation"
            role="navigation"
        >
            // Sidebar header: toggle button
            <div class="sidebar-header">
                <button
                    class="sidebar-toggle"
                    on:click=move |_| sidebar.update(|s| *s = s.toggle())
                    title=move || if sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false) { "Collapse sidebar" } else { "Expand sidebar" }
                    aria-label=move || if sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false) { "Collapse sidebar" } else { "Expand sidebar" }
                >
                    <svg
                        class=move || format!(
                            "sidebar-toggle-icon {}",
                            if sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false) { "" } else { "sidebar-toggle-icon--flipped" }
                        )
                        width="16" height="16" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round"
                    >
                        <path d="M11 19l-7-7 7-7"/>
                        <path d="M18 19l-7-7 7-7" opacity="0.4"/>
                    </svg>
                </button>
            </div>

            // Navigation content (scrollable)
            <nav class="sidebar-nav">
                // Dashboard - pinned at top
                <SidebarItem
                    item=&DASHBOARD_ITEM
                    is_expanded=Signal::derive(move || sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false))
                    is_active=Signal::derive(move || {
                        let path = location.pathname.try_get().unwrap_or_default();
                        path == "/" || path == "/dashboard"
                    })
                />

                <div class="sidebar-divider"></div>

                // Workflow groups
                {move || {
                    groups.try_get().unwrap_or_default().into_iter().map(|group| {
                        view! {
                            <SidebarGroup
                                group=group
                                is_expanded=Signal::derive(move || sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false))
                            />
                        }
                    }).collect::<Vec<_>>()
                }}
            </nav>

            // Profile toggle footer
            <ProfileToggle
                is_expanded=Signal::derive(move || sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false))
            />
        </aside>
    }
}

/// A single sidebar navigation group (e.g., "Observe" with sub-items)
#[component]
fn SidebarGroup(group: &'static NavGroup, is_expanded: Signal<bool>) -> impl IntoView {
    let location = use_location();
    let (group_open, set_group_open) = signal(!group.collapsed_by_default);
    let items = group.items;
    let label = group.label;
    let icon_path = group.icon;
    let alt_shortcut = group.alt_shortcut;

    // Check if any item in this group is active
    let group_is_active = move || {
        let path = location.pathname.try_get().unwrap_or_default();
        items.iter().any(|item| {
            if item.route == "/" {
                path == "/" || path == "/dashboard"
            } else {
                path == item.route || path.starts_with(&format!("{}/", item.route))
            }
        })
    };

    // Single-item groups navigate directly instead of expanding
    let is_single = items.len() == 1;
    // In collapsed mode, multi-item groups should still be navigable.
    // We link the group icon to the first item and avoid rendering sub-items,
    // since items intentionally don't carry their own icons.
    let is_expanded_now = move || is_expanded.try_get().unwrap_or(false);

    view! {
        <div class="sidebar-group">
            // Group header
            {if is_single {
                // Single-item group: direct link
                let item = items[0];
                view! {
                    <a
                        href=item.route
                        class=move || format!(
                            "sidebar-group-header {}",
                            if group_is_active() { "sidebar-group-header--active" } else { "" }
                        )
                        title=move || {
                            if is_expanded.try_get().unwrap_or(false) {
                                label.to_string()
                            } else {
                                match alt_shortcut {
                                    Some(n) => format!("{} (Alt+{})", label, n),
                                    None => label.to_string(),
                                }
                            }
                        }
                    >
                        <svg class="sidebar-icon" width="18" height="18" viewBox="0 0 24 24"
                            fill="none" stroke="currentColor" stroke-width="2"
                            stroke-linecap="round" stroke-linejoin="round"
                        >
                            <path d=icon_path/>
                        </svg>
                        <Show when=move || is_expanded.try_get().unwrap_or(false)>
                            <span class="sidebar-label">{label}</span>
                            {alt_shortcut.map(|n| view! {
                                <kbd class="sidebar-kbd">{format!("Alt+{}", n)}</kbd>
                            })}
                        </Show>
                    </a>
                }.into_any()
            } else {
                let first_item = items[0];

                // Multi-item group:
                // - Expanded: button toggles open/closed and reveals sub-items
                // - Collapsed: link navigates to the first item (keeps icon-only rail usable)
                view! {
                    {move || {
                        if is_expanded_now() {
                            view! {
                                <button
                                    class=move || format!(
                                        "sidebar-group-header {}",
                                        if group_is_active() { "sidebar-group-header--active" } else { "" }
                                    )
                                    on:click=move |_| set_group_open.update(|v| *v = !*v)
                                    title=move || label.to_string()
                                    aria-expanded=move || {
                                        group_open
                                            .try_get()
                                            .unwrap_or(!group.collapsed_by_default)
                                            .to_string()
                                    }
                                >
                                    <svg class="sidebar-icon" width="18" height="18" viewBox="0 0 24 24"
                                        fill="none" stroke="currentColor" stroke-width="2"
                                        stroke-linecap="round" stroke-linejoin="round"
                                    >
                                        <path d=icon_path/>
                                    </svg>
                                    <span class="sidebar-label">{label}</span>
                                    {alt_shortcut.map(|n| view! {
                                        <kbd class="sidebar-kbd">{format!("Alt+{}", n)}</kbd>
                                    })}
                                    <svg
                                        class=move || format!(
                                            "sidebar-chevron {}",
                                            if group_open
                                                .try_get()
                                                .unwrap_or(!group.collapsed_by_default)
                                            {
                                                "sidebar-chevron--open"
                                            } else {
                                                ""
                                            }
                                        )
                                        width="14" height="14" viewBox="0 0 24 24"
                                        fill="none" stroke="currentColor" stroke-width="2"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
                                    </svg>
                                </button>
                            }
                            .into_any()
                        } else {
                            view! {
                                <a
                                    href=first_item.route
                                    class=move || format!(
                                        "sidebar-group-header {}",
                                        if group_is_active() { "sidebar-group-header--active" } else { "" }
                                    )
                                    title=move || {
                                        match alt_shortcut {
                                            Some(n) => format!("{} (Alt+{})", label, n),
                                            None => label.to_string(),
                                        }
                                    }
                                >
                                    <svg class="sidebar-icon" width="18" height="18" viewBox="0 0 24 24"
                                        fill="none" stroke="currentColor" stroke-width="2"
                                        stroke-linecap="round" stroke-linejoin="round"
                                    >
                                        <path d=icon_path/>
                                    </svg>
                                </a>
                            }
                            .into_any()
                        }
                    }}
                }
                .into_any()
            }}

            // Sub-items (only for multi-item groups, when expanded)
            <Show when=move || {
                !is_single
                    && is_expanded_now()
                    && group_open
                        .try_get()
                        .unwrap_or(!group.collapsed_by_default)
            }>
                <div class=move || {
                    "sidebar-items"
                }>
                    {items.iter().filter(|i| !i.hidden).map(|item| {
                        view! {
                            <SidebarItem
                                item=item
                                is_expanded=is_expanded
                                is_active=Signal::derive(move || {
                                    let path = location.pathname.try_get().unwrap_or_default();
                                    if item.route == "/" {
                                        path == "/" || path == "/dashboard"
                                    } else {
                                        path == item.route || path.starts_with(&format!("{}/", item.route))
                                    }
                                })
                            />
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </Show>
        </div>
    }
}

/// A single navigation item (link)
#[component]
fn SidebarItem(
    item: &'static NavItem,
    is_expanded: Signal<bool>,
    is_active: Signal<bool>,
) -> impl IntoView {
    let label = item.label;
    let route = item.route;
    let icon_path = item.icon;

    view! {
        <a
            href=route
            class=move || format!(
                "sidebar-item {}",
                if is_active.try_get().unwrap_or(false) { "sidebar-item--active" } else { "" }
            )
            title=move || {
                if !is_expanded.try_get().unwrap_or(false) { label.to_string() } else { String::new() }
            }
            aria-current=move || if is_active.try_get().unwrap_or(false) { Some("page") } else { None }
        >
            {icon_path.map(|path| view! {
                <svg class="sidebar-icon" width="16" height="16" viewBox="0 0 24 24"
                    fill="none" stroke="currentColor" stroke-width="2"
                    stroke-linecap="round" stroke-linejoin="round"
                >
                    <path d=path/>
                </svg>
            })}
            <Show when=move || is_expanded.try_get().unwrap_or(false)>
                <span class="sidebar-label">{label}</span>
            </Show>
        </a>
    }
}

/// Clickable profile indicator in sidebar footer.
/// Shows "Primary" or "Full" and toggles on click.
#[component]
fn ProfileToggle(is_expanded: Signal<bool>) -> impl IntoView {
    let ui_profile = use_ui_profile();
    let settings = use_settings();

    let toggle = move |_| {
        let current = ui_profile.get_untracked();
        let next = match current {
            UiProfile::Primary => UiProfile::Full,
            UiProfile::Full => UiProfile::Primary,
        };
        update_setting(settings, |s| {
            s.ui_profile = Some(next);
        });
    };

    let label = move || match ui_profile.try_get().unwrap_or(UiProfile::Primary) {
        UiProfile::Primary => "Primary",
        UiProfile::Full => "Full",
    };

    let title = move || match ui_profile.try_get().unwrap_or(UiProfile::Primary) {
        UiProfile::Primary => "Primary profile \u{2014} click to switch to Full",
        UiProfile::Full => "Full profile \u{2014} click to switch to Primary",
    };

    view! {
        <div class="sidebar-footer">
            <button
                class="sidebar-profile-toggle"
                on:click=toggle
                title=title
            >
                // Swap icon
                <svg class="sidebar-icon" width="16" height="16" viewBox="0 0 24 24"
                    fill="none" stroke="currentColor" stroke-width="2"
                    stroke-linecap="round" stroke-linejoin="round"
                >
                    <path d="M16 3h5v5M4 20L21 3M21 16v5h-5M15 15l6 6M4 4l5 5"/>
                </svg>
                <Show when=move || is_expanded.try_get().unwrap_or(false)>
                    <span class="sidebar-label">{label}</span>
                </Show>
            </button>
        </div>
    }
}
