//! Reviews page
//!
//! Human-in-the-loop review queue management.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Dialog, EmptyState, EmptyStateVariant,
    ErrorDisplay, LoadingDisplay, PageScaffold, PageScaffoldActions, RefreshButton, Select, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow, Textarea,
};
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use adapteros_api_types::review::{
    PauseKind, PausedInferenceInfo, Review, ReviewAssessment, SubmitReviewRequest,
};
use leptos::prelude::*;
use std::sync::Arc;

/// Reviews queue page
#[component]
pub fn Reviews() -> impl IntoView {
    let (reviews, refetch) =
        use_api_resource(
            |client: Arc<ApiClient>| async move { client.list_paused_reviews().await },
        );

    // Selected review for detail view
    let selected_review: RwSignal<Option<PausedInferenceInfo>> = RwSignal::new(None);

    // Callback when a review row is clicked
    let on_select = Callback::new(move |info: PausedInferenceInfo| {
        selected_review.set(Some(info));
    });

    // Close detail dialog
    let on_close = Callback::new(move |_: ()| {
        selected_review.set(None);
    });

    // After submission, refresh and close
    let on_submit = Callback::new(move |_: ()| {
        selected_review.set(None);
        refetch.run(());
    });

    view! {
        <PageScaffold
            title="Human Review"
            subtitle="Human-in-the-loop review management".to_string()
        >
            <PageScaffoldActions slot>
                <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
            </PageScaffoldActions>

            {move || {
                match reviews.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <LoadingDisplay message="Loading reviews..."/> }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <ReviewsQueue
                                paused=data.paused
                                total=data.total
                                on_select=on_select
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch.run(()))
                            />
                        }.into_any()
                    }
                }
            }}

            // Review detail dialog
            {move || {
                selected_review.get().map(|review| {
                    view! {
                        <ReviewDetailDialog
                            review=review
                            on_close=on_close
                            on_submit=on_submit
                        />
                    }
                })
            }}
        </PageScaffold>
    }
}

