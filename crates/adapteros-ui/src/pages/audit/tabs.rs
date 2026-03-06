//! Audit page tab components
//!
//! Individual tab views for the audit event timeline.

use crate::api::{AuditLogEntry, AuditLogsResponse};
use crate::components::{
    Badge, BadgeVariant, Card, EmptyState, EmptyStateVariant, ErrorDisplay, SkeletonTable, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::LoadingState;
use crate::utils::{format_datetime, status_display_label};
use leptos::prelude::*;

/// Page size for client-side pagination (reduces initial DOM nodes)
const AUDIT_PAGE_SIZE: usize = 25;

// ============================================================================
// Timeline Tab
// ============================================================================

#[component]
pub fn TimelineTab(
    logs: ReadSignal<LoadingState<AuditLogsResponse>>,
    /// Optional retry callback for error state
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
) -> impl IntoView {
    // Client-side pagination to reduce DOM nodes
    let visible_count = RwSignal::new(AUDIT_PAGE_SIZE);

    view! {
        <Card>
            {move || {
                match logs.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <SkeletonTable rows=5 columns=6 />
                        }
                        .into_any()
                    }
                    LoadingState::Loaded(data) => {
                        if data.logs.is_empty() {
                            view! {
                                <EmptyState
                                    variant=EmptyStateVariant::Empty
                                    title="No audit events found"
                                    description="Audit events appear as actions are performed in the system."
                                />
                            }
                            .into_any()
                        } else {
                            let log_count = data.logs.len();
                            let total = data.total;
                            let logs_data = data.logs;

                            view! {
                                <div class="overflow-x-auto">
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>"Timestamp"</TableHead>
                                                <TableHead>"Action"</TableHead>
                                                <TableHead>"Resource"</TableHead>
                                                <TableHead>"Events"</TableHead>
                                                <TableHead>"User"</TableHead>
                                                <TableHead>"Status"</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {move || {
                                                let count = visible_count.get().min(log_count);
                                                logs_data
                                                    .iter()
                                                    .take(count)
                                                    .map(|entry| {
                                                        view! { <TimelineRow entry=entry.clone()/> }
                                                    })
                                                    .collect::<Vec<_>>()
                                            }}
                                        </TableBody>
                                    </Table>
                                </div>

                                // Show more button if there are hidden items
                                {move || {
                                    let count = visible_count.get();
                                    let remaining = log_count.saturating_sub(count);
                                    if remaining > 0 {
                                        view! {
                                            <div class="flex items-center justify-center py-4 border-t">
                                                <button
                                                    class="text-sm text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                                    on:click=move |_| {
                                                        visible_count.update(|c| *c = (*c + AUDIT_PAGE_SIZE).min(log_count));
                                                    }
                                                >
                                                    {format!("Show more ({} remaining)", remaining)}
                                                </button>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }
                                }}

                                <div class="flex items-center justify-between mt-4 pt-4 border-t">
                                    <p class="text-sm text-muted-foreground">
                                        {format!("Showing {} of {} events", visible_count.get().min(log_count), total)}
                                    </p>
                                </div>
                            }
                            .into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        let retry = on_retry;
                        match retry {
                            Some(callback) => {
                                view! { <ErrorDisplay error=e on_retry=callback/> }.into_any()
                            }
                            None => view! { <ErrorDisplay error=e /> }.into_any(),
                        }
                    }
                }
            }}
        </Card>
    }
}

#[component]
fn TimelineRow(entry: AuditLogEntry) -> impl IntoView {
    let status_variant = match entry.status.as_str() {
        "success" => BadgeVariant::Success,
        "failure" => BadgeVariant::Destructive,
        "pending" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    };
    let status_label = status_display_label(&entry.status);

    // Link to Run Detail whenever a resource_id is present (MVP correlation)
    let is_run_resource = entry.resource_id.is_some();
    let run_link = entry.resource_id.clone().map(|id| format!("/runs/{}", id));

    let event_labels = derive_event_labels(&entry, is_run_resource);

    view! {
        <TableRow>
            <TableCell>
                <div>
                    <p class="text-sm font-mono">{format_datetime(&entry.timestamp)}</p>
                    <p class="text-xs text-muted-foreground font-mono">{entry.id.clone()}</p>
                </div>
            </TableCell>
            <TableCell>
                <Badge variant=BadgeVariant::Outline>{entry.action.clone()}</Badge>
            </TableCell>
            <TableCell>
                <div>
                    <p class="text-sm">{entry.resource_type.clone()}</p>
                    {match (entry.resource_id.clone(), run_link.clone()) {
                        (Some(id), Some(link)) => {
                            view! {
                                <a
                                    href=link
                                    class="text-xs text-primary hover:underline font-mono"
                                    title="View Run Detail"
                                >
                                    {id}
                                </a>
                            }.into_any()
                        }
                        (Some(id), None) => {
                            view! { <p class="text-xs text-muted-foreground font-mono">{id}</p> }.into_any()
                        }
                        _ => view! { <span></span> }.into_any()
                    }}
                </div>
            </TableCell>
            <TableCell>
                <div class="space-y-1">
                    {event_labels.into_iter().map(|label| {
                        view! {
                            <span class="text-xs text-muted-foreground">{label}</span>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </TableCell>
            <TableCell>
                <div>
                    <p class="text-sm">{entry.user_id.clone()}</p>
                    <p class="text-xs text-muted-foreground">{entry.user_role.clone()}</p>
                </div>
            </TableCell>
            <TableCell>
                <span title=entry.status.clone()>
                    <Badge variant=status_variant>{status_label}</Badge>
                </span>
            </TableCell>
        </TableRow>
    }
}

fn derive_event_labels(entry: &AuditLogEntry, is_run_resource: bool) -> Vec<String> {
    let mut labels = Vec::new();
    let action = entry.action.to_lowercase();

    let metadata = entry
        .metadata_json
        .as_ref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());

    // Run created
    if action.contains("run") && (action.contains("create") || action.contains("start")) {
        labels.push("Run created".to_string());
    }

    // Adapter selected
    if action.contains("adapter")
        || metadata
            .as_ref()
            .and_then(|v| v.get("adapter_id"))
            .is_some()
    {
        labels.push("Adapter selected".to_string());
    }

    // Cache hit/miss
    if action.contains("cache") {
        if action.contains("hit") {
            labels.push("Cache hit".to_string());
        } else if action.contains("miss") {
            labels.push("Cache miss".to_string());
        } else {
            labels.push("Cache event".to_string());
        }
    } else if let Some(value) = metadata.as_ref().and_then(|v| v.get("cache_hit")) {
        if value.as_bool() == Some(true) {
            labels.push("Cache hit".to_string());
        } else if value.as_bool() == Some(false) {
            labels.push("Cache miss".to_string());
        }
    }

    // Verification result
    if action.contains("verify")
        || action.contains("receipt")
        || metadata.as_ref().and_then(|v| v.get("verified")).is_some()
    {
        labels.push("Verification result".to_string());
    }

    if labels.is_empty() && is_run_resource {
        labels.push("Run created: Unknown".to_string());
        labels.push("Adapter selected: Unknown".to_string());
        labels.push("Cache: Unknown".to_string());
        labels.push("Verification: Unknown".to_string());
    }

    if labels.is_empty() {
        labels.push("Event details unavailable".to_string());
    }

    labels
}
