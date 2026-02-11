//! StartMenu - Workflow-based navigation
//!
//! Navigation structure follows the workflow spine:
//! Dashboard (pinned) → Data → Train → Deploy → Route → Infer → Observe → Govern → Org
//!
//! Each group shows Alt+N shortcut hint and can be expanded/collapsed.

use crate::components::layout::nav_registry::{
    build_start_menu_modules, StartMenuModule, DASHBOARD_ITEM,
};
use crate::signals::use_ui_profile;
use leptos::html;
use leptos::prelude::*;

/// Module launcher menu with workflow-based navigation structure
#[component]
pub fn StartMenu(on_close: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let ui_profile = use_ui_profile();
    let modules = Signal::derive(move || build_start_menu_modules(ui_profile.get()));
    let menu_ref = NodeRef::<html::Div>::new();
    Effect::new({
        move |_| {
            if let Some(menu) = menu_ref.get() {
                let _ = menu.focus();
            }
        }
    });

    // Track which modules are expanded (Tools starts collapsed)
    let expanded_modules = RwSignal::new(Vec::new());
    Effect::new(move || {
        let Some(mods) = modules.try_get() else {
            return;
        };
        let initial_expanded: Vec<bool> = mods.iter().map(|m| !m.collapsed).collect();
        let _ = expanded_modules.try_set(initial_expanded);
    });

    let on_escape = {
        let on_close = on_close.clone();
        move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" {
                ev.prevent_default();
                ev.stop_propagation();
                on_close();
            }
        }
    };

    let on_close_click = on_close.clone();
    let on_close_dashboard = on_close.clone();
    let on_close_modules = on_close;

    view! {
        <div class="fixed inset-0 z-50">
            <button
                class="absolute inset-0 bg-background/60 backdrop-blur-sm"
                aria-label="Close start menu"
                on:click=move |_| on_close_click()
            />
            <div
                node_ref=menu_ref
                class="absolute left-0 w-80 bg-background border border-border rounded-lg shadow-xl"
                style="bottom: 3.5rem; left: 0.5rem; max-height: calc(100vh - 6rem);"
                role="dialog"
                aria-modal="true"
                tabindex="0"
                on:keydown=on_escape
            >
                // Header
                <div class="p-4 border-b border-border">
                    <h2 class="heading-4">"adapterOS"</h2>
                    <p class="text-xs text-muted-foreground">"Control Plane"</p>
                </div>

                // Command Palette hint
                <div class="px-4 py-2 border-b border-border">
                    <div class="flex items-center gap-2 text-sm text-muted-foreground">
                        <kbd class="px-1.5 py-0.5 rounded bg-muted border border-border font-mono text-xs">"⌘K"</kbd>
                        <span>"Command Palette"</span>
                    </div>
                </div>

                // Module list
                <div class="p-2 overflow-y-auto" style="max-height: calc(100vh - 16rem);">
                    // Dashboard - pinned at top
                    {
                        let on_close = on_close_dashboard.clone();
                        view! {
                            <a
                                href=DASHBOARD_ITEM.route
                                class="flex items-center gap-2 px-3 py-2 rounded-md hover:bg-muted/50 transition-colors mb-2"
                                on:click=move |_| on_close()
                            >
                                <svg
                                    class="w-4 h-4 text-primary shrink-0"
                                    fill="none"
                                    stroke="currentColor"
                                    viewBox="0 0 24 24"
                                    stroke-width="2"
                                >
                                    <path stroke-linecap="round" stroke-linejoin="round" d=DASHBOARD_ITEM.icon.unwrap_or("")/>
                                </svg>
                                <span class="text-sm font-medium">{DASHBOARD_ITEM.label}</span>
                            </a>
                        }
                    }

                    <div class="border-t border-border/50 my-2"></div>

                    // Workflow groups
                    {move || {
                        modules
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(idx, module)| {
                                let on_close = on_close_modules.clone();
                                // Alt shortcut is idx+1 for full profile
                                let alt_hint = idx + 1;
                                view! {
                                    <ModuleSection
                                        module=module
                                        expanded=expanded_modules
                                        index=idx
                                        alt_hint=alt_hint
                                        on_navigate=on_close
                                    />
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </div>
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
    alt_hint: usize,
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
                // Alt shortcut hint
                <kbd class="px-1 py-0.5 rounded bg-muted/50 border border-border/50 font-mono text-xs text-muted-foreground">
                    {format!("Alt+{}", alt_hint)}
                </kbd>
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
