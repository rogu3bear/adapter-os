//! Skeleton loader components for async loading placeholders
//!
//! Provides shimmer animation placeholders that match the shape of content
//! being loaded, reducing perceived loading time and layout shift.

use leptos::prelude::*;

/// Skeleton animation variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SkeletonVariant {
    /// Shimmer effect - gradient sweep animation
    #[default]
    Shimmer,
    /// Pulse effect - opacity fade animation
    Pulse,
}

impl SkeletonVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Shimmer => "skeleton",
            Self::Pulse => "skeleton skeleton-pulse",
        }
    }
}

/// Base skeleton component - rectangular placeholder
#[component]
pub fn Skeleton(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Optional width (CSS value like "100%" or "8rem")
    #[prop(optional, into)]
    width: Option<String>,
    /// Optional height (CSS value like "1rem" or "100px")
    #[prop(optional, into)]
    height: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("{} {}", variant.class(), class);
    let style = format!(
        "{}{}",
        width.map(|w| format!("width: {};", w)).unwrap_or_default(),
        height
            .map(|h| format!("height: {};", h))
            .unwrap_or_default()
    );

    view! {
        <div class=full_class style=style aria-hidden="true"></div>
    }
}

/// Skeleton text line - simulates a line of text
#[component]
pub fn SkeletonText(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Width of the text line (defaults to "100%")
    #[prop(optional, into)]
    width: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("{} skeleton-text {}", variant.class(), class);
    let w = width.unwrap_or_else(|| "100%".to_string());

    view! {
        <div class=full_class style=format!("width: {};", w) aria-hidden="true"></div>
    }
}

/// Skeleton card - matches Card component layout
#[component]
pub fn SkeletonCard(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Whether to show a header section
    #[prop(optional)]
    has_header: bool,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("card skeleton-card {}", class);
    let variant_class = variant.class();

    view! {
        <div class=full_class aria-hidden="true">
            {if has_header {
                Some(view! {
                    <div class="card-header">
                        <div class=format!("{} skeleton-text", variant_class) style="width: 33%; height: 1.25rem;"></div>
                        <div class=format!("{} skeleton-text", variant_class) style="width: 66%; height: 0.875rem; margin-top: 0.5rem;"></div>
                    </div>
                })
            } else {
                None
            }}
            <div class="card-content" style="display: flex; flex-direction: column; gap: 0.75rem;">
                <div class=format!("{} skeleton-text", variant_class) style="width: 100%;"></div>
                <div class=format!("{} skeleton-text", variant_class) style="width: 85%;"></div>
                <div class=format!("{} skeleton-text", variant_class) style="width: 70%;"></div>
            </div>
        </div>
    }
}

