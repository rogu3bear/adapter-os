//! Documents management page
//!
//! Provides document list, detail, and management functionality.

use crate::api::client::{ChunkListResponse, DocumentListParams, DocumentResponse};
use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, BreadcrumbItem, BreadcrumbTrail, Button, ButtonSize, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, CopyableId, Dialog, IconExternalLink, InlineProgress,
    LoadingDisplay, ProgressStage, ProgressStages, RefreshButton, Select, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{
    use_api, use_api_resource, use_conditional_polling, use_delete_dialog, LoadingState,
};
use crate::signals::{try_use_route_context, SelectedEntity};
use crate::utils::format_bytes;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde::Deserialize;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

/// Get badge variant based on document status
fn status_badge_variant(status: &str) -> BadgeVariant {
    match status {
        "indexed" | "ready" => BadgeVariant::Success,
        "processing" | "uploaded" | "chunked" | "embedded" => BadgeVariant::Warning,
        "failed" => BadgeVariant::Destructive,
        _ => BadgeVariant::Secondary,
    }
}

/// Compute progress stage state from document status.
///
/// Returns (current_stage, completed_stages, error_stages).
fn document_processing_state(status: &str) -> (Option<String>, Vec<String>, Vec<String>) {
    let stage_order = [
        "uploaded",
        "processing",
        "chunked",
        "embedded",
        "indexed",
        "ready",
    ];

    if status == "failed" {
        return (
            None,
            vec!["uploaded".to_string()],
            vec!["processing".to_string()],
        );
    }

    let position = match status {
        "uploaded" => 0,
        "processing" => 1,
        "chunked" => 2,
        "embedded" => 3,
        "indexed" | "ready" => stage_order.len(),
        _ => 0,
    };

    let completed: Vec<String> = stage_order[..position.min(stage_order.len())]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let current = if position < stage_order.len() {
        Some(stage_order[position].to_string())
    } else {
        None
    };

    (current, completed, vec![])
}

