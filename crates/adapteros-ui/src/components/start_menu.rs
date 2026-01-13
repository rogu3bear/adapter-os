//! Start Menu component
//!
//! A compact launcher with grouped modules that opens from the "Start" button
//! in the bottom taskbar. Features collapsible sections, disabled items with
//! explanations, and keyboard navigation.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

/// Represents the enabled/disabled state of a menu item
#[derive(Debug, Clone, PartialEq)]
pub enum MenuItemState {
    /// Item is enabled and can be clicked
    Enabled,
    /// Item is disabled with a reason shown to the user
    Disabled(&'static str),
}

/// A single menu item in the start menu
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// Display label
    pub label: &'static str,
    /// SVG icon path (can be replaced with proper icons later)
    pub icon: &'static str,
    /// Route path to navigate to
    pub path: &'static str,
    /// Whether the item is enabled or disabled (with reason)
    pub state: MenuItemState,
}

impl MenuItem {
    pub fn enabled(label: &'static str, icon: &'static str, path: &'static str) -> Self {
        Self {
            label,
            icon,
            path,
            state: MenuItemState::Enabled,
        }
    }

    pub fn disabled(
        label: &'static str,
        icon: &'static str,
        path: &'static str,
        reason: &'static str,
    ) -> Self {
        Self {
            label,
            icon,
            path,
            state: MenuItemState::Disabled(reason),
        }
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self.state, MenuItemState::Enabled)
    }
}

/// A group of menu items with a collapsible header
#[derive(Debug, Clone)]
pub struct MenuGroup {
    /// Group name
    pub name: &'static str,
    /// Items in this group
    pub items: Vec<MenuItem>,
}

/// Get the default menu groups
fn get_menu_groups() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            name: "Core",
            items: vec![
                MenuItem::enabled("Dashboard", "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6", "/dashboard"),
                MenuItem::enabled("Adapters", "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10", "/adapters"),
                MenuItem::enabled("Chat", "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z", "/chat"),
                MenuItem::enabled("System", "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z", "/system"),
                MenuItem::enabled("Settings", "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4", "/settings"),
            ],
        },
        MenuGroup {
            name: "Model Ops",
            items: vec![
                MenuItem::enabled("Models", "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z", "/models"),
                MenuItem::enabled("Stacks", "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10", "/stacks"),
                MenuItem::enabled("Policies", "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z", "/policies"),
                MenuItem::enabled("Training", "M4.26 10.147a60.436 60.436 0 00-.491 6.347A48.627 48.627 0 0112 20.904a48.627 48.627 0 018.232-4.41 60.46 60.46 0 00-.491-6.347m-15.482 0a50.57 50.57 0 00-2.658-.813A59.905 59.905 0 0112 3.493a59.902 59.902 0 0110.399 5.84c-.896.248-1.783.52-2.658.814m-15.482 0A50.697 50.697 0 0112 13.489a50.702 50.702 0 017.74-3.342M6.75 15a.75.75 0 100-1.5.75.75 0 000 1.5zm0 0v-3.675A55.378 55.378 0 0112 8.443m-7.007 11.55A5.981 5.981 0 006.75 15.75v-1.5", "/training"),
                MenuItem::enabled("Datasets", "M3 10h18M3 14h18m-9-4v8m-7 0h14a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z", "/datasets"),
            ],
        },
        MenuGroup {
            name: "Operations",
            items: vec![
                MenuItem::enabled("Monitoring", "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z", "/monitoring"),
                MenuItem::enabled("Workers", "M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7m0 0a3 3 0 01-3 3m0 3h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008zm-3 6h.008v.008h-.008v-.008zm0-6h.008v.008h-.008v-.008z", "/workers"),
                MenuItem::enabled("Routing", "M13 10V3L4 14h7v7l9-11h-7z", "/routing"),
            ],
        },
        MenuGroup {
            name: "Admin",
            items: vec![
                MenuItem::enabled("Admin", "M5.121 17.804A13.937 13.937 0 0112 16c2.5 0 4.847.655 6.879 1.804M15 10a3 3 0 11-6 0 3 3 0 016 0zm6 2a9 9 0 11-18 0 9 9 0 0118 0z", "/admin"),
                MenuItem::enabled("Audit Log", "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z", "/audit"),
                MenuItem::enabled("Error Monitor", "M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z", "/errors"),
                MenuItem::disabled("Plugins", "M17 14v6m-3-3h6M6 10h2a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v2a2 2 0 002 2zm10 0h2a2 2 0 002-2V6a2 2 0 00-2-2h-2a2 2 0 00-2 2v2a2 2 0 002 2zM6 20h2a2 2 0 002-2v-2a2 2 0 00-2-2H6a2 2 0 00-2 2v2a2 2 0 002 2z", "/plugins", "coming soon"),
                MenuItem::disabled("Federation", "M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z", "/federation", "coming soon"),
            ],
        },
    ]
}

