//! Dataset detail tab components.
//!
//! Extracted from datasets.rs to reduce nesting and improve maintainability.

use crate::api::{
    ApiClient, DatasetFileResponse, DatasetPreviewResponse, DatasetResponse,
    DatasetStatisticsResponse, DatasetVersionsResponse, JsonlValidationDiagnostic,
};
use crate::components::{
    Badge, BadgeVariant, Card, CopyableId, EmptyState, ErrorDisplay, Select, SkeletonCard,
    SkeletonDetailSection, SkeletonTable, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow, Toggle,
};
use crate::hooks::{use_api, use_api_resource, LoadingState};
use crate::utils::{format_bytes, format_date};
use leptos::prelude::*;
use std::sync::Arc;

fn trust_state_badge_variant(state: &str) -> BadgeVariant {
    match state {
        "allowed" | "trusted" | "approved" => BadgeVariant::Success,
        "needs_approval" | "pending" => BadgeVariant::Warning,
        "blocked" | "rejected" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

fn validation_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "valid" | "ready" => BadgeVariant::Success,
        "invalid" | "failed" => BadgeVariant::Destructive,
        "pending" | "processing" => BadgeVariant::Warning,
        _ => BadgeVariant::Secondary,
    }
}

/// Preview tab content: first N examples with limit/pretty-json controls.
#[component]
pub fn DatasetDetailTabPreview(
    dataset_id: String,
    on_refresh: Callback<()>,
    /// When this signal changes, the preview will refetch (e.g. when parent Refresh is clicked).
    #[prop(optional)]
    refresh_trigger: Option<ReadSignal<u32>>,
) -> impl IntoView {
    let preview_limit = RwSignal::new("10".to_string());
    let pretty_json = RwSignal::new(true);
    let (preview, preview_refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = dataset_id.clone();
        let limit_raw = preview_limit.try_get().unwrap_or_default();
        async move {
            let limit = limit_raw.parse::<usize>().ok();
            client.preview_dataset(&id, limit).await
        }
    });

    if let Some(trigger) = refresh_trigger {
        Effect::new(move |_| {
            let _ = trigger.try_get();
            preview_refetch.run(());
        });
    }

    let trigger_refresh = StoredValue::new(on_refresh);

    view! {
        <Card>
            <div class="p-4 space-y-3">
                <div class="flex items-center justify-between gap-3 flex-wrap">
                    <div>
                        <h3 class="heading-4">"Preview"</h3>
                        <p class="text-sm text-muted-foreground">
                            "First N examples (read-only) for a quick sanity check."
                        </p>
                    </div>
                    <div class="flex items-center gap-3">
                        <Select
                            value=preview_limit
                            options=vec![
                                ("10".to_string(), "10".to_string()),
                                ("25".to_string(), "25".to_string()),
                                ("50".to_string(), "50".to_string()),
                            ]
                            class="w-24".to_string()
                        />
                        <Toggle
                            checked=pretty_json
                            label="Pretty JSON".to_string()
                            class="w-auto"
                        />
                    </div>
                </div>

                {move || match preview.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonCard has_header=true/> }.into_any()
                    }
                    LoadingState::Loaded(DatasetPreviewResponse { examples, total_examples, .. }) => {
                        if examples.is_empty() {
                            view! {
                                <EmptyState
                                    title="No preview available"
                                    description="This dataset has no readable examples, or you don't have access."
                                />
                            }.into_any()
                        } else {
                            let pretty = pretty_json.try_get().unwrap_or(true);
                            view! {
                                <div class="space-y-2">
                                    <div class="text-xs text-muted-foreground">
                                        {format!(
                                            "Returned {} example(s) (server reported total_examples={})",
                                            examples.len(),
                                            total_examples
                                        )}
                                    </div>
                                    <div class="space-y-3">
                                        {examples.into_iter().enumerate().map(|(idx, ex)| {
                                            let rendered = if pretty {
                                                serde_json::to_string_pretty(&ex).unwrap_or_else(|_| ex.to_string())
                                            } else {
                                                serde_json::to_string(&ex).unwrap_or_else(|_| ex.to_string())
                                            };
                                            view! {
                                                <div class="rounded border border-muted bg-muted/30 p-3">
                                                    <div class="text-xs text-muted-foreground mb-2">
                                                        {format!("Example {}", idx + 1)}
                                                    </div>
                                                    <pre class="font-mono text-xs whitespace-pre-wrap break-words">{rendered}</pre>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh.with_value(|f| f.run(())))
                            />
                        }.into_any()
                    }
                }}
            </div>
        </Card>
    }
}

