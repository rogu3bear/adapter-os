//! Status indicator components
//!
//! Includes badges, status indicators, and backend status displays.

use leptos::prelude::*;

/// Badge variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Success,
    Warning,
    Destructive,
    Outline,
}

impl BadgeVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Default => "badge-default",
            Self::Secondary => "badge-secondary",
            Self::Success => "badge-success",
            Self::Warning => "badge-warning",
            Self::Destructive => "badge-destructive",
            Self::Outline => "badge-outline",
        }
    }
}

/// Badge component
#[component]
pub fn Badge(
    #[prop(optional)] variant: BadgeVariant,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let full_class = format!("badge {} {}", variant.class(), class);

    view! {
        <span class=full_class>
            {children()}
        </span>
    }
}

/// Status indicator dot
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum StatusColor {
    #[default]
    Gray,
    Green,
    Yellow,
    Red,
    Blue,
}

impl StatusColor {
    fn dot_class(&self) -> &'static str {
        match self {
            Self::Gray => "bg-muted-foreground",
            Self::Green => "bg-status-success",
            Self::Yellow => "bg-status-warning",
            Self::Red => "bg-status-error",
            Self::Blue => "bg-status-info",
        }
    }

    fn pulse_class(&self) -> &'static str {
        match self {
            Self::Gray => "bg-muted",
            Self::Green => "bg-status-success/80",
            Self::Yellow => "bg-status-warning/80",
            Self::Red => "bg-status-error/80",
            Self::Blue => "bg-status-info/80",
        }
    }
}

/// Status indicator with pulsing dot
#[component]
pub fn StatusIndicator(
    #[prop(optional)] color: StatusColor,
    #[prop(optional)] pulsing: bool,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    // Generate aria-label based on color if no text label is provided
    let status_label = move || {
        let status_text = match color {
            StatusColor::Gray => "Unknown",
            StatusColor::Green => "Active",
            StatusColor::Yellow => "Warning",
            StatusColor::Red => "Error",
            StatusColor::Blue => "Info",
        };
        format!("Status: {}", status_text)
    };

    // Clone label for use in closure
    let label_for_check = label.clone();

    view! {
        <div class="flex items-center gap-2" role="status">
            <span class="relative flex h-3 w-3">
                {move || {
                    if pulsing {
                        view! {
                            <span
                                class=format!("animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 {}", color.pulse_class())
                                aria-hidden="true"
                            ></span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
                {move || {
                    if label_for_check.is_some() {
                        // Dot is decorative when label text is present
                        view! {
                            <span
                                class=format!("relative inline-flex rounded-full h-3 w-3 {}", color.dot_class())
                                aria-hidden="true"
                            ></span>
                        }.into_any()
                    } else {
                        // Dot conveys meaning when no label text
                        view! {
                            <span
                                class=format!("relative inline-flex rounded-full h-3 w-3 {}", color.dot_class())
                                aria-label=status_label()
                            ></span>
                        }.into_any()
                    }
                }}
            </span>
            {label.map(|l| view! {
                <span class="text-sm text-muted-foreground">{l}</span>
            })}
        </div>
    }
}

// =============================================================================
// BackendStatusBadge - Display backend selection status with downgrade info
// =============================================================================

/// Backend status for display purposes
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BackendStatus {
    /// Backend is operating normally
    #[default]
    Normal,
    /// Backend was downgraded from requested
    Downgraded,
    /// Backend is unavailable
    Unavailable,
    /// Backend status is unknown/loading
    Unknown,
}

impl BackendStatus {
    fn badge_variant(&self) -> BadgeVariant {
        match self {
            Self::Normal => BadgeVariant::Success,
            Self::Downgraded => BadgeVariant::Warning,
            Self::Unavailable => BadgeVariant::Destructive,
            Self::Unknown => BadgeVariant::Secondary,
        }
    }

    fn status_color(&self) -> StatusColor {
        match self {
            Self::Normal => StatusColor::Green,
            Self::Downgraded => StatusColor::Yellow,
            Self::Unavailable => StatusColor::Red,
            Self::Unknown => StatusColor::Gray,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Normal => "Active",
            Self::Downgraded => "Downgraded",
            Self::Unavailable => "Unavailable",
            Self::Unknown => "Unknown",
        }
    }
}

/// Backend status badge component
///
/// Displays the current backend status with optional downgrade information.
/// Used in dashboards and system status pages to show inference backend health.
///
/// # Props
/// - `backend_name`: The name of the backend (e.g., "Metal", "CoreML", "MLX")
/// - `status`: Current backend status
/// - `requested_backend`: Optional - the originally requested backend (for downgrade display)
/// - `downgrade_reason`: Optional - reason for downgrade
/// - `show_details`: Whether to show expanded details (default: false)
#[component]
pub fn BackendStatusBadge(
    /// Name of the current backend
    #[prop(into)]
    backend_name: String,
    /// Current status
    #[prop(optional)]
    status: BackendStatus,
    /// Originally requested backend (if different)
    #[prop(optional, into)]
    requested_backend: Option<String>,
    /// Reason for downgrade
    #[prop(optional, into)]
    downgrade_reason: Option<String>,
    /// Show expanded details
    #[prop(optional)]
    show_details: bool,
) -> impl IntoView {
    let is_downgraded = requested_backend.is_some()
        && requested_backend.as_ref() != Some(&backend_name)
        && status == BackendStatus::Downgraded;

    view! {
        <div class="inline-flex flex-col gap-1">
            // Main badge with status
            <div class="inline-flex items-center gap-2">
                <StatusIndicator
                    color=status.status_color()
                    pulsing=matches!(status, BackendStatus::Downgraded)
                />
                <Badge variant=status.badge_variant()>
                    {backend_name.clone()}
                </Badge>
                {move || {
                    if status != BackendStatus::Normal {
                        view! {
                            <span class="text-xs text-muted-foreground">
                                {format!("({})", status.label())}
                            </span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
            </div>

            // Downgrade details (if applicable and show_details is true)
            {move || {
                if show_details && is_downgraded {
                    view! {
                        <div class="text-xs text-muted-foreground pl-5 space-y-1">
                            {requested_backend.clone().map(|req| view! {
                                <div class="flex items-center gap-1">
                                    <span class="text-warning">"Requested:"</span>
                                    <span>{req}</span>
                                </div>
                            })}
                            {downgrade_reason.clone().map(|reason| view! {
                                <div class="flex items-center gap-1">
                                    <span class="text-warning">"Reason:"</span>
                                    <span>{reason}</span>
                                </div>
                            })}
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

/// Compact backend status indicator for use in tables/lists
#[component]
pub fn BackendStatusIndicator(
    /// Name of the current backend
    #[prop(into)]
    backend_name: String,
    /// Whether this is a downgraded backend
    #[prop(optional)]
    is_downgraded: bool,
) -> impl IntoView {
    let status = if is_downgraded {
        BackendStatus::Downgraded
    } else {
        BackendStatus::Normal
    };

    view! {
        <div class="inline-flex items-center gap-1.5">
            <span
                class=format!(
                    "inline-block w-2 h-2 rounded-full {}",
                    status.status_color().dot_class()
                )
                aria-hidden="true"
            ></span>
            <span class="text-sm">{backend_name}</span>
            {move || {
                if is_downgraded {
                    view! {
                        <span class="text-xs text-warning">"(fallback)"</span>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}
