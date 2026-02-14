//! Review detail page
//!
//! Displays full pause context and provides a structured form to submit
//! a human review (assessment, issues, suggestions, comments, confidence).

use crate::api::{report_error_with_toast, ApiClient};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ErrorDisplay, FormField, Input,
    LoadingDisplay, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, RefreshButton, Select,
    Textarea,
};
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use adapteros_api_types::review::{
    InferenceState, InferenceStateResponse, IssueSeverity, PauseKind, Review, ReviewAssessment,
    ReviewIssue, ReviewScope, SubmitReviewRequest,
};
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn ReviewDetail() -> impl IntoView {
    let params = leptos_router::hooks::use_params_map();
    let pause_id = move || params.with(|p| p.get("pause_id").unwrap_or_default());

    let (pause_state, refetch) = use_api_resource({
        move |client: Arc<ApiClient>| {
            let id = pause_id();
            async move { client.get_pause_details(&id).await }
        }
    });

    // Derive breadcrumb label: show pause kind once loaded, fall back to ID
    let breadcrumb_label =
        Signal::derive(
            move || match pause_state.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(ref data) => match &data.state {
                    InferenceState::Paused(reason) => match &reason.kind {
                        PauseKind::ReviewNeeded => "Review Needed".to_string(),
                        PauseKind::PolicyApproval => "Policy Approval".to_string(),
                        PauseKind::ResourceWait => "Resource Wait".to_string(),
                        PauseKind::UserRequested => "User Requested".to_string(),
                        PauseKind::ThreatEscalation => "Threat Escalation".to_string(),
                    },
                    _ => format!("{:?}", data.state),
                },
                _ => pause_id(),
            },
        );

    view! {
        <PageScaffold
            title="Review Detail"
            subtitle="Inspect pause context and submit a review".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Govern", "/reviews"),
                PageBreadcrumbItem::new("Reviews", "/reviews"),
                PageBreadcrumbItem::current(breadcrumb_label.try_get().unwrap_or_default()),
            ]
        >
            <PageScaffoldActions slot>
                <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
            </PageScaffoldActions>

            {move || match pause_state.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Idle | LoadingState::Loading => {
                    view! { <LoadingDisplay message="Loading pause details..."/> }.into_any()
                }
                LoadingState::Error(e) if e.is_not_found() => {
                    view! {
                        <div class="flex min-h-[40vh] flex-col items-center justify-center px-4">
                            <Card class="p-8 max-w-md w-full text-center">
                                <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                <h2 class="heading-3 mb-2">"Review not found"</h2>
                                <p class="text-muted-foreground mb-6">
                                    "This paused inference may have been resumed or doesn\u{2019}t exist."
                                </p>
                                <a href="/reviews" class="btn btn-primary btn-md">
                                    "View all reviews"
                                </a>
                            </Card>
                        </div>
                    }.into_any()
                }
                LoadingState::Error(e) => {
                    view! { <ErrorDisplay error=e on_retry=refetch.as_callback()/> }.into_any()
                }
                LoadingState::Loaded(data) => {
                    view! { <ReviewDetailBody pause=data/> }.into_any()
                }
            }}
        </PageScaffold>
    }
}

