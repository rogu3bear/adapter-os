//! Error page subcomponents
//!
//! Reusable components for error monitoring UI.

use crate::components::{Card, DetailRow};
use adapteros_api_types::telemetry::ClientErrorItem;
use leptos::prelude::*;

/// Detail card for a single client error, using DetailRow for key-value display.
#[component]
pub fn ErrorDetailCard(
    /// The error to display
    error: ClientErrorItem,
) -> impl IntoView {
    let time_display = format_timestamp(&error.client_timestamp);
    let created_display = format_date_time(&error.created_at);
    let status_display = error
        .http_status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".to_string());
    let page_display = error.page.clone().unwrap_or_else(|| "—".to_string());
    let code_display = error.code.clone().unwrap_or_else(|| "—".to_string());
    let failure_code_display = error
        .failure_code
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let user_id_display = error.user_id.clone().unwrap_or_else(|| "—".to_string());

    view! {
        <Card>
            <div class="p-4 space-y-1">
                <h3 class="heading-4 mb-3">"Error Details"</h3>
                <DetailRow label="Time" value=time_display mono=true/>
                <DetailRow label="Created" value=created_display mono=true/>
                <DetailRow label="Type" value=error.error_type.clone()/>
                <DetailRow label="Message" value=error.message.clone() accent=true/>
                <DetailRow label="HTTP Status" value=status_display/>
                <DetailRow label="Page" value=page_display mono=true/>
                <DetailRow label="Code" value=code_display mono=true/>
                <DetailRow label="Failure Code" value=failure_code_display mono=true/>
                <DetailRow label="Error ID" value=error.id.clone() mono=true/>
                <DetailRow label="Tenant ID" value=error.tenant_id.clone() mono=true/>
                <DetailRow label="User ID" value=user_id_display mono=true/>
            </div>
        </Card>
    }
}

fn format_timestamp(ts: &str) -> String {
    if let Some(time_start) = ts.find('T') {
        let time_part = &ts[time_start + 1..];
        if let Some(end) = time_part.find('.').or_else(|| time_part.find('Z')) {
            return time_part[..end].to_string();
        }
        if time_part.len() >= 8 {
            return time_part[..8].to_string();
        }
    }
    ts.to_string()
}

fn format_date_time(ts: &str) -> String {
    if ts.len() >= 16 {
        format!("{} {}", &ts[0..10], &ts[11..16])
    } else {
        ts.to_string()
    }
}

/// Summary stat card for error analytics (label + count).
#[component]
pub fn ErrorSummaryCard(
    /// Label (e.g. "Total Errors (24h)")
    #[prop(into)]
    label: String,
    /// Display value (e.g. count or formatted string)
    #[prop(into)]
    value: String,
) -> impl IntoView {
    view! {
        <Card>
            <div class="p-4">
                <div class="text-sm font-medium text-muted-foreground">{label}</div>
                <div class="text-2xl font-bold mt-1">{value}</div>
            </div>
        </Card>
    }
}

/// Section displaying a grid of error summary cards.
#[component]
pub fn ErrorSummarySection(
    /// Cards as children (e.g. ErrorSummaryCard components)
    children: Children,
) -> impl IntoView {
    view! {
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
            {children()}
        </div>
    }
}
