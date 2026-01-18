//! Reviews page
//!
//! Human-in-the-loop review queue management.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Card, EmptyState, EmptyStateVariant, ErrorDisplay, LoadingDisplay,
    PageHeader, RefreshButton, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::review::{PauseKind, PausedInferenceInfo};
use leptos::prelude::*;
use std::sync::Arc;

/// Reviews queue page
#[component]
pub fn Reviews() -> impl IntoView {
    let (reviews, refetch) = use_api_resource(|client: Arc<ApiClient>| async move {
        client.list_paused_reviews().await
    });

    let refetch_trigger = RwSignal::new(0u32);

    // Store refetch in a StoredValue for sharing
    let refetch_stored = StoredValue::new(refetch);

    // Call refetch when trigger changes
    Effect::new(move |_| {
        let _ = refetch_trigger.get();
        refetch_stored.with_value(|f| f());
    });

    let trigger_refresh = move || {
        refetch_trigger.update(|n| *n = n.wrapping_add(1));
    };

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Reviews Queue"
                subtitle="Human-in-the-loop review management"
            >
                <RefreshButton on_click=Callback::new(move |_| trigger_refresh())/>
            </PageHeader>

            {move || {
                match reviews.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading reviews..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! { <ReviewsQueue paused=data.paused total=data.total /> }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh())
                            />
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Reviews queue component
#[component]
fn ReviewsQueue(paused: Vec<PausedInferenceInfo>, total: usize) -> impl IntoView {
    if paused.is_empty() {
        return view! {
            <Card>
                <EmptyState
                    variant=EmptyStateVariant::Empty
                    title="No pending reviews"
                    description="Items requiring human review will appear here."
                />
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <div class="p-4 border-b">
                <p class="text-sm text-muted-foreground">
                    {format!("{} item{} awaiting review", total, if total == 1 { "" } else { "s" })}
                </p>
            </div>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Pause ID"</TableHead>
                        <TableHead>"Inference ID"</TableHead>
                        <TableHead>"Type"</TableHead>
                        <TableHead>"Duration"</TableHead>
                        <TableHead>"Preview"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {paused
                        .into_iter()
                        .map(|info| {
                            view! { <ReviewRow info=info /> }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Individual review row component
#[component]
fn ReviewRow(info: PausedInferenceInfo) -> impl IntoView {
    let pause_id = info.pause_id.clone();
    let pause_id_short = if pause_id.len() > 12 {
        format!("{}...", &pause_id[..12])
    } else {
        pause_id.clone()
    };

    let inference_id = info.inference_id.clone();
    let inference_id_short = if inference_id.len() > 12 {
        format!("{}...", &inference_id[..12])
    } else {
        inference_id.clone()
    };

    let kind_badge = pause_kind_badge(&info.kind);
    let duration = format_duration(info.duration_secs);
    let preview = info
        .context_preview
        .clone()
        .unwrap_or_else(|| "No preview available".to_string());
    let preview_title = preview.clone();

    view! {
        <TableRow>
            <TableCell>
                <span class="font-mono text-sm" title=pause_id>
                    {pause_id_short}
                </span>
            </TableCell>
            <TableCell>
                <span class="font-mono text-sm" title=inference_id>
                    {inference_id_short}
                </span>
            </TableCell>
            <TableCell>
                <Badge variant=kind_badge.0>
                    {kind_badge.1}
                </Badge>
            </TableCell>
            <TableCell>
                <span class="text-sm text-muted-foreground">{duration}</span>
            </TableCell>
            <TableCell>
                <p class="text-sm text-muted-foreground truncate max-w-xs" title=preview_title>
                    {preview}
                </p>
            </TableCell>
        </TableRow>
    }
}

/// Get badge variant and label for pause kind
fn pause_kind_badge(kind: &PauseKind) -> (BadgeVariant, &'static str) {
    match kind {
        PauseKind::ReviewNeeded => (BadgeVariant::Warning, "Review Needed"),
        PauseKind::PolicyApproval => (BadgeVariant::Destructive, "Policy Approval"),
        PauseKind::ResourceWait => (BadgeVariant::Secondary, "Resource Wait"),
        PauseKind::UserRequested => (BadgeVariant::Default, "User Requested"),
    }
}

/// Format duration in human-readable form
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        if remaining_secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, remaining_secs)
        }
    } else {
        let hours = secs / 3600;
        let remaining_mins = (secs % 3600) / 60;
        if remaining_mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, remaining_mins)
        }
    }
}