#[component]
fn ReviewDetailBody(pause: InferenceStateResponse) -> impl IntoView {
    let InferenceState::Paused(reason) = pause.state.clone() else {
        return view! {
            <Card title="Not Paused".to_string() description="This inference is not currently paused for review.">
                <div class="text-sm text-muted-foreground">
                    "State: "
                    <span class="font-mono">{format!("{:?}", pause.state)}</span>
                </div>
            </Card>
        }
        .into_any();
    };

    let alive = use_scope_alive();

    // Form state
    let reviewer = RwSignal::new("human".to_string());
    let assessment = RwSignal::new("approved".to_string());
    let comments = RwSignal::new(String::new());
    let confidence = RwSignal::new(String::new());

    let issues: RwSignal<Vec<IssueEditor>> = RwSignal::new(Vec::new());
    let suggestions: RwSignal<Vec<SuggestionEditor>> = RwSignal::new(Vec::new());

    let submitting = RwSignal::new(false);
    let submit_error: RwSignal<Option<String>> = RwSignal::new(None);
    let submit_ok = RwSignal::new(false);

    let add_issue = Callback::new(move |_: ()| {
        issues.update(|list| list.push(IssueEditor::new()));
    });
    let add_suggestion = Callback::new(move |_: ()| {
        suggestions.update(|list| list.push(SuggestionEditor::new()));
    });

    // Reason/context info
    let kind_badge = pause_kind_badge(&reason.kind);
    let duration = pause.paused_duration_secs.unwrap_or(0);

    // Select options
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

    let severity_options = vec![
        ("info".to_string(), "Info".to_string()),
        ("low".to_string(), "Low".to_string()),
        ("medium".to_string(), "Medium".to_string()),
        ("high".to_string(), "High".to_string()),
        ("critical".to_string(), "Critical".to_string()),
    ];

    let scope_options = vec![
        ("logic".to_string(), "Logic".to_string()),
        ("edge_cases".to_string(), "Edge Cases".to_string()),
        ("security".to_string(), "Security".to_string()),
        ("performance".to_string(), "Performance".to_string()),
        ("style".to_string(), "Style".to_string()),
        ("api_design".to_string(), "API Design".to_string()),
        ("testing".to_string(), "Testing".to_string()),
        ("documentation".to_string(), "Documentation".to_string()),
    ];

    let pause_id_for_submit = reason.pause_id.clone();
    let handle_submit = Callback::new(move |_: ()| {
        submitting.set(true);
        submit_error.set(None);
        submit_ok.set(false);

        let review = Review {
            assessment: parse_assessment(&assessment.try_get().unwrap_or_default()),
            issues: issues
                .try_get()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|e| e.into_review_issue())
                .collect(),
            suggestions: suggestions
                .try_get()
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.text.try_get().unwrap_or_default().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            comments: {
                let c = comments.try_get().unwrap_or_default().trim().to_string();
                if c.is_empty() {
                    None
                } else {
                    Some(c)
                }
            },
            confidence: parse_confidence(&confidence.try_get().unwrap_or_default()),
        };

        let request = SubmitReviewRequest {
            pause_id: pause_id_for_submit.clone(),
            review,
            reviewer: reviewer.try_get().unwrap_or_default().trim().to_string(),
        };

        let client = Arc::new(ApiClient::new());
        let alive = alive.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match client.submit_review(&request).await {
                Ok(resp) if resp.accepted => {
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        submit_ok.set(true);
                        submitting.set(false);
                    }
                }
                Ok(resp) => {
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        submit_error.set(Some(
                            resp.message
                                .unwrap_or_else(|| "Review was not accepted".to_string()),
                        ));
                        submitting.set(false);
                    }
                }
                Err(e) => {
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        report_error_with_toast(
                            &e,
                            "Failed to submit review",
                            Some("/reviews"),
                            true,
                        );
                        submit_error.set(Some(format!("Failed to submit review: {}", e)));
                        submitting.set(false);
                    }
                }
            }
        });
    });

    view! {
        <div class="space-y-6">
            <Card title="Pause Context".to_string() description="This inference is currently paused awaiting human input.">
                <div class="grid grid-cols-2 gap-4 text-sm">
                    <div>
                        <span class="text-muted-foreground">"Pause ID: "</span>
                        <span class="font-mono">{reason.pause_id.clone()}</span>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Inference ID: "</span>
                        <a
                            href=format!("/runs/{}", pause.inference_id)
                            class="link link-default font-mono"
                            title="View run details"
                        >
                            {pause.inference_id.clone()}
                        </a>
                    </div>
                    <div class="flex items-center gap-2">
                        <span class="text-muted-foreground">"Kind: "</span>
                        <Badge variant=kind_badge.0>{kind_badge.1}</Badge>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Paused At: "</span>
                        <span class="font-mono">{pause.paused_at.clone().unwrap_or_else(|| "unknown".to_string())}</span>
                    </div>
                    <div>
                        <span class="text-muted-foreground">"Duration: "</span>
                        <span>{format_duration(duration)}</span>
                    </div>
                </div>

                <div class="mt-4 grid gap-4">
                    {reason.context.question.clone().map(|q| view! {
                        <div class="rounded-md bg-muted p-3">
                            <p class="text-sm font-medium text-muted-foreground mb-1">"Question"</p>
                            <p class="text-sm whitespace-pre-wrap">{q}</p>
                        </div>
                    })}

                    {reason.context.code.clone().map(|code| view! {
                        <div class="rounded-md bg-muted p-3">
                            <p class="text-sm font-medium text-muted-foreground mb-1">"Text / Code So Far"</p>
                            <pre class="text-xs whitespace-pre-wrap font-mono">{code}</pre>
                        </div>
                    })}

                    {(!reason.context.scope.is_empty()).then(|| {
                        let scopes = reason.context.scope.clone().into_iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", ");
                        view! {
                            <div class="text-sm">
                                <span class="text-muted-foreground">"Scope: "</span>
                                <span class="font-mono">{scopes}</span>
                            </div>
                        }
                    })}

                    {reason.context.metadata.clone().map(|m| {
                        let pretty = serde_json::to_string_pretty(&m).unwrap_or_else(|_| "{}".to_string());
                        view! {
                            <div class="rounded-md bg-muted p-3">
                                <p class="text-sm font-medium text-muted-foreground mb-1">"Metadata"</p>
                                <pre class="text-xs whitespace-pre-wrap font-mono">{pretty}</pre>
                            </div>
                        }
                    })}
                </div>
            </Card>

            <Card title="Submit Review".to_string() description="This is currently a gate-only mechanism: the control plane forwards the review to the worker to resume inference. The review itself is not persisted.">
                <div class="grid gap-4">
                    <FormField label="Reviewer" name="reviewer">
                        <Input
                            value=reviewer
                            placeholder="human"
                            input_type="text".to_string()
                        />
                    </FormField>

                    <FormField label="Assessment" name="assessment">
                        <Select
                            value=assessment
                            options=assessment_options
                        />
                    </FormField>

                    <FormField label="Comments (optional)" name="comments">
                        <Textarea
                            value=comments
                            placeholder="Write any additional context, rationale, or next steps..."
                            rows=4
                        />
                    </FormField>

                    <FormField label="Confidence (0.0 - 1.0, optional)" name="confidence">
                        <Input
                            value=confidence
                            placeholder="e.g. 0.85"
                            input_type="number".to_string()
                            class="max-w-xs".to_string()
                        />
                    </FormField>

                    <div class="pt-2 border-t">
                        <div class="flex items-center justify-between gap-2">
                            <div class="space-y-0.5">
                                <p class="text-sm font-medium text-foreground">"Issues"</p>
                                <p class="text-xs text-muted-foreground">"Add structured issues (severity, category, description)."</p>
                            </div>
                            <Button
                                variant=ButtonVariant::Secondary
                                size=ButtonSize::Sm
                                on_click=Callback::new(move |_| add_issue.run(()))
                            >
                                "Add Issue"
                            </Button>
                        </div>

                        <div class="mt-3 grid gap-3">
                            {
                                let severity_options = severity_options.clone();
                                let scope_options = scope_options.clone();
                                move || {
                                    issues.try_get().unwrap_or_default().into_iter().map(|issue| {
                                        let id = issue.id.clone();
                                        let remove = Callback::new(move |_: ()| {
                                            issues.update(|list| list.retain(|i| i.id != id));
                                        });
                                        let sev_opts = severity_options.clone();
                                        let cat_opts = scope_options.clone();
                                        view! {
                                            <div class="rounded-lg border border-border bg-card p-3 space-y-3">
                                                <div class="flex items-center justify-between gap-2">
                                                    <p class="text-xs text-muted-foreground font-mono">{format!("issue:{}", issue.id)}</p>
                                                    <Button
                                                        variant=ButtonVariant::Ghost
                                                        size=ButtonSize::IconSm
                                                        aria_label="Remove issue".to_string()
                                                        on_click=Callback::new(move |_| remove.run(()))
                                                    >
                                                        <crate::components::IconTrash/>
                                                    </Button>
                                                </div>

                                                <div class="grid grid-cols-2 gap-3">
                                                    <FormField label="Severity" name=format!("issue_{}_severity", issue.id)>
                                                        <Select value=issue.severity options=sev_opts/>
                                                    </FormField>
                                                    <FormField label="Category" name=format!("issue_{}_category", issue.id)>
                                                        <Select value=issue.category options=cat_opts/>
                                                    </FormField>
                                                </div>

                                                <FormField label="Description" name=format!("issue_{}_description", issue.id)>
                                                    <Textarea
                                                        value=issue.description
                                                        placeholder="Describe the issue clearly."
                                                        rows=3
                                                    />
                                                </FormField>

                                                <div class="grid grid-cols-2 gap-3">
                                                    <FormField label="Location (optional)" name=format!("issue_{}_location", issue.id)>
                                                        <Input
                                                            value=issue.location
                                                            placeholder="e.g. crates/foo/src/lib.rs:42"
                                                            input_type="text".to_string()
                                                        />
                                                    </FormField>
                                                    <FormField label="Suggested Fix (optional)" name=format!("issue_{}_suggested_fix", issue.id)>
                                                        <Input
                                                            value=issue.suggested_fix
                                                            placeholder="e.g. Add bounds check before indexing"
                                                            input_type="text".to_string()
                                                        />
                                                    </FormField>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()
                                }
                            }
                        </div>
                    </div>

                    <div class="pt-2 border-t">
                        <div class="flex items-center justify-between gap-2">
                            <div class="space-y-0.5">
                                <p class="text-sm font-medium text-foreground">"Suggestions"</p>
                                <p class="text-xs text-muted-foreground">"Optional improvements that don’t necessarily block approval."</p>
                            </div>
                            <Button
                                variant=ButtonVariant::Secondary
                                size=ButtonSize::Sm
                                on_click=Callback::new(move |_| add_suggestion.run(()))
                            >
                                "Add Suggestion"
                            </Button>
                        </div>

                        <div class="mt-3 grid gap-3">
                            {move || {
                                suggestions.try_get().unwrap_or_default().into_iter().map(|s| {
                                    let id = s.id.clone();
                                    let remove = Callback::new(move |_: ()| {
                                        suggestions.update(|list| list.retain(|x| x.id != id));
                                    });
                                    view! {
                                        <div class="rounded-lg border border-border bg-card p-3 space-y-2">
                                            <div class="flex items-center justify-between gap-2">
                                                <p class="text-xs text-muted-foreground font-mono">{format!("suggestion:{}", s.id)}</p>
                                                <Button
                                                    variant=ButtonVariant::Ghost
                                                    size=ButtonSize::IconSm
                                                    aria_label="Remove suggestion".to_string()
                                                    on_click=Callback::new(move |_| remove.run(()))
                                                >
                                                    <crate::components::IconTrash/>
                                                </Button>
                                            </div>
                                            <FormField label="Suggestion" name=format!("suggestion_{}", s.id)>
                                                <Input
                                                    value=s.text
                                                    placeholder="Add a concise suggestion..."
                                                    input_type="text".to_string()
                                                />
                                            </FormField>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()
                            }}
                        </div>
                    </div>

                    {move || submit_error.try_get().flatten().map(|e| view! {
                        <div class="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
                            {e}
                        </div>
                    })}

                    {move || submit_ok.try_get().unwrap_or(false).then(|| view! {
                        <div class="rounded-md border border-success/30 bg-success/10 p-3 text-sm text-success">
                            "Review submitted. If the worker is still connected, inference should resume automatically."
                            <div class="mt-2">
                                <a href="/reviews" class="text-sm font-medium text-primary hover:underline">"Back to queue"</a>
                            </div>
                        </div>
                    })}

                    <div class="flex items-center justify-end gap-2 pt-2">
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| handle_submit.run(()))
                            disabled=submitting
                            loading=submitting
                        >
                            "Submit Review"
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
    }
    .into_any()
}

#[derive(Clone)]
struct IssueEditor {
    id: String,
    severity: RwSignal<String>,
    category: RwSignal<String>,
    description: RwSignal<String>,
    location: RwSignal<String>,
    suggested_fix: RwSignal<String>,
}

impl IssueEditor {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            severity: RwSignal::new("medium".to_string()),
            category: RwSignal::new("logic".to_string()),
            description: RwSignal::new(String::new()),
            location: RwSignal::new(String::new()),
            suggested_fix: RwSignal::new(String::new()),
        }
    }

    fn into_review_issue(self) -> Option<ReviewIssue> {
        let description = self
            .description
            .try_get()
            .unwrap_or_default()
            .trim()
            .to_string();
        if description.is_empty() {
            return None;
        }

        let location = self
            .location
            .try_get()
            .unwrap_or_default()
            .trim()
            .to_string();
        let suggested_fix = self
            .suggested_fix
            .try_get()
            .unwrap_or_default()
            .trim()
            .to_string();

        Some(ReviewIssue {
            severity: parse_severity(&self.severity.try_get().unwrap_or_default()),
            category: parse_scope(&self.category.try_get().unwrap_or_default()),
            description,
            location: if location.is_empty() {
                None
            } else {
                Some(location)
            },
            suggested_fix: if suggested_fix.is_empty() {
                None
            } else {
                Some(suggested_fix)
            },
        })
    }
}

#[derive(Clone)]
struct SuggestionEditor {
    id: String,
    text: RwSignal<String>,
}

impl SuggestionEditor {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            text: RwSignal::new(String::new()),
        }
    }
}

