//! StartMenu - Module-based navigation
//!
//! Navigation structure for the control plane:
//! - Core workflow: Operate, Build, Configure, Data, Verify
//! - Organization: Org (users, roles, API keys) - admin functions
//! - Interactive: Tools (Chat, Routing Debug, Run Diff)
//! - Personal: Account (Profile, Preferences) - collapsed by default

use crate::components::layout::nav_registry::{build_start_menu_modules, StartMenuModule};
use crate::signals::use_ui_profile;
use leptos::prelude::*;

/// Module launcher menu with six-module navigation structure
#[component]
pub fn StartMenu(on_close: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let ui_profile = use_ui_profile();
    let modules = Signal::derive(move || build_start_menu_modules(ui_profile.get()));

    // Track which modules are expanded (Tools starts collapsed)
    let expanded_modules = RwSignal::new(Vec::new());
    Effect::new(move || {
        let initial_expanded: Vec<bool> = modules.get().iter().map(|m| !m.collapsed).collect();
        expanded_modules.set(initial_expanded);
    });

    view! {
        <div
            class="absolute left-0 w-80 bg-background border border-border rounded-lg shadow-xl z-50"
            style="bottom: 100%; margin-bottom: 0.5rem; max-height: calc(100vh - 6rem);"
        >
            // Header
            <div class="p-4 border-b border-border">
                <h2 class="text-lg font-semibold">"adapterOS"</h2>
                <p class="text-xs text-muted-foreground">"Control Plane"</p>
            </div>

            // Module list
            <div class="p-2 overflow-y-auto" style="max-height: calc(100vh - 12rem);">
                {move || {
                    modules
                        .get()
                        .into_iter()
                        .enumerate()
                        .map(|(idx, module)| {
                            let on_close = on_close.clone();
                            view! {
                                <ModuleSection
                                    module=module
                                    expanded=expanded_modules
                                    index=idx
                                    on_navigate=on_close
                                />
                            }
                        })
                        .collect::<Vec<_>>()
                }}
            </div>

        </div>
    }
}

/// Collapsible module section
#[component]
fn ModuleSection(
    module: StartMenuModule,
    expanded: RwSignal<Vec<bool>>,
    index: usize,
    on_navigate: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let is_expanded = move || expanded.get().get(index).copied().unwrap_or(true);

    let toggle = move |_| {
        expanded.update(|v| {
            if let Some(val) = v.get_mut(index) {
                *val = !*val;
            }
        });
    };

    let name = module.name;
    let icon = module.icon;
    let items = module.items;

    view! {
        <div class="mb-1">
            // Module header (clickable to expand/collapse)
            <button
                class="w-full flex items-center gap-2 px-3 py-2 rounded-md hover:bg-muted/50 transition-colors"
                on:click=toggle
            >
                <svg
                    class="w-4 h-4 text-primary shrink-0"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d=icon/>
                </svg>
                <span class="text-sm font-medium flex-1 text-left">{name}</span>
                <svg
                    class=move || format!(
                        "w-3 h-3 text-muted-foreground transition-transform {}",
                        if is_expanded() { "rotate-180" } else { "" }
                    )
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                </svg>
            </button>

            // Items (shown when expanded)
            <Show when=is_expanded>
                <div class="ml-6 space-y-0.5 mt-0.5">
                    {items.iter().map(|(label, href)| {
                        let on_nav = on_navigate.clone();
                        view! {
                            <a
                                href=*href
                                class="flex items-center px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/30 rounded-md transition-colors"
                                on:click=move |_| on_nav()
                            >
                                {*label}
                            </a>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </Show>
        </div>
    }
}
