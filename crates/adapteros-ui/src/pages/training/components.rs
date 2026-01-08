//! Training page list components
//!
//! Components for displaying training job lists and status.

use crate::components::{
    Badge, BadgeVariant, Card, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;

use super::utils::format_date;

/// Status filter dropdown
#[component]
pub fn StatusFilter(filter: RwSignal<String>) -> impl IntoView {
    view! {
        <select
            class="flex h-10 w-40 rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            on:change=move |ev| {
                filter.set(event_target_value(&ev));
            }
        >
            <option value="">"All Status"</option>
            <option value="pending">"Pending"</option>
            <option value="running">"Running"</option>
            <option value="completed">"Completed"</option>
            <option value="failed">"Failed"</option>
            <option value="cancelled">"Cancelled"</option>
        </select>
    }
}

/// Training job list table
#[component]
pub fn TrainingJobList(
    jobs: Vec<TrainingJobResponse>,
    selected_id: RwSignal<Option<String>>,
    on_select: impl Fn(String) + Copy + Send + 'static,
) -> impl IntoView {
    if jobs.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <p class="text-muted-foreground">"No training jobs found. Create one to get started."</p>
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

                            view! {
                                <tr
                                    class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                    class:bg-muted=move || selected_id.get().as_ref() == Some(&job_id)
                                    on:click=move |_| on_select(job_id_for_click.clone())
                                >
                                    <TableCell>
                                        <div>
                                            <p class="font-medium">{job.adapter_name.clone()}</p>
                                            <p class="text-xs text-muted-foreground font-mono">
                                                {job.id.clone().chars().take(8).collect::<String>()}"..."
                                            </p>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <JobStatusBadge status=status_for_badge/>
                                    </TableCell>
                                    <TableCell>
                                        <ProgressBar progress=job.progress_pct.unwrap_or(0.0) status=status_for_progress/>
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
    let (variant, label) = match status.as_str() {
        "pending" => (BadgeVariant::Secondary, "Pending"),
        "running" => (BadgeVariant::Default, "Running"),
        "completed" => (BadgeVariant::Success, "Completed"),
        "failed" => (BadgeVariant::Destructive, "Failed"),
        "cancelled" => (BadgeVariant::Warning, "Cancelled"),
        "paused" => (BadgeVariant::Secondary, "Paused"),
        _ => (BadgeVariant::Secondary, "Unknown"),
    };

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
        "completed" => "h-full transition-all duration-300 bg-green-500",
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