/// Issues tab content: validation errors and diagnostics.
#[component]
pub fn DatasetDetailTabIssues(
    validation_errors: Option<Vec<String>>,
    validation_diagnostics: Option<Vec<JsonlValidationDiagnostic>>,
) -> impl IntoView {
    let errors = validation_errors.unwrap_or_default();
    let diags = validation_diagnostics.unwrap_or_default();

    view! {
        {move || {
            if errors.is_empty() && diags.is_empty() {
                view! {
                    <Card>
                        <EmptyState
                            title="No issues detected"
                            description="This dataset has no validation errors or diagnostics."
                        />
                    </Card>
                }.into_any()
            } else {
                let errs = errors.clone();
                let dgs = diags.clone();
                view! {
                    <div class="space-y-4">
                        {(!errors.is_empty()).then(move || view! {
                            <Card>
                                <div class="p-4">
                                    <h3 class="heading-4 mb-2" id="validation-errors">"Validation Errors"</h3>
                                    <ul class="space-y-2 text-sm text-destructive">
                                        {errs.into_iter().map(|err| view! { <li>{err}</li> }).collect_view()}
                                    </ul>
                                </div>
                            </Card>
                        })}

                        {(!diags.is_empty()).then(move || view! {
                            <Card>
                                <div class="p-4">
                                    <h3 class="heading-4 mb-2">"Validation Diagnostics"</h3>
                                    <div class="space-y-3 text-sm">
                                        {dgs.into_iter().map(|diag| view! {
                                            <div class="rounded border border-muted p-3">
                                                <div class="flex items-center justify-between">
                                                    <span class="text-muted-foreground">"Line"</span>
                                                    <span class="font-mono">{diag.line_number.to_string()}</span>
                                                </div>
                                                {diag.raw_snippet.map(|snippet| view! {
                                                    <div class="mt-2 font-mono text-xs text-muted-foreground truncate">{snippet}</div>
                                                })}
                                                {diag.missing_fields.map(|fields| view! {
                                                    <div class="mt-2">
                                                        <span class="text-muted-foreground">"Missing: "</span>
                                                        <span>{fields.join(", ")}</span>
                                                    </div>
                                                })}
                                                {diag.invalid_field_types.map(|fields| view! {
                                                    <div class="mt-2">
                                                        <span class="text-muted-foreground">"Invalid types: "</span>
                                                        <span>
                                                            {fields
                                                                .iter()
                                                                .map(|field| format!("{} ({} -> {})", field.field, field.actual, field.expected))
                                                                .collect::<Vec<_>>()
                                                                .join(", ")}
                                                        </span>
                                                    </div>
                                                })}
                                                {diag.contract_version_expected.map(|version| view! {
                                                    <div class="mt-2 text-muted-foreground">
                                                        "Contract version expected: " {version}
                                                    </div>
                                                })}
                                            </div>
                                        }).collect_view()}
                                    </div>
                                </div>
                            </Card>
                        })}
                    </div>
                }.into_any()
            }
        }}
    }
}