fn slugify_id(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[derive(Clone, Debug, Default)]
struct DocumentStatusCounts {
    // Kept for API forward-compat; some backends return an `all` count even if the UI doesn't use it.
    #[allow(dead_code)]
    all: u64,
    indexed: u64,
    processing: u64,
    failed: u64,
}

/// Documents list page
#[component]
pub fn Documents() -> impl IntoView {
    // Filter state - use RwSignal<String> for Select component
    let status_filter = RwSignal::new(String::new());
    let (current_page, set_current_page) = signal(1u32);
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);
    let show_upload_dialog = RwSignal::new(false);
    let navigate = use_navigate();
    let seeded_demo_fixtures = RwSignal::new(false);

    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    let (status_counts, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let _trigger = refetch_trigger.get();
        async move {
            let base_params = |status: Option<String>| DocumentListParams {
                status,
                page: Some(1),
                limit: Some(1),
            };

            let all = client.list_documents(Some(&base_params(None))).await?.total;
            let indexed = client
                .list_documents(Some(&base_params(Some("indexed".to_string()))))
                .await?
                .total;
            let processing = client
                .list_documents(Some(&base_params(Some("processing".to_string()))))
                .await?
                .total;
            let failed = client
                .list_documents(Some(&base_params(Some("failed".to_string()))))
                .await?
                .total;

            Ok(DocumentStatusCounts {
                all,
                indexed,
                processing,
                failed,
            })
        }
    });

    // Demo guarantee: the ingest demo script expects a fast "Failed" filter path.
    //
    // If testkit is enabled (E2E_MODE) and there are no failed docs yet, seed two
    // deterministic fixtures:
    // - doc-failed-keep: stays failed, so the Failed pill always has something
    // - doc-failed-demo: can be reprocessed live for the "watch it advance" step
    Effect::new(move || {
        if seeded_demo_fixtures.get() {
            return;
        }

        let counts = match status_counts.get() {
            LoadingState::Loaded(c) => c,
            _ => return,
        };

        if counts.failed > 0 {
            seeded_demo_fixtures.set(true);
            return;
        }

        seeded_demo_fixtures.set(true);

        #[cfg(target_arch = "wasm32")]
        let set_refetch_trigger = set_refetch_trigger;
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let client = Arc::new(ApiClient::new());

            let _ = client
                .post::<_, Value>(
                    "/testkit/create_document_fixture",
                    &serde_json::json!({
                        "tenant_id": "default",
                        "document_id": "doc-failed-keep",
                        "status": "failed",
                        "name": "Failed (keep)"
                    }),
                )
                .await;

            let _ = client
                .post::<_, Value>(
                    "/testkit/create_document_fixture",
                    &serde_json::json!({
                        "tenant_id": "default",
                        "document_id": "doc-failed-demo",
                        "status": "failed",
                        "name": "Failed (reprocess)"
                    }),
                )
                .await;

            // Refresh counts/list once seeded (no-op if testkit is disabled).
            set_refetch_trigger.update(|t| *t += 1);
        });
    });

    let (documents, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let status_val = status_filter.get();
        let status = if status_val.is_empty() {
            None
        } else {
            Some(status_val)
        };
        let page = current_page.get();
        let _trigger = refetch_trigger.get();
        async move {
            let params = DocumentListParams {
                status,
                page: Some(page),
                limit: Some(20),
            };
            client.list_documents(Some(&params)).await
        }
    });

    // Refetch and reset page on filter change
    Effect::new(move || {
        let _ = status_filter.get();
        set_current_page.set(1);
        set_refetch_trigger.update(|t| *t += 1);
    });

    view! {
        <div class="shell-page space-y-6">
            <div class="flex items-center justify-between">
                <h1 class="heading-1">"Documents"</h1>
                <div class="flex items-center gap-4">
                    // Status filter
                    <Select
                        value=status_filter
                        options=vec![
                            ("".to_string(), "All Statuses".to_string()),
                            ("indexed".to_string(), "Ready/Indexed".to_string()),
                            ("processing".to_string(), "Processing".to_string()),
                            ("failed".to_string(), "Failed".to_string()),
                        ]
                        class="w-40".to_string()
                    />
                    <Button
                        variant=ButtonVariant::Ghost
                        size=ButtonSize::Sm
                        on_click=Callback::new({
                            let navigate = navigate.clone();
                            move |_| navigate("/training", Default::default())
                        })
                    >
                        "Go to Training"
                    </Button>
                    <RefreshButton
                        on_click=Callback::new({
                            let refetch = refetch.clone();
                            move |_| refetch()
                        })
                    />
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| show_upload_dialog.set(true))
                    >
                        "Upload Document"
                    </Button>
                </div>
            </div>

            {move || {
                match documents.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading documents..."/>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let total_pages = data.pages;
                        let current = current_page.get();
                        view! {
                            // Pipeline summary (click to filter)
                            <div class="rounded-lg border bg-card p-3">
                                <div class="flex flex-wrap items-center gap-2">
                                    {move || {
                                        let active = status_filter.get();
                                        let button = |label: &'static str,
                                                      count: Option<u64>,
                                                      value: &'static str,
                                                      badge_variant: BadgeVariant| {
                                            let is_active = !value.is_empty() && active == value;
                                            view! {
                                                <Button
                                                    variant=if is_active { ButtonVariant::Secondary } else { ButtonVariant::Ghost }
                                                    size=ButtonSize::Sm
                                                    on_click=Callback::new({
                                                        let status_filter = status_filter;
                                                        move |_| status_filter.set(value.to_string())
                                                    })
                                                >
                                                    <span class="flex items-center gap-2">
                                                        <span class="text-sm">{label}</span>
                                                        <Badge variant=badge_variant>
                                                            {count.map(|c| c.to_string()).unwrap_or_else(|| "…".to_string())}
                                                        </Badge>
                                                    </span>
                                                </Button>
                                            }
                                        };

                                        match status_counts.get() {
                                            LoadingState::Loaded(counts) => view! {
                                                {button("Ready/Indexed", Some(counts.indexed), "indexed", BadgeVariant::Success)}
                                                {button("Processing", Some(counts.processing), "processing", BadgeVariant::Warning)}
                                                {button("Failed", Some(counts.failed), "failed", BadgeVariant::Destructive)}
                                            }.into_any(),
                                            _ => view! {
                                                {button("Ready/Indexed", None, "indexed", BadgeVariant::Success)}
                                                {button("Processing", None, "processing", BadgeVariant::Warning)}
                                                {button("Failed", None, "failed", BadgeVariant::Destructive)}
                                            }.into_any(),
                                        }
                                    }}
                                </div>
                            </div>

                            <DocumentsList
                                documents=data.data.clone()
                                on_upload=Callback::new(move |_| show_upload_dialog.set(true))
                                on_refetch=Callback::new(move |_| set_refetch_trigger.update(|t| *t += 1))
                            />

                            // Pagination
                            {if total_pages > 1 {
                                view! {
                                    <div class="flex items-center justify-center gap-2 mt-6">
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Sm
                                            disabled=Signal::derive(move || current <= 1)
                                            on_click=Callback::new(move |_| set_current_page.update(|p| *p = p.saturating_sub(1).max(1)))
                                        >
                                            "Previous"
                                        </Button>
                                        <span class="text-sm text-muted-foreground">
                                            {format!("Page {} of {}", current, total_pages)}
                                        </span>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            size=ButtonSize::Sm
                                            disabled=Signal::derive(move || current >= total_pages)
                                            on_click=Callback::new(move |_| set_current_page.update(|p| *p = (*p + 1).min(total_pages)))
                                        >
                                            "Next"
                                        </Button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}

            <DocumentUploadDialog
                open=show_upload_dialog
                on_success=Callback::new({
                    let refetch = refetch.clone();
                    move |doc_id| {
                        refetch();
                        navigate(&format!("/documents/{}", doc_id), Default::default());
                    }
                })
            />
        </div>
    }
}

#[component]
fn DocumentsList(
    documents: Vec<DocumentResponse>,
    on_upload: Callback<()>,
    on_refetch: Callback<()>,
) -> impl IntoView {
    if documents.is_empty() {
        return view! {
            <Card>
                <div class="py-8 text-center">
                    <p class="text-muted-foreground">"No documents found"</p>
                    <p class="text-sm text-muted-foreground mt-2">
                        "Upload documents to begin indexing for RAG."
                    </p>
                    <div class="mt-4 flex justify-center">
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| on_upload.run(()))
                        >
                            "Upload Document"
                        </Button>
                    </div>
                </div>
            </Card>
        }
        .into_any();
    }

    let client = use_api();
    let delete_state = use_delete_dialog();
    let reprocessing_id = RwSignal::new(Option::<String>::None);

    let delete_state_for_cancel = delete_state.clone();
    let on_cancel_delete = Callback::new(move |_| {
        delete_state_for_cancel.cancel();
    });

    let delete_state_for_confirm = delete_state.clone();
    let on_confirm_delete = {
        let client = Arc::clone(&client);
        Callback::new(move |_| {
            if let Some(id) = delete_state_for_confirm.get_pending_id() {
                delete_state_for_confirm.start_delete();
                let client = Arc::clone(&client);
                let delete_state = delete_state_for_confirm.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match client.delete_document(&id).await {
                        Ok(_) => {
                            delete_state.finish_delete(Ok(()));
                            on_refetch.run(());
                        }
                        Err(e) => {
                            delete_state.finish_delete(Err(format!("Delete failed: {}", e)));
                        }
                    }
                });
            }
        })
    };

    let delete_state_for_rows = delete_state.clone();
    let delete_state_for_loading = delete_state.clone();

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Status"</TableHead>
                        <TableHead>"Size"</TableHead>
                        <TableHead>"Chunks"</TableHead>
                        <TableHead>"Type"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead class="text-right">"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {documents
                        .into_iter()
                        .map(|doc| {
                            let id = doc.document_id.clone();
                            let id_link = id.clone();
                            let id_reprocess = id.clone();
                            let id_delete = id.clone();
                            let name = doc.name.clone();
                            let name_for_delete = name.clone();
                            let status = doc.status.clone();
                            let status_variant = status_badge_variant(&status);
                            let size = format_bytes(doc.size_bytes);
                            let chunks = doc.chunk_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
                            let mime = doc.mime_type.clone();
                            let created = doc.created_at.clone();
                            let error = doc.error_message.clone();
                            let delete_state = delete_state_for_rows.clone();
                            let client = Arc::clone(&client);
                            let is_terminal_ready = matches!(status.as_str(), "indexed" | "ready");
                            let is_failed = status == "failed";
                            let is_in_flight = !is_terminal_ready && !is_failed;

                            view! {
                                <TableRow>
                                    <TableCell>
                                        <a
                                            href=format!("/documents/{}", id_link)
                                            class="font-medium hover:underline"
                                        >
                                            {name}
                                        </a>
                                    </TableCell>
                                    <TableCell>
                                        <div class="space-y-1">
                                            <Badge variant=status_variant>
                                                {status}
                                            </Badge>
                                            {error
                                                .clone()
                                                .filter(|err| !err.is_empty())
                                                .map(|err| {
                                                    let err_title = err.clone();
                                                    view! {
                                                        <div class="text-xs text-destructive line-clamp-1" title=err_title>
                                                            {err}
                                                        </div>
                                                    }
                                                })}
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground">{size}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground">{chunks}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-xs text-muted-foreground font-mono">{mime}</span>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-sm text-muted-foreground">{created}</span>
                                    </TableCell>
                                    <TableCell class="text-right">
                                        <div class="flex items-center justify-end gap-1.5">
                                            {is_in_flight.then(|| {
                                                view! {
                                                    <InlineProgress
                                                        label=Signal::derive(|| "Processing".to_string())
                                                    />
                                                }
                                            })}
                                            {is_terminal_ready.then(|| {
                                                let doc_id_for_train = id.clone();
                                                view! {
                                                    <a
                                                        href=format!(
                                                            "/documents/{}#train-adapter-cta",
                                                            doc_id_for_train
                                                        )
                                                        class="btn btn-ghost btn-sm"
                                                        aria_label="Train using this document"
                                                    >
                                                        "Train"
                                                    </a>
                                                }
                                            })}
                                            {(!is_in_flight).then(|| {
                                                let aria = if is_failed {
                                                    "Retry document"
                                                } else {
                                                    "Reprocess document"
                                                };
                                                let label = if is_failed { "Retry" } else { "Reprocess" };
                                                view! {
                                                    <Button
                                                        variant=ButtonVariant::Ghost
                                                        size=ButtonSize::Sm
                                                        aria_label=aria
                                                        disabled=Signal::derive({
                                                            let id = id_reprocess.clone();
                                                            move || reprocessing_id.get().as_deref() == Some(id.as_str())
                                                        })
                                                        on_click=Callback::new({
                                                            let client = Arc::clone(&client);
                                                            let id = id_reprocess.clone();
                                                            let is_failed = is_failed;
                                                            move |_| {
                                                                let client = Arc::clone(&client);
                                                                let id = id.clone();
                                                                reprocessing_id.set(Some(id.clone()));
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    if is_failed {
                                                                        let _ = client.retry_document(&id).await;
                                                                    } else {
                                                                        let _ = client.process_document(&id).await;
                                                                    }
                                                                    reprocessing_id.set(None);
                                                                    on_refetch.run(());
                                                                });
                                                            }
                                                        })
                                                    >
                                                        <svg
                                                            xmlns="http://www.w3.org/2000/svg"
                                                            class="h-4 w-4"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                        >
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                                                        </svg>
                                                        <span class="ml-1">{label}</span>
                                                    </Button>
                                                }
                                            })}
                                            {is_failed.then(|| {
                                                let error_href = "/errors".to_string();
                                                view! {
                                                    <a
                                                        href=error_href
                                                        class="inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-accent text-muted-foreground"
                                                        title="Open incidents/errors"
                                                        aria-label="Open incidents/errors"
                                                    >
                                                        <IconExternalLink class="h-4 w-4".to_string() aria_label="".to_string() />
                                                    </a>
                                                }
                                            })}
                                            <Button
                                                variant=ButtonVariant::Ghost
                                                size=ButtonSize::Sm
                                                aria_label="Delete document"
                                                on_click=Callback::new({
                                                    let delete_state = delete_state.clone();
                                                    move |_| {
                                                        delete_state.confirm(id_delete.clone(), name_for_delete.clone());
                                                    }
                                                })
                                            >
                                                <svg
                                                    xmlns="http://www.w3.org/2000/svg"
                                                    class="h-4 w-4 text-destructive"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                >
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                                </svg>
                                            </Button>
                                        </div>
                                    </TableCell>
                                </TableRow>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>
        </Card>
        <ConfirmationDialog
            open=delete_state.show
            title="Delete Document"
            description="Are you sure you want to delete this document and all associated chunks? This action cannot be undone."
            severity=ConfirmationSeverity::Destructive
            confirm_text="Delete"
            cancel_text="Cancel"
            on_confirm=on_confirm_delete
            on_cancel=on_cancel_delete
            loading=Signal::derive(move || delete_state_for_loading.is_deleting())
        />
    }
    .into_any()
}

