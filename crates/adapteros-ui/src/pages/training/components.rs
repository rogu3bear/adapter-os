//! Training page list components
//!
//! Components for displaying training job lists and status.

use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Checkbox, Select, StatusVariant, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;

use super::state::{CoremlFilterState, CoremlState};
use super::utils::{format_backend, format_backend_or, format_date};

/// Status filter dropdown
fn training_status_filter_options() -> Vec<(String, String)> {
    vec![
        ("".to_string(), "All Status".to_string()),
        ("pending".to_string(), "Pending".to_string()),
        ("running".to_string(), "Running".to_string()),
        ("completed".to_string(), "Completed".to_string()),
        ("failed".to_string(), "Failed".to_string()),
        ("cancelled".to_string(), "Cancelled".to_string()),
    ]
}

#[component]
pub fn TrainingStatusFilter(filter: RwSignal<String>) -> impl IntoView {
    view! {
        <Select
            value=filter
            options=training_status_filter_options()
            class="w-40".to_string()
        />
    }
}

pub use TrainingStatusFilter as StatusFilter;

/// CoreML filter checkboxes
#[component]
pub fn CoremlFilters(filter: RwSignal<CoremlFilterState>) -> impl IntoView {
    let requested_checked = Signal::derive(move || filter.try_get().unwrap_or_default().requested);
    let exported_checked = Signal::derive(move || filter.try_get().unwrap_or_default().exported);
    let fallback_checked = Signal::derive(move || filter.try_get().unwrap_or_default().fallback);

    view! {
        <div class="flex items-center gap-3 px-3 py-2 rounded-md border">
            <Checkbox
                checked=requested_checked
                on_change=Callback::new(move |checked| {
                    filter.update(|f| f.requested = checked);
                })
                label="CoreML requested"
                class="text-sm"
            />
            <Checkbox
                checked=exported_checked
                on_change=Callback::new(move |checked| {
                    filter.update(|f| f.exported = checked);
                })
                label="CoreML exported"
                class="text-sm"
            />
            <Checkbox
                checked=fallback_checked
                on_change=Callback::new(move |checked| {
                    filter.update(|f| f.fallback = checked);
                })
                label="CoreML fallback"
                class="text-sm"
            />
        </div>
    }
}

/// Training job list table
#[component]
pub fn TrainingJobList(
    jobs: Vec<TrainingJobResponse>,
    selected_id: RwSignal<Option<String>>,
    on_select: impl Fn(String) + Copy + Send + 'static,
    #[prop(optional)] on_create: Option<Callback<()>>,
) -> impl IntoView {
    if jobs.is_empty() {
        return view! {
            <Card>
                <div class="flex flex-col items-center justify-center py-12 text-center">
                    <div class="rounded-full bg-muted p-3 mb-4">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="h-8 w-8 text-muted-foreground"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <path d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/>
                        </svg>
                    </div>
                    <h3 class="heading-4 text-foreground mb-1">"No adapters in training yet"</h3>
                    <p class="text-sm text-muted-foreground max-w-sm mb-6">
                        "Create an adapter from your files, then move directly into conversation."
                    </p>
                    {on_create.map(|cb| view! {
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| cb.run(()))
                        >
                            "Create Adapter"
                        </Button>
                    })}
                </div>
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Adapter"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Progress"</TableHead>
                        <TableHead>"Backend"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead>"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {jobs
                        .into_iter()
                        .map(|job| {
                            let job_id = job.id.clone();
                            let job_id_for_click = job_id.clone();
                            let status_for_badge = job.status.clone();
                            let status_for_progress = job.status.clone();
                            let coreml_state = CoremlState::from_job(&job);
                            let coreml_state_for_badges = coreml_state.clone();
                            let coreml_state_for_backend = coreml_state.clone();

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&job_id)
                                    on:click=move |_| on_select(job_id_for_click.clone())
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{job.adapter_name.clone()}</p>
                                            <p class="text-xs text-muted-foreground font-mono">
                                                {adapteros_id::short_id(&job.id)}
                                            </p>
                                            <CoremlBadges state=coreml_state_for_badges/>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <JobStatusBadge status=status_for_badge/>
                                    </TableCell>
                                    <TableCell>
                                        <ProgressBar progress=job.progress_pct.unwrap_or(0.0) status=status_for_progress/>
                                    </TableCell>
                                    <TableCell>
                                        <div class="text-sm">
                                            <p class="font-medium">
                                                {format_backend_or(job.backend.as_deref(), "Pending")}
                                            </p>
                                            {job.requested_backend.clone().map(|req| view! {
                                                <p class="text-xs text-muted-foreground">{"Requested: "}{format_backend(&req)}</p>
                                            })}
                                            {coreml_state_for_backend.coreml_fallback.then(|| view! {
                                                <p class="text-xs text-status-error">
                                                    {"Fallback: "}{coreml_state_for_backend.fallback_reason.clone().unwrap_or_else(|| "CoreML requested".to_string())}
                                                </p>
                                            })}
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">
                                            {format_date(&job.created_at)}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-primary">"View"</span>
                                    </TableCell>
                                </tr>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
    }
    .into_any()
}

/// Job status badge component
#[component]
pub fn JobStatusBadge(status: String) -> impl IntoView {
    let (status_key, label) = match status.as_str() {
        "pending" => ("pending", "Pending"),
        "running" => ("running", "Running"),
        "completed" => ("completed", "Completed"),
        "failed" => ("failed", "Failed"),
        "cancelled" => ("cancelled", "Cancelled"),
        _ => ("unknown", "Unknown"),
    };
    let variant = StatusVariant::from_status(status_key).to_badge_variant();

    view! {
        <Badge variant=variant>
            {label}
        </Badge>
    }
}

/// Progress bar component
#[component]
pub fn ProgressBar(progress: f32, status: String) -> impl IntoView {
    let progress_pct = format!("{:.0}%", progress);

    let bar_class = match status.as_str() {
        "running" => "h-full transition-all duration-300 bg-primary",
        "completed" => "h-full transition-all duration-300 bg-status-success",
        "failed" => "h-full transition-all duration-300 bg-destructive",
        _ => "h-full transition-all duration-300 bg-muted-foreground",
    };

    view! {
        <div class="flex items-center gap-2">
            <div class="flex-1 h-2 bg-muted rounded-full overflow-hidden">
                <div
                    class=bar_class
                    style=format!("width: {}%", progress)
                />
            </div>
            <span class="text-xs text-muted-foreground w-10 text-right">{progress_pct}</span>
        </div>
    }
}

/// CoreML badges for quick list scanning
#[component]
pub fn CoremlBadges(state: CoremlState) -> impl IntoView {
    let mut badges = Vec::new();

    if state.coreml_requested {
        badges.push(
            view! { <Badge variant=BadgeVariant::Secondary>"CoreML requested"</Badge> }.into_any(),
        );
    }

    if state.coreml_exported {
        badges.push(
            view! { <Badge variant=BadgeVariant::Success>"CoreML exported"</Badge> }.into_any(),
        );
    } else if state.coreml_export_requested {
        badges.push(
            view! { <Badge variant=BadgeVariant::Default>"CoreML export pending"</Badge> }
                .into_any(),
        );
    }

    if state.coreml_fallback {
        badges.push(
            view! { <Badge variant=BadgeVariant::Destructive>"CoreML fallback"</Badge> }.into_any(),
        );
    }

    view! {
        <div class="flex flex-wrap gap-1 mt-2">
            {badges}
        </div>
    }
}
