//! Agent Orchestration management page
//!
//! Dashboard for managing multi-agent sessions, worker executors,
//! and orchestration rules.

use crate::api::{ApiClient, ApiError};
use crate::components::{
    Card, EmptyState, EmptyStateVariant, ErrorDisplay, LoadingDisplay, PageHeader, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
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
    let trimmed = id.trim();
    if trimmed.len() > 12 {
        format!("{}...", &trimmed[..12])
    } else {
        trimmed.to_string()
    }
}

#[component]
pub fn Agents() -> impl IntoView {
    // Fetch orchestration config
    let (config, _refetch_config) = use_api_resource(|client: Arc<ApiClient>| async move {
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

    view! {
        <div class="space-y-6">
            <PageHeader
                title="Agent Orchestration"
                subtitle="Manage multi-agent sessions and worker executors"
            />

            <div class="grid gap-6 md:grid-cols-3">
                <Card class="md:col-span-1">
                    <h3 class="text-sm font-semibold mb-4">"Orchestration Status"</h3>
                    {move || match config.get() {
                        LoadingState::Loaded(cfg) => {
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
                                        <span class="text-sm text-muted-foreground">"Max Adapters"</span>
                                        <span class="text-sm font-mono">{cfg.max_adapters_per_request}</span>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loading | LoadingState::Idle => {
                            view! { <div class="animate-pulse h-20 bg-muted rounded-md"></div> }.into_any()
                        }
                        LoadingState::Error(_) => {
                            view! { <p class="text-xs text-destructive">"Failed to load config"</p> }.into_any()
                        }
                    }}
                </Card>

                <div class="md:col-span-2">
                    <Card>
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
                                        on_action=Some(refetch_sessions.clone())
                                    />
                                }.into_any(),
                                other => view! {
                                    <ErrorDisplay error=other on_retry=refetch_sessions.clone()/>
                                }.into_any(),
                            }
                        }}
                    </Card>
                </div>
            </div>
        </div>
    }
}
