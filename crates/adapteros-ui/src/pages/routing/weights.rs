//! Router Weights editor
//!
//! View and adjust the feature importance weights used by the K-sparse LoRA router.
//! Changes are persisted per-tenant in tenant settings.

use crate::api::{report_error_with_toast, use_api_client, ApiClient};
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, Spinner};
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// Response from GET /v1/tenants/{tenant_id}/router/weights
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RouterWeightsResponse {
    pub tenant_id: String,
    pub language_weight: f64,
    pub framework_weight: f64,
    pub symbol_hits_weight: f64,
    pub path_tokens_weight: f64,
    pub prompt_verb_weight: f64,
    pub orthogonal_weight: f64,
    pub diversity_weight: f64,
    pub similarity_penalty: f64,
    pub total_weight: f64,
    pub is_default: bool,
}

/// Request for PUT /v1/tenants/{tenant_id}/router/weights
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UpdateRouterWeightsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_hits_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_tokens_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_verb_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orthogonal_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diversity_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_penalty: Option<f64>,
}

/// Routing Weights editor component
#[component]
pub fn RoutingWeights() -> impl IntoView {
    let client = use_api_client();
    let saving = RwSignal::new(false);
    let resetting = RwSignal::new(false);

    // Local weight signals for slider controls
    let language_w = RwSignal::new(0.0_f64);
    let framework_w = RwSignal::new(0.0_f64);
    let symbol_hits_w = RwSignal::new(0.0_f64);
    let path_tokens_w = RwSignal::new(0.0_f64);
    let prompt_verb_w = RwSignal::new(0.0_f64);
    let orthogonal_w = RwSignal::new(0.0_f64);
    let diversity_w = RwSignal::new(0.0_f64);
    let similarity_w = RwSignal::new(0.0_f64);
    let is_default = RwSignal::new(true);

    // Total weight (derived)
    let total_weight = Signal::derive(move || {
        language_w.try_get().unwrap_or(0.0)
            + framework_w.try_get().unwrap_or(0.0)
            + symbol_hits_w.try_get().unwrap_or(0.0)
            + path_tokens_w.try_get().unwrap_or(0.0)
            + prompt_verb_w.try_get().unwrap_or(0.0)
            + orthogonal_w.try_get().unwrap_or(0.0)
            + diversity_w.try_get().unwrap_or(0.0)
            + similarity_w.try_get().unwrap_or(0.0)
    });

    // Fetch current weights
    let (weights_state, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client
            .get::<RouterWeightsResponse>("/v1/tenants/me/router/weights")
            .await
    });

    // Populate local signals when data loads
    Effect::new(move || {
        if let LoadingState::Loaded(ref data) =
            weights_state.try_get().unwrap_or(LoadingState::Idle)
        {
            language_w.set(data.language_weight);
            framework_w.set(data.framework_weight);
            symbol_hits_w.set(data.symbol_hits_weight);
            path_tokens_w.set(data.path_tokens_weight);
            prompt_verb_w.set(data.prompt_verb_weight);
            orthogonal_w.set(data.orthogonal_weight);
            diversity_w.set(data.diversity_weight);
            similarity_w.set(data.similarity_penalty);
            is_default.set(data.is_default);
        }
    });

    // Save handler
    let on_save = {
        let client = client.clone();
        move |_| {
            saving.set(true);
            let client = client.clone();
            let req = UpdateRouterWeightsRequest {
                language_weight: Some(language_w.try_get().unwrap_or(0.0)),
                framework_weight: Some(framework_w.try_get().unwrap_or(0.0)),
                symbol_hits_weight: Some(symbol_hits_w.try_get().unwrap_or(0.0)),
                path_tokens_weight: Some(path_tokens_w.try_get().unwrap_or(0.0)),
                prompt_verb_weight: Some(prompt_verb_w.try_get().unwrap_or(0.0)),
                orthogonal_weight: Some(orthogonal_w.try_get().unwrap_or(0.0)),
                diversity_weight: Some(diversity_w.try_get().unwrap_or(0.0)),
                similarity_penalty: Some(similarity_w.try_get().unwrap_or(0.0)),
            };
            let refetch = refetch.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .put::<UpdateRouterWeightsRequest, RouterWeightsResponse>(
                        "/v1/tenants/me/router/weights",
                        &req,
                    )
                    .await
                {
                    Ok(data) => {
                        is_default.set(data.is_default);
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to save weights", None, false);
                    }
                }
                saving.set(false);
                refetch.run(());
            });
        }
    };

    // Reset handler
    let on_reset = {
        let client = client.clone();
        move |_| {
            resetting.set(true);
            let client = client.clone();
            let refetch = refetch.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .post_empty::<RouterWeightsResponse>("/v1/tenants/me/router/weights/reset")
                    .await
                {
                    Ok(data) => {
                        language_w.set(data.language_weight);
                        framework_w.set(data.framework_weight);
                        symbol_hits_w.set(data.symbol_hits_weight);
                        path_tokens_w.set(data.path_tokens_weight);
                        prompt_verb_w.set(data.prompt_verb_weight);
                        orthogonal_w.set(data.orthogonal_weight);
                        diversity_w.set(data.diversity_weight);
                        similarity_w.set(data.similarity_penalty);
                        is_default.set(data.is_default);
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to reset weights", None, false);
                    }
                }
                resetting.set(false);
                refetch.run(());
            });
        }
    };

    let total_variant = Signal::derive(move || {
        let t = total_weight.try_get().unwrap_or(0.0);
        if (t - 1.0).abs() < 0.05 {
            BadgeVariant::Success
        } else {
            BadgeVariant::Warning
        }
    });

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <div>
                    <h2 class="heading-2">"Router Weights"</h2>
                    <p class="text-muted-foreground mt-1">
                        "Adjust feature importance weights that determine how the K-sparse router scores adapters."
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    {move || {
                        if is_default.try_get().unwrap_or(true) {
                            Some(view! {
                                <Badge variant=BadgeVariant::Secondary>"Using Defaults"</Badge>
                            })
                        } else {
                            Some(view! {
                                <Badge variant=BadgeVariant::Default>"Custom Weights"</Badge>
                            })
                        }
                    }}
                </div>
            </div>

            {move || {
                match weights_state.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <Card>
                                <p class="text-destructive">{e.to_string()}</p>
                            </Card>
                        }.into_any()
                    }
                    LoadingState::Loaded(_) => {
                        view! {
                            <div class="space-y-4">
                                // Total weight summary
                                <Card>
                                    <div class="flex items-center justify-between">
                                        <span class="text-sm font-medium">"Total Weight"</span>
                                        <Badge variant=total_variant.try_get().unwrap_or(BadgeVariant::Warning)>
                                            {move || format!("{:.4}", total_weight.try_get().unwrap_or(0.0))}
                                        </Badge>
                                    </div>
                                    <p class="text-xs text-muted-foreground mt-1">
                                        "Weights should sum to approximately 1.0 for normalized scoring."
                                    </p>
                                </Card>

                                // Weight sliders
                                <Card title="Feature Weights".to_string()>
                                    <div class="weight-editor-grid">
                                        <WeightSlider label="Language" signal=language_w description="Language detection signal strength" />
                                        <WeightSlider label="Framework" signal=framework_w description="Framework detection signal strength" />
                                        <WeightSlider label="Symbol Hits" signal=symbol_hits_w description="Code symbol matching signal" />
                                        <WeightSlider label="Path Tokens" signal=path_tokens_w description="File path token matching signal" />
                                        <WeightSlider label="Prompt Verb" signal=prompt_verb_w description="Prompt verb classification signal" />
                                        <WeightSlider label="Orthogonal" signal=orthogonal_w description="Orthogonal constraint penalty" />
                                        <WeightSlider label="Diversity" signal=diversity_w description="Adapter diversity bonus" />
                                        <WeightSlider label="Similarity Penalty" signal=similarity_w description="Duplicate adapter similarity penalty" />
                                    </div>
                                </Card>

                                // Action buttons
                                <div class="flex items-center gap-3">
                                    <Button
                                        variant=ButtonVariant::Primary
                                        loading=Signal::derive(move || saving.try_get().unwrap_or(false))
                                        on_click=Callback::new(on_save.clone())
                                    >
                                        "Save Weights"
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Outline
                                        loading=Signal::derive(move || resetting.try_get().unwrap_or(false))
                                        on_click=Callback::new(on_reset.clone())
                                    >
                                        "Reset to Defaults"
                                    </Button>
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Individual weight slider control
#[component]
fn WeightSlider(
    label: &'static str,
    signal: RwSignal<f64>,
    description: &'static str,
) -> impl IntoView {
    let display_value = Signal::derive(move || format!("{:.4}", signal.try_get().unwrap_or(0.0)));

    // Percentage for bar width
    let bar_pct = Signal::derive(move || {
        let v = signal.try_get().unwrap_or(0.0);
        format!("{:.1}%", (v * 100.0).min(100.0).max(0.0))
    });

    view! {
        <div class="weight-slider-row">
            <div class="weight-slider-header">
                <span class="text-sm font-medium">{label}</span>
                <span class="text-sm font-mono text-muted-foreground">{move || display_value.try_get().unwrap_or_default()}</span>
            </div>
            <p class="text-xs text-muted-foreground">{description}</p>
            <div class="weight-slider-track">
                <input
                    type="range"
                    min="0"
                    max="0.5"
                    step="0.001"
                    class="weight-slider-input"
                    aria_label=label
                    prop:value=move || signal.try_get().unwrap_or(0.0).to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                            signal.set(v);
                        }
                    }
                />
                <div class="weight-slider-bar" style:width=move || bar_pct.try_get().unwrap_or_default()></div>
            </div>
        </div>
    }
}
