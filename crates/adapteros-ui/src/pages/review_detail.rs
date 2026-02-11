//! Review detail page
//!
//! Displays full pause context and provides a structured form to submit
//! a human review (assessment, issues, suggestions, comments, confidence).

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ErrorDisplay, Input,
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
    let breadcrumb_label = Signal::derive(move || match pause_state.get() {
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
    });

    view! {
        <PageScaffold
            title="Review Detail"
            subtitle="Inspect pause context and submit a review".to_string()
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Govern", "/reviews"),
                PageBreadcrumbItem::new("Reviews", "/reviews"),
                PageBreadcrumbItem::current(breadcrumb_label.get()),
            ]
        >
            <PageScaffoldActions slot>
                <RefreshButton on_click=Callback::new(move |_| refetch.run(()))/>
            </PageScaffoldActions>

            {move || match pause_state.get() {
                LoadingState::Idle | LoadingState::Loading => {
                    view! { <LoadingDisplay message="Loading pause details..."/> }.into_any()
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
            assessment: parse_assessment(&assessment.get()),
            issues: issues
                .get()
                .into_iter()
                .filter_map(|e| e.into_review_issue())
                .collect(),
            suggestions: suggestions
                .get()
                .into_iter()
                .map(|s| s.text.get().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            comments: {
                let c = comments.get().trim().to_string();
                if c.is_empty() {
                    None
                } else {
                    Some(c)
                }
            },
            confidence: parse_confidence(&confidence.get()),
        };

        let request = SubmitReviewRequest {
            pause_id: pause_id_for_submit.clone(),
            review,
            reviewer: reviewer.get().trim().to_string(),
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
                    submit_error.set(Some(
                        resp.message
                            .unwrap_or_else(|| "Review was not accepted".to_string()),
                    ));
                    submitting.set(false);
                }
                Err(e) => {
                    submit_error.set(Some(format!("Failed to submit review: {}", e)));
                    submitting.set(false);
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
                    <Input
                        value=reviewer
                        label="Reviewer"
                        placeholder="human"
                        input_type="text".to_string()
                    />

                    <Select
                        value=assessment
                        options=assessment_options
                        label="Assessment"
                    />

                    <Textarea
                        value=comments
                        label="Comments (optional)"
                        placeholder="Write any additional context, rationale, or next steps..."
                        rows=4
                    />

                    <Input
                        value=confidence
                        label="Confidence (0.0 - 1.0, optional)"
                        placeholder="e.g. 0.85"
                        input_type="number".to_string()
                        class="max-w-xs".to_string()
                    />

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
                            {move || {
                                issues.get().into_iter().map(|issue| {
                                    let id = issue.id.clone();
                                    let remove = Callback::new(move |_: ()| {
                                        issues.update(|list| list.retain(|i| i.id != id));
                                    });
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
                                                <Select value=issue.severity options=severity_options.clone() label="Severity"/>
                                                <Select value=issue.category options=scope_options.clone() label="Category"/>
                                            </div>

                                            <Textarea
                                                value=issue.description
                                                label="Description"
                                                placeholder="Describe the issue clearly."
                                                rows=3
                                            />

                                            <div class="grid grid-cols-2 gap-3">
                                                <Input
                                                    value=issue.location
                                                    label="Location (optional)"
                                                    placeholder="e.g. crates/foo/src/lib.rs:42"
                                                    input_type="text".to_string()
                                                />
                                                <Input
                                                    value=issue.suggested_fix
                                                    label="Suggested Fix (optional)"
                                                    placeholder="e.g. Add bounds check before indexing"
                                                    input_type="text".to_string()
                                                />
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()
                            }}
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
                                suggestions.get().into_iter().map(|s| {
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
                                            <Input
                                                value=s.text
                                                aria_label="Suggestion"
                                                placeholder="Add a concise suggestion..."
                                                input_type="text".to_string()
                                            />
                                        </div>
                                    }
                                }).collect::<Vec<_>>()
                            }}
                        </div>
                    </div>

                    {move || submit_error.get().map(|e| view! {
                        <div class="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
                            {e}
                        </div>
                    })}

                    {move || submit_ok.get().then(|| view! {
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
        let description = self.description.get().trim().to_string();
        if description.is_empty() {
            return None;
        }

        let location = self.location.get().trim().to_string();
        let suggested_fix = self.suggested_fix.get().trim().to_string();

        Some(ReviewIssue {
            severity: parse_severity(&self.severity.get()),
            category: parse_scope(&self.category.get()),
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
