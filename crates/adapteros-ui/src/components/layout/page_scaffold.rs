//! PageScaffold - Standardized page layout component
//!
//! Provides a consistent page structure with:
//! - Page title with optional breadcrumbs
//! - Primary action slot (single CTA, right-aligned)
//! - Secondary actions slot (right-aligned)
//! - Main content area
//!
//! Usage:
//! ```rust,ignore
//! view! {
//!     <PageScaffold
//!         title="Adapters"
//!         breadcrumbs=vec![
//!             BreadcrumbItem::new("Build", "/adapters"),
//!             BreadcrumbItem::current("Adapters"),
//!         ]
//!     >
//!         <PageScaffoldPrimaryAction slot>
//!             <Button>"Create"</Button>
//!         </PageScaffoldPrimaryAction>
//!         <PageScaffoldActions slot>
//!             <Button variant=ButtonVariant::Ghost>"Import"</Button>
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

/// Slot for secondary actions in PageScaffold
#[slot]
pub struct PageScaffoldActions {
    children: Children,
}

/// Slot for inline status badge/content next to the page title.
#[slot]
pub struct PageScaffoldStatus {
    children: Children,
}

/// Slot for single primary action in PageScaffold
#[slot]
pub struct PageScaffoldPrimaryAction {
    children: Children,
}

/// Optional right-rail inspector content.
#[slot]
pub struct PageScaffoldInspector {
    children: Children,
}

/// Standardized page layout component
///
/// Renders a page with consistent header and content area.
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
    /// Optional status content rendered inline with the title.
    #[prop(optional)]
    page_scaffold_status: Option<PageScaffoldStatus>,
    /// Optional primary action slot (rendered in header, right side before secondary actions)
    #[prop(optional)]
    page_scaffold_primary_action: Option<PageScaffoldPrimaryAction>,
    /// Optional secondary actions slot (rendered in header, right side)
    #[prop(optional)]
    page_scaffold_actions: Option<PageScaffoldActions>,
    /// Optional context/details rail rendered on wide screens.
    #[prop(optional)]
    page_scaffold_inspector: Option<PageScaffoldInspector>,
    /// Keep full-width OS layout by default.
    #[prop(optional, default = true)]
    full_width: bool,
    /// Main content
    children: Children,
) -> impl IntoView {
    let shell_page_class = if full_width {
        "shell-page shell-page--full"
    } else {
        "shell-page shell-page--readable"
    };
    let content_class = if page_scaffold_inspector.is_some() {
        "page-scaffold-content page-scaffold-content--with-inspector"
    } else {
        "page-scaffold-content"
    };

    view! {
        <div class=shell_page_class>
            // Page header
            <header class="page-scaffold-header">
                // Breadcrumb navigation
                {breadcrumbs.map(|crumbs| {
                    view! {
                        <crate::components::BreadcrumbTrail items=crumbs class="mb-3" />
                    }
                })}

                // Title row with actions
                <div class="page-scaffold-header-main">
                    <div class="min-w-0 w-full">
                        <div class="page-scaffold-title-row">
                            <h1 class="heading-1 break-words">{title}</h1>
                            {page_scaffold_status.map(|status| view! {
                                <div class="page-scaffold-title-status">
                                    {(status.children)()}
                                </div>
                            })}
                        </div>
                        {subtitle.map(|s| view! {
                            <p class="body-small text-muted-foreground mt-1">{s}</p>
                        })}
                    </div>
                    {match (page_scaffold_primary_action, page_scaffold_actions) {
                        (Some(primary_action), Some(actions)) => view! {
                            <div class="page-scaffold-header-actions">
                                <div class="page-scaffold-primary-action">
                                    {(primary_action.children)()}
                                </div>
                                <details class="page-scaffold-overflow">
                                    <summary
                                        class="page-scaffold-overflow-trigger"
                                        role="button"
                                        aria-label="More actions"
                                    >
                                        <svg class="h-4 w-4" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                                            <circle cx="10" cy="4" r="1.5"/>
                                            <circle cx="10" cy="10" r="1.5"/>
                                            <circle cx="10" cy="16" r="1.5"/>
                                        </svg>
                                    </summary>
                                    <div class="page-scaffold-overflow-menu">
                                        <div class="page-scaffold-overflow-content">{(actions.children)()}</div>
                                    </div>
                                </details>
                            </div>
                        }
                        .into_any(),
                        (Some(primary_action), None) => view! {
                            <div class="page-scaffold-header-actions">
                                {(primary_action.children)()}
                            </div>
                        }
                        .into_any(),
                        (None, Some(actions)) => view! {
                            <div class="page-scaffold-header-actions">
                                <details class="page-scaffold-overflow">
                                    <summary
                                        class="page-scaffold-overflow-trigger"
                                        role="button"
                                        aria-label="More actions"
                                    >
                                        <svg class="h-4 w-4" fill="currentColor" viewBox="0 0 20 20" aria-hidden="true">
                                            <circle cx="10" cy="4" r="1.5"/>
                                            <circle cx="10" cy="10" r="1.5"/>
                                            <circle cx="10" cy="16" r="1.5"/>
                                        </svg>
                                    </summary>
                                    <div class="page-scaffold-overflow-menu">
                                        <div class="page-scaffold-overflow-content">{(actions.children)()}</div>
                                    </div>
                                </details>
                            </div>
                        }
                        .into_any(),
                        (None, None) => view! {}.into_any(),
                    }}
                </div>
            </header>

            // Content area
            <div class=content_class>
                // Main content
                <section class="page-scaffold-main">
                    {children()}
                </section>
                {page_scaffold_inspector.map(|inspector| {
                    view! {
                        <aside class="page-scaffold-inspector" aria-label="Context and details">
                            {(inspector.children)()}
                        </aside>
                    }
                })}
            </div>
        </div>
    }
}