/// Document upload dialog with validation and progress.
#[component]
fn DocumentUploadDialog(open: RwSignal<bool>, on_success: Callback<String>) -> impl IntoView {
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

    #[cfg(target_arch = "wasm32")]
    const SUPPORTED_EXTENSIONS: &[&str] = &[".pdf", ".txt", ".md"];

    let uploading = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);
    let selected_file_name = RwSignal::new(None::<String>);
    let selected_file_size = RwSignal::new(None::<u64>);

    #[cfg(not(target_arch = "wasm32"))]
    let _ = on_success;
    let upload_status = RwSignal::new(None::<String>);
    let uploaded_status = RwSignal::new(None::<String>);

    #[cfg(target_arch = "wasm32")]
    let file_ref: RwSignal<Option<SendWrapper<web_sys::File>>> = RwSignal::new(None);

    // Reset state when dialog closes
    Effect::new(move || {
        if !open.get() {
            uploading.set(false);
            error_msg.set(None);
            selected_file_name.set(None);
            selected_file_size.set(None);
            upload_status.set(None);
            uploaded_status.set(None);
            #[cfg(target_arch = "wasm32")]
            file_ref.set(None);
        }
    });

    #[cfg(target_arch = "wasm32")]
    let handle_file_change = {
        move |ev: web_sys::Event| {
            use wasm_bindgen::JsCast;
            let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            if let Some(file) = files.get(0) {
                let size = file.size() as u64;
                let name = file.name();
                let name_lower = name.to_lowercase();

                if size > MAX_FILE_SIZE {
                    error_msg.set(Some(format!(
                        "File too large. Maximum size is {} MB.",
                        MAX_FILE_SIZE / 1024 / 1024
                    )));
                    selected_file_name.set(None);
                    selected_file_size.set(None);
                    file_ref.set(None);
                    return;
                }

                let ext_ok = SUPPORTED_EXTENSIONS
                    .iter()
                    .any(|ext| name_lower.ends_with(ext));
                if !ext_ok {
                    error_msg.set(Some(format!(
                        "Unsupported file type. Supported: {}",
                        SUPPORTED_EXTENSIONS.join(", ")
                    )));
                    selected_file_name.set(None);
                    selected_file_size.set(None);
                    file_ref.set(None);
                    return;
                }

                error_msg.set(None);
                selected_file_name.set(Some(name));
                selected_file_size.set(Some(size));
                file_ref.set(Some(SendWrapper::new(file)));
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_file_change = |_ev: web_sys::Event| {};

    let handle_upload = Callback::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file_wrapper) = file_ref.get() else {
                error_msg.set(Some("Please select a file first.".into()));
                return;
            };
            uploading.set(true);
            error_msg.set(None);
            upload_status.set(Some("Uploading document...".into()));
            uploaded_status.set(None);

            let file = file_wrapper.take();
            let on_success = on_success.clone();
            let open = open;

            wasm_bindgen_futures::spawn_local(async move {
                let client = ApiClient::new();
                match client.upload_document(&file).await {
                    Ok(response) => {
                        upload_status.set(Some("Upload complete. Indexing started.".into()));
                        uploaded_status.set(Some(response.status.clone()));
                        uploading.set(false);
                        open.set(false);
                        on_success.run(response.document_id);
                    }
                    Err(e) => {
                        error_msg.set(Some(e.to_string()));
                        upload_status.set(None);
                        uploading.set(false);
                    }
                }
            });
        }
    });

    let upload_disabled =
        Signal::derive(move || uploading.get() || selected_file_name.get().is_none());

    view! {
        <Dialog
            open=open
            title="Upload Document"
            description="Upload a document to index for RAG retrieval."
        >
            <div class="space-y-4 py-2">
                <div class="space-y-2">
                    <label class="text-sm font-medium">"File"</label>
                    <input
                        type="file"
                        accept=".pdf,.txt,.md"
                        class="block w-full text-sm"
                        disabled=move || uploading.get()
                        on:change=handle_file_change
                    />
                    <p class="text-xs text-muted-foreground">
                        "Supported: PDF, TXT, Markdown, HTML, JSON, JSONL · Max 50 MB"
                    </p>
                    {move || selected_file_name.get().map(|name| {
                        let size = selected_file_size.get().unwrap_or_default();
                        view! {
                            <div class="text-sm text-muted-foreground">
                                {name} " · " {format_bytes(size as i64)}
                            </div>
                        }
                    })}
                </div>

                {move || upload_status.get().map(|status| view! {
                    <div class="text-sm text-muted-foreground">{status}</div>
                })}

                {move || uploaded_status.get().map(|status| view! {
                    <div class="flex items-center gap-2 text-sm">
                        <span class="text-muted-foreground">"Indexing Status"</span>
                        <Badge variant=status_badge_variant(&status)>{status}</Badge>
                    </div>
                })}

                {move || error_msg.get().map(|err| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                        {err}
                    </div>
                })}
            </div>

            <div class="flex justify-end gap-2">
                <Button
                    variant=ButtonVariant::Outline
                    on_click=Callback::new(move |_| open.set(false))
                    disabled=Signal::derive(move || uploading.get())
                >
                    "Cancel"
                </Button>
                <Button
                    variant=ButtonVariant::Primary
                    loading=Signal::derive(move || uploading.get())
                    disabled=upload_disabled
                    on_click=handle_upload
                >
                    "Upload"
                </Button>
            </div>
        </Dialog>
    }
}

