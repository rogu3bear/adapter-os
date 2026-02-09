//! Reviews queue page
//!
//! Human-in-the-loop review queue management.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Card, Column, CopyableId, DataTable, Input, PageBreadcrumbItem,
    PageScaffold, PageScaffoldActions, RefreshButton, Select,
};
use crate::hooks::{use_api_resource, use_polling, use_navigate, LoadingState};
use adapteros_api_types::review::{PauseKind, PausedInferenceInfo};
use leptos::prelude::*;
use std::sync::Arc;

/// Reviews queue page (`/reviews`)
#[component]
pub fn Reviews() -> impl IntoView {
    // Fetch queue.
    let (queue, refetch) = use_api_resource(|client: Arc<ApiClient>| async move {
        let resp = client.list_paused_reviews().await?;
        Ok(resp.paused)
    });

    // Polling fallback: until a dedicated reviews SSE stream exists, polling keeps the queue fresh.
    let _cancel_polling = use_polling(10_000, move || async move {
        refetch.run(());
    });

    // Client-side filters.
    let kind_filter = RwSignal::new("all".to_string());
    let search = RwSignal::new(String::new());

    // Derive a filtered LoadingState<Vec<PausedInferenceInfo>> for DataTable.
    let (table_state, set_table_state) = signal::<LoadingState<Vec<PausedInferenceInfo>>>(LoadingState::Idle);
    Effect::new(move || {
        let raw = queue.get();
        let kind = kind_filter.get();
        let query = search.get();

        let next = match raw {
            LoadingState::Idle => LoadingState::Idle,
            LoadingState::Loading => LoadingState::Loading,
            LoadingState::Error(e) => LoadingState::Error(e),
            LoadingState::Loaded(mut items) => {
                let kind = kind.trim().to_string();
                let kind = if kind == "all" { None } else { parse_kind(&kind) };

                if let Some(kind) = kind {
                    items.retain(|i| i.kind == kind);
                }

                let q = query.trim().to_ascii_lowercase();
                if !q.is_empty() {
                    items.retain(|i| {
                        i.pause_id.to_ascii_lowercase().contains(&q)
                            || i.inference_id.to_ascii_lowercase().contains(&q)
                            || i.context_preview
                                .as_deref()
                                .unwrap_or("")
                                .to_ascii_lowercase()
                                .contains(&q)
                    });
                }

                // Show longest-waiting first.
                items.sort_by_key(|i| std::cmp::Reverse(i.duration_secs));

                LoadingState::Loaded(items)
            }
        };

        set_table_state.set(next);
    });

    // Counts (total from raw, filtered from table_state).
    let total = Signal::derive(move || match queue.get() {
        LoadingState::Loaded(items) => items.len(),
        _ => 0,
    });
    let filtered = Signal::derive(move || match table_state.get() {
        LoadingState::Loaded(items) => items.len(),
        _ => 0,
    });

    let kind_options = vec![
        ("all".to_string(), "All".to_string()),
        ("review_needed".to_string(), "Review Needed".to_string()),
        ("policy_approval".to_string(), "Policy Approval".to_string()),
        ("resource_wait".to_string(), "Resource Wait".to_string()),
        ("user_requested".to_string(), "User Requested".to_string()),
        ("threat_escalation".to_string(), "Threat Escalation".to_string()),
    ];

    let columns: Vec<Column<PausedInferenceInfo>> = vec![
        Column::custom("Pause", |row: &PausedInferenceInfo| {
            view! { <CopyableId id=row.pause_id.clone() truncate=18/> }
        })
        .with_class("w-[220px]".to_string()),
        Column::custom("Inference", |row: &PausedInferenceInfo| {
            view! { <CopyableId id=row.inference_id.clone() truncate=18/> }
        })
        .with_class("w-[220px]".to_string()),
        Column::custom("Kind", |row: &PausedInferenceInfo| {
            let (variant, label) = kind_badge(&row.kind);
            view! { <Badge variant=variant>{label}</Badge> }
        })
        .with_class("w-[150px]".to_string()),
        Column::custom("Waiting", |row: &PausedInferenceInfo| {
            view! { <span class="text-sm text-muted-foreground">{format_duration(row.duration_secs)}</span> }
        })
        .with_class("w-[120px]".to_string()),
        Column::custom("Preview", |row: &PausedInferenceInfo| {
            let text = row
                .context_preview
                .clone()
                .unwrap_or_else(|| "No preview".to_string());
            let title = text.clone();
            view! {
                <span class="text-sm text-muted-foreground truncate max-w-[420px]" title=title>
                    {text}
                </span>
            }
        }),
        Column::custom("", |row: &PausedInferenceInfo| {
            let href = format!("/reviews/{}", row.pause_id);
            view! {
                <div class="flex justify-end">
                    <a
                        href=href
                        class="btn btn-secondary btn-sm"
                        on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                    >
                        "Open"
                    </a>
                </div>
            }
        })
        .with_class("w-[90px] text-right".to_string()),
    ];

    // Row click navigates to detail.
    let navigate = use_navigate();
    let on_row_click = Callback::new(move |row: PausedInferenceInfo| {
        navigate(&format!("/reviews/{}", row.pause_id));
    });

    view! {
        <PageScaffold
            title="Reviews"
            subtitle="Items paused awaiting human input".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Operate", "/reviews"),
                PageBreadcrumbItem::current("Reviews"),
            ]
        >
            <PageScaffoldActions slot>
                <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
            </PageScaffoldActions>

            <Card class="space-y-4".to_string()>
                <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
                    <div class="flex items-center gap-2">
                        <Badge variant=BadgeVariant::Secondary>
                            {move || format!("{} total", total.get())}
                        </Badge>
                        <Badge variant=BadgeVariant::Secondary>
                            {move || format!("{} shown", filtered.get())}
                        </Badge>
                    </div>

                    <div class="grid gap-3 md:grid-cols-2 md:items-end">
                        <Select
                            value=kind_filter
                            options=kind_options
                            label="Kind"
                        />
                        <Input
                            value=search
                            label="Search"
                            placeholder="pause_id, inference_id, preview..."
                            input_type="text".to_string()
                        />
                    </div>
                </div>

                <div class="border-t border-border pt-4">
                    <DataTable
                        data=table_state
                        columns=columns
                        on_retry=refetch.as_callback()
                        empty_title="No reviews in queue"
                        empty_description="Paused inferences that require human review will appear here."
                        on_row_click=on_row_click
                        card=false
                        class="table-fixed".to_string()
                    />
                </div>
            </Card>
        </PageScaffold>
    }
}

fn parse_kind(kind: &str) -> Option<PauseKind> {
    match kind {
        "review_needed" => Some(PauseKind::ReviewNeeded),
        "policy_approval" => Some(PauseKind::PolicyApproval),
        "resource_wait" => Some(PauseKind::ResourceWait),
        "user_requested" => Some(PauseKind::UserRequested),
        "threat_escalation" => Some(PauseKind::ThreatEscalation),
        _ => None,
    }
}

fn kind_badge(kind: &PauseKind) -> (BadgeVariant, &'static str) {
    match kind {
        PauseKind::ReviewNeeded => (BadgeVariant::Warning, "Review Needed"),
        PauseKind::PolicyApproval => (BadgeVariant::Destructive, "Policy Approval"),
        PauseKind::ResourceWait => (BadgeVariant::Secondary, "Resource Wait"),
        PauseKind::UserRequested => (BadgeVariant::Default, "User Requested"),
        PauseKind::ThreatEscalation => (BadgeVariant::Destructive, "Threat Escalation"),
    }
}

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
