//! System Info section component

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, DetailGridRow, ErrorDisplay, Spinner,
};
use crate::hooks::{use_health, LoadingState};
use crate::utils::status_display_label;
use adapteros_api_types::{EffectiveSettingsResponse, HealthResponse, SystemSettings};
use leptos::prelude::*;

/// System Info section
#[component]
pub fn SystemInfoSection() -> impl IntoView {
    // Fetch health info
    let (health, refetch) = use_health();
    let runtime_settings = RwSignal::new(None::<SystemSettings>);
    let effective_settings = RwSignal::new(None::<EffectiveSettingsResponse>);
    let runtime_error = RwSignal::new(None::<String>);
    let runtime_loading = RwSignal::new(false);
    let runtime_loaded_once = RwSignal::new(false);

    let load_runtime_settings = Callback::new(move |_| {
        if runtime_loading.get_untracked() {
            return;
        }
        runtime_loading.set(true);
        runtime_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let settings_result = client.get_settings().await;
            let effective_result = client.get_effective_settings().await;

            match (settings_result, effective_result) {
                (Ok(settings), Ok(effective)) => {
                    runtime_settings.set(Some(settings));
                    effective_settings.set(Some(effective));
                }
                (Err(e), _) => runtime_error.set(Some(e.user_message())),
                (_, Err(e)) => runtime_error.set(Some(e.user_message())),
            }

            runtime_loaded_once.set(true);
            runtime_loading.set(false);
        });
    });

    {
        let load_runtime_settings = load_runtime_settings;
        Effect::new(move || {
            if !runtime_loaded_once.get() {
                load_runtime_settings.run(());
            }
        });
    }

    view! {
        <div class="space-y-6 max-w-2xl">
            // UI Version
            <Card title="UI Version".to_string() description="Frontend application version.".to_string()>
                <div class="space-y-2">
                    <DetailGridRow label="Version" mono=true>
                        <span class="text-sm font-mono">{env!("CARGO_PKG_VERSION")}</span>
                    </DetailGridRow>
                    <DetailGridRow label="Build ID" mono=true>
                        <span class="text-sm font-mono">{option_env!("AOS_BUILD_ID").unwrap_or("unknown")}</span>
                    </DetailGridRow>
                    <DetailGridRow label="Framework">
                        <span class="text-sm">"Leptos 0.7 (CSR)"</span>
                    </DetailGridRow>
                    <DetailGridRow label="Target" mono=true>
                        <span class="text-sm font-mono">"wasm32-unknown-unknown"</span>
                    </DetailGridRow>
                </div>
            </Card>

            // API Version
            <Card title="API Version".to_string() description="Backend API and runtime information.".to_string()>
                <div class="flex items-center justify-between mb-4">
                    <span class="text-sm text-muted-foreground">"Backend health status from /healthz"</span>
                    <Button
                        variant=ButtonVariant::Outline
                        size=crate::components::ButtonSize::Sm
                        on_click=Callback::new(move |_| refetch.run(()))
                    >
                        "Refresh"
                    </Button>
                </div>

                {move || {
                    match health.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center gap-2">
                                    <Spinner/>
                                    <span class="text-sm text-muted-foreground">"Loading..."</span>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            view! { <HealthInfo health=data/> }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <ErrorDisplay error=e on_retry=Callback::new(move |_| refetch.run(()))/>
                            }.into_any()
                        }
                    }
                }}
            </Card>

            // Build Info
            <Card title="Build Information".to_string() description="Compilation and environment details.".to_string()>
                <div class="space-y-2">
                    <DetailGridRow label="API Schema Version" mono=true>
                        <span class="text-sm font-mono">{adapteros_api_types::API_SCHEMA_VERSION}</span>
                    </DetailGridRow>
                    <DetailGridRow label="Build Profile">
                        <span class="text-sm">
                            {if cfg!(debug_assertions) { "Debug" } else { "Release" }}
                        </span>
                    </DetailGridRow>
                </div>
            </Card>

            // Runtime Settings
            <Card title="Runtime Settings".to_string() description="Server-managed settings and source metadata.".to_string()>
                <div class="flex items-center justify-between mb-4">
                    <span class="text-sm text-muted-foreground">"Backed by /v1/settings and /v1/settings/effective"</span>
                    <Button
                        variant=ButtonVariant::Outline
                        size=crate::components::ButtonSize::Sm
                        on_click=Callback::new({
                            let load_runtime_settings = load_runtime_settings;
                            move |_| load_runtime_settings.run(())
                        })
                    >
                        "Refresh Runtime Settings"
                    </Button>
                </div>

                {move || {
                    if runtime_loading.get() {
                        return view! {
                            <div class="flex items-center gap-2">
                                <Spinner/>
                                <span class="text-sm text-muted-foreground">"Loading runtime settings..."</span>
                            </div>
                        }.into_any();
                    }

                    if let Some(err) = runtime_error.get() {
                        return view! {
                            <div class="text-sm text-destructive">{err}</div>
                        }.into_any();
                    }

                    let Some(settings) = runtime_settings.get() else {
                        return view! {
                            <div class="text-sm text-muted-foreground">"No runtime settings available yet."</div>
                        }.into_any();
                    };

                    let source = settings
                        .effective_source
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    let pending = settings.pending_restart_fields.clone();
                    let managed_count = effective_settings
                        .get()
                        .map(|e| e.managed_keys.len())
                        .unwrap_or(0);

                    view! {
                        <div class="space-y-2">
                            <DetailGridRow label="Effective Source" mono=true>
                                <span class="text-sm font-mono">{source}</span>
                            </DetailGridRow>
                            <DetailGridRow label="Applied At" mono=true>
                                <span class="text-sm font-mono">{settings.applied_at.unwrap_or_else(|| "n/a".to_string())}</span>
                            </DetailGridRow>
                            <DetailGridRow label="Managed Keys">
                                <span class="text-sm">{managed_count.to_string()}</span>
                            </DetailGridRow>
                            <DetailGridRow label="Pending Restart Fields">
                                <span class="text-sm">{pending.len().to_string()}</span>
                            </DetailGridRow>
                        </div>
                    }.into_any()
                }}
            </Card>
        </div>
    }
}

