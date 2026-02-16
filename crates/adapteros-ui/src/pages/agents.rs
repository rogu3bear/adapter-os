//! Agent Orchestration management page
//!
//! Dashboard for managing multi-agent sessions, worker executors,
//! and orchestration rules.

use crate::api::{ApiClient, ApiError};
use crate::components::{
    Card, Column, DataTable, EmptyState, EmptyStateVariant, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, RefreshButton,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::utils::format_relative_time;
use adapteros_api_types::orchestration::OrchestrationConfig;
use leptos::prelude::*;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct OrchestrationSession {
    #[serde(alias = "session_id")]
    id: String,
    #[serde(default, alias = "state")]
    status: String,
    #[serde(default, alias = "started_at")]
    created_at: String,
    #[serde(default, alias = "adapter_ids")]
    adapters: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum OrchestrationSessionsResponse {
    List(Vec<OrchestrationSession>),
    Wrapped { sessions: Vec<OrchestrationSession> },
}

impl OrchestrationSessionsResponse {
    fn into_sessions(self) -> Vec<OrchestrationSession> {
        match self {
            Self::List(list) => list,
            Self::Wrapped { sessions } => sessions,
        }
    }
}

fn short_id(id: &str) -> String {
    adapteros_id::short_id(id)
}

#[component]
pub fn Agents() -> impl IntoView {
    // Fetch orchestration config
    let (config, refetch_config) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .get::<OrchestrationConfig>("/v1/orchestration/config")
            .await
    });
    let (sessions, refetch_sessions) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .get::<OrchestrationSessionsResponse>("/v1/orchestration/sessions")
            .await
            .map(|resp| resp.into_sessions())
    });

    let refetch_all = {
        Callback::new(move |_| {
            refetch_config.run(());
            refetch_sessions.run(());
        })
    };

    view! {
        <PageScaffold
            title="Agent Orchestration"
            subtitle="Monitor active sessions and orchestration settings"
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Org"),
                PageBreadcrumbItem::current("Agents"),
            ]
        >
            <PageScaffoldActions slot>
                <span
                    class="text-xs text-muted-foreground"
                    title="Session creation is API-only for now. Use POST /v1/orchestration/sessions."
                >
                    "Read-only beta · Create via API: /v1/orchestration/sessions"
                </span>
                <RefreshButton on_click=refetch_all/>
            </PageScaffoldActions>

            <div class="grid gap-6 md:grid-cols-3">
                <Card class="md:col-span-1">
                    <h3 class="text-sm font-semibold mb-4">"Orchestration Detail"</h3>
                    {move || match config.try_get().unwrap_or(LoadingState::Idle) {
                        LoadingState::Loaded(cfg) => {
                            let default_stack_label = cfg
                                .default_adapter_stack
                                .as_deref()
                                .map(short_id)
                                .unwrap_or_else(|| "—".to_string());
                            let default_stack_title =
                                cfg.default_adapter_stack.clone().unwrap_or_default();
                            let fallback_label = if cfg.fallback_enabled {
                                cfg.fallback_adapter
                                    .as_deref()
                                    .map(|id| format!("Enabled ({})", short_id(id)))
                                    .unwrap_or_else(|| "Enabled".to_string())
                            } else {
                                "Disabled".to_string()
                            };
                            let fallback_title = if cfg.fallback_enabled {
                                cfg.fallback_adapter.clone().unwrap_or_default()
                            } else {
                                String::new()
                            };
                            let cache_label = if cfg.cache_enabled {
                                format!("Enabled ({}s)", cfg.cache_ttl_seconds)
                            } else {
                                "Disabled".to_string()
                            };
                            let thresholds_label = {
                                let entropy = cfg
                                    .entropy_threshold
                                    .map(|v| format!("Entropy {:.2}", v));
                                let confidence = cfg
                                    .confidence_threshold
                                    .map(|v| format!("Confidence {:.2}", v));
                                match (entropy, confidence) {
                                    (None, None) => "—".to_string(),
                                    (Some(e), None) => e,
                                    (None, Some(c)) => c,
                                    (Some(e), Some(c)) => format!("{}, {}", e, c),
                                }
                            };
                            let rules_enabled =
                                cfg.custom_rules.iter().filter(|rule| rule.enabled).count();
                            let rules_total = cfg.custom_rules.len();
                            let rules_label = if rules_total == 0 {
                                "0".to_string()
                            } else {
                                format!("{}/{}", rules_enabled, rules_total)
                            };
                            let telemetry_label = if cfg.telemetry_enabled {
                                "Enabled"
                            } else {
                                "Disabled"
                            };

                            view! {
                                <div class="space-y-4">
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Enabled"</span>
                                        <span class=if cfg.enabled { "text-status-success" } else { "text-status-error" }>
                                            {if cfg.enabled { "Active" } else { "Disabled" }}
                                        </span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Strategy"</span>
                                        <span class="text-sm font-mono">{cfg.routing_strategy}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Default Stack"</span>
                                        <span class="text-sm font-mono" title=default_stack_title>{default_stack_label}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Max Adapters"</span>
                                        <span class="text-sm font-mono">{cfg.max_adapters_per_request}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Timeout"</span>
                                        <span class="text-sm font-mono">{format!("{} ms", cfg.timeout_ms)}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Fallback"</span>
                                        <span class="text-sm font-mono" title=fallback_title>{fallback_label}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Thresholds"</span>
                                        <span class="text-sm font-mono">{thresholds_label}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Cache"</span>
                                        <span class="text-sm font-mono">{cache_label}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Telemetry"</span>
                                        <span class="text-sm font-mono">{telemetry_label}</span>
                                    </div>
                                    <div class="flex justify-between items-center">
                                        <span class="text-sm text-muted-foreground">"Custom Rules"</span>
                                        <span class="text-sm font-mono">{rules_label}</span>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loading | LoadingState::Idle => {
                            view! {
                                <div class="flex items-center justify-center h-32 text-muted-foreground gap-2">
                                    <crate::components::Spinner />
                                    <span class="text-sm">"Loading config\u{2026}"</span>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Error(ref e) => {
                            view! {
                                <div class="flex items-center gap-2 text-destructive p-3 rounded-md bg-destructive/10">
                                    <svg class="w-4 h-4 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <circle cx="12" cy="12" r="10"/>
                                        <line x1="12" y1="8" x2="12" y2="12"/>
                                        <line x1="12" y1="16" x2="12.01" y2="16"/>
                                    </svg>
                                    <p class="text-xs">{format!("Failed to load config: {}", e)}</p>
                                </div>
                            }.into_any()
                        }
                    }}
                </Card>

                <div class="md:col-span-2">
                    <Card>
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-sm font-semibold">"Active Sessions"</h3>
                            {move || {
                                if let LoadingState::Loaded(data) = sessions.try_get().unwrap_or(LoadingState::Idle) {
                                    if data.is_empty() {
                                        None
                                    } else {
                                        Some(view! {
                                            <span class="text-xs text-muted-foreground">{format!("{} active", data.len())}</span>
                                        })
                                    }
                                } else {
                                    None
                                }
                            }}
                        </div>
                        {move || match sessions.try_get().unwrap_or(LoadingState::Idle) {
                            LoadingState::Error(ApiError::NotFound(_)) => view! {
                                <EmptyState
                                    title="Sessions Unavailable"
                                    description="Orchestration sessions endpoint is not available on this backend."
                                    variant=EmptyStateVariant::Unavailable
                                    action_label="Retry"
                                    on_action=refetch_sessions.as_callback()
                                />
                            }.into_any(),
                            LoadingState::Error(ApiError::Structured { ref code, .. }) if code == "NOT_FOUND" => view! {
                                <EmptyState
                                    title="Sessions Unavailable"
                                    description="Orchestration sessions endpoint is not available on this backend."
                                    variant=EmptyStateVariant::Unavailable
                                    action_label="Retry"
                                    on_action=refetch_sessions.as_callback()
                                />
                            }.into_any(),
                            _ => {
                                let columns: Vec<Column<OrchestrationSession>> = vec![
                                    Column::custom("Session", |s: &OrchestrationSession| {
                                        let id = s.id.clone();
                                        let label = short_id(&id);
                                        view! {
                                            <span class="font-mono" title=id>{label}</span>
                                        }
                                    }),
                                    Column::custom("Status", |s: &OrchestrationSession| {
                                        let status = if s.status.is_empty() {
                                            "unknown".to_string()
                                        } else {
                                            s.status.clone()
                                        };
                                        view! { <span>{status}</span> }
                                    }),
                                    Column::custom("Created", |s: &OrchestrationSession| {
                                        let created = if s.created_at.is_empty() {
                                            "-".to_string()
                                        } else {
                                            format_relative_time(&s.created_at)
                                        };
                                        view! { <span class="text-muted-foreground">{created}</span> }
                                    }),
                                    Column::custom("Adapters", |s: &OrchestrationSession| {
                                        let adapters = s.adapters.clone().unwrap_or_default();
                                        let label = if adapters.is_empty() {
                                            "\u{2014}".to_string()
                                        } else {
                                            let visible = adapters
                                                .iter()
                                                .take(3)
                                                .map(|id| short_id(id))
                                                .collect::<Vec<_>>()
                                                .join(", ");
                                            if adapters.len() > 3 {
                                                format!("{}, +{}", visible, adapters.len() - 3)
                                            } else {
                                                visible
                                            }
                                        };
                                        let title = if adapters.is_empty() {
                                            String::new()
                                        } else {
                                            adapters.join(", ")
                                        };
                                        view! { <span title=title>{label}</span> }
                                    }),
                                ];

                                view! {
                                    <DataTable
                                        data=sessions
                                        columns=columns
                                        on_retry=refetch_sessions.as_callback()
                                        empty_title="No Active Sessions"
                                        empty_description="Multi-agent inference sessions will appear here when active."
                                        card=false
                                    />
                                }.into_any()
                            }
                        }}
                    </Card>
                </div>
            </div>
        </PageScaffold>
    }
}
