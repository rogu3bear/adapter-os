//! Training job detail tab components
//!
//! Extracted tab content for Overview, Configuration, Backend, Export, and Metrics.

use crate::api::use_api_client;
use crate::components::{Card, DetailRow, Spinner};
use crate::constants::ui_language;
use crate::hooks::use_polling;
use adapteros_api_types::{
    TrainingJobResponse, TrainingMetricEntry, TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS,
    TRAINING_QUANTIZATION_PROBE_STATUS_DISABLED, TRAINING_QUANTIZATION_PROBE_STATUS_FAILED,
    TRAINING_QUANTIZATION_PROBE_STATUS_SUCCESS, TRAINING_QUANTIZATION_PROBE_STATUS_UNAVAILABLE,
};
use leptos::prelude::*;

use crate::pages::training::components::CoremlBadges;
use crate::pages::training::state::CoremlState;
use crate::pages::training::utils::{
    format_backend_or, format_date, format_duration, format_number,
};

/// Overview tab - job metadata and timestamps
#[component]
pub fn OverviewTabContent(
    job: TrainingJobResponse,
    job_id: String,
    #[prop(optional)] adapter_id: Option<String>,
) -> impl IntoView {
    // adapter_id accepted for call-site compat; artifact display removed.
    let _ = adapter_id;
    view! {
        <Card title="Job Details".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Job ID" value=job_id mono=true/>
                <DetailRow label="Adapter" value=job.adapter_name.clone()/>
                {job.category.clone().map(|cat| view! {
                    <DetailRow label="Category" value=cat/>
                })}
                {job.dataset_id.clone().map(|ds| view! {
                    <DetailRow label="Dataset" value=ds/>
                })}
                <DetailRow
                    label="Created"
                    value=format_date(&job.created_at)
                    data_testid="training-detail-created-row".to_string()
                />
                {job.started_at.clone().map(|ts| view! {
                    <DetailRow
                        label="Started"
                        value=format_date(&ts)
                        data_testid="training-detail-started-row".to_string()
                    />
                })}
                {job.completed_at.clone().map(|ts| view! {
                    <DetailRow
                        label="Completed"
                        value=format_date(&ts)
                        data_testid="training-detail-completed-row".to_string()
                    />
                })}
            </div>
        </Card>
    }
}

/// Configuration tab - training parameters
#[component]
pub fn ConfigurationTabContent(job: TrainingJobResponse) -> impl IntoView {
    view! {
        <Card title="Training Parameters".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow label="Total Epochs" value=job.total_epochs.to_string()/>
                <DetailRow label="Current Epoch" value=job.current_epoch.unwrap_or(0).to_string()/>
                <DetailRow label="Learning Rate" value=format!("{:.6}", job.learning_rate)/>
                {job.current_loss.map(|loss| view! {
                    <DetailRow label="Current Loss" value=format!("{:.4}", loss)/>
                })}
                {job.tokens_per_second.map(|tps| view! {
                    <DetailRow label="Tokens/sec" value=format!("{:.1}", tps)/>
                })}
            </div>
        </Card>
    }
}

