//! PageScaffold - Standardized page layout component
//!
//! Provides a consistent page structure with:
//! - Page title with optional breadcrumbs
//! - Primary actions slot (right-aligned)
//! - Main content area
//! - Optional inspector panel (right side)
//!
//! Usage:
//! ```rust,ignore
//! view! {
//!     <PageScaffold
//!         title="Adapters"
//!         breadcrumbs=vec![
//!             BreadcrumbItem::new("Deploy", "/adapters"),
//!             BreadcrumbItem::current("Adapters"),
//!         ]
//!     >
//!         <PageScaffoldActions slot>
//!             <Button>"Create"</Button>
//!         </PageScaffoldActions>
//!         <div>"Main content here"</div>
//!     </PageScaffold>
//! }
//! ```

use leptos::prelude::*;

/// Breadcrumb item for navigation hierarchy
#[derive(Debug, Clone)]
pub struct BreadcrumbItem {
    /// Display label for the breadcrumb
    pub label: String,
    /// Optional navigation href (None for current page)
    pub href: Option<String>,
}

impl BreadcrumbItem {
    /// Create a new breadcrumb with a link
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    /// Create a non-clickable breadcrumb label (e.g. nav group name)
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }

    /// Create a breadcrumb for the current page (no link)
    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

/// Slot for primary actions in PageScaffold
#[slot]
pub struct PageScaffoldActions {
    children: Children,
}

/// Slot for inspector panel in PageScaffold
#[slot]
pub struct PageScaffoldInspector {
    children: Children,
}

/// Standardized page layout component
///
/// Renders a page with consistent header, content area, and optional inspector panel.
#[component]
pub fn PageScaffold(
    /// Page title
    #[prop(into)]
    title: String,
    /// Optional subtitle/description
    #[prop(optional, into)]
    subtitle: Option<String>,
    /// Optional breadcrumb navigation
    #[prop(optional)]
    breadcrumbs: Option<Vec<BreadcrumbItem>>,
    /// Optional actions slot (rendered in header, right side)
    #[prop(optional)]
    page_scaffold_actions: Option<PageScaffoldActions>,
    /// Optional inspector slot (rendered as right panel)
    #[prop(optional)]
    page_scaffold_inspector: Option<PageScaffoldInspector>,
    /// Main content
    children: Children,
) -> impl IntoView {
    let has_inspector = page_scaffold_inspector.is_some();

    view! {
        <div class="shell-page">
            // Page header
            <header class="page-scaffold-header">
                // Breadcrumb navigation
                {breadcrumbs.map(|crumbs| {
                    view! {
                        <nav class="page-scaffold-breadcrumbs" aria-label="Breadcrumb">
                            <ol class="flex items-center gap-1.5 text-sm text-muted-foreground mb-3">
                                {
                                let crumb_count = crumbs.len();
                                crumbs.into_iter().enumerate().map(|(idx, crumb)| {
                                    let label = crumb.label.clone();
                                    let href = crumb.href.clone();
                                    let is_last = idx == crumb_count - 1;

                                    view! {
                                        <li class="flex items-center">
                                            {if idx > 0 {
                                                Some(view! {
                                                    <span class="mx-1.5 text-muted-foreground/50" aria-hidden="true">
                                                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                                                        </svg>
                                                    </span>
                                                })
                                            } else {
                                                None
                                            }}
                                            {if let Some(href) = href {
                                                view! {
                                                    <a href=href class="hover:text-foreground transition-colors">
                                                        {label}
                                                    </a>
                                                }.into_any()
                                            } else if is_last {
                                                view! {
                                                    <span class="text-foreground font-medium" aria-current="page">
                                                        {label}
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span>{label}</span>
                                                }.into_any()
                                            }}
                                        </li>
                                    }
                                }).collect::<Vec<_>>()
                            }
                            </ol>
                        </nav>
                    }
                })}

                // Title row with actions
                <div class="flex items-center justify-between gap-4">
                    <div class="min-w-0">
                        <h1 class="heading-1 truncate">{title}</h1>
                        {subtitle.map(|s| view! {
                            <p class="body-small text-muted-foreground mt-1">{s}</p>
                        })}
                    </div>
                    {page_scaffold_actions.map(|actions| view! {
                        <div class="flex items-center gap-2 shrink-0">
                            {(actions.children)()}
                        </div>
                    })}
                </div>
            </header>

            // Content area (with optional inspector)
            <div class=move || {
                if has_inspector {
                    "page-scaffold-content page-scaffold-content--with-inspector"
                } else {
                    "page-scaffold-content"
                }
            }>
                // Main content
                <main class="page-scaffold-main">
                    {children()}
                </main>

                // Inspector panel (if provided)
                {page_scaffold_inspector.map(|inspector| view! {
                    <aside class="page-scaffold-inspector">
                        {(inspector.children)()}
                    </aside>
                })}
            </div>
        </div>
    }
}
