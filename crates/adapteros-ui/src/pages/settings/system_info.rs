//! System Info section component

use crate::api::{AllModelsStatusResponse, ApiClient, ModelLoadStatus, ModelWithStatsResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, DetailGridRow, ErrorDisplay, Input, Select,
    Spinner, Textarea,
};
use crate::hooks::{use_health, LoadingState};
use crate::signals::use_notifications;
use crate::utils::status_display_label;
use adapteros_api_types::{
    EffectiveSettingsResponse, HealthResponse, ModelSettings, SystemSettings, UpdateSettingsRequest,
};
use leptos::prelude::*;
use std::collections::HashSet;

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
    let model_roots_text = RwSignal::new(String::new());
    let model_path_text = RwSignal::new(String::new());
    let manifest_path_text = RwSignal::new(String::new());
    let model_roots_error = RwSignal::new(None::<String>);
    let model_roots_dirty = RwSignal::new(false);
    let model_roots_saving = RwSignal::new(false);
    let registered_models = RwSignal::new(Vec::<ModelWithStatsResponse>::new());
    let selected_model_id = RwSignal::new(String::new());
    let activating_model = RwSignal::new(false);
    let model_activation_error = RwSignal::new(None::<String>);
    let notifications = use_notifications();

    let model_options = Signal::derive(move || {
        let mut options = vec![("".to_string(), "Select a registered model".to_string())];
        options.extend(registered_models.get().into_iter().map(|model| {
            let label = if model.name == model.id {
                model.id.clone()
            } else {
                format!("{} ({})", model.name, model.id)
            };
            (model.id, label)
        }));
        options
    });

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
            let models_result = client.list_models().await;
            let statuses_result = client.list_models_status().await;

            match (settings_result, effective_result) {
                (Ok(settings), Ok(effective)) => {
                    let models = models_result
                        .map(|resp| resp.models)
                        .unwrap_or_else(|_| Vec::new());
                    let selected_id = resolve_selected_model_id(
                        &settings,
                        &models,
                        statuses_result.as_ref().ok(),
                    );

                    model_roots_text.set(settings.models.discovery_roots.join("\n"));
                    model_path_text.set(
                        settings
                            .models
                            .selected_model_path
                            .clone()
                            .unwrap_or_default(),
                    );
                    manifest_path_text.set(
                        settings
                            .models
                            .selected_manifest_path
                            .clone()
                            .unwrap_or_default(),
                    );
                    selected_model_id.set(selected_id);
                    registered_models.set(models);
                    model_roots_error.set(None);
                    model_activation_error.set(None);
                    model_roots_dirty.set(false);
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

    Effect::new(move || {
        let text = model_roots_text.get();
        let model_path = model_path_text.get();
        let manifest_path = manifest_path_text.get();
        let (current_roots, current_model_path, current_manifest_path) = runtime_settings
            .get()
            .map(|s| {
                (
                    s.models.discovery_roots,
                    s.models.selected_model_path,
                    s.models.selected_manifest_path,
                )
            })
            .unwrap_or_else(|| (Vec::new(), None, None));
        let parsed = parse_discovery_roots_text(&text);
        let parsed_model_path = parse_optional_text(&model_path);
        let parsed_manifest_path = parse_optional_text(&manifest_path);
        model_roots_dirty.set(
            parsed != current_roots
                || parsed_model_path != current_model_path
                || parsed_manifest_path != current_manifest_path,
        );
    });

    let notifications_for_save = notifications.clone();
    let save_model_roots = Callback::new(move |_| {
        if model_roots_saving.get_untracked() {
            return;
        }

        let roots = parse_discovery_roots_text(&model_roots_text.get_untracked());
        let selected_model_path = parse_optional_text(&model_path_text.get_untracked());
        let selected_manifest_path = parse_optional_text(&manifest_path_text.get_untracked());
        if roots.is_empty() {
            model_roots_error.set(Some(
                "Enter at least one directory path for model discovery.".to_string(),
            ));
            return;
        }

        model_roots_saving.set(true);
        model_roots_error.set(None);

        let notifications = notifications_for_save.clone();
        let load_runtime_settings = load_runtime_settings;
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let request = UpdateSettingsRequest {
                general: None,
                models: Some(ModelSettings {
                    discovery_roots: roots,
                    selected_model_path,
                    selected_manifest_path,
                }),
                server: None,
                security: None,
                performance: None,
            };

            match client.update_settings(&request).await {
                Ok(_) => {
                    model_roots_saving.set(false);
                    model_roots_dirty.set(false);
                    notifications.success("Settings updated", "Model discovery roots were saved.");
                    load_runtime_settings.run(());
                }
                Err(e) => {
                    model_roots_saving.set(false);
                    let message = e.user_message();
                    model_roots_error.set(Some(message.clone()));
                    notifications.error("Failed to update settings", &message);
                }
            }
        });
    });

    let reset_model_roots = Callback::new(move |_| {
        let (current_roots, current_model_path, current_manifest_path) = runtime_settings
            .get_untracked()
            .map(|s| {
                (
                    s.models.discovery_roots,
                    s.models.selected_model_path,
                    s.models.selected_manifest_path,
                )
            })
            .unwrap_or_else(|| (Vec::new(), None, None));
        model_roots_text.set(current_roots.join("\n"));
        model_path_text.set(current_model_path.unwrap_or_default());
        manifest_path_text.set(current_manifest_path.unwrap_or_default());
        model_roots_error.set(None);
        model_roots_dirty.set(false);
    });

    let on_model_changed = Callback::new(move |model_id: String| {
        if model_id.trim().is_empty() {
            model_activation_error.set(None);
            return;
        }
        if let Some(path) = model_path_for_id(&registered_models.get_untracked(), &model_id) {
            model_path_text.set(path);
        }
        model_activation_error.set(None);
    });

    let notifications_for_activate = notifications.clone();
    let activate_selected_model = Callback::new(move |_| {
        if activating_model.get_untracked() {
            return;
        }

        let model_id = selected_model_id.get_untracked().trim().to_string();
        if model_id.is_empty() {
            model_activation_error.set(Some("Select a model to activate.".to_string()));
            return;
        }

        let discovery_roots = runtime_settings
            .get_untracked()
            .map(|s| s.models.discovery_roots)
            .unwrap_or_else(|| parse_discovery_roots_text(&model_roots_text.get_untracked()));
        if discovery_roots.is_empty() {
            model_activation_error.set(Some(
                "Configure at least one discovery root before activating a model.".to_string(),
            ));
            return;
        }

        let selected_model_path = model_path_for_id(&registered_models.get_untracked(), &model_id)
            .or_else(|| parse_optional_text(&model_path_text.get_untracked()));
        let selected_manifest_path = parse_optional_text(&manifest_path_text.get_untracked());

        activating_model.set(true);
        model_activation_error.set(None);

        let notifications = notifications_for_activate.clone();
        let load_runtime_settings = load_runtime_settings;
        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::new();
            let request = UpdateSettingsRequest {
                general: None,
                models: Some(ModelSettings {
                    discovery_roots,
                    selected_model_path,
                    selected_manifest_path,
                }),
                server: None,
                security: None,
                performance: None,
            };

            if let Err(e) = client.update_settings(&request).await {
                let message = e.user_message();
                model_activation_error.set(Some(message.clone()));
                notifications.error("Failed to update settings", &message);
                activating_model.set(false);
                return;
            }

            match client.load_model(&model_id).await {
                Ok(_) => {
                    notifications.success(
                        "Base model activated",
                        &format!("{} is now active for inference.", model_id),
                    );
                    load_runtime_settings.run(());
                }
                Err(e) => {
                    let message = e.user_message();
                    model_activation_error.set(Some(message.clone()));
                    notifications.error("Failed to activate model", &message);
                }
            }

            activating_model.set(false);
        });
    });

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

            <Card title="Model Discovery Roots".to_string() description="Control where setup/model discovery searches for local base models.".to_string()>
                <div class="space-y-4">
                    <Select
                        value=selected_model_id
                        options=model_options.get()
                        label="Active base model".to_string()
                        on_change=on_model_changed
                    />
                    <p class="text-xs text-muted-foreground">
                        "Choose a registered model and activate it from Settings. This updates runtime model settings and requests model load without editing env variables."
                    </p>
                    {move || {
                        model_activation_error.get().map(|error| {
                            view! { <div class="text-sm text-destructive">{error}</div> }
                        })
                    }}
                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Primary
                            loading=activating_model
                            on_click=activate_selected_model
                        >
                            "Activate Selected Model"
                        </Button>
                    </div>
                    <Input
                        value=model_path_text
                        label="Worker model path".to_string()
                        placeholder="var/models/Qwen3.5-9B-MLX-4bit".to_string()
                    />
                    <Input
                        value=manifest_path_text
                        label="Worker manifest path (optional)".to_string()
                        placeholder="manifests/qwen35-9b-mlx-base-only.yaml".to_string()
                    />
                    <Textarea
                        value=model_roots_text
                        label="Discovery roots".to_string()
                        placeholder="var/models\n~/.lmstudio/models".to_string()
                        rows=4
                        hint="One path per line (commas and semicolons are also accepted).".to_string()
                    />
                    <p class="text-xs text-muted-foreground">
                        "Worker model path and manifest are used by startup preflight when env overrides are not set. Discovery roots are used by setup discover/seed and model import safety checks."
                    </p>

                    {move || {
                        model_roots_error.get().map(|error| {
                            view! {
                                <div class="text-sm text-destructive">{error}</div>
                            }
                        })
                    }}

                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Outline
                            disabled=Signal::derive(move || !model_roots_dirty.get())
                            on_click=reset_model_roots
                        >
                            "Reset"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            disabled=Signal::derive(move || !model_roots_dirty.get())
                            loading=model_roots_saving
                            on_click=save_model_roots
                        >
                            "Save Roots"
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
    }
}