/// Backend tab - backend selection and device info
#[component]
pub fn BackendTabContent(job: TrainingJobResponse, coreml_state: CoremlState) -> impl IntoView {
    view! {
        <Card title="Backend Selection".to_string()>
            <div class="grid gap-3 text-sm md:grid-cols-2">
                <DetailRow
                    label="Requested Backend"
                    value=format_backend_or(job.requested_backend.as_deref(), "Not specified")
                />
                <DetailRow
                    label="Selected Backend"
                    value=format_backend_or(job.backend.as_deref(), "Pending")
                />
                {job.backend_reason.clone().map(|reason| view! {
                    <DetailRow label="Selection Reason" value=reason/>
                })}
                {job.backend_device.clone().map(|device| view! {
                    <DetailRow label="Device" value=device/>
                })}
            </div>

            {coreml_state.coreml_fallback.then(|| view! {
                <div class="mt-3 rounded-lg border border-status-error bg-status-error/10 p-3">
                    <p class="text-sm text-status-error">
                        {"CoreML was requested, but the job ran on "}
                        {format_backend_or(job.backend.as_deref(), "a different backend")}
                        {"."}
                    </p>
                    {coreml_state.fallback_reason.clone().map(|reason| view! {
                        <p class="text-xs text-status-error mt-1">{"Reason: "}{reason}</p>
                    })}
                </div>
            })}
        </Card>

        // Reproducibility settings (collapsible section)
        {(job.determinism_mode.is_some() || job.training_seed.is_some()).then(|| view! {
            <Card title="Reproducibility Settings".to_string() class="mt-4".to_string()>
                <div class="grid gap-3 text-sm md:grid-cols-2">
                    {job.determinism_mode.clone().map(|mode| view! {
                        <DetailRow label=ui_language::REPRODUCIBLE_MODE value=mode/>
                    })}
                    {job.training_seed.map(|seed| view! {
                        <DetailRow label="Training Seed" value=seed.to_string() mono=true/>
                    })}
                </div>
            </Card>
        })}

        // CoreML training fallback info
        {job.coreml_training_fallback.clone().map(|reason| view! {
            <Card title="CoreML Training Fallback".to_string() class="mt-4".to_string()>
                <p class="text-sm text-muted-foreground">{reason}</p>
            </Card>
        })}
    }
}

/// Export tab - CoreML export status
#[component]
pub fn ExportTabContent(
    job: TrainingJobResponse,
    coreml_state: CoremlState,
    coreml_export_requested: bool,
) -> impl IntoView {
    // job accepted for call-site compat; artifact/verification display removed.
    let _ = job;
    view! {
        <Card title="CoreML Export Status".to_string()>
            <div class="space-y-4">
                <div class="flex items-center gap-3">
                    <CoremlBadges state=coreml_state.clone()/>
                </div>

                <div class="grid gap-3 text-sm md:grid-cols-2">
                    <DetailRow
                        label="Export Requested"
                        value=if coreml_export_requested { "Yes".to_string() } else { "No".to_string() }
                    />
                    {coreml_state.export_status.clone().map(|status| view! {
                        <DetailRow label="Export Status" value=status/>
                    })}
                    {coreml_state.export_reason.clone().map(|reason| view! {
                        <DetailRow label="Export Reason" value=reason/>
                    })}
                </div>
            </div>
        </Card>
    }
}

/// Build an SVG path string from data points mapped to a 0-100 viewBox.
fn build_svg_path(values: &[f32], min_val: f32, range: f32) -> String {
    let points: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = if values.len() > 1 {
                (i as f64 / (values.len() - 1) as f64) * 100.0
            } else {
                50.0
            };
            let y = 100.0 - ((v as f64 - min_val as f64) / range as f64 * 80.0 + 10.0);
            format!("{:.1},{:.1}", x, y)
        })
        .collect();
    if points.len() > 1 {
        format!("M {} L {}", points[0], points[1..].join(" L "))
    } else if !points.is_empty() {
        format!("M {} L {}", points[0], points[0])
    } else {
        String::new()
    }
}

/// Compute min/max across one or two slices of f32, returning (min, max, range).
fn curve_bounds(a: &[f32], b: &[f32]) -> (f32, f32, f32) {
    let min = a
        .iter()
        .chain(b.iter())
        .copied()
        .fold(f32::INFINITY, f32::min);
    let max = a
        .iter()
        .chain(b.iter())
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let range = (max - min).max(0.001);
    (min, max, range)
}

