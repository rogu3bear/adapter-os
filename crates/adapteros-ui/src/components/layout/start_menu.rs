//! StartMenu - Module launcher menu
//!
//! Windows-style start menu with all application pages organized by category.

use leptos::prelude::*;

/// Module launcher menu with all application pages
#[component]
pub fn StartMenu(on_close: impl Fn() + Clone + 'static) -> impl IntoView {
    let on_close_clone = on_close.clone();

    // All module categories - comprehensive list of all pages
    let modules = vec![
        (
            "Core",
            vec![
                ("Dashboard", "/"),
                ("Chat", "/chat"),
                ("Adapters", "/adapters"),
            ],
        ),
        (
            "Resources",
            vec![
                ("Models", "/models"),
                ("Stacks", "/stacks"),
                ("Collections", "/collections"),
                ("Datasets", "/datasets"),
                ("Documents", "/documents"),
                ("Repositories", "/repositories"),
            ],
        ),
        (
            "Operations",
            vec![
                ("Training", "/training"),
                ("Workers", "/workers"),
                ("Agents", "/agents"),
                ("Routing", "/routing"),
                ("Runs", "/runs"),
            ],
        ),
        (
            "Monitoring",
            vec![
                ("System", "/system"),
                ("Monitoring", "/monitoring"),
                ("Audit", "/audit"),
                ("Errors", "/errors"),
            ],
        ),
        (
            "Administration",
            vec![
                ("Admin", "/admin"),
                ("Policies", "/policies"),
                ("Settings", "/settings"),
            ],
        ),
        (
            "Developer",
            vec![("Diff", "/diff"), ("Style Audit", "/style-audit")],
        ),
    ];

    view! {
        <div
            class="absolute left-0 w-96 bg-background border border-border rounded-lg shadow-xl z-50"
            style="bottom: 100%; margin-bottom: 0.5rem;"
        >
            // Header
            <div class="p-4 border-b border-border">
                <h2 class="text-lg font-semibold">"adapterOS"</h2>
                <p class="text-xs text-muted-foreground">"Module Launcher"</p>
            </div>

            // Module grid - two column layout for better organization
            <div class="p-3 max-h-[28rem] overflow-y-auto">
                <div class="grid grid-cols-2 gap-x-4">
                    {modules.into_iter().map(|(category, items)| {
                        let on_close = on_close_clone.clone();
                        view! {
                            <div class="mb-3">
                                <h3 class="text-xs font-medium text-muted-foreground uppercase tracking-wider px-2 mb-1">
                                    {category}
                                </h3>
                                <div class="space-y-0.5">
                                    {items.into_iter().map(|(label, href)| {
                                        let on_close = on_close.clone();
                                        view! {
                                            <a
                                                href=href
                                                class="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-muted/50 transition-colors"
                                                on:click=move |_| on_close()
                                            >
                                                <div class="w-6 h-6 rounded bg-primary/10 flex items-center justify-center shrink-0">
                                                    <span class="text-primary text-xs">{label.chars().next().unwrap_or('?')}</span>
                                                </div>
                                                <span class="text-sm font-medium truncate">{label}</span>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>

            // Footer
            <div class="p-3 border-t border-border">
                <div class="flex items-center justify-between text-xs text-muted-foreground">
                    <span>"v0.1.0"</span>
                    <a href="/settings" class="hover:text-foreground transition-colors" on:click=move |_| on_close()>
                        "Settings"
                    </a>
                </div>
            </div>
        </div>
    }
}
