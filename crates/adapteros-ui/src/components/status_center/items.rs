//! Status item components
//!
//! Individual status item display with severity indicators.

use leptos::prelude::*;

/// Severity level for status items
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatusItemSeverity {
    /// Informational status
    #[default]
    Info,
    /// Success/healthy status
    Success,
    /// Warning status
    Warning,
    /// Error/critical status
    Error,
}

impl StatusItemSeverity {
    /// Get the CSS class for the indicator dot
    pub fn dot_class(&self) -> &'static str {
        match self {
            Self::Info => "status-item-dot-info",
            Self::Success => "status-item-dot-success",
            Self::Warning => "status-item-dot-warning",
            Self::Error => "status-item-dot-error",
        }
    }

    /// Get the CSS class for the value text
    pub fn text_class(&self) -> &'static str {
        match self {
            Self::Info => "status-item-text-info",
            Self::Success => "status-item-text-success",
            Self::Warning => "status-item-text-warning",
            Self::Error => "status-item-text-error",
        }
    }
}

/// Individual status item component
///
/// Displays a status line with:
/// - Colored indicator dot
/// - Label
/// - Value
/// - Optional detail text
#[component]
pub fn StatusItem(
    /// The label for this status item
    #[prop(into)]
    label: String,
    /// The value to display
    #[prop(into)]
    value: String,
    /// Severity level (determines color)
    #[prop(optional)]
    severity: StatusItemSeverity,
    /// Optional detail text (shown below value)
    #[prop(optional, into)]
    detail: String,
    /// Whether to show a pulsing indicator (for active/live status)
    #[prop(optional)]
    pulsing: bool,
) -> impl IntoView {
    let has_detail = !detail.is_empty();

    view! {
        <div class="status-item">
            <div class="status-item-main">
                // Indicator dot
                <span class="status-item-indicator">
                    {if pulsing {
                        view! {
                            <span class=format!("status-item-pulse {}", severity.dot_class())></span>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }}
                    <span class=format!("status-item-dot {}", severity.dot_class())></span>
                </span>

                // Label
                <span class="status-item-label">{label}</span>

                // Spacer
                <span class="status-item-spacer"></span>

                // Value
                <span class=format!("status-item-value {}", severity.text_class())>
                    {value}
                </span>
            </div>

            // Optional detail
            {if has_detail {
                view! {
                    <div class="status-item-detail">
                        {detail}
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </div>
    }
}

/// Status item for displaying availability/unavailable state
#[component]
pub fn StatusItemAvailability(
    /// The label for this status item
    #[prop(into)]
    label: String,
    /// Whether the item is available
    available: bool,
    /// Optional detail when unavailable
    #[prop(optional, into)]
    unavailable_reason: String,
) -> impl IntoView {
    let (value, severity) = if available {
        ("Available".to_string(), StatusItemSeverity::Success)
    } else {
        ("Unavailable".to_string(), StatusItemSeverity::Error)
    };

    let detail = if !available && !unavailable_reason.is_empty() {
        unavailable_reason
    } else {
        String::new()
    };

    view! {
        <StatusItem
            label=label
            value=value
            severity=severity
            detail=detail
        />
    }
}

/// Status item for displaying memory/percentage values
#[component]
pub fn StatusItemMemory(
    /// The label for this status item
    #[prop(into)]
    label: String,
    /// Used amount (e.g., MB)
    used: Option<u64>,
    /// Total amount (e.g., MB)
    total: Option<u64>,
    /// Unit suffix (default: "MB")
    #[prop(optional, into)]
    unit: String,
    /// Whether data is available
    #[prop(optional)]
    available: bool,
) -> impl IntoView {
    let unit = if unit.is_empty() {
        "MB".to_string()
    } else {
        unit
    };

    let (value, severity, detail): (String, StatusItemSeverity, String) =
        match (available, used, total) {
            (false, _, _) => (
                "Unavailable".to_string(),
                StatusItemSeverity::Warning,
                String::new(),
            ),
            (true, Some(u), Some(t)) => {
                let pct = if t > 0 {
                    (u as f64 / t as f64 * 100.0) as u64
                } else {
                    0
                };
                let sev = if pct > 90 {
                    StatusItemSeverity::Error
                } else if pct > 75 {
                    StatusItemSeverity::Warning
                } else {
                    StatusItemSeverity::Success
                };
                (
                    format!("{} / {} {}", u, t, unit),
                    sev,
                    format!("{}% used", pct),
                )
            }
            (true, Some(u), None) => (
                format!("{} {}", u, unit),
                StatusItemSeverity::Info,
                String::new(),
            ),
            (true, None, _) => ("N/A".to_string(), StatusItemSeverity::Info, String::new()),
        };

    view! {
        <StatusItem
            label=label
            value=value
            severity=severity
            detail=detail
        />
    }
}