/// Reviews queue component
#[component]
fn ReviewsQueue(
    paused: Vec<PausedInferenceInfo>,
    total: usize,
    on_select: Callback<PausedInferenceInfo>,
) -> impl IntoView {
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
                        <TableHead>"Action"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {paused
                        .into_iter()
                        .map(|info| {
                            view! { <ReviewRow info=info on_select=on_select /> }
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
fn ReviewRow(info: PausedInferenceInfo, on_select: Callback<PausedInferenceInfo>) -> impl IntoView {
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

    // Clone info for the click handler
    let info_for_click = info.clone();

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
            <TableCell>
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| {
                        on_select.run(info_for_click.clone());
                    })
                >
                    "Review"
                </Button>
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
        PauseKind::ThreatEscalation => (BadgeVariant::Destructive, "Threat Escalation"),
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

/// Review detail dialog component
///
/// Shows pause context, trigger kind, and submission form for
/// approving, rejecting, or requesting changes on a paused inference.
#[component]
fn ReviewDetailDialog(
    review: PausedInferenceInfo,
    on_close: Callback<()>,
    on_submit: Callback<()>,
) -> impl IntoView {
    let alive = use_scope_alive();

    // Form state
    let assessment = RwSignal::new("approved".to_string());
    let comment = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let submit_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Dialog open state
    let dialog_open = RwSignal::new(true);

    // Watch for dialog close via backdrop/escape
    Effect::new(move || {
        if !dialog_open.get() {
            on_close.run(());
        }
    });

    // Assessment options for the select
    let assessment_options = vec![
        ("approved".to_string(), "Approved".to_string()),
        (
            "approved_with_suggestions".to_string(),
            "Approved with Suggestions".to_string(),
        ),
        ("needs_changes".to_string(), "Needs Changes".to_string()),
        ("rejected".to_string(), "Rejected".to_string()),
        ("inconclusive".to_string(), "Inconclusive".to_string()),
    ];

    // Create the submit handler
    let pause_id = review.pause_id.clone();
    let alive_for_submit = alive.clone();
    let handle_submit = move |_| {
        submitting.set(true);
        submit_error.set(None);

        let pause_id = pause_id.clone();
        let assessment_value = assessment.get();
        let comment_value = comment.get();
        let on_submit = on_submit;
        let alive = alive_for_submit.clone();

        // Parse assessment
        let review_assessment = match assessment_value.as_str() {
            "approved" => ReviewAssessment::Approved,
            "approved_with_suggestions" => ReviewAssessment::ApprovedWithSuggestions,
            "needs_changes" => ReviewAssessment::NeedsChanges,
            "rejected" => ReviewAssessment::Rejected,
            "inconclusive" => ReviewAssessment::Inconclusive,
            _ => ReviewAssessment::Approved,
        };

        // Build the review request
        let request = SubmitReviewRequest {
            pause_id,
            review: Review {
                assessment: review_assessment,
                issues: vec![],
                suggestions: vec![],
                comments: if comment_value.is_empty() {
                    None
                } else {
                    Some(comment_value)
                },
                confidence: None,
            },
            reviewer: "human".to_string(),
        };

        // Submit via API
        let client = Arc::new(ApiClient::new());
        wasm_bindgen_futures::spawn_local(async move {
            match client.submit_review(&request).await {
                Ok(response) => {
                    if response.accepted {
                        if alive.load(std::sync::atomic::Ordering::SeqCst) {
                            on_submit.run(());
                        }
                    } else {
                        submit_error.set(Some(
                            response
                                .message
                                .unwrap_or_else(|| "Review was not accepted".to_string()),
                        ));
                        submitting.set(false);
                    }
                }
                Err(e) => {
                    submit_error.set(Some(format!("Failed to submit review: {}", e)));
                    submitting.set(false);
                }
            }
        });
    };

    // Quick action buttons
    let pause_id_approve = review.pause_id.clone();
    let alive_for_approve = alive.clone();
    let handle_approve = move |_| {
        submitting.set(true);
        submit_error.set(None);

        let pause_id = pause_id_approve.clone();
        let on_submit = on_submit;
        let alive = alive_for_approve.clone();
        let request = SubmitReviewRequest {
            pause_id,
            review: Review::approved(None),
            reviewer: "human".to_string(),
        };

        let client = Arc::new(ApiClient::new());
        wasm_bindgen_futures::spawn_local(async move {
            match client.submit_review(&request).await {
                Ok(response) => {
                    if response.accepted {
                        if alive.load(std::sync::atomic::Ordering::SeqCst) {
                            on_submit.run(());
                        }
                    } else {
                        submit_error.set(Some(
                            response
                                .message
                                .unwrap_or_else(|| "Review was not accepted".to_string()),
                        ));
                        submitting.set(false);
                    }
                }
                Err(e) => {
                    submit_error.set(Some(format!("Failed to submit review: {}", e)));
                    submitting.set(false);
                }
            }
        });
    };

    let pause_id_reject = review.pause_id.clone();
    let handle_reject = move |_| {
        submitting.set(true);
        submit_error.set(None);

        let pause_id = pause_id_reject.clone();
        let on_submit = on_submit;
        let alive = alive.clone();
        let request = SubmitReviewRequest {
            pause_id,
            review: Review {
                assessment: ReviewAssessment::Rejected,
                issues: vec![],
                suggestions: vec![],
                comments: None,
                confidence: None,
            },
            reviewer: "human".to_string(),
        };

        let client = Arc::new(ApiClient::new());
        wasm_bindgen_futures::spawn_local(async move {
            match client.submit_review(&request).await {
                Ok(response) => {
                    if response.accepted {
                        if alive.load(std::sync::atomic::Ordering::SeqCst) {
                            on_submit.run(());
                        }
                    } else {
                        submit_error.set(Some(
                            response
                                .message
                                .unwrap_or_else(|| "Review was not accepted".to_string()),
                        ));
                        submitting.set(false);
                    }
                }
                Err(e) => {
                    submit_error.set(Some(format!("Failed to submit review: {}", e)));
                    submitting.set(false);
                }
            }
        });
    };

    // Context info
    let kind_badge = pause_kind_badge(&review.kind);
    let duration = format_duration(review.duration_secs);
    let context_preview = review
        .context_preview
        .clone()
        .unwrap_or_else(|| "No context available".to_string());

    view! {
        <Dialog
            open=dialog_open
            title="Review Paused Inference"
            description="Review the context and submit your assessment."
        >
            <div class="space-y-6">
                // Context info section
                <div class="space-y-4">
                    <h3 class="text-sm font-medium text-foreground">"Pause Context"</h3>

                    // Metadata grid
                    <div class="grid grid-cols-2 gap-4 text-sm">
                        <div>
                            <span class="text-muted-foreground">"Pause ID: "</span>
                            <span class="font-mono">{review.pause_id.clone()}</span>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Inference ID: "</span>
                            <span class="font-mono">{review.inference_id.clone()}</span>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Trigger: "</span>
                            <Badge variant=kind_badge.0>
                                {kind_badge.1}
                            </Badge>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Duration: "</span>
                            <span>{duration}</span>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Paused At: "</span>
                            <span>{review.paused_at.clone()}</span>
                        </div>
                    </div>

                    // Context preview
                    <div class="rounded-md bg-muted p-3">
                        <p class="text-sm font-medium text-muted-foreground mb-1">"Content Preview"</p>
                        <p class="text-sm whitespace-pre-wrap">{context_preview}</p>
                    </div>
                </div>

                // Quick actions
                <div class="space-y-2">
                    <h3 class="text-sm font-medium text-foreground">"Quick Actions"</h3>
                    <div class="flex gap-2">
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(handle_approve)
                            disabled=submitting
                            loading=submitting
                        >
                            "Approve"
                        </Button>
                        <Button
                            variant=ButtonVariant::Destructive
                            on_click=Callback::new(handle_reject)
                            disabled=submitting
                        >
                            "Reject"
                        </Button>
                    </div>
                </div>

                // Detailed submission form
                <div class="space-y-4 pt-4 border-t">
                    <h3 class="text-sm font-medium text-foreground">"Detailed Review"</h3>

                    <Select
                        value=assessment
                        options=assessment_options
                        label="Assessment".to_string()
                    />

                    <Textarea
                        value=comment
                        label="Comments (optional)".to_string()
                        placeholder="Add any comments, suggestions, or issues..."
                        rows=4
                    />

                    // Error display
                    {move || submit_error.get().map(|err| {
                        view! {
                            <div class="text-sm text-destructive" role="alert">
                                {err}
                            </div>
                        }
                    })}

                    // Submit button
                    <div class="flex justify-end gap-2">
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(move |_| on_close.run(()))
                            disabled=submitting
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(handle_submit)
                            disabled=submitting
                            loading=submitting
                        >
                            "Submit Review"
                        </Button>
                    </div>
                </div>
            </div>
        </Dialog>
    }
}
