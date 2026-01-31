//! Breadcrumb trail component
//!
//! Route-aware breadcrumb navigation that automatically generates
//! breadcrumb trails from the current route path.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::{IconChevronRight, IconHome};

/// Breadcrumb item for custom trails
#[derive(Debug, Clone)]
pub struct BreadcrumbItem {
    /// Display label
    pub label: String,
    /// Navigation href (None = current page, not clickable)
    pub href: Option<String>,
}

impl BreadcrumbItem {
    /// Create a clickable breadcrumb
    pub fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    /// Create a non-clickable breadcrumb (current page)
    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

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
///     BreadcrumbItem::link("Adapters", "/adapters"),
///     BreadcrumbItem::current("my-adapter"),
/// ]/>
/// ```
#[component]
pub fn BreadcrumbTrail(
    /// Custom breadcrumb items (overrides auto-generation)
    #[prop(optional)]
    items: Option<Vec<BreadcrumbItem>>,
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
            return vec![BreadcrumbItem::current("Dashboard")];
        }

        let mut result = vec![BreadcrumbItem::link("Home", "/")];
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
                result.push(BreadcrumbItem::link(label, current_path.clone()));
            }
        }

        result
    };

    view! {
        <nav aria-label="Breadcrumb" class="flex items-center gap-1 text-sm text-muted-foreground">
            {move || crumbs().into_iter().enumerate().map(|(i, crumb)| {
                let is_first = i == 0;
                let _is_last = crumb.href.is_none();

                view! {
                    // Separator (except for first item)
                    {(!is_first).then(|| view! {
                        <IconChevronRight class="h-4 w-4 text-muted-foreground/50".to_string()/>
                    })}

                    // Breadcrumb item
                    {if let Some(href) = crumb.href {
                        view! {
                            <a
                                href=href
                                class="hover:text-foreground transition-colors flex items-center gap-1"
                            >
                                {is_first.then(|| view! {
                                    <IconHome class="h-4 w-4".to_string()/>
                                })}
                                <span>{crumb.label}</span>
                            </a>
                        }.into_any()
                    } else {
                        view! {
                            <span class="text-foreground font-medium">{crumb.label}</span>
                        }.into_any()
                    }}
                }
            }).collect::<Vec<_>>()}
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