/// Versions tab content: dataset version history.
#[component]
pub fn DatasetDetailTabVersions(
    dataset_id: String,
    versions: ReadSignal<LoadingState<DatasetVersionsResponse>>,
    dataset_version_id: Option<String>,
    on_refresh: Callback<()>,
) -> impl IntoView {
    let dataset_version_id_store = StoredValue::new(dataset_version_id.clone());
    let trigger_refresh = StoredValue::new(on_refresh);

    view! {
        <Card>
            <div class="p-4 space-y-3">
                <h3 class="heading-4">"Versions"</h3>
                {dataset_version_id_store.get_value().map(|id| view! {
                    <CopyableId id=id label="Current dataset_version_id".to_string() truncate=28 />
                })}
                {move || match versions.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonTable rows=3 columns=3/> }.into_any()
                    }
                    LoadingState::Loaded(DatasetVersionsResponse { versions: vers, .. }) => {
                        if vers.is_empty() {
                            view! { <p class="text-sm text-muted-foreground">"No dataset versions found."</p> }.into_any()
                        } else {
                            let current = dataset_version_id_store.get_value();
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Version"</TableHead>
                                            <TableHead>"Label"</TableHead>
                                            <TableHead>"Trust"</TableHead>
                                            <TableHead>"Hash"</TableHead>
                                            <TableHead>"Created"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {vers.into_iter().map(|version| {
                                            let trust_state = version.trust_state.clone().unwrap_or_else(|| "unknown".to_string());
                                            let trust_variant = trust_state_badge_variant(&trust_state);
                                            let hash = version
                                                .hash_b3
                                                .clone()
                                                .map(|h| h.chars().take(10).collect::<String>())
                                                .unwrap_or_else(|| "—".to_string());
                                            let is_current = current.as_ref().map(|c| c == &version.dataset_version_id).unwrap_or(false);
                                            let row_class = if is_current { "bg-muted/50".to_string() } else { String::new() };
                                            view! {
                                                <TableRow class=row_class>
                                                    <TableCell>
                                                        <div class="space-y-1">
                                                            <div class="flex items-center gap-2">
                                                                <div class="font-medium">
                                                                    {"v"}{version.version_number.to_string()}
                                                                </div>
                                                                {is_current.then(|| view! { <Badge variant=BadgeVariant::Success>"Current"</Badge> })}
                                                            </div>
                                                            <div class="text-xs text-muted-foreground font-mono truncate max-w-xs">
                                                                {version.dataset_version_id.clone()}
                                                            </div>
                                                            {version.repo_slug.clone().map(|slug| view! {
                                                                <div class="text-xs text-muted-foreground truncate">{slug}</div>
                                                            })}
                                                        </div>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm text-muted-foreground">
                                                            {version.version_label.clone().unwrap_or_else(|| "—".to_string())}
                                                        </span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <Badge variant=trust_variant>{trust_state}</Badge>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="font-mono text-xs text-muted-foreground">{hash}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm text-muted-foreground">
                                                            {format_date(&version.created_at)}
                                                        </span>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(_) => {
                        view! { <p class="text-sm text-muted-foreground">"Versions unavailable"</p> }.into_any()
                    }
                }}
            </div>
        </Card>

        {move || {
            let opt_view = match versions.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(DatasetVersionsResponse { versions: vers, .. }) => {
                    let preferred = dataset_version_id_store.get_value()
                        .or_else(|| vers.first().map(|v| v.dataset_version_id.clone()));
                    preferred.map(|id| view! {
                        <Card>
                            <div class="p-4 space-y-2">
                                <h3 class="heading-4">"Usage"</h3>
                                <p class="text-sm text-muted-foreground">
                                    "Use a dataset version ID in inference or training to pin the exact data snapshot."
                                </p>
                                <div class="rounded-md bg-muted p-3 font-mono text-sm break-all">
                                    {format!("dataset_version_id: \"{}\"", id)}
                                </div>
                            </div>
                        </Card>
                    })
                }
                _ => None,
            };
            opt_view.into_any()
        }}
    }
}

/// Files tab content: dataset file list with expandable content preview.
#[component]
pub fn DatasetDetailTabFiles(
    dataset_id: String,
    files: ReadSignal<LoadingState<Vec<DatasetFileResponse>>>,
    on_refresh: Callback<()>,
) -> impl IntoView {
    let client = use_api();
    let expanded_file_id = RwSignal::new(Option::<String>::None);
    let file_content = RwSignal::new(Option::<Result<String, String>>::None);
    let file_content_truncated = RwSignal::new(false);
    let trigger_refresh = StoredValue::new(on_refresh);

    view! {
        <Card>
            <div class="p-4 space-y-3">
                <div>
                    <h3 class="heading-4">"Files"</h3>
                    <p class="text-sm text-muted-foreground">
                        "Individual files within this dataset. Click a row to preview content."
                    </p>
                </div>
                {move || match files.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! { <SkeletonTable rows=3 columns=5/> }.into_any()
                    }
                    LoadingState::Loaded(file_list) => {
                        if file_list.is_empty() {
                            view! {
                                <EmptyState
                                    title="No files"
                                    description="This dataset has no individual files, or files are not tracked separately."
                                />
                            }.into_any()
                        } else {
                            let ds_id = dataset_id.clone();
                            let client_for_fetch = StoredValue::new(client.clone());
                            view! {
                                <Table>
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead>"Name"</TableHead>
                                            <TableHead>"Size"</TableHead>
                                            <TableHead>"MIME Type"</TableHead>
                                            <TableHead>"Created"</TableHead>
                                            <TableHead>"Hash"</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {file_list.into_iter().map(|file| {
                                            let ds_id = ds_id.clone();
                                            let fid = file.file_id.clone();
                                            let fid_for_click = fid.clone();
                                            let fid_for_expand = fid.clone();
                                            let hash_short = if file.hash.len() > 10 {
                                                format!("{}...", &file.hash[..10])
                                            } else {
                                                file.hash.clone()
                                            };
                                            let mime = file.mime_type.clone().unwrap_or_else(|| "—".to_string());
                                            let size = format_bytes(file.size_bytes);
                                            view! {
                                                <TableRow
                                                    class="cursor-pointer hover:bg-muted/50"
                                                    on:click=move |_| {
                                                        let current = expanded_file_id.get_untracked();
                                                        if current.as_deref() == Some(fid_for_click.as_str()) {
                                                            expanded_file_id.set(None);
                                                            file_content.set(None);
                                                            file_content_truncated.set(false);
                                                        } else {
                                                            expanded_file_id.set(Some(fid_for_click.clone()));
                                                            file_content.set(None);
                                                            file_content_truncated.set(false);
                                                            #[cfg(target_arch = "wasm32")]
                                                            {
                                                            let fid = fid_for_click.clone();
                                                            let ds_id = ds_id.clone();
                                                            let client = Arc::clone(&client_for_fetch.get_value());
                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match client.get_dataset_file_content(&ds_id, &fid).await {
                                                                    Ok(content) => {
                                                                        let lines: Vec<&str> = content.lines().collect();
                                                                        let truncated = lines.len() > 200;
                                                                        let display = if truncated {
                                                                            lines[..200].join("\n")
                                                                        } else {
                                                                            content.clone()
                                                                        };
                                                                        let _ = file_content.try_set(Some(Ok(display)));
                                                                        let _ = file_content_truncated.try_set(truncated);
                                                                    }
                                                                    Err(e) => {
                                                                        let _ = file_content.try_set(Some(Err(e.to_string())));
                                                                    }
                                                                }
                                                            });
                                                            }
                                                        }
                                                    }
                                                >
                                                    <TableCell>
                                                        <span class="font-medium">{file.file_name.clone()}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm">{size}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm text-muted-foreground">{mime}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="text-sm text-muted-foreground">{format_date(&file.created_at)}</span>
                                                    </TableCell>
                                                    <TableCell>
                                                        <span class="font-mono text-xs text-muted-foreground" title=file.hash.clone()>{hash_short}</span>
                                                    </TableCell>
                                                </TableRow>
                                                {move || {
                                                    let is_expanded = expanded_file_id.try_get().flatten().as_deref() == Some(fid_for_expand.as_str());
                                                    is_expanded.then(|| {
                                                        let content_view = match file_content.try_get().flatten() {
                                                            None => view! {
                                                                <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                                                    <span class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent"></span>
                                                                    "Loading content..."
                                                                </div>
                                                            }.into_any(),
                                                            Some(Ok(text)) => {
                                                                let truncated = file_content_truncated.get_untracked();
                                                                view! {
                                                                    <div>
                                                                        <pre class="text-sm bg-muted/30 rounded-lg p-3 overflow-x-auto whitespace-pre-wrap max-h-64 overflow-y-auto font-mono">{text}</pre>
                                                                        {truncated.then(|| view! {
                                                                            <p class="text-xs text-muted-foreground mt-2">
                                                                                "Showing first 200 lines. Download the file for full content."
                                                                            </p>
                                                                        })}
                                                                    </div>
                                                                }.into_any()
                                                            }
                                                            Some(Err(e)) => view! {
                                                                <p class="text-sm text-destructive">{format!("Failed to load content: {}", e)}</p>
                                                            }.into_any(),
                                                        };
                                                        view! {
                                                            <tr>
                                                                <td colspan="5" class="p-3 border-t border-border bg-muted/10">
                                                                    {content_view}
                                                                </td>
                                                            </tr>
                                                        }
                                                    })
                                                }}
                                            }
                                        }).collect::<Vec<_>>()}
                                    </TableBody>
                                </Table>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| trigger_refresh.with_value(|f| f.run(())))
                            />
                        }.into_any()
                    }
                }}
            </div>
        </Card>
    }
}