/// Skeleton table - matches Table component structure
#[component]
pub fn SkeletonTable(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Number of skeleton rows to render
    #[prop(default = 5)]
    rows: usize,
    /// Number of columns
    #[prop(default = 3)]
    columns: usize,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("skeleton-table {}", class);
    let variant_class = variant.class();

    // Pre-calculate column widths for visual interest
    let col_widths: Vec<&str> = (0..columns)
        .map(|col| match col % 3 {
            0 => "80%",
            1 => "60%",
            _ => "40%",
        })
        .collect();

    view! {
        <div class="table-wrapper" aria-hidden="true">
            <table class=format!("table {}", full_class)>
                <thead class="table-header">
                    <tr class="table-row">
                        {(0..columns)
                            .map(|_| {
                                view! {
                                    <th class="table-header-cell">
                                        <div
                                            class=format!("{} skeleton-text", variant_class)
                                            style="width: 60%;"
                                        ></div>
                                    </th>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </tr>
                </thead>
                <tbody class="table-body">
                    {(0..rows)
                        .map(|_| {
                            let widths = col_widths.clone();
                            view! {
                                <tr class="table-row">
                                    {widths
                                        .into_iter()
                                        .map(|width| {
                                            view! {
                                                <td class="table-cell">
                                                    <div
                                                        class=format!("{} skeleton-text", variant_class)
                                                        style=format!("width: {};", width)
                                                    ></div>
                                                </td>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

/// Skeleton avatar - circular placeholder
#[component]
pub fn SkeletonAvatar(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Size of the avatar (defaults to "2.5rem")
    #[prop(optional, into)]
    size: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let s = size.unwrap_or_else(|| "2.5rem".to_string());
    let full_class = format!("{} skeleton-avatar {}", variant.class(), class);

    view! {
        <div class=full_class style=format!("width: {}; height: {};", s, s) aria-hidden="true"></div>
    }
}

/// Skeleton button - button-shaped placeholder
#[component]
pub fn SkeletonButton(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Width of the button (defaults to "6rem")
    #[prop(optional, into)]
    width: Option<String>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let w = width.unwrap_or_else(|| "6rem".to_string());
    let full_class = format!("{} {}", variant.class(), class);

    view! {
        <div
            class=full_class
            style=format!("width: {}; height: 2.5rem;", w)
            aria-hidden="true"
        ></div>
    }
}

/// Skeleton detail section - simulates a detail/definition list layout
///
/// Useful for detail pages that show key-value pairs (e.g., adapter details,
/// dataset overview, etc.)
#[component]
pub fn SkeletonDetailSection(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Number of rows to render
    #[prop(default = 4)]
    rows: usize,
    /// Whether to include a title skeleton
    #[prop(optional)]
    has_title: bool,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("card skeleton-detail-section {}", class);
    let variant_class = variant.class();

    view! {
        <div class=full_class aria-hidden="true">
            {if has_title {
                Some(view! {
                    <div class="card-header">
                        <div
                            class=format!("{} skeleton-text", variant_class)
                            style="width: 40%; height: 1.25rem;"
                        ></div>
                    </div>
                })
            } else {
                None
            }}
            <div class="card-content" style="display: flex; flex-direction: column; gap: 0.75rem;">
                {(0..rows)
                    .map(|i| {
                        // Vary widths for visual interest
                        let label_width = match i % 3 {
                            0 => "25%",
                            1 => "30%",
                            _ => "20%",
                        };
                        let value_width = match i % 4 {
                            0 => "45%",
                            1 => "60%",
                            2 => "35%",
                            _ => "50%",
                        };
                        view! {
                            <div class="flex justify-between items-center py-1">
                                <div
                                    class=format!("{} skeleton-text", variant_class)
                                    style=format!("width: {};", label_width)
                                ></div>
                                <div
                                    class=format!("{} skeleton-text", variant_class)
                                    style=format!("width: {};", value_width)
                                ></div>
                            </div>
                        }
                    })
                    .collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Skeleton page header - simulates a page header with title and optional actions
#[component]
pub fn SkeletonPageHeader(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Whether to show action button skeletons
    #[prop(optional)]
    has_actions: bool,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("skeleton-page-header {}", class);
    let variant_class = variant.class();

    view! {
        <div class=format!("{} flex items-center justify-between mb-6", full_class) aria-hidden="true">
            <div>
                <div
                    class=format!("{} skeleton-text", variant_class)
                    style="width: 12rem; height: 2rem; margin-bottom: 0.5rem;"
                ></div>
                <div
                    class=format!("{} skeleton-text", variant_class)
                    style="width: 20rem; height: 1rem;"
                ></div>
            </div>
            {if has_actions {
                Some(view! {
                    <div class="flex gap-2">
                        <div
                            class=format!("{}", variant_class)
                            style="width: 5rem; height: 2.5rem; border-radius: 0.375rem;"
                        ></div>
                        <div
                            class=format!("{}", variant_class)
                            style="width: 6rem; height: 2.5rem; border-radius: 0.375rem;"
                        ></div>
                    </div>
                })
            } else {
                None
            }}
        </div>
    }
}

/// Skeleton stats grid - simulates a grid of stat cards
#[component]
pub fn SkeletonStatsGrid(
    /// Animation variant (shimmer or pulse)
    #[prop(optional)]
    variant: SkeletonVariant,
    /// Number of stat cards
    #[prop(default = 4)]
    count: usize,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let full_class = format!("grid gap-4 md:grid-cols-2 lg:grid-cols-4 {}", class);
    let variant_class = variant.class();

    view! {
        <div class=full_class aria-hidden="true">
            {(0..count)
                .map(|_| {
                    view! {
                        <div class="card p-4">
                            <div
                                class=format!("{} skeleton-text", variant_class)
                                style="width: 50%; height: 0.875rem; margin-bottom: 0.5rem;"
                            ></div>
                            <div
                                class=format!("{} skeleton-text", variant_class)
                                style="width: 40%; height: 2rem;"
                            ></div>
                        </div>
                    }
                })
                .collect::<Vec<_>>()}
        </div>
    }
}