fn parse_discovery_roots_text(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    raw.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let value = part.to_string();
            if seen.insert(value.clone()) {
                Some(value)
            } else {
                None
            }
        })
        .collect()
}

fn model_path_for_id(models: &[ModelWithStatsResponse], model_id: &str) -> Option<String> {
    models.iter().find_map(|model| {
        if model.id == model_id {
            model
                .model_path
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(|path| path.to_string())
        } else {
            None
        }
    })
}

fn normalize_path_for_match(path: &str) -> String {
    path.trim()
        .trim_end_matches('/')
        .strip_prefix("./")
        .unwrap_or(path.trim().trim_end_matches('/'))
        .to_string()
}

fn resolve_selected_model_id(
    settings: &SystemSettings,
    models: &[ModelWithStatsResponse],
    statuses: Option<&AllModelsStatusResponse>,
) -> String {
    if let Some(selected_path) = settings.models.selected_model_path.as_deref() {
        let target = normalize_path_for_match(selected_path);
        if let Some(model) = models.iter().find(|model| {
            model
                .model_path
                .as_deref()
                .map(normalize_path_for_match)
                .map(|candidate| candidate == target)
                .unwrap_or(false)
        }) {
            return model.id.clone();
        }
    }

    statuses
        .and_then(|status| {
            status
                .models
                .iter()
                .find(|model| model.status == ModelLoadStatus::Ready)
                .map(|model| model.model_id.clone())
        })
        .unwrap_or_default()
}

fn parse_optional_text(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
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
