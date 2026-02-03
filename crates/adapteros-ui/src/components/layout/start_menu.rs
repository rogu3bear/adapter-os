//! StartMenu - Module-based navigation
//!
//! Navigation structure for the control plane:
//! - Core workflow: Operate, Build, Configure, Data, Verify
//! - Organization: Org (users, roles, API keys) - admin functions
//! - Interactive: Tools (Chat, Routing Debug, Run Diff)
//! - Personal: Account (Profile, Preferences) - collapsed by default

use leptos::prelude::*;

/// Navigation module definition
#[derive(Clone)]
struct NavModule {
    name: &'static str,
    icon: &'static str,
    items: &'static [(&'static str, &'static str)], // (label, href)
    collapsed: bool,
}

/// Build the navigation modules
///
/// Navigation philosophy:
/// - Core workflow modules (Operate, Build, Configure, Data, Verify) are primary
/// - Organization management (Org) handles users, roles, API keys - admin functions
/// - Account (User, Settings) is personal - also accessible via topbar user menu
/// - Tools contains interactive utilities (Chat, Routing Debug, Run Diff)
fn build_modules() -> Vec<NavModule> {
    vec![
        NavModule {
            name: "Operate",
            icon: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z",
            items: &[
                ("Dashboard", "/"),
                ("Infrastructure", "/system"),
                ("Workers", "/workers"),
                ("Metrics", "/monitoring"),
                ("Incidents", "/errors"),
            ],
            collapsed: false,
        },
        NavModule {
            name: "Build",
            icon: "M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z",
            items: &[("Training", "/training"), ("Agents", "/agents")],
            collapsed: false,
        },
        NavModule {
            name: "Configure",
            icon: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
            items: &[
                ("Adapters", "/adapters"),
                ("Runtime Stacks", "/stacks"),
                ("Policies", "/policies"),
                ("Models", "/models"),
            ],
            collapsed: false,
        },
        NavModule {
            name: "Data",
            icon: "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4",
            items: &[
                ("Datasets", "/datasets"),
                ("Documents", "/documents"),
                ("Collections", "/collections"),
                ("Repositories", "/repositories"),
            ],
            collapsed: false,
        },
        NavModule {
            name: "Verify",
            icon: "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z",
            items: &[
                ("Audit", "/audit"),
                ("Runs", "/runs"),
                ("Reviews", "/reviews"),
            ],
            collapsed: false,
        },
        NavModule {
            name: "Org",
            // Building icon - represents organization/company management
            icon: "M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4",
            items: &[
                ("Users", "/admin"),
                ("Roles", "/admin?tab=roles"),
                ("API Keys", "/admin?tab=keys"),
                ("Organization", "/admin?tab=org"),
            ],
            collapsed: false,
        },
        NavModule {
            name: "Tools",
            // Wrench icon - represents utilities and interactive features
            icon: "M10.343 3.94c.09-.542.56-.94 1.11-.94h1.093c.55 0 1.02.398 1.11.94l.149.894c.07.424.384.764.78.93.398.164.855.142 1.205-.108l.737-.527a1.125 1.125 0 011.45.12l.773.774c.39.389.44 1.002.12 1.45l-.527.737c-.25.35-.272.806-.107 1.204.165.397.505.71.93.78l.893.15c.543.09.94.56.94 1.109v1.094c0 .55-.397 1.02-.94 1.11l-.893.149c-.425.07-.765.383-.93.78-.165.398-.143.854.107 1.204l.527.738c.32.447.269 1.06-.12 1.45l-.774.773a1.125 1.125 0 01-1.449.12l-.738-.527c-.35-.25-.806-.272-1.203-.107-.397.165-.71.505-.781.929l-.149.894c-.09.542-.56.94-1.11.94h-1.094c-.55 0-1.019-.398-1.11-.94l-.148-.894c-.071-.424-.384-.764-.781-.93-.398-.164-.854-.142-1.204.108l-.738.527c-.447.32-1.06.269-1.45-.12l-.773-.774a1.125 1.125 0 01-.12-1.45l.527-.737c.25-.35.273-.806.108-1.204-.165-.397-.505-.71-.93-.78l-.894-.15c-.542-.09-.94-.56-.94-1.109v-1.094c0-.55.398-1.02.94-1.11l.894-.149c.424-.07.765-.383.93-.78.165-.398.143-.854-.107-1.204l-.527-.738a1.125 1.125 0 01.12-1.45l.773-.773a1.125 1.125 0 011.45-.12l.737.527c.35.25.807.272 1.204.107.397-.165.71-.505.78-.929l.15-.894z",
            items: &[
                ("Chat", "/chat"),
                ("Routing Debug", "/routing"),
                ("Run Diff", "/diff"),
            ],
            collapsed: false, // Tools now visible by default - Chat is a core feature
        },
        NavModule {
            name: "Account",
            // User circle icon - represents personal settings
            icon: "M5.121 17.804A13.937 13.937 0 0112 16c2.5 0 4.847.655 6.879 1.804M15 10a3 3 0 11-6 0 3 3 0 016 0zm6 2a9 9 0 11-18 0 9 9 0 0118 0z",
            items: &[
                ("Profile", "/user"),
                ("Preferences", "/settings"),
            ],
            collapsed: true, // Personal settings collapsed - less frequently accessed
        },
    ]
}

/// Module launcher menu with six-module navigation structure
#[component]
pub fn StartMenu(on_close: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let modules = build_modules();

    // Track which modules are expanded (Tools starts collapsed)
    let initial_expanded: Vec<bool> = modules.iter().map(|m| !m.collapsed).collect();
    let expanded_modules = RwSignal::new(initial_expanded);

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
                {modules.into_iter().enumerate().map(|(idx, module)| {
                    let on_close = on_close.clone();
                    view! {
                        <ModuleSection
                            module=module
                            expanded=expanded_modules
                            index=idx
                            on_navigate=on_close
                        />
                    }
                }).collect::<Vec<_>>()}
            </div>

        </div>
    }
}

/// Collapsible module section
#[component]
fn ModuleSection(
    module: NavModule,
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
