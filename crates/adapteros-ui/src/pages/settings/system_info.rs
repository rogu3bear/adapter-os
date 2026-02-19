//! System Info section component

use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, DetailGridRow, ErrorDisplay, Spinner,
};
use crate::hooks::{use_health, LoadingState};
use adapteros_api_types::HealthResponse;
use leptos::prelude::*;

/// System Info section
#[component]
pub fn SystemInfoSection() -> impl IntoView {
    // Fetch health info
    let (health, refetch) = use_health();

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

    view! {
        <div class="space-y-2">
            <DetailGridRow label="Status">
                <Badge variant=status_variant>
                    {health.status.clone()}
                </Badge>
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