fn parse_assessment(s: &str) -> ReviewAssessment {
    match s {
        "approved" => ReviewAssessment::Approved,
        "approved_with_suggestions" => ReviewAssessment::ApprovedWithSuggestions,
        "needs_changes" => ReviewAssessment::NeedsChanges,
        "rejected" => ReviewAssessment::Rejected,
        "inconclusive" => ReviewAssessment::Inconclusive,
        _ => ReviewAssessment::Approved,
    }
}

fn parse_severity(s: &str) -> IssueSeverity {
    match s {
        "info" => IssueSeverity::Info,
        "low" => IssueSeverity::Low,
        "medium" => IssueSeverity::Medium,
        "high" => IssueSeverity::High,
        "critical" => IssueSeverity::Critical,
        _ => IssueSeverity::Medium,
    }
}

fn parse_scope(s: &str) -> ReviewScope {
    match s {
        "logic" => ReviewScope::Logic,
        "edge_cases" => ReviewScope::EdgeCases,
        "security" => ReviewScope::Security,
        "performance" => ReviewScope::Performance,
        "style" => ReviewScope::Style,
        "api_design" => ReviewScope::ApiDesign,
        "testing" => ReviewScope::Testing,
        "documentation" => ReviewScope::Documentation,
        _ => ReviewScope::Logic,
    }
}

fn parse_confidence(s: &str) -> Option<f32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
}

fn pause_kind_badge(kind: &PauseKind) -> (BadgeVariant, &'static str) {
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
