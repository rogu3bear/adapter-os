//! 404 Not Found page

use crate::components::NotFoundSurface;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Suggest related pages based on the current URL path segments.
fn suggestions_for_path(path: &str) -> Vec<(&'static str, &'static str)> {
    let mut suggestions = Vec::new();

    if path.starts_with("/adapters") {
        suggestions.push(("View all adapters", "/adapters"));
    }
    if path.starts_with("/training") {
        suggestions.push(("View training jobs", "/training"));
    }
    if path.starts_with("/stacks") {
        suggestions.push(("View stacks", "/stacks"));
    }
    if path.starts_with("/workers") {
        suggestions.push(("View workers", "/workers"));
    }
    if path.starts_with("/models") {
        suggestions.push(("View models", "/models"));
    }
    if path.starts_with("/documents") {
        suggestions.push(("View documents", "/documents"));
    }
    if path.starts_with("/datasets") {
        suggestions.push(("View datasets", "/datasets"));
    }
    if path.starts_with("/repositories") {
        suggestions.push(("View repositories", "/repositories"));
    }
    if path.starts_with("/settings") {
        suggestions.push(("Go to settings", "/settings"));
    }
    if path.starts_with("/routing") {
        suggestions.push(("View routing decisions", "/routing"));
    }
    if path.starts_with("/chat") {
        suggestions.push(("Open chat", "/chat"));
    }

    suggestions
}

/// Not found page
#[component]
pub fn NotFound() -> impl IntoView {
    let location = use_location();
    let pathname = location.pathname.get_untracked();
    let suggestions = suggestions_for_path(&pathname);

    view! {
        <NotFoundSurface
            title="Page not found"
            description="The requested page does not exist."
            action_label="Go to Dashboard"
            action_href="/"
            class="min-h-[60vh]".to_string()
        >
            <div class="text-sm text-muted-foreground mb-4">
                "Requested path: "
                <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">{pathname}</code>
            </div>
            {(!suggestions.is_empty()).then(|| {
                let links: Vec<_> = suggestions
                    .iter()
                    .map(|(label, href)| {
                        view! {
                            <a
                                href=*href
                                class="block px-3 py-2 rounded-md text-sm text-primary hover:bg-accent/30 transition-colors"
                            >
                                {*label}" \u{2192}"
                            </a>
                        }
                    })
                    .collect();

                view! {
                    <div>
                        <p class="text-sm text-muted-foreground mb-2">"Did you mean:"</p>
                        <div class="space-y-1">
                            {links}
                        </div>
                    </div>
                }
            })}
        </NotFoundSurface>
    }
}