/// Individual menu item component
#[component]
fn StartMenuItem(
    item: MenuItem,
    #[prop(into)] on_navigate: Callback<&'static str>,
) -> impl IntoView {
    let is_enabled = item.is_enabled();
    let label = item.label;
    let icon_path = item.icon;
    let path = item.path;
    let disabled_reason = match &item.state {
        MenuItemState::Disabled(reason) => Some(*reason),
        MenuItemState::Enabled => None,
    };

    let base_class = if is_enabled {
        "group flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-all cursor-pointer hover:bg-accent hover:text-accent-foreground"
    } else {
        "group flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-all cursor-not-allowed opacity-50"
    };

    view! {
        <button
            class=base_class
            disabled=!is_enabled
            on:click=move |_| {
                if is_enabled {
                    on_navigate.run(path);
                }
            }
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d=icon_path/>
            </svg>
            <span class="flex-1 text-left">{label}</span>
            {disabled_reason.map(|reason| view! {
                <span class="text-xs text-muted-foreground italic">
                    {"("}{reason}{")"}
                </span>
            })}
        </button>
    }
}

/// Collapsible group component
#[component]
fn StartMenuGroup(
    group: MenuGroup,
    #[prop(into)] on_navigate: Callback<&'static str>,
) -> impl IntoView {
    let collapsed = RwSignal::new(false);
    let name = group.name;
    let items = group.items;

    // Get group icon based on name
    let group_icon = match name {
        "Core" => "M17.657 18.657A8 8 0 016.343 7.343S7 9 9 10c0-2 .5-5 2.986-7C14 5 16.09 5.777 17.656 7.343A7.975 7.975 0 0120 13a7.975 7.975 0 01-2.343 5.657z",
        "Model Ops" => "M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z",
        "Operations" => "M13 10V3L4 14h7v7l9-11h-7z",
        "Admin" => "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        _ => "M4 6h16M4 12h16M4 18h16",
    };

    // Compute the chevron class dynamically
    let chevron_class = move || {
        if collapsed.get() {
            "h-4 w-4 transition-transform duration-150 -rotate-90"
        } else {
            "h-4 w-4 transition-transform duration-150"
        }
    };

    view! {
        <div class="mb-2">
            // Group header
            <button
                class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground hover:bg-muted/50 transition-colors"
                on:click=move |_| collapsed.update(|v| *v = !*v)
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class=chevron_class
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
                </svg>
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-4 w-4"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d=group_icon/>
                </svg>
                <span>{name}</span>
            </button>

            // Group items
            <div
                class="ml-2 mt-1 space-y-0.5 overflow-hidden transition-all duration-150"
                class:hidden=move || collapsed.get()
            >
                {items
                    .into_iter()
                    .map(|item| {
                        let on_nav = on_navigate.clone();
                        view! {
                            <StartMenuItem item=item on_navigate=on_nav/>
                        }
                    })
                    .collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Renders the menu content (groups)
#[component]
fn StartMenuContent(#[prop(into)] on_navigate: Callback<&'static str>) -> impl IntoView {
    let menu_groups = get_menu_groups();

    view! {
        <div class="p-3">
            {menu_groups
                .into_iter()
                .map(|group| {
                    let on_nav = on_navigate.clone();
                    view! {
                        <StartMenuGroup group=group on_navigate=on_nav/>
                    }
                })
                .collect::<Vec<_>>()}
        </div>
    }
}

/// Start Menu component
///
/// A compact launcher with grouped modules that opens from the "Start" button
/// in the bottom taskbar.
#[component]
pub fn StartMenu(
    /// Signal controlling whether the menu is open
    #[prop(into)]
    open: RwSignal<bool>,
) -> impl IntoView {
    let navigate = use_navigate();

    // Handle navigation and close menu
    let on_navigate = Callback::new(move |path: &'static str| {
        navigate(path, Default::default());
        open.set(false);
    });

    // Handle backdrop click
    let close_menu = move |_| open.set(false);

    // Handle escape key
    Effect::new(move || {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        if open.get() {
            let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                if event.key() == "Escape" {
                    open.set(false);
                }
            }) as Box<dyn FnMut(_)>);

            let window = web_sys::window().expect("no global window exists");
            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());

            // Keep closure alive
            closure.forget();
        }
    });

    view! {
        // Backdrop and menu container
        <Show
            when=move || open.get()
            fallback=|| ()
        >
            // Backdrop - covers entire screen
            <div
                class="fixed inset-0 z-40 bg-black/20 backdrop-blur-sm"
                style="animation: fadeIn 150ms ease-out"
                on:click=close_menu
            />

            // Menu panel - positioned at bottom left
            <div
                class="fixed bottom-14 left-4 z-50 w-72 max-h-[70vh] overflow-y-auto rounded-lg border bg-popover text-popover-foreground shadow-xl"
                style="animation: slideUp 200ms ease-out"
                on:click=|e| e.stop_propagation()
            >
                // Header
                <div class="sticky top-0 z-10 flex items-center gap-3 border-b bg-popover/95 backdrop-blur-sm px-4 py-3">
                    <div class="flex h-8 w-8 items-center justify-center rounded-md bg-primary text-primary-foreground font-bold text-sm">
                        "AOS"
                    </div>
                    <div>
                        <h2 class="text-sm font-semibold">"AdapterOS"</h2>
                        <p class="text-xs text-muted-foreground">"Control Plane"</p>
                    </div>
                </div>

                // Menu groups
                <StartMenuContent on_navigate=on_navigate.clone()/>

                // Footer
                <div class="sticky bottom-0 border-t bg-popover/95 backdrop-blur-sm px-4 py-2">
                    <p class="text-xs text-muted-foreground text-center">
                        "Press "<kbd class="px-1 py-0.5 rounded bg-muted font-mono text-[10px]">"Esc"</kbd>" to close"
                    </p>
                </div>
            </div>
        </Show>
    }
}

/// Start button component for the taskbar
#[component]
pub fn StartButton(
    /// Signal controlling whether the menu is open
    #[prop(into)]
    open: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <button
            class="flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-primary-foreground font-medium text-sm hover:bg-primary/90 transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
            on:click=move |_| open.update(|v| *v = !*v)
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-5 w-5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16"/>
            </svg>
            "Start"
        </button>
    }
}

/// Taskbar component with Start button
#[component]
pub fn Taskbar() -> impl IntoView {
    let menu_open = RwSignal::new(false);

    view! {
        <div class="fixed bottom-0 left-0 right-0 z-30 h-12 border-t bg-background/95 backdrop-blur-sm px-4 flex items-center gap-4">
            <StartButton open=menu_open/>
            <StartMenu open=menu_open/>

            // Spacer
            <div class="flex-1"/>

            // Quick info area (placeholder)
            <div class="text-xs text-muted-foreground">
                "AdapterOS v0.1.0"
            </div>
        </div>
    }
}
