//! Agent Orchestration management page
//!
//! Dashboard for managing multi-agent sessions, worker executors,
//! and orchestration rules.

use crate::api::ApiClient;
use crate::components::{Card, EmptyState, PageHeader};
use crate::hooks::use_api_resource;
use adapteros_api_types::orchestration::OrchestrationConfig;
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn Agents() -> impl IntoView {
    // Fetch orchestration config
    let (config, _refetch_config) = use_api_resource(|client: Arc<ApiClient>| async move {
        client
            .get::<OrchestrationConfig>("/v1/orchestration/config")
            .await
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
                        crate::hooks::LoadingState::Loaded(cfg) => {
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
                        crate::hooks::LoadingState::Loading | crate::hooks::LoadingState::Idle => {
                            view! { <div class="animate-pulse h-20 bg-muted rounded-md"></div> }.into_any()
                        }
                        crate::hooks::LoadingState::Error(_) => {
                            view! { <p class="text-xs text-destructive">"Failed to load config"</p> }.into_any()
                        }
                    }}
                </Card>

                <div class="md:col-span-2">
                    <Card>
                        <EmptyState
                            title="No Active Sessions"
                            description="Multi-agent inference sessions will appear here when active."
                            icon="activity"
                        />
                    </Card>
                </div>
            </div>
        </div>
    }
}