/// Health info display
#[component]
fn HealthInfo(health: HealthResponse) -> impl IntoView {
    let status_variant = match health.status.as_str() {
        "ok" | "healthy" => BadgeVariant::Success,
        "degraded" | "warning" => BadgeVariant::Warning,
        _ => BadgeVariant::Destructive,
    };
    let health_status = health.status.clone();
    let health_status_label = status_display_label(&health_status);

    view! {
        <div class="space-y-2">
            <DetailGridRow label="Status">
                <span title=health_status.clone()>
                    <Badge variant=status_variant>
                        {health_status_label}
                    </Badge>
                </span>
            </DetailGridRow>
            <DetailGridRow label="Version" mono=true>
                <span class="text-sm font-mono">{health.version.clone()}</span>
            </DetailGridRow>
            <DetailGridRow label="Schema Version" mono=true>
                <span class="text-sm font-mono">{health.schema_version.clone()}</span>
            </DetailGridRow>

            // Model runtime health
            {health.models.map(|models| view! {
                <div class="mt-4 pt-4 border-t">
                    <h4 class="text-sm font-medium mb-2">"Model Runtime"</h4>
                    <div class="space-y-2">
                        <DetailGridRow label="Models Loaded">
                            <span class="text-sm">
                                {format!("{} / {}", models.loaded_count, models.total_models)}
                            </span>
                        </DetailGridRow>
                        <DetailGridRow label="Health">
                            {if models.healthy {
                                view! {
                                    <Badge variant=BadgeVariant::Success>"Healthy"</Badge>
                                }.into_any()
                            } else {
                                view! {
                                    <Badge variant=BadgeVariant::Destructive>"Unhealthy"</Badge>
                                }.into_any()
                            }}
                        </DetailGridRow>
                        {if models.inconsistencies_count > 0 {
                            Some(view! {
                                <DetailGridRow label="Inconsistencies">
                                    <span class="text-sm text-destructive">
                                        {models.inconsistencies_count.to_string()}
                                    </span>
                                </DetailGridRow>
                            })
                        } else {
                            None
                        }}
                    </div>
                </div>
            })}
        </div>
    }
}
