//! Breadcrumb trail component
//!
//! Route-aware breadcrumb navigation that automatically generates
//! breadcrumb trails from the current route path.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::layout::BreadcrumbItem;
use crate::components::{IconChevronRight, IconHome};

/// Route-aware breadcrumb trail
///
/// Automatically generates breadcrumbs from the current route path,
/// or uses custom items if provided.
///
/// # Example
/// ```ignore
/// // Auto-generated from route
/// <BreadcrumbTrail/>
///
/// // Custom items
/// <BreadcrumbTrail items=vec![
///     BreadcrumbItem::new("Adapters", "/adapters"),
///     BreadcrumbItem::current("my-adapter"),
/// ]/>
/// ```
#[component]
pub fn BreadcrumbTrail(
    /// Custom breadcrumb items (overrides auto-generation)
    #[prop(optional)]
    items: Option<Vec<BreadcrumbItem>>,
    /// Additional CSS classes to apply to the root <nav> element
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let location = use_location();

    // Generate breadcrumbs from route path if not provided
    let crumbs = move || {
        if let Some(ref custom) = items {
            return custom.clone();
        }

        let path = location.pathname.get();
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if segments.is_empty() {
            return vec![BreadcrumbItem::current("Home")];
        }

        let mut result = vec![BreadcrumbItem::new("Home", "/")];
        let mut current_path = String::new();

        for (i, segment) in segments.iter().enumerate() {
            current_path.push('/');
            current_path.push_str(segment);

            // Humanize the segment label
            let label = humanize_segment(segment);

            if i == segments.len() - 1 {
                // Last segment is current page
                result.push(BreadcrumbItem::current(label));
            } else {
                result.push(BreadcrumbItem::new(label, current_path.clone()));
            }
        }

        result
    };

    let base_class = "flex items-center text-sm text-muted-foreground";
    let combined_class = if class.is_empty() {
        base_class.to_string()
    } else {
        format!("{} {}", base_class, class)
    };

    view! {
        <nav aria-label="Breadcrumb" class=combined_class>
            <ol class="flex flex-wrap items-center gap-1.5">
                {move || crumbs().into_iter().enumerate().map(|(i, crumb)| {
                    let is_first = i == 0;
                    let _is_last = crumb.href.is_none();

                    view! {
                        <li class="flex items-center gap-1.5">
                            // Separator (except for first item)
                            {(!is_first).then(|| view! {
                                <IconChevronRight class="h-4 w-4 text-muted-foreground/50 shrink-0".to_string()/>
                            })}

                            // Breadcrumb item
                            {if let Some(href) = crumb.href {
                                view! {
                                    <a
                                        href=href
                                        class="hover:text-foreground transition-colors flex items-center gap-1.5"
                                    >
                                        {is_first.then(|| view! {
                                            <IconHome class="h-4 w-4 shrink-0".to_string()/>
                                        })}
                                        <span class="truncate">{crumb.label}</span>
                                    </a>
                                }.into_any()
                            } else {
                                view! {
                                    <span class="text-foreground font-medium truncate" aria-current="page">
                                        {crumb.label}
                                    </span>
                                }.into_any()
                            }}
                        </li>
                    }
                }).collect::<Vec<_>>()}
            </ol>
        </nav>
    }
}

/// Convert URL segment to human-readable label
pub fn humanize_segment(segment: &str) -> String {
    // Handle common patterns
    match segment {
        "api-keys" => "API Keys".to_string(),
        "api_keys" => "API Keys".to_string(),
        _ => {
            // Replace hyphens/underscores with spaces and capitalize
            segment
                .split(['-', '_'])
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().chain(chars).collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}