/// Dual-line SVG chart used for loss and perplexity curves.
#[component]
fn DualCurveChart(
    /// Primary data series (blue).
    primary: Vec<f32>,
    /// Secondary data series (orange).
    secondary: Vec<f32>,
    #[prop(into)] primary_label: String,
    #[prop(into)] secondary_label: String,
    /// Optional epoch index to mark (vertical line).
    #[prop(optional)]
    best_epoch: Option<u32>,
    /// Total epochs for best-epoch marker positioning.
    #[prop(optional)]
    total_epochs: Option<u32>,
) -> impl IntoView {
    let (min_val, max_val, range) = curve_bounds(&primary, &secondary);

    let primary_path = build_svg_path(&primary, min_val, range);
    let secondary_path = build_svg_path(&secondary, min_val, range);

    // Best-epoch marker X position (percentage)
    let marker_x = best_epoch.map(|be| {
        let total = total_epochs.unwrap_or(1).max(1);
        if total > 1 {
            (be as f64 / (total - 1) as f64) * 100.0
        } else {
            50.0
        }
    });

    view! {
        <div class="space-y-2">
            // Legend
            <div class="flex gap-4 text-xs text-muted-foreground">
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-0.5" style="background:var(--color-primary)"></span>
                    {primary_label}
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-0.5" style="background:var(--color-warning)"></span>
                    {secondary_label}
                </span>
                {marker_x.map(|_| view! {
                    <span class="flex items-center gap-1">
                        <span class="inline-block w-0.5 h-3" style="background:var(--color-success)"></span>
                        "Best epoch"
                    </span>
                })}
            </div>

            // Chart
            <div class="relative h-40 bg-muted/30 rounded-md p-2">
                <svg class="w-full h-full" viewBox="0 0 100 100" preserveAspectRatio="none">
                    // Grid lines
                    <line x1="0" y1="25" x2="100" y2="25" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>
                    <line x1="0" y1="50" x2="100" y2="50" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>
                    <line x1="0" y1="75" x2="100" y2="75" stroke="currentColor" stroke-opacity="0.1" stroke-width="0.5"/>

                    // Best epoch marker
                    {marker_x.map(|x| view! {
                        <line
                            x1=format!("{:.1}", x) y1="0"
                            x2=format!("{:.1}", x) y2="100"
                            stroke="var(--color-success)"
                            stroke-width="1"
                            stroke-dasharray="4 2"
                            vector-effect="non-scaling-stroke"
                        />
                    })}

                    // Primary curve (train)
                    <path
                        d=primary_path
                        fill="none"
                        stroke="var(--color-primary)"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        vector-effect="non-scaling-stroke"
                    />

                    // Secondary curve (val)
                    <path
                        d=secondary_path
                        fill="none"
                        stroke="var(--color-warning)"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        vector-effect="non-scaling-stroke"
                    />
                </svg>

                // Y-axis labels
                <div class="absolute left-0 top-0 h-full flex flex-col justify-between text-2xs text-muted-foreground py-1">
                    <span>{format!("{:.3}", max_val)}</span>
                    <span>{format!("{:.3}", min_val)}</span>
                </div>
            </div>
        </div>
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProvenanceChipTone {
    Standard,
    Muted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProvenanceChip {
    label: String,
    tone: ProvenanceChipTone,
}

fn normalize_status_token(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('_', " ")
        .replace('-', " ")
}

fn gate_source_label(gate_source: &str) -> String {
    if gate_source.eq_ignore_ascii_case(TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS) {
        "Policy-computed (gate source)".to_string()
    } else {
        let readable = normalize_status_token(gate_source);
        format!("Gate source: {readable}")
    }
}

fn quantization_provenance_chips(
    gate_source: Option<&str>,
    probe_status: Option<&str>,
) -> Vec<ProvenanceChip> {
    let (Some(gate_source), Some(probe_status)) = (gate_source, probe_status) else {
        return vec![ProvenanceChip {
            label: "Provenance unavailable (legacy report)".to_string(),
            tone: ProvenanceChipTone::Muted,
        }];
    };

    let mut chips = vec![ProvenanceChip {
        label: gate_source_label(gate_source),
        tone: ProvenanceChipTone::Standard,
    }];

    match probe_status.trim().to_ascii_lowercase().as_str() {
        TRAINING_QUANTIZATION_PROBE_STATUS_SUCCESS => chips.push(ProvenanceChip {
            label: "Native probe (informational)".to_string(),
            tone: ProvenanceChipTone::Standard,
        }),
        TRAINING_QUANTIZATION_PROBE_STATUS_UNAVAILABLE
        | TRAINING_QUANTIZATION_PROBE_STATUS_FAILED => chips.push(ProvenanceChip {
            label: "Probe unavailable".to_string(),
            tone: ProvenanceChipTone::Muted,
        }),
        TRAINING_QUANTIZATION_PROBE_STATUS_DISABLED => {}
        _ => chips.push(ProvenanceChip {
            label: format!("Probe status: {}", normalize_status_token(probe_status)),
            tone: ProvenanceChipTone::Muted,
        }),
    }

    chips
}

/// Metrics tab - live or final metrics display
#[component]
pub fn MetricsTabContent(
    job: TrainingJobResponse,
    job_id_for_metrics: String,
    is_running: bool,
    is_completed: bool,
) -> impl IntoView {
    // For completed jobs, fetch the training report
    let report_signal: RwSignal<Option<adapteros_api_types::TrainingReportResponse>> =
        RwSignal::new(None);
    let report_error: RwSignal<Option<String>> = RwSignal::new(None);
    let report_loading = RwSignal::new(is_completed);

    if is_completed {
        let client = use_api_client();
        let job_id_report = job_id_for_metrics.clone();
        Effect::new(move |_| {
            let client = client.clone();
            let job_id = job_id_report.clone();
            gloo_timers::callback::Timeout::new(0, move || {
                wasm_bindgen_futures::spawn_local(async move {
                    match client.get_training_report(&job_id).await {
                        Ok(resp) => {
                            let _ = report_signal.try_set(Some(resp));
                            let _ = report_error.try_set(None);
                        }
                        Err(e) => {
                            let _ = report_error.try_set(Some(e.user_message()));
                        }
                    }
                    let _ = report_loading.try_set(false);
                });
            })
            .forget();
        });
    }

    view! {
        {move || {
            if !is_completed || report_loading.try_get().unwrap_or(false) {
                return ().into_any();
            }

            let maybe_report = report_signal.try_get().flatten();
            let Some(report) = maybe_report else {
                return ().into_any();
            };

            let quantization_report = report.report.quantization_report.as_ref();
            let chips = quantization_provenance_chips(
                quantization_report.map(|report| report.gate_source.as_str()),
                quantization_report.map(|report| report.probe_status.as_str()),
            );

            view! {
                <div class="mb-3 flex flex-wrap items-center gap-2 text-2xs">
                    {chips.into_iter().map(|chip| {
                        let class = match chip.tone {
                            ProvenanceChipTone::Standard => "inline-flex items-center rounded-full border border-border/60 bg-muted/40 px-2 py-0.5 text-muted-foreground".to_string(),
                            ProvenanceChipTone::Muted => "inline-flex items-center rounded-full border border-border/60 bg-muted/20 px-2 py-0.5 text-muted-foreground".to_string(),
                        };
                        view! {
                            <span class=class>{chip.label}</span>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            }.into_any()
        }}

        // Final metrics (for completed jobs)
        {is_completed.then(|| view! {
            <Card title="Final Metrics".to_string()>
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

        // Training quality report (for completed jobs)
        {is_completed.then(|| view! {
            <div class="mt-4">
                {move || {
                    if report_loading.try_get().unwrap_or(false) {
                        view! {
                            <Card title="Training Quality Report".to_string()>
                                <div class="h-32 flex items-center justify-center text-muted-foreground gap-2">
                                    <Spinner />
                                    <span>"Loading training report\u{2026}"</span>
                                </div>
                            </Card>
                        }.into_any()
                    } else if let Some(err) = report_error.try_get().flatten() {
                        view! {
                            <Card title="Training Quality Report".to_string()>
                                <div class="text-sm text-muted-foreground">{err}</div>
                            </Card>
                        }.into_any()
                    } else if let Some(ref resp) = report_signal.try_get().flatten() {
                        let curves = &resp.report.curves;
                        let summary = &resp.report.summary;

                        let train_loss = curves.train_loss.clone();
                        let val_loss = curves.val_loss.clone();
                        let train_ppl = curves.train_ppl.clone();
                        let val_ppl = curves.val_ppl.clone();

                        // Quality indicators
                        let final_train_loss = curves.train_loss.last().copied().unwrap_or(0.0);
                        let final_val_loss = curves.val_loss.last().copied().unwrap_or(0.0);
                        let overfitting = final_val_loss > final_train_loss * 1.5 && final_train_loss > 0.0;

                        let first_ppl = curves.val_ppl.first().copied().unwrap_or(1.0);
                        let best_ppl = curves.val_ppl.iter().copied().fold(f32::INFINITY, f32::min);
                        let ppl_improvement = if first_ppl > 0.0 {
                            ((first_ppl - best_ppl) / first_ppl * 100.0) as f64
                        } else {
                            0.0
                        };

                        let best_epoch = summary.best_epoch;
                        let final_epoch = summary.final_epoch;
                        let early_stopped = summary.early_stopped;
                        let total_steps = summary.total_steps;
                        let total_tokens = summary.total_tokens;

                        view! {
                            <div class="space-y-4">
                                // Loss curves
                                <Card title="Loss Curves".to_string()>
                                    <DualCurveChart
                                        primary=train_loss
                                        secondary=val_loss
                                        primary_label="Train Loss"
                                        secondary_label="Val Loss"
                                        best_epoch=best_epoch
                                        total_epochs=final_epoch
                                    />
                                    <p class="text-xs text-muted-foreground text-center mt-2">
                                        "Loss over training epochs"
                                    </p>
                                </Card>

                                // Perplexity curves
                                <Card title="Perplexity Curves".to_string() class="mt-4".to_string()>
                                    <DualCurveChart
                                        primary=train_ppl
                                        secondary=val_ppl
                                        primary_label="Train PPL"
                                        secondary_label="Val PPL"
                                    />
                                    <div class="flex justify-between text-xs text-muted-foreground mt-2">
                                        <span>"Perplexity over training epochs"</span>
                                        <span class="text-status-success font-medium">
                                            {format!("{:.1}% improvement", ppl_improvement)}
                                        </span>
                                    </div>
                                </Card>

                                // Quality summary
                                <Card title="Quality Summary".to_string() class="mt-4".to_string()>
                                    <div class="grid gap-3 text-sm md:grid-cols-2">
                                        <div>
                                            <span class="text-muted-foreground">"Final Train Loss"</span>
                                            <span class="ml-2 font-mono">{format!("{:.4}", final_train_loss)}</span>
                                        </div>
                                        <div>
                                            <span class="text-muted-foreground">"Final Val Loss"</span>
                                            <span class="ml-2 font-mono">{format!("{:.4}", final_val_loss)}</span>
                                            {overfitting.then(|| view! {
                                                <span class="ml-2 text-xs text-status-warning font-medium">"Overfitting detected"</span>
                                            })}
                                        </div>
                                        <DetailRow
                                            label="Best Epoch"
                                            value=format!("{} / {}", best_epoch, final_epoch)
                                        />
                                        <div>
                                            <span class="text-muted-foreground">"Early Stopped"</span>
                                            <span class="ml-2">
                                                {if early_stopped {
                                                    view! { <span class="text-status-warning font-medium">"Yes"</span> }.into_any()
                                                } else {
                                                    view! { <span>"No"</span> }.into_any()
                                                }}
                                            </span>
                                        </div>
                                        <DetailRow label="Total Steps" value=format_number(total_steps)/>
                                        <DetailRow label="Total Tokens" value=format_number(total_tokens)/>
                                        <div>
                                            <span class="text-muted-foreground">"Perplexity Improvement"</span>
                                            <span class="ml-2 font-mono text-status-success">
                                                {format!("{:.1}%", ppl_improvement)}
                                            </span>
                                        </div>
                                    </div>
                                </Card>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <Card title="Training Quality Report".to_string()>
                                <div class="text-sm text-muted-foreground">"No training report available."</div>
                            </Card>
                        }.into_any()
                    }
                }}
            </div>
        })}

        // Live metrics chart (for running jobs)
        {is_running.then(|| {
            view! {
                <Card title="Training Metrics".to_string()>
                    <MetricsChart job_id=job_id_for_metrics.clone()/>
                </Card>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantization_chips_success_show_native_probe_without_unavailable() {
        let chips = quantization_provenance_chips(
            Some(TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS),
            Some(TRAINING_QUANTIZATION_PROBE_STATUS_SUCCESS),
        );
        let labels: Vec<String> = chips.into_iter().map(|chip| chip.label).collect();
        assert!(labels.contains(&"Policy-computed (gate source)".to_string()));
        assert!(labels.contains(&"Native probe (informational)".to_string()));
        assert!(!labels.contains(&"Probe unavailable".to_string()));
    }

    #[test]
    fn quantization_chips_unavailable_show_probe_unavailable_without_native_probe() {
        let chips = quantization_provenance_chips(
            Some(TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS),
            Some(TRAINING_QUANTIZATION_PROBE_STATUS_UNAVAILABLE),
        );
        let labels: Vec<String> = chips.into_iter().map(|chip| chip.label).collect();
        assert!(labels.contains(&"Policy-computed (gate source)".to_string()));
        assert!(labels.contains(&"Probe unavailable".to_string()));
        assert!(!labels.contains(&"Native probe (informational)".to_string()));
    }

    #[test]
    fn quantization_chips_legacy_report_show_fallback_chip() {
        let chips = quantization_provenance_chips(None, None);
        assert_eq!(chips.len(), 1);
        assert_eq!(chips[0].label, "Provenance unavailable (legacy report)");
    }
}

/// Metrics chart component - displays training loss curve with summary stats
#[component]
pub fn MetricsChart(job_id: String) -> impl IntoView {
    let client = use_api_client();
    let metrics: RwSignal<Vec<TrainingMetricEntry>> = RwSignal::new(vec![]);
    let loading = RwSignal::new(true);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    // Initial fetch
    let job_id_clone = job_id.clone();
    {
        let client = client.clone();
        Effect::new(move |_| {
            let job_id = job_id_clone.clone();
            let client = client.clone();
            gloo_timers::callback::Timeout::new(0, move || {
                wasm_bindgen_futures::spawn_local(async move {
                    match client.get_training_metrics(&job_id).await {
                        Ok(response) => {
                            let _ = metrics.try_set(response.metrics);
                            let _ = error.try_set(None);
                        }
                        Err(e) => {
                            let _ = error.try_set(Some(e.user_message()));
                        }
                    }
                    let _ = loading.try_set(false);
                });
            })
            .forget();
        });
    }

    // Poll for updates every 3 seconds
    let job_id_poll = job_id.clone();
    {
        let client = client.clone();
        let _ = use_polling(3_000, move || {
            let job_id = job_id_poll.clone();
            let client = client.clone();
            async move {
                if let Ok(response) = client.get_training_metrics(&job_id).await {
                    let _ = metrics.try_set(response.metrics);
                }
            }
        });
    }

    view! {
        <div class="space-y-4">
            {move || {
                if loading.try_get().unwrap_or(true) {
                    view! {
                        <div class="h-32 flex items-center justify-center text-muted-foreground gap-2">
                            <crate::components::Spinner />
                            <span>"Loading metrics\u{2026}"</span>
                        </div>
                    }.into_any()
                } else if let Some(err) = error.try_get().flatten() {
                    view! {
                        <div class="h-32 flex items-center justify-center text-status-error text-sm">
                            "Metrics unavailable: "{err}
                        </div>
                    }.into_any()
                } else if metrics.try_get().unwrap_or_default().is_empty() {
                    view! {
                        <div class="h-32 flex items-center justify-center text-muted-foreground">
                            "No metrics data yet..."
                        </div>
                    }.into_any()
                } else {
                    let data = metrics.try_get().unwrap_or_default();
                    let latest = data.last();
                    let losses: Vec<f32> = data.iter().map(|m| m.loss as f32).collect();
                    let (min_val, _max_val, _range) = curve_bounds(&losses, &[]);

                    view! {
                        <div>
                            <div class="grid grid-cols-4 gap-4 mb-4">
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Steps"</p>
                                    <p class="text-lg font-semibold">{data.len()}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Epoch"</p>
                                    <p class="text-lg font-semibold">{latest.map(|m| m.epoch).unwrap_or(0)}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Latest Loss"</p>
                                    <p class="text-lg font-semibold">{format!("{:.4}", latest.map(|m| m.loss).unwrap_or(0.0))}</p>
                                </div>
                                <div class="text-center">
                                    <p class="text-xs text-muted-foreground">"Min Loss"</p>
                                    <p class="text-lg font-semibold text-status-success">{format!("{:.4}", min_val)}</p>
                                </div>
                            </div>
                            <DualCurveChart
                                primary=losses
                                secondary=vec![]
                                primary_label="Loss"
                                secondary_label=""
                            />
                            <p class="text-xs text-muted-foreground text-center mt-2">
                                "Loss over training steps"
                            </p>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