/// Document detail page
#[component]
pub fn DocumentDetail() -> impl IntoView {
    let params = use_params_map();

    // Get document ID from URL
    let document_id = Memo::new(move |_| params.get().get("id").unwrap_or_default());

    // Refetch trigger
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);
    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    // Fetch document details
    let (document, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = document_id.get();
        let _trigger = refetch_trigger.get();
        async move { client.get_document(&id).await }
    });

    // Poll while the document is mid-pipeline so the "stages" UI advances during demos.
    let should_poll = Signal::derive(
        move || matches!(document.get(), LoadingState::Loaded(ref doc) if !matches!(doc.status.as_str(), "indexed" | "ready" | "failed")),
    );
    let _ = use_conditional_polling(2000, should_poll, move || async move {
        set_refetch_trigger.update(|t| *t += 1);
    });

    // Fetch document chunks
    let (chunks, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = document_id.get();
        let _trigger = refetch_trigger.get();
        async move { client.get_document_chunks(&id).await }
    });

    // Action states
    let (deleting, set_deleting) = signal(false);
    let (processing, set_processing) = signal(false);
    let (action_error, set_action_error) = signal(Option::<String>::None);

    // Publish document selection to RouteContext for contextual actions in Command Palette
    {
        let document = document.clone();
        Effect::new(move || {
            if let Some(route_ctx) = try_use_route_context() {
                if let LoadingState::Loaded(doc) = document.get() {
                    route_ctx.set_selected(SelectedEntity::with_status(
                        "document",
                        doc.document_id.clone(),
                        doc.name.clone(),
                        doc.status.clone(),
                    ));
                }
            }
        });
    }

    view! {
        <div class="space-y-6">
            // Breadcrumb navigation
            <BreadcrumbTrail items=vec![
                BreadcrumbItem::link("Documents", "/documents"),
                BreadcrumbItem::current(document_id.get()),
            ]/>

            <div class="flex items-center justify-between">
                <h1 class="heading-1">"Document Details"</h1>
                <div class="flex items-center gap-2">
                    <RefreshButton
                        on_click=Callback::new({
                            let refetch = refetch.clone();
                            move |_| refetch()
                        })
                    />
                </div>
            </div>

            // Action error message
            {move || action_error.get().map(|err| view! {
                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                    <p class="text-destructive">{err}</p>
                </div>
            })}

            {move || {
                match document.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <LoadingDisplay message="Loading document..."/>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        let chunks_data = match chunks.get() {
                            LoadingState::Loaded(c) => Some(c),
                            _ => None,
                        };
                        view! {
                            <DocumentDetailContent
                                document=data
                                chunks=chunks_data
                                deleting=deleting
                                set_deleting=set_deleting
                                processing=processing
                                set_processing=set_processing
                                set_action_error=set_action_error
                                refetch_trigger=set_refetch_trigger
                            />
                        }.into_any()
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                <p class="text-destructive">{e.to_string()}</p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

#[component]
fn DocumentDetailContent(
    document: DocumentResponse,
    chunks: Option<ChunkListResponse>,
    deleting: ReadSignal<bool>,
    set_deleting: WriteSignal<bool>,
    processing: ReadSignal<bool>,
    set_processing: WriteSignal<bool>,
    set_action_error: WriteSignal<Option<String>>,
    refetch_trigger: WriteSignal<u32>,
) -> AnyView {
    let navigate = use_navigate();
    let status_variant = status_badge_variant(&document.status);
    let doc_id = document.document_id.clone();
    let doc_id_for_delete = doc_id.clone();
    let doc_id_for_process = doc_id.clone();
    let doc_id_for_retry = doc_id.clone();

    // Delete confirmation dialog state
    let show_delete_dialog = RwSignal::new(false);
    let doc_name_for_confirm = document.name.clone();

    // Open delete confirmation dialog
    let open_delete_dialog = move |_| {
        show_delete_dialog.set(true);
    };

    // Handle cancel/close of delete dialog
    let on_cancel_delete = Callback::new(move |_| {
        // Reset any error state when dialog is dismissed
        set_action_error.set(None);
    });

    // Delete action (called from confirmation dialog)
    let delete_action = {
        let doc_id_for_delete = doc_id_for_delete.clone();
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            let client = Arc::new(ApiClient::new());
            let id = doc_id_for_delete.clone();
            let navigate = navigate.clone();
            set_deleting.set(true);
            set_action_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_document(&id).await {
                    Ok(_) => {
                        set_deleting.set(false);
                        show_delete_dialog.set(false);
                        navigate("/documents", Default::default());
                    }
                    Err(e) => {
                        set_action_error.set(Some(format!("Delete failed: {}", e)));
                        set_deleting.set(false);
                    }
                }
            });
        })
    };

    // Process action (for reprocessing)
    let process_action = move |_| {
        let client = Arc::new(ApiClient::new());
        let id = doc_id_for_process.clone();
        set_processing.set(true);
        set_action_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match client.process_document(&id).await {
                Ok(_) => {
                    set_processing.set(false);
                    refetch_trigger.update(|t| *t += 1);
                }
                Err(e) => {
                    set_action_error.set(Some(format!("Process failed: {}", e)));
                    set_processing.set(false);
                }
            }
        });
    };

    // Retry action (for failed documents)
    let retry_action = move |_| {
        let client = Arc::new(ApiClient::new());
        let id = doc_id_for_retry.clone();
        set_processing.set(true);
        set_action_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match client.retry_document(&id).await {
                Ok(_) => {
                    set_processing.set(false);
                    refetch_trigger.update(|t| *t += 1);
                }
                Err(e) => {
                    set_action_error.set(Some(format!("Retry failed: {}", e)));
                    set_processing.set(false);
                }
            }
        });
    };

    let is_failed = document.status == "failed";
    let is_indexed = matches!(document.status.as_str(), "indexed" | "ready");
    let status_for_stages = document.status.clone();
    let issue_error_message = document.error_message.clone();
    let issue_error_code = document.error_code.clone();
    let status_for_eligibility = document.status.clone();
    let eligible_chunks = {
        let from_doc = document.chunk_count.unwrap_or(0);
        let from_chunks = chunks.as_ref().map(|c| c.total_chunks).unwrap_or(0);
        from_chunks.max(from_doc)
    };
    let is_eligible_for_training = is_indexed && eligible_chunks > 0;
    let not_eligible_reason = match status_for_eligibility.as_str() {
        "failed" => "Document failed processing.",
        "processing" | "uploaded" | "chunked" | "embedded" => "Document is still processing.",
        "indexed" | "ready" => "No chunks available yet.",
        other => {
            // Keep the reason anchored to the backend status string.
            // This avoids inventing pipeline states not guaranteed by the API.
            return view! { <span>{format!("Status: {}", other)}</span> }.into_any();
        }
    };

    #[derive(Debug, Deserialize)]
    struct TrainingJobStubResponse {
        job_id: String,
    }

    view! {
        <div class="grid gap-6 md:grid-cols-2">
            // Basic Info
            <Card title="Basic Information".to_string()>
                <div class="space-y-3">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{document.name.clone()}</p>
                    </div>
                    <div>
                        <CopyableId
                            id=document.document_id.clone()
                            label="Document ID".to_string()
                            truncate=28
                        />
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Hash (BLAKE3)"</p>
                        <p class="font-mono text-sm truncate">{document.hash_b3.clone()}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Tenant"</p>
                        <p class="font-mono text-sm">{document.tenant_id.clone()}</p>
                    </div>
                </div>
            </Card>

            // Status
            <Card title="Status".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center gap-2">
                        <Badge variant=status_variant>
                            {document.status.clone()}
                        </Badge>
                        {document.deduplicated.then(|| view! {
                            <Badge variant=BadgeVariant::Secondary>
                                "Deduplicated"
                            </Badge>
                        })}
                    </div>

                    // Error info for failed documents
                    {document.error_message.clone().map(|err| view! {
                        <div class="rounded-lg border border-destructive bg-destructive/10 p-3 mt-3">
                            <p class="text-sm font-medium text-destructive">"Error"</p>
                            <p class="text-sm text-destructive/80 mt-1">{err}</p>
                            {document.error_code.clone().map(|code| view! {
                                <p class="text-xs text-destructive/60 mt-1 font-mono">"Code: "{code}</p>
                            })}
                        </div>
                    })}

                    // Retry info
                    {(document.retry_count > 0).then(|| view! {
                        <div class="text-sm text-muted-foreground">
                            "Retries: "{document.retry_count}" / "{document.max_retries}
                        </div>
                    })}

                    // Training entry point (unchanged behavior)
                    {is_indexed.then(|| {
                        let doc_id_for_train = doc_id.clone();
                        let navigate = navigate.clone();
                        view! {
                            <div id="train-adapter-cta" class="pt-2">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| {
                                        let client = Arc::new(ApiClient::new());
                                        let doc_id = doc_id_for_train.clone();
                                        let navigate = navigate.clone();

                                        // Demo flow: if testkit is enabled server-side, create a deterministic
                                        // completed training job stub so we can immediately show job detail
                                        // progress + adapter/chat handoff. If testkit isn't enabled, fall back
                                        // to the normal document->training wizard flow.
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let job_id = format!("job-doc-{}", slugify_id(&doc_id));
                                            let adapter_id =
                                                format!("adapter-doc-{}", slugify_id(&doc_id));

                                            let body = serde_json::json!({
                                                "job_id": job_id,
                                                "repo_id": "repo-doc-demo",
                                                "status": "completed",
                                                // Dev no-auth currently uses tenant_id="default".
                                                "tenant_id": "default",
                                                "adapter_id": adapter_id,
                                                "adapter_name": format!("Adapter from {}", doc_id),
                                                "base_model_id": "model-doc-demo",
                                                "stack_id": "stack-doc-demo"
                                            });

                                            match client
                                                .post::<_, TrainingJobStubResponse>(
                                                    "/testkit/create_training_job_stub",
                                                    &body,
                                                )
                                                .await
                                            {
                                                Ok(resp) => {
                                                    navigate(
                                                        &format!("/training?job_id={}", resp.job_id),
                                                        Default::default(),
                                                    );
                                                }
                                                Err(_) => {
                                                    navigate(
                                                        &format!(
                                                            "/training?source=document&document_id={}",
                                                            doc_id
                                                        ),
                                                        Default::default(),
                                                    );
                                                }
                                            }
                                        });
                                    })
                                >
                                    "Train Adapter"
                                </Button>
                            </div>
                        }
                    })}

                    // Recovery actions
                    <div class="pt-2 border-t">
                        <p class="text-xs font-medium text-muted-foreground mt-3">"Recovery actions"</p>
                        <div class="flex flex-wrap gap-2 mt-2">
                            {is_failed.then(|| {
                                view! {
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        size=ButtonSize::Sm
                                        disabled=Signal::derive(move || processing.get())
                                        on_click=Callback::new(retry_action)
                                    >
                                        {move || if processing.get() { "Retrying..." } else { "Retry" }}
                                    </Button>
                                }
                            })}
                            <Button
                                variant=ButtonVariant::Secondary
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || processing.get())
                                on_click=Callback::new(process_action)
                            >
                                {move || if processing.get() { "Processing..." } else { "Reprocess" }}
                            </Button>
                            <Button
                                variant=ButtonVariant::Destructive
                                size=ButtonSize::Sm
                                disabled=Signal::derive(move || deleting.get())
                                on_click=Callback::new(open_delete_dialog)
                            >
                                {move || if deleting.get() { "Deleting..." } else { "Delete" }}
                            </Button>
                            {is_failed.then(|| {
                                // Current /errors UI does not expose a document-id filter, so keep this as a plain link.
                                view! {
                                    <a href="/errors" class="text-sm text-primary hover:underline self-center">
                                        "View error context"
                                    </a>
                                }
                            })}
                        </div>
                    </div>
                </div>
            </Card>

            // Eligibility (informational only; does not gate actions)
            <Card title="Eligibility".to_string()>
                <div class="space-y-2">
                    {is_eligible_for_training.then(|| view! {
                        <div class="flex items-center gap-2">
                            <Badge variant=BadgeVariant::Success>"Eligible"</Badge>
                            <span class="text-sm">"Eligible for training"</span>
                        </div>
                        <p class="text-sm text-muted-foreground">
                            {format!("Chunks available: {}", eligible_chunks)}
                        </p>
                    })}
                    {(!is_eligible_for_training).then(|| view! {
                        <div class="flex items-center gap-2">
                            <Badge variant=BadgeVariant::Secondary>"Not yet eligible"</Badge>
                            <span class="text-sm">{not_eligible_reason}</span>
                        </div>
                        <p class="text-sm text-muted-foreground">
                            {format!("Status: {}", status_for_eligibility)}
                        </p>
                    })}
                </div>
            </Card>

            // Delete confirmation dialog
            <ConfirmationDialog
                open=show_delete_dialog
                title="Delete Document"
                description=format!(
                    "This will permanently delete the document '{}' and all associated chunks. This action cannot be undone.",
                    doc_name_for_confirm
                )
                severity=ConfirmationSeverity::Destructive
                confirm_text="Delete"
                typed_confirmation=doc_name_for_confirm.clone()
                on_confirm=delete_action
                on_cancel=on_cancel_delete
                loading=Signal::derive(move || deleting.get())
            />
        </div>

        // Processing stages (shown when not yet indexed)
        {(!is_indexed).then(|| {
            let stages = vec![
                ProgressStage::new("uploaded", "Uploaded"),
                ProgressStage::new("processing", "Processing"),
                ProgressStage::new("chunked", "Chunked"),
                ProgressStage::new("embedded", "Embedded"),
                ProgressStage::new("indexed", "Indexed"),
                ProgressStage::new("ready", "Ready"),
            ];
            let (current, completed, errors) = document_processing_state(&status_for_stages);
            let current_signal = Signal::derive({
                let current = current.clone();
                move || current.clone()
            });
            let completed_signal = Signal::derive({
                let completed = completed.clone();
                move || completed.clone()
            });
            let error_signal = Signal::derive({
                let errors = errors.clone();
                move || errors.clone()
            });
            view! {
                <Card title="Processing Progress".to_string() class="mt-6".to_string()>
                    <ProgressStages
                        stages=stages
                        current_stage=current_signal
                        completed_stages=completed_signal
                        error_stages=error_signal
                    />
                </Card>
            }
        })}

        // Issue section (shown when document processing failed)
        {is_failed.then(|| {
            view! {
                <Card title="Issue".to_string() class="mt-6".to_string()>
                    <div class="space-y-3">
                        <div class="flex items-center gap-2">
                            <Badge variant=BadgeVariant::Destructive>
                                "Failed"
                            </Badge>
                        </div>
                        {issue_error_message.clone().map(|msg| view! {
                            <div>
                                <p class="text-sm text-muted-foreground">"Error Message"</p>
                                <p class="text-sm">{msg}</p>
                            </div>
                        })}
                        {issue_error_code.clone().map(|code| view! {
                            <div>
                                <p class="text-sm text-muted-foreground">"Error Code"</p>
                                <p class="font-mono text-sm">{code}</p>
                            </div>
                        })}
                        <p class="text-sm text-muted-foreground">
                            "Check the "
                            <a href="/errors" class="text-primary hover:underline">"Errors page"</a>
                            " for more details and investigation tools."
                        </p>
                    </div>
                </Card>
            }
        })}

        // File Details
        <Card title="File Details".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Size"</p>
                    <p class="font-medium">{format_bytes(document.size_bytes)}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"MIME Type"</p>
                    <p class="font-mono text-sm">{document.mime_type.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Chunks"</p>
                    <p class="font-medium">{document.chunk_count.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Storage Path"</p>
                    <p class="font-mono text-sm truncate" title=document.storage_path.clone()>{document.storage_path.clone()}</p>
                </div>
            </div>
        </Card>

        // Timestamps
        <Card title="Timestamps".to_string() class="mt-6".to_string()>
            <div class="grid gap-4 md:grid-cols-4">
                <div>
                    <p class="text-sm text-muted-foreground">"Created At"</p>
                    <p class="font-medium">{document.created_at.clone()}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Updated At"</p>
                    <p class="font-medium">{document.updated_at.clone().unwrap_or_else(|| "-".to_string())}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Processing Started"</p>
                    <p class="font-medium">{document.processing_started_at.clone().unwrap_or_else(|| "-".to_string())}</p>
                </div>
                <div>
                    <p class="text-sm text-muted-foreground">"Processing Completed"</p>
                    <p class="font-medium">{document.processing_completed_at.clone().unwrap_or_else(|| "-".to_string())}</p>
                </div>
            </div>
        </Card>

        // Chunks preview (if available)
        {chunks.map(|chunk_data| {
            if chunk_data.chunks.is_empty() {
                view! {
                    <Card title="Document Chunks".to_string() class="mt-6".to_string()>
                        <p class="text-muted-foreground">"No chunks available"</p>
                    </Card>
                }.into_any()
            } else {
                let total = chunk_data.total_chunks;
                let preview_chunks = chunk_data.chunks.into_iter().take(5).collect::<Vec<_>>();
                view! {
                    <Card title=format!("Document Chunks ({} total)", total) class="mt-6".to_string()>
                        <div class="space-y-4">
                            {preview_chunks.into_iter().map(|chunk| {
                                view! {
                                    <div class="rounded-lg border p-3">
                                        <div class="flex items-center justify-between mb-2">
                                            <span class="text-sm font-medium">"Chunk "{chunk.chunk_index + 1}</span>
                                            <span class="text-xs text-muted-foreground font-mono">{chunk.chunk_id.clone()}</span>
                                        </div>
                                        <p class="text-sm text-muted-foreground line-clamp-3">{chunk.text.clone()}</p>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                            {(total > 5).then(|| view! {
                                <p class="text-sm text-muted-foreground text-center">
                                    "Showing 5 of "{total}" chunks"
                                </p>
                            })}
                        </div>
                    </Card>
                }.into_any()
            }
        })}
    }
    .into_any()
}
