//! Training page
//!
//! Complete training jobs management with list view, detail panel, and job creation.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity,
    FormField, Input, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::validation::{rules, use_form_errors, validate_field, ValidationRule};
use adapteros_api_types::TrainingJobResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Training jobs page with list and detail panels
#[component]
pub fn Training() -> impl IntoView {
    // Selected job ID for detail panel
    let selected_job_id = RwSignal::new(None::<String>);

    // Status filter
    let status_filter = RwSignal::new(String::new());

    // Dialog open state
    let create_dialog_open = RwSignal::new(false);

    // Fetch training jobs
    let (jobs, refetch_jobs) =
        use_api_resource(
            move |client: Arc<ApiClient>| async move { client.list_training_jobs().await },
        );

    // Store refetch in a signal for sharing
    let refetch_signal = StoredValue::new(refetch_jobs);

    // Polling for live updates (every 5 seconds when jobs are running)
    // Return value (stop fn) intentionally ignored - polling runs until unmount
    let _ = use_polling(5000, move || async move {
        refetch_signal.with_value(|f| f());
    });

    let on_job_select = move |job_id: String| {
        selected_job_id.set(Some(job_id));
    };

    let on_close_detail = move || {
        selected_job_id.set(None);
    };

    let on_job_created = move || {
        create_dialog_open.set(false);
        refetch_signal.with_value(|f| f());
    };

    // Dynamic class for left panel width
    let left_panel_class = move || {
        if selected_job_id.get().is_some() {
            "w-1/2 space-y-6 pr-4"
        } else {
            "flex-1 space-y-6 pr-4"
        }
    };

    view! {
        <div class="p-6 flex h-full">
                // Left panel: Job list
                <div class=left_panel_class>
                    <div class="flex items-center justify-between">
                        <h1 class="text-3xl font-bold tracking-tight">"Training Jobs"</h1>
                        <div class="flex items-center gap-2">
                            <StatusFilter filter=status_filter/>
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(move |_| create_dialog_open.set(true))
                            >
                                "New Training Job"
                            </Button>
                        </div>
                    </div>

                    {move || {
                        match jobs.get() {
                            LoadingState::Idle | LoadingState::Loading => {
                                view! {
                                    <div class="flex items-center justify-center py-12">
                                        <Spinner/>
                                    </div>
                                }.into_any()
                            }
                            LoadingState::Loaded(data) => {
                                let filter = status_filter.get();
                                let filtered_jobs: Vec<_> = data.jobs.iter()
                                    .filter(|j| filter.is_empty() || j.status == filter)
                                    .cloned()
                                    .collect();
                                view! {
                                    <TrainingJobList
                                        jobs=filtered_jobs
                                        selected_id=selected_job_id
                                        on_select=on_job_select
                                    />
                                }.into_any()
                            }
                            LoadingState::Error(e) => {
                                view! {
                                    <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                        <p class="text-destructive">{e.to_string()}</p>
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                </div>

                // Right panel: Job detail (when selected)
                {move || {
                    selected_job_id.get().map(|job_id| {
                        view! {
                            <div class="w-1/2 border-l pl-4">
                                <TrainingJobDetail
                                    job_id=job_id
                                    on_close=on_close_detail
                                    on_cancelled=move || refetch_signal.with_value(|f| f())
                                />
                            </div>
                        }
                    })
                }}

            // Create job dialog
            <CreateJobDialog
                open=create_dialog_open
                on_created=on_job_created
            />
        </div>
    }
}

/// Status filter dropdown
#[component]
fn StatusFilter(filter: RwSignal<String>) -> impl IntoView {
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
fn TrainingJobList(
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
fn JobStatusBadge(status: String) -> impl IntoView {
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
fn ProgressBar(progress: f32, status: String) -> impl IntoView {
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

/// Training job detail panel
#[component]
fn TrainingJobDetail(
    job_id: String,
    on_close: impl Fn() + Copy + 'static,
    on_cancelled: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let job_id_for_fetch = job_id.clone();

    // Fetch job details
    let (job, refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = job_id_for_fetch.clone();
        async move { client.get_training_job(&id).await }
    });

    // Store refetch in a signal for sharing
    let refetch_signal = StoredValue::new(refetch);

    // Poll for updates on running jobs
    // Return value (stop fn) intentionally ignored - polling runs until unmount
    let _ = use_polling(3000, move || async move {
        refetch_signal.with_value(|f| f());
    });

    // Cancel job handler
    let job_id_for_cancel = job_id.clone();
    let cancelling = RwSignal::new(false);
    let show_cancel_confirm = RwSignal::new(false);

    // Handle cancel dialog dismiss
    let on_cancel_dismiss = Callback::new(move |_| {
        // Reset cancelling state if user dismisses dialog during loading
        cancelling.set(false);
    });

    // Create the cancel callback outside the reactive closure
    let cancel_callback = Callback::new(move |_| {
        let job_id = job_id_for_cancel.clone();
        cancelling.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            match client.cancel_training_job(&job_id).await {
                Ok(_) => {
                    show_cancel_confirm.set(false);
                    on_cancelled();
                }
                Err(_) => {
                    // On error, close dialog - user can retry via UI
                    show_cancel_confirm.set(false);
                }
            }
            cancelling.set(false);
        });
    });

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="text-xl font-semibold">"Job Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            {move || {
                match job.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        view! {
                            <JobDetailContent
                                job=data
                                cancelling=cancelling
                                show_cancel_confirm=show_cancel_confirm
                                on_cancel=cancel_callback
                                on_cancel_dismiss=on_cancel_dismiss
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Job detail content
#[component]
fn JobDetailContent(
    job: TrainingJobResponse,
    cancelling: RwSignal<bool>,
    show_cancel_confirm: RwSignal<bool>,
    on_cancel: Callback<()>,
    on_cancel_dismiss: Callback<()>,
) -> impl IntoView {
    // Clone values before view! to avoid move issues
    let status = job.status.clone();
    let status_for_badge = job.status.clone();
    let status_for_progress = job.status.clone();
    let job_id = job.id.clone();
    let job_id_for_logs = job.id.clone();

    let is_running = status == "running";
    let is_pending = status == "pending";
    let is_completed = status == "completed";
    let can_cancel = is_running || is_pending;
    let show_progress = is_running || is_completed;

    view! {
        // Status and progress
        <Card title="Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center justify-between">
                    <JobStatusBadge status=status_for_badge/>
                    {can_cancel.then(|| view! {
                        <Button
                            variant=ButtonVariant::Destructive
                            on_click=Callback::new(move |_| show_cancel_confirm.set(true))
                        >
                            "Cancel Job"
                        </Button>
                    })}
                </div>

                {show_progress.then(|| view! {
                    <div class="space-y-2">
                        <div class="flex justify-between text-sm">
                            <span>"Progress"</span>
                            <span class="font-medium">{format!("{:.1}%", job.progress_pct.unwrap_or(0.0))}</span>
                        </div>
                        <ProgressBar progress=job.progress_pct.unwrap_or(0.0) status=status_for_progress/>
                    </div>
                })}

                {job.error_message.clone().map(|err| view! {
                    <div class="rounded-lg border border-destructive bg-destructive/10 p-3">
                        <p class="text-sm text-destructive">{err}</p>
                    </div>
                })}
            </div>
        </Card>

        // Job metadata
        <Card title="Details".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm">
                <DetailRow label="Job ID" value=job_id/>
                <DetailRow label="Adapter" value=job.adapter_name.clone()/>
                {job.category.clone().map(|cat| view! {
                    <DetailRow label="Category" value=cat/>
                })}
                {job.dataset_id.clone().map(|ds| view! {
                    <DetailRow label="Dataset" value=ds/>
                })}
                <DetailRow label="Created" value=format_date(&job.created_at)/>
                {job.started_at.clone().map(|ts| view! {
                    <DetailRow label="Started" value=format_date(&ts)/>
                })}
                {job.completed_at.clone().map(|ts| view! {
                    <DetailRow label="Completed" value=format_date(&ts)/>
                })}
            </div>
        </Card>

        // Training configuration
        <Card title="Configuration".to_string() class="mt-4".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Epochs" value=format!("{} / {}", job.current_epoch.unwrap_or(0), job.total_epochs)/>
                <DetailRow label="Learning Rate" value=format!("{:.6}", job.learning_rate)/>
                {job.current_loss.map(|loss| view! {
                    <DetailRow label="Current Loss" value=format!("{:.4}", loss)/>
                })}
                {job.tokens_per_second.map(|tps| view! {
                    <DetailRow label="Tokens/sec" value=format!("{:.1}", tps)/>
                })}
            </div>
        </Card>

        // Backend information
        {job.backend.clone().map(|backend| view! {
            <Card title="Backend".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm">
                    <DetailRow label="Backend" value=backend/>
                    {job.backend_device.clone().map(|device| view! {
                        <DetailRow label="Device" value=device/>
                    })}
                    {job.determinism_mode.clone().map(|mode| view! {
                        <DetailRow label="Determinism" value=mode/>
                    })}
                    {job.training_seed.map(|seed| view! {
                        <DetailRow label="Seed" value=seed.to_string()/>
                    })}
                </div>
            </Card>
        })}

        // Metrics (for completed jobs)
        {is_completed.then(|| view! {
            <Card title="Final Metrics".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm md:grid-cols-2">
                    {job.tokens_processed.map(|tokens| view! {
                        <DetailRow label="Tokens Processed" value=format_number(tokens)/>
                    })}
                    {job.examples_processed.map(|examples| view! {
                        <DetailRow label="Examples Processed" value=format_number(examples)/>
                    })}
                    {job.training_time_ms.map(|ms| view! {
                        <DetailRow label="Training Time" value=format_duration(ms)/>
                    })}
                    {job.peak_gpu_memory_mb.map(|mem| view! {
                        <DetailRow label="Peak GPU Memory" value=format!("{:.1} MB", mem)/>
                    })}
                </div>
            </Card>
        })}

        // Artifacts (for completed jobs)
        {job.aos_path.clone().map(|path| view! {
            <Card title="Artifacts".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm">
                    <DetailRow label="Adapter Path" value=path/>
                    {job.adapter_id.clone().map(|id| view! {
                        <DetailRow label="Adapter ID" value=id/>
                    })}
                    {job.package_hash_b3.clone().map(|hash| view! {
                        <div>
                            <span class="text-muted-foreground">"Package Hash: "</span>
                            <span class="font-mono text-xs break-all">{hash}</span>
                        </div>
                    })}
                </div>
            </Card>
        })}

        // Live logs placeholder (for running jobs)
        {is_running.then(|| view! {
            <Card title="Live Logs".to_string() class="mt-4".to_string()>
                <LogViewer job_id=job_id_for_logs/>
            </Card>
        })}

        // Cancel confirmation dialog
        <ConfirmationDialog
            open=show_cancel_confirm
            title="Cancel Training Job"
            description="Are you sure you want to cancel this training job? Progress will be lost, but you can start a new job with the same configuration."
            severity=ConfirmationSeverity::Warning
            confirm_text="Cancel Job"
            on_confirm=on_cancel
            on_cancel=on_cancel_dismiss
            loading=Signal::derive(move || cancelling.get())
        />
    }
}

/// Detail row component
#[component]
fn DetailRow(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex justify-between">
            <span class="text-muted-foreground">{label}</span>
            <span class="font-medium">{value}</span>
        </div>
    }
}

/// Log viewer component (placeholder with simulated logs)
#[component]
fn LogViewer(job_id: String) -> impl IntoView {
    // In a real implementation, this would use SSE to stream logs
    // For now, we show a placeholder with simulated log output
    let logs = RwSignal::new(vec![
        format!("[{job_id}] Training started..."),
        "Loading dataset...".to_string(),
        "Initializing model...".to_string(),
    ]);

    // Simulate new log entries
    Effect::new(move || {
        let interval = gloo_timers::callback::Interval::new(2000, move || {
            logs.update(|l| {
                if l.len() < 20 {
                    l.push(format!(
                        "[{:.2}s] Step {} - loss: {:.4}",
                        l.len() as f64 * 2.0,
                        l.len(),
                        2.5 - (l.len() as f64 * 0.1)
                    ));
                }
            });
        });
        std::mem::forget(interval);
    });

    view! {
        <div class="h-48 overflow-auto bg-zinc-950 rounded-md p-3 font-mono text-xs text-green-400">
            {move || {
                logs.get().into_iter().map(|line| {
                    view! { <div>{line}</div> }
                }).collect::<Vec<_>>()
            }}
        </div>
    }
}

/// Create job dialog
#[component]
fn CreateJobDialog(
    open: RwSignal<bool>,
    on_created: impl Fn() + Clone + Send + Sync + 'static,
) -> impl IntoView {
    // Form state
    let adapter_name = RwSignal::new(String::new());
    let epochs = RwSignal::new("10".to_string());
    let learning_rate = RwSignal::new("0.0001".to_string());
    let batch_size = RwSignal::new("4".to_string());
    let rank = RwSignal::new("8".to_string());
    let alpha = RwSignal::new("16".to_string());
    let dataset_id = RwSignal::new(String::new());
    let category = RwSignal::new("code".to_string());

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Form validation state
    let form_errors = use_form_errors();

    // File upload state
    let uploading = RwSignal::new(false);
    let upload_status = RwSignal::new(String::new());

    let on_created_clone = on_created.clone();

    // Handle file upload - uploads document then converts to dataset
    // This handler is WASM-only since it uses web_sys APIs
    #[cfg(target_arch = "wasm32")]
    let handle_file_upload = {
        let dataset_id = dataset_id.clone();
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;

            let target = ev.target().unwrap();
            let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();

            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    let file_name = file.name();
                    uploading.set(true);
                    upload_status.set(format!("Uploading {}...", file_name));
                    error.set(None);

                    wasm_bindgen_futures::spawn_local(async move {
                        let client = ApiClient::new();

                        // Step 1: Upload document
                        match client.upload_document(&file).await {
                            Ok(doc) => {
                                upload_status.set("Processing document...".to_string());
                                let doc_id = doc.document_id.clone();

                                // Step 2: Poll until indexed (max 60 attempts = 60 seconds)
                                for _ in 0..60 {
                                    gloo_timers::future::TimeoutFuture::new(1000).await;
                                    match client.get_document(&doc_id).await {
                                        Ok(status) => {
                                            match status.status.as_str() {
                                                "indexed" => {
                                                    // Step 3: Create dataset from document
                                                    upload_status
                                                        .set("Creating dataset...".to_string());
                                                    match client
                                                        .create_dataset_from_documents(
                                                            vec![doc_id.clone()],
                                                            Some(file_name.clone()),
                                                        )
                                                        .await
                                                    {
                                                        Ok(ds) => {
                                                            dataset_id.set(ds.id);
                                                            upload_status
                                                                .set("Dataset ready!".to_string());
                                                            uploading.set(false);
                                                            return;
                                                        }
                                                        Err(e) => {
                                                            error.set(Some(format!(
                                                                "Failed to create dataset: {}",
                                                                e
                                                            )));
                                                            uploading.set(false);
                                                            upload_status.set(String::new());
                                                            return;
                                                        }
                                                    }
                                                }
                                                "failed" => {
                                                    error.set(Some(format!(
                                                        "Document processing failed: {}",
                                                        status.error_message.unwrap_or_default()
                                                    )));
                                                    uploading.set(false);
                                                    upload_status.set(String::new());
                                                    return;
                                                }
                                                _ => {
                                                    // Still processing, continue polling
                                                    upload_status.set(format!(
                                                        "Processing document ({})...",
                                                        status.status
                                                    ));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error.set(Some(format!(
                                                "Failed to check status: {}",
                                                e
                                            )));
                                            uploading.set(false);
                                            upload_status.set(String::new());
                                            return;
                                        }
                                    }
                                }
                                // Timeout
                                error.set(Some("Document processing timed out".to_string()));
                                uploading.set(false);
                                upload_status.set(String::new());
                            }
                            Err(e) => {
                                error.set(Some(format!("Upload failed: {}", e)));
                                uploading.set(false);
                                upload_status.set(String::new());
                            }
                        }
                    });
                }
            }
        }
    };

    // No-op handler for non-wasm (native) compilation
    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_upload = move |_ev: web_sys::Event| {
        // File upload not supported outside WASM
        let _ = (uploading, upload_status, error, dataset_id);
    };

    let submit = move |_: ()| {
        // Clear previous errors
        form_errors.update(|e| e.clear_all());
        error.set(None);

        // Validate all fields
        let name = adapter_name.get();
        let epochs_str = epochs.get();
        let lr_str = learning_rate.get();
        let batch_str = batch_size.get();
        let rank_str = rank.get();
        let alpha_str = alpha.get();

        let mut has_errors = false;

        // Validate adapter name
        if let Some(err) = validate_field(&name, &rules::adapter_name()) {
            form_errors.update(|e| e.set("adapter_name", err));
            has_errors = true;
        }

        // Validate epochs (1-1000)
        if let Some(err) = validate_field(
            &epochs_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 1000 },
            ],
        ) {
            form_errors.update(|e| e.set("epochs", err));
            has_errors = true;
        }

        // Validate learning rate (0 < lr <= 1)
        if let Some(err) = validate_field(&lr_str, &rules::learning_rate()) {
            form_errors.update(|e| e.set("learning_rate", err));
            has_errors = true;
        }

        // Validate batch size (1-256)
        if let Some(err) = validate_field(
            &batch_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ],
        ) {
            form_errors.update(|e| e.set("batch_size", err));
            has_errors = true;
        }

        // Validate rank (1-256, typically 4, 8, 16, 32, 64)
        if let Some(err) = validate_field(
            &rank_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 256 },
            ],
        ) {
            form_errors.update(|e| e.set("rank", err));
            has_errors = true;
        }

        // Validate alpha (1-512)
        if let Some(err) = validate_field(
            &alpha_str,
            &[
                ValidationRule::Required,
                ValidationRule::IntRange { min: 1, max: 512 },
            ],
        ) {
            form_errors.update(|e| e.set("alpha", err));
            has_errors = true;
        }

        if has_errors {
            return;
        }

        submitting.set(true);

        let epochs_val: u32 = epochs_str.parse().unwrap_or(10);
        let lr_val: f32 = lr_str.parse().unwrap_or(0.0001);
        let batch_val: u32 = batch_str.parse().unwrap_or(4);
        let rank_val: u32 = rank_str.parse().unwrap_or(8);
        let alpha_val: u32 = alpha_str.parse().unwrap_or(16);
        let ds_id = dataset_id.get();
        let cat = category.get();

        let on_created = on_created_clone.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();

            // Build the request body
            let request = serde_json::json!({
                "adapter_name": name,
                "config": {
                    "rank": rank_val,
                    "alpha": alpha_val,
                    "targets": ["q_proj", "v_proj"],
                    "epochs": epochs_val,
                    "learning_rate": lr_val,
                    "batch_size": batch_val,
                },
                "category": cat,
                "dataset_id": if ds_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(ds_id.clone()) },
                "synthetic_mode": ds_id.is_empty(),
            });

            match client
                .post::<_, TrainingJobResponse>("/v1/training/jobs", &request)
                .await
            {
                Ok(_) => {
                    submitting.set(false);
                    // Reset form
                    adapter_name.set(String::new());
                    epochs.set("10".to_string());
                    learning_rate.set("0.0001".to_string());
                    batch_size.set("4".to_string());
                    rank.set("8".to_string());
                    alpha.set("16".to_string());
                    dataset_id.set(String::new());
                    form_errors.update(|e| e.clear_all());
                    on_created();
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    submitting.set(false);
                }
            }
        });
    };

    let close = move |_: ()| {
        open.set(false);
        error.set(None);
        form_errors.update(|e| e.clear_all());
    };

    view! {
        {move || {
            if !open.get() {
                return view! {}.into_any();
            }

            view! {
                // Backdrop
                <div
                    class="fixed inset-0 z-50 bg-black/80"
                    on:click=move |_| close(())
                />

                // Dialog
                <div class="dialog-content">
                    // Header
                    <div class="flex items-center justify-between mb-4">
                        <div>
                            <h2 class="text-lg font-semibold">"New Training Job"</h2>
                            <p class="text-sm text-muted-foreground">"Configure and start a new adapter training job"</p>
                        </div>
                        <button
                            class="rounded-sm opacity-70 hover:opacity-100"
                            on:click=move |_| close(())
                        >
                            <svg
                                xmlns="http://www.w3.org/2000/svg"
                                width="24"
                                height="24"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <path d="M18 6 6 18"/>
                                <path d="m6 6 12 12"/>
                            </svg>
                        </button>
                    </div>

                    // Error message
                    {move || error.get().map(|e| view! {
                        <div class="mb-4 rounded-lg border border-destructive bg-destructive/10 p-3">
                            <p class="text-sm text-destructive">{e}</p>
                        </div>
                    })}

                    // Form
                    <div class="space-y-4">
                        <FormField
                            label="Adapter Name"
                            name="adapter_name"
                            required=true
                            help="Name for the trained adapter (letters, numbers, hyphens)"
                            error=Signal::derive(move || form_errors.get().get("adapter_name").cloned())
                        >
                            <Input
                                value=adapter_name
                                placeholder="my-code-adapter".to_string()
                            />
                        </FormField>

                        <div class="space-y-2">
                            <label class="text-sm font-medium">"Category"</label>
                            <select
                                class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                on:change=move |ev| category.set(event_target_value(&ev))
                            >
                                <option value="code" selected=true>"Code"</option>
                                <option value="framework">"Framework"</option>
                                <option value="codebase">"Codebase"</option>
                                <option value="docs">"Documentation"</option>
                                <option value="domain">"Domain"</option>
                            </select>
                        </div>

                        // File upload section
                        <div class="space-y-2">
                            <label class="text-sm font-medium">"Training Data"</label>
                            <div class="space-y-3">
                                // File upload input
                                <div>
                                    <input
                                        type="file"
                                        accept=".md,.txt,.pdf"
                                        class="block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary file:text-primary-foreground hover:file:bg-primary/90 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                                        disabled=move || uploading.get() || submitting.get()
                                        on:change=handle_file_upload.clone()
                                    />
                                    <p class="text-xs text-muted-foreground mt-1">
                                        "Upload a document (.md, .txt, .pdf) to create a training dataset"
                                    </p>
                                </div>

                                // Upload status
                                {move || {
                                    let status = upload_status.get();
                                    if status.is_empty() {
                                        None
                                    } else {
                                        let is_ready = status.contains("ready");
                                        let class = if is_ready {
                                            "text-sm text-green-600 flex items-center gap-2"
                                        } else {
                                            "text-sm text-muted-foreground flex items-center gap-2"
                                        };
                                        Some(view! {
                                            <div class=class>
                                                {if !is_ready {
                                                    view! {
                                                        <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg class="h-4 w-4 text-green-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                                                        </svg>
                                                    }.into_any()
                                                }}
                                                <span>{status}</span>
                                            </div>
                                        })
                                    }
                                }}

                                // Or divider
                                <div class="relative">
                                    <div class="absolute inset-0 flex items-center">
                                        <span class="w-full border-t" />
                                    </div>
                                    <div class="relative flex justify-center text-xs uppercase">
                                        <span class="bg-background px-2 text-muted-foreground">"or use existing dataset"</span>
                                    </div>
                                </div>

                                // Dataset ID input
                                <Input
                                    value=dataset_id
                                    label="Dataset ID".to_string()
                                    placeholder="ds-abc123".to_string()
                                />
                            </div>
                        </div>

                        <div class="border-t pt-4 mt-4">
                            <h3 class="text-sm font-medium mb-3">"Training Parameters"</h3>
                            <div class="grid gap-4 grid-cols-2">
                                <FormField
                                    label="Epochs"
                                    name="epochs"
                                    required=true
                                    help="Number of training iterations (1-1000)"
                                    error=Signal::derive(move || form_errors.get().get("epochs").cloned())
                                >
                                    <Input
                                        value=epochs
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Learning Rate"
                                    name="learning_rate"
                                    required=true
                                    help="Step size for optimization (0.0001-0.01 recommended)"
                                    error=Signal::derive(move || form_errors.get().get("learning_rate").cloned())
                                >
                                    <Input
                                        value=learning_rate
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="Batch Size"
                                    name="batch_size"
                                    required=true
                                    help="Examples per training step (1-256)"
                                    error=Signal::derive(move || form_errors.get().get("batch_size").cloned())
                                >
                                    <Input
                                        value=batch_size
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <FormField
                                    label="LoRA Rank"
                                    name="rank"
                                    required=true
                                    help="Adapter rank dimension (4, 8, 16 typical)"
                                    error=Signal::derive(move || form_errors.get().get("rank").cloned())
                                >
                                    <Input
                                        value=rank
                                        input_type="number".to_string()
                                    />
                                </FormField>
                            </div>
                        </div>

                        <div class="border-t pt-4 mt-4">
                            <h3 class="text-sm font-medium mb-3">"LoRA Configuration"</h3>
                            <div class="grid gap-4 grid-cols-2">
                                <FormField
                                    label="Alpha"
                                    name="alpha"
                                    required=true
                                    help="Scaling factor (typically 2x rank)"
                                    error=Signal::derive(move || form_errors.get().get("alpha").cloned())
                                >
                                    <Input
                                        value=alpha
                                        input_type="number".to_string()
                                    />
                                </FormField>
                                <div class="space-y-2">
                                    <label class="text-sm font-medium">"Target Layers"</label>
                                    <div class="text-sm text-muted-foreground p-2 bg-muted rounded-md">
                                        "q_proj, v_proj (default)"
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Footer
                    <div class="flex justify-end gap-2 mt-6">
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(close.clone())
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            loading=submitting.get()
                            on_click=Callback::new(submit.clone())
                        >
                            "Start Training"
                        </Button>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Format a date string for display
fn format_date(date_str: &str) -> String {
    // Simple formatting - just show date and time
    // In a real app, use a proper date library
    if date_str.len() >= 16 {
        format!("{} {}", &date_str[0..10], &date_str[11..16])
    } else {
        date_str.to_string()
    }
}

/// Format a large number with commas
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format duration in milliseconds to human readable
fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;

    if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
}