/// Details tab content: overview and statistics.
#[component]
pub fn DatasetDetailTabDetails(
    data: DatasetResponse,
    stats: ReadSignal<LoadingState<DatasetStatisticsResponse>>,
    dataset_version_id_display: String,
) -> impl IntoView {
    view! {
        <div class="grid gap-6 md:grid-cols-2">
            <Card>
                <div class="p-4">
                    <h3 class="heading-4 mb-4">"Overview"</h3>
                    <dl class="space-y-3">
                        <CopyableId id=data.id.clone() display_name=data.display_name.clone().unwrap_or_default() label="ID".to_string() truncate=24 />
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Type"</dt>
                            <dd>
                                {match data.dataset_type.as_deref() {
                                    Some("identity") => view! { <Badge variant=BadgeVariant::Secondary>"Identity Set"</Badge> }.into_any(),
                                    _ => view! { <Badge variant=BadgeVariant::Outline>"Standard"</Badge> }.into_any(),
                                }}
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Format"</dt>
                            <dd>{data.format.to_uppercase()}</dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Status"</dt>
                            <dd>
                                <Badge variant={
                                    match data.status.as_str() {
                                        "ready" | "indexed" => BadgeVariant::Success,
                                        "processing" => BadgeVariant::Warning,
                                        "failed" | "error" => BadgeVariant::Destructive,
                                        _ => BadgeVariant::Secondary,
                                    }
                                }>{data.status.clone()}</Badge>
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Validation"</dt>
                            <dd>
                                {data.validation_status.clone().map(|s| {
                                    let v = validation_badge_variant(&s);
                                    view! { <Badge variant=v>{s}</Badge> }
                                })}
                            </dd>
                        </div>
                        <div class="flex justify-between" id="trust-state">
                            <dt class="text-muted-foreground">"Trust State"</dt>
                            <dd>
                                {data.trust_state.clone().map(|s| {
                                    let v = trust_state_badge_variant(&s);
                                    view! { <Badge variant=v>{s}</Badge> }
                                })}
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Current Version"</dt>
                            <dd class="font-mono text-xs truncate max-w-sm">
                                {dataset_version_id_display.clone()}
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"File Count"</dt>
                            <dd>{data.file_count.unwrap_or(0)}</dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Total Size"</dt>
                            <dd>{data.total_size_bytes.map(format_bytes).unwrap_or_else(|| "—".to_string())}</dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">"Created"</dt>
                            <dd>{format_date(&data.created_at)}</dd>
                        </div>
                        {data.hash_b3.clone().map(|hash| view! {
                            <div class="flex justify-between">
                                <dt class="text-muted-foreground">"Hash (B3)"</dt>
                                <dd class="font-mono text-xs truncate max-w-sm">{hash}</dd>
                            </div>
                        })}
                    </dl>
                </div>
            </Card>

            <Card>
                <div class="p-4">
                    <h3 class="heading-4 mb-4">"Statistics"</h3>
                    {move || match stats.try_get().unwrap_or(LoadingState::Idle) {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! { <SkeletonDetailSection rows=3/> }.into_any()
                        }
                        LoadingState::Loaded(stats_data) => {
                            view! {
                                <dl class="space-y-3">
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Examples"</dt>
                                        <dd>{stats_data.num_examples.to_string()}</dd>
                                    </div>
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Total Tokens"</dt>
                                        <dd>{stats_data.total_tokens.to_string()}</dd>
                                    </div>
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Avg Input Length"</dt>
                                        <dd>{format!("{:.1}", stats_data.avg_input_length)}</dd>
                                    </div>
                                    <div class="flex justify-between">
                                        <dt class="text-muted-foreground">"Avg Target Length"</dt>
                                        <dd>{format!("{:.1}", stats_data.avg_target_length)}</dd>
                                    </div>
                                </dl>
                            }.into_any()
                        }
                        LoadingState::Error(_) => {
                            view! { <p class="text-sm text-muted-foreground">"Statistics unavailable"</p> }.into_any()
                        }
                    }}
                </div>
            </Card>
        </div>
    }
}
