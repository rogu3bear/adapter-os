//! Backend readiness widget for the training dashboard.
//! Shows CoreML/Metal/MLX capability summary plus base model status before launch.

use crate::api::ApiClient;
use crate::components::{Badge, BadgeVariant, Card, ErrorDisplay, Spinner};
use crate::hooks::{use_api_resource, LoadingState};
use adapteros_api_types::{
    model_status::ModelLoadStatus, training::TrainingBackendReadinessResponse,
};
use leptos::prelude::*;
use std::sync::Arc;

/// Training backend readiness card.
#[component]
pub fn BackendReadinessPanel() -> impl IntoView {
    let (readiness, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.get_training_backend_readiness().await
    });
    let refetch_signal = StoredValue::new(refetch);
    let card_title = "Backend readiness".to_string();
    let card_description =
        "Validate CoreML/Metal/MLX availability before launching training.".to_string();

    view! {
        <Card
            title=card_title.clone()
            description=card_description.clone()
        >
            {move || match readiness.get() {
                LoadingState::Idle | LoadingState::Loading => view! {
                    <div class="flex justify-center py-6">
                        <Spinner/>
                    </div>
                }.into_any(),
                LoadingState::Error(error) => view! {
                    <ErrorDisplay
                        error=error
                        on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                    />
                }.into_any(),
                LoadingState::Loaded(data) => view! {
                    <BackendReadinessContent readiness=data/>
                }.into_any(),
            }}
        </Card>
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ReadinessViewState {
    severity: &'static str,
    message: String,
    variant: BadgeVariant,
}

fn derive_readiness_view_state(readiness: &TrainingBackendReadinessResponse) -> ReadinessViewState {
    if let Some(base) = readiness.base_model.as_ref() {
        if base.status == ModelLoadStatus::Error {
            return ReadinessViewState {
                severity: "error",
                message: "Base model is in error; manual intervention may be required".into(),
                variant: BadgeVariant::Destructive,
            };
        }
    }

    if !readiness.ready {
        return ReadinessViewState {
            severity: "blocked",
            message: readiness
                .fallback_reason
                .clone()
                .unwrap_or_else(|| "Backend unavailable".into()),
            variant: BadgeVariant::Destructive,
        };
    }

    if let Some(fallback) = readiness.fallback_backend {
        return ReadinessViewState {
            severity: "fallback",
            message: format!("Using {} fallback", fallback.as_str()),
            variant: BadgeVariant::Warning,
        };
    }

    ReadinessViewState {
        severity: "ready",
        message: format!("Using {}", readiness.resolved_backend.as_str()),
        variant: BadgeVariant::Success,
    }
}

#[component]
fn BackendReadinessContent(readiness: TrainingBackendReadinessResponse) -> impl IntoView {
    let summary = derive_readiness_view_state(&readiness);
    let fallback_label = readiness
        .fallback_backend
        .map(|b| {
            let reason = readiness.fallback_reason.as_deref().unwrap_or("fallback");
            format!("{} ({})", b.as_str(), reason)
        })
        .unwrap_or_else(|| "None".to_string());

    let coreml_line = coreml_summary(&readiness);
    let warnings = readiness.warnings.clone();

    view! {
        <div class="space-y-4">
            <div class="flex items-center justify-between gap-3">
                <div class="space-y-1">
                    <p class="text-xs uppercase tracking-wide text-muted-foreground">
                        "Requested backend"
                    </p>
                    <div class="flex items-center gap-2">
                        <Badge variant=summary.variant>{summary.severity.to_uppercase()}</Badge>
                        <span class="text-sm text-muted-foreground">{summary.message}</span>
                    </div>
                </div>
                <div class="text-right">
                    <p class="text-xs uppercase tracking-wide text-muted-foreground">
                        "Resolved"
                    </p>
                    <p class="font-semibold">
                        {readiness.resolved_backend.as_str().to_string()}
                    </p>
                </div>
            </div>

            <div class="grid gap-3 md:grid-cols-2">
                <InfoRow
                    label="Policy"
                    value=readiness.backend_policy.as_str().to_string()
                />
                <InfoRow label="Fallback" value=fallback_label/>
                <InfoRow
                    label="CoreML"
                    value=coreml_line.clone()
                    accent=readiness.coreml.available
                />
                <InfoRow
                    label="Capabilities"
                    value=format_capabilities(&readiness)
                />
            </div>

            <div class="grid gap-3 md:grid-cols-2">
                <CoremlCard readiness=readiness.clone() />
                <BaseModelCard base=readiness.base_model.clone() />
            </div>

            {(!warnings.is_empty()).then(|| view! {
                <div class="rounded-lg border border-warning/50 bg-warning/10 p-3 space-y-1">
                    <p class="text-xs font-semibold uppercase tracking-wide text-warning">
                        "Warnings"
                    </p>
                    <ul class="list-disc pl-4 text-sm text-warning">
                        {warnings.into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
                    </ul>
                </div>
            })}
        </div>
    }
}

#[component]
fn CoremlCard(readiness: TrainingBackendReadinessResponse) -> impl IntoView {
    let coreml = readiness.coreml.clone();
    let badge_variant = if coreml.available {
        BadgeVariant::Success
    } else {
        BadgeVariant::Secondary
    };
    let compute_units = coreml
        .compute_units_effective
        .or(coreml.compute_units_preference)
        .unwrap_or_else(|| "n/a".to_string());

    view! {
        <div class="rounded-lg border p-3 space-y-2">
            <div class="flex items-center justify-between">
                <div>
                    <p class="text-sm font-medium">"CoreML readiness"</p>
                    <p class="text-xs text-muted-foreground">
                        {coreml_summary(&readiness)}
                    </p>
                </div>
                <Badge variant=badge_variant>
                    {if coreml.available { "Available" } else { "Unavailable" }}
                </Badge>
            </div>
            <div class="grid grid-cols-2 gap-2 text-xs">
                <span class="text-muted-foreground">"Compute units"</span>
                <span class="text-right font-mono">{compute_units}</span>
                <span class="text-muted-foreground">"GPU"</span>
                <span class="text-right">
                    {if coreml.gpu_used { "used" } else if coreml.gpu_available { "available" } else { "missing" }}
                </span>
                <span class="text-muted-foreground">"ANE"</span>
                <span class="text-right">
                    {if coreml.ane_used { "used" } else if coreml.ane_available { "available" } else { "missing" }}
                </span>
            </div>
        </div>
    }
}

#[component]
fn BaseModelCard(base: Option<adapteros_api_types::TrainingBaseModelReadiness>) -> impl IntoView {
    let (status_label, badge_variant) = base
        .as_ref()
        .map(|b| match b.status {
            ModelLoadStatus::Ready => ("Active", BadgeVariant::Success),
            ModelLoadStatus::Loading => ("Loading", BadgeVariant::Secondary),
            ModelLoadStatus::Unloading => ("Unloading", BadgeVariant::Secondary),
            ModelLoadStatus::Checking => ("Checking", BadgeVariant::Secondary),
            ModelLoadStatus::Error => ("Error", BadgeVariant::Destructive),
            ModelLoadStatus::NoModel => ("Unloaded", BadgeVariant::Secondary),
        })
        .unwrap_or(("Unknown", BadgeVariant::Secondary));

    let retry_line = base.as_ref().map(|b| {
        if b.retry_exhausted {
            format!(
                "Automatic retries exhausted ({} max) — reload base model.",
                b.max_retries
            )
        } else {
            format!("Automatic retries available (max {}).", b.max_retries)
        }
    });

    view! {
        <div class="rounded-lg border p-3 space-y-2">
            <div class="flex items-center justify-between">
                <div>
                    <p class="text-sm font-medium">"Base model status"</p>
                    <p class="text-xs text-muted-foreground">
                        {base.as_ref()
                            .and_then(|b| b.model_name.clone().or(b.model_id.clone()))
                            .unwrap_or_else(|| "No model recorded".to_string())}
                    </p>
                </div>
                <Badge variant=badge_variant>{status_label}</Badge>
            </div>
            {base.map(|b| {
                let err = b.error_message.clone();
                view! {
                    <div class="space-y-1">
                        {err.map(|e| view! {
                            <p class="text-xs text-destructive break-words">{e}</p>
                        })}
                        {retry_line.map(|line| view! {
                            <p class="text-xs text-muted-foreground">{line}</p>
                        })}
                    </div>
                }
            })}
        </div>
    }
}

#[component]
fn InfoRow(label: &'static str, value: String, #[prop(optional)] accent: bool) -> impl IntoView {
    let value_class = if accent {
        "font-semibold text-foreground"
    } else {
        "text-muted-foreground"
    };

    view! {
        <div class="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
            <span class="text-xs uppercase tracking-wide text-muted-foreground">{label}</span>
            <span class=format!("text-sm {}", value_class)>{value}</span>
        </div>
    }
}

fn coreml_summary(readiness: &TrainingBackendReadinessResponse) -> String {
    let coreml = &readiness.coreml;
    if !coreml.available {
        return "Unavailable".to_string();
    }

    let mut parts = Vec::new();
    if let Some(pref) = coreml.compute_units_preference.as_ref() {
        parts.push(format!("pref {}", pref.replace('_', " ")));
    }
    if let Some(eff) = coreml.compute_units_effective.as_ref() {
        parts.push(format!("using {}", eff.replace('_', " ")));
    }
    if coreml.ane_used {
        parts.push("ANE".to_string());
    }
    if coreml.gpu_used {
        parts.push("GPU".to_string());
    }

    if parts.is_empty() {
        "Available".to_string()
    } else {
        parts.join(" · ")
    }
}

fn format_capabilities(readiness: &TrainingBackendReadinessResponse) -> String {
    let caps = &readiness.capabilities;
    let mut parts = Vec::new();
    if caps.has_coreml {
        parts.push("CoreML");
    }
    if caps.has_ane {
        parts.push("ANE");
    }
    if caps.has_metal {
        parts.push("Metal");
    }
    if caps.has_mlx {
        parts.push("MLX");
    }
    if parts.is_empty() {
        "CPU only".to_string()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_api_types::training::TrainingBackendKind;

    fn sample_response() -> TrainingBackendReadinessResponse {
        TrainingBackendReadinessResponse {
            schema_version: "1.0.0".to_string(),
            requested_backend: TrainingBackendKind::CoreML,
            backend_policy: adapteros_api_types::TrainingBackendPolicy::Auto,
            resolved_backend: TrainingBackendKind::CoreML,
            fallback_backend: None,
            fallback_reason: None,
            ready: true,
            warnings: vec![],
            capabilities: adapteros_api_types::TrainingBackendCapabilities {
                has_coreml: true,
                has_ane: true,
                has_metal: true,
                has_mlx: true,
                has_mlx_bridge: None,
                metal_device_name: None,
                gpu_memory_bytes: None,
            },
            coreml: adapteros_api_types::TrainingCoremlReadiness {
                available: true,
                gpu_available: true,
                ane_available: true,
                compute_units_preference: Some("cpu_and_ne".to_string()),
                compute_units_effective: Some("cpu_and_ne".to_string()),
                gpu_used: false,
                ane_used: true,
                production_mode: false,
            },
            base_model: None,
        }
    }

    #[test]
    fn view_state_marks_fallback() {
        let mut readiness = sample_response();
        readiness.fallback_backend = Some(TrainingBackendKind::Mlx);
        readiness.resolved_backend = TrainingBackendKind::Mlx;
        readiness.fallback_reason = Some("coreml_unavailable".to_string());

        let summary = derive_readiness_view_state(&readiness);
        assert_eq!(summary.severity, "fallback");
        assert_eq!(summary.variant, BadgeVariant::Warning);
    }

    #[test]
    fn view_state_marks_error_on_base_model_failure() {
        let mut readiness = sample_response();
        readiness.base_model = Some(adapteros_api_types::TrainingBaseModelReadiness {
            status: ModelLoadStatus::Error,
            model_id: Some("model-1".to_string()),
            model_name: Some("Demo".to_string()),
            error_message: Some("load failed".to_string()),
            retry_exhausted: true,
            max_retries: 3,
        });

        let summary = derive_readiness_view_state(&readiness);
        assert_eq!(summary.severity, "error");
        assert_eq!(summary.variant, BadgeVariant::Destructive);
    }

    #[test]
    fn view_state_ready_when_no_fallback() {
        let readiness = sample_response();
        let summary = derive_readiness_view_state(&readiness);

        assert_eq!(summary.severity, "ready");
        assert_eq!(summary.variant, BadgeVariant::Success);
    }
}
