//! 404 Not Found page

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
        <div class="flex min-h-[60vh] flex-col items-center justify-center px-4">
            <div class="card p-8 max-w-md w-full text-center">
                <div class="text-6xl font-bold text-muted-foreground mb-2">"404"</div>
                <h1 class="heading-2 mb-2">"Page not found"</h1>
                <p class="text-muted-foreground mb-6">
                    "The page "
                    <code class="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">{pathname}</code>
                    " does not exist."
                </p>

                {(!suggestions.is_empty()).then(|| {
                    let links: Vec<_> = suggestions.iter().map(|(label, href)| {
                        view! {
                            <a
                                href=*href
                                class="block px-3 py-2 rounded-md text-sm text-primary hover:bg-accent/30 transition-colors"
                            >
                                {*label}" \u{2192}"
                            </a>
                        }
                    }).collect();

                    view! {
                        <div class="mb-6">
                            <p class="text-sm text-muted-foreground mb-2">"Did you mean:"</p>
                            <div class="space-y-1">
                                {links}
                            </div>
                        </div>
                    }
                })}

                <div class="flex items-center justify-center gap-3">
                    <a
                        href="/"
                        class="btn btn-primary btn-md"
                    >
                        "Go to Dashboard"
                    </a>
                </div>
            </div>
        </div>
    }
}
