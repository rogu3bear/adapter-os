//! Agent Orchestration management page
//!
//! Dashboard for managing multi-agent sessions, worker executors,
//! and orchestration rules.

use crate::api::{ApiClient, ApiError};
use crate::components::{
    Card, EmptyState, EmptyStateVariant, ErrorDisplay, LoadingDisplay, PageBreadcrumbItem,
    PageScaffold, PageScaffoldActions, RefreshButton, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
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
            subtitle="Manage multi-agent sessions and worker executors"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Operate", "/agents"),
                PageBreadcrumbItem::current("Agents"),
            ]
        >
            <PageScaffoldActions slot>
                <RefreshButton on_click=refetch_all/>
            </PageScaffoldActions>

            <div class="grid gap-6 md:grid-cols-3">
                <Card class="md:col-span-1">
                    <h3 class="text-sm font-semibold mb-4">"Orchestration Detail"</h3>
                    {move || match config.get() {
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
                            view! { <div class="animate-pulse h-32 bg-muted rounded-md"></div> }.into_any()
                        }
                        LoadingState::Error(_) => {
                            view! { <p class="text-xs text-destructive">"Failed to load config"</p> }.into_any()
                        }
                    }}
                </Card>

                <div class="md:col-span-2">
                    <Card>
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-sm font-semibold">"Active Sessions"</h3>
                            {move || {
                                if let LoadingState::Loaded(data) = sessions.get() {
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
                        {move || match sessions.get() {
                            LoadingState::Idle | LoadingState::Loading => {
                                view! { <LoadingDisplay message="Loading sessions..."/> }.into_any()
                            }
                            LoadingState::Loaded(data) => {
                                if data.is_empty() {
                                    view! {
                                        <EmptyState
                                            title="No Active Sessions"
                                            description="Multi-agent inference sessions will appear here when active."
                                            icon="activity"
                                        />
                                    }.into_any()
                                } else {
                                    view! {
                                        <Table>
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>"Session"</TableHead>
                                                    <TableHead>"Status"</TableHead>
                                                    <TableHead>"Created"</TableHead>
                                                    <TableHead>"Adapters"</TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {data
                                                    .into_iter()
                                                    .map(|session| {
                                                        let session_id = session.id.clone();
                                                        let session_label = short_id(&session_id);
                                                        let status = if session.status.is_empty() {
                                                            "unknown".to_string()
                                                        } else {
                                                            session.status.clone()
                                                        };
                                                        let created_at = if session.created_at.is_empty() {
                                                            "-".to_string()
                                                        } else {
                                                            session.created_at.clone()
                                                        };
                                                        let adapters = session.adapters.unwrap_or_default();
                                                        let adapters_label = if adapters.is_empty() {
                                                            "—".to_string()
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
                                                        let adapters_title = if adapters.is_empty() {
                                                            String::new()
                                                        } else {
                                                            adapters.join(", ")
                                                        };

                                                        view! {
                                                            <TableRow>
                                                                <TableCell class="font-mono">
                                                                    <span title=session_id>{session_label}</span>
                                                                </TableCell>
                                                                <TableCell>{status}</TableCell>
                                                                <TableCell class="text-muted-foreground">{created_at}</TableCell>
                                                                <TableCell>
                                                                    <span title=adapters_title>{adapters_label}</span>
                                                                </TableCell>
                                                            </TableRow>
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()}
                                            </TableBody>
                                        </Table>
                                    }
                                    .into_any()
                                }
                            }
                            LoadingState::Error(error) => match error {
                                ApiError::NotFound(_) => view! {
                                    <EmptyState
                                        title="Sessions Unavailable"
                                        description="Orchestration sessions endpoint is not available on this backend."
                                        variant=EmptyStateVariant::Unavailable
                                        action_label="Retry"
                                        on_action=refetch_sessions.as_callback()
                                    />
                                }.into_any(),
                                ApiError::Structured { code, .. } if code == "NOT_FOUND" => view! {
                                    <EmptyState
                                        title="Sessions Unavailable"
                                        description="Orchestration sessions endpoint is not available on this backend."
                                        variant=EmptyStateVariant::Unavailable
                                        action_label="Retry"
                                        on_action=refetch_sessions.as_callback()
                                    />
                                }.into_any(),
                                other => view! {
                                    <ErrorDisplay error=other on_retry=refetch_sessions.as_callback()/>
                                }.into_any(),
                            }
                        }}
                    </Card>
                </div>
            </div>
        </PageScaffold>
    }
}
