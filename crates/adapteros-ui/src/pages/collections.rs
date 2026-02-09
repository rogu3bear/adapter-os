//! Collections management page
//!
//! Provides UI for managing document collections including:
//! - List view with pagination
//! - Collection detail view with documents
//! - Create collection form
//! - Add/remove documents from collections
//!
//! Uses canonical Dialog and ErrorDisplay components for consistency.

use crate::api::{
    ApiClient, CollectionDetailResponse, CollectionResponse, CreateCollectionRequest,
    DocumentListParams, DocumentListResponse,
};
use crate::components::{
    async_state::AsyncBoundary, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card,
    Checkbox, ConfirmationDialog, ConfirmationSeverity, CopyableId, Dialog, Input,
    PageBreadcrumbItem, PageScaffold, PageScaffoldActions, RefreshButton, Select, Spinner, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow, Textarea,
};
use crate::hooks::{use_api_resource, use_scope_alive, LoadingState};
use crate::signals::use_notifications;
use crate::utils::{format_bytes, format_date};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use std::collections::HashSet;
use std::sync::Arc;

/// Collections list page
#[component]
pub fn Collections() -> impl IntoView {
    // Pagination state
    let (page, set_page) = signal(1u32);
    let limit = 20u32;

    // Dialog state for creating new collection
    let show_create_dialog = RwSignal::new(false);

    // Form fields for new collection
    let new_name = RwSignal::new(String::new());
    let new_description = RwSignal::new(String::new());
    let (creating, set_creating) = signal(false);
    let create_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Trigger for refetch
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);

    // Fetch collections with pagination
    let (collections, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let current_page = page.get();
        let _trigger = refetch_trigger.get(); // Subscribe to trigger changes
        async move { client.list_collections(current_page, limit).await }
    });

    // Refetch function
    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    // Create collection handler
    let on_create = {
        move |_| {
            let name = new_name.get();
            let description = new_description.get();

            if name.trim().is_empty() {
                create_error.set(Some("Name is required".to_string()));
                return;
            }

            set_creating.set(true);
            create_error.set(None);

            let refetch = refetch;
            let client = Arc::new(ApiClient::new());
            wasm_bindgen_futures::spawn_local(async move {
                let request = CreateCollectionRequest {
                    name: name.trim().to_string(),
                    description: if description.trim().is_empty() {
                        None
                    } else {
                        Some(description.trim().to_string())
                    },
                };

                match client.create_collection(&request).await {
                    Ok(_) => {
                        let _ = show_create_dialog.try_set(false);
                        let _ = new_name.try_set(String::new());
                        let _ = new_description.try_set(String::new());
                        refetch();
                    }
                    Err(e) => {
                        let _ = create_error.try_set(Some(e.to_string()));
                    }
                }
                let _ = set_creating.try_set(false);
            });
        }
    };

    view! {
        <PageScaffold
            title="Collections"
            subtitle="Organize documents into collections for RAG-enabled inference"
        >
            <PageScaffoldActions slot>
                <RefreshButton
                    on_click=Callback::new({
                        let refetch = refetch;
                        move |_| refetch()
                    })
                />
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                >
                    "New Collection"
                </Button>
            </PageScaffoldActions>

            // Main content
            <AsyncBoundary
                state=collections
                render=move |data| view! {
                    <CollectionsList
                        collections=data.data
                        total=data.total
                        page=data.page
                        pages=data.pages
                        on_page_change=move |p| set_page.set(p)
                    />
                }
            />

            // Create Collection Dialog
            <Dialog
                open=show_create_dialog
                title="Create Collection"
                description="Create a new document collection for organizing your data."
            >
                // Form
                <div class="grid gap-4 py-4">
                    <Input
                        value=new_name
                        label="Name".to_string()
                        placeholder="My Collection".to_string()
                        required=true
                    />
                    <Textarea
                        value=new_description
                        label="Description (optional)".to_string()
                        placeholder="A collection of documents for...".to_string()
                    />

                    // Error display
                    {move || create_error.get().map(|e| view! {
                        <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                            {e}
                        </div>
                    })}
                </div>

                // Footer
                <div class="flex justify-end gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| show_create_dialog.set(false))
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        loading=creating.get()
                        disabled=creating.get()
                        on_click=Callback::new(on_create)
                    >
                        "Create"
                    </Button>
                </div>
            </Dialog>
        </PageScaffold>
    }
}

/// Collections list component
#[component]
fn CollectionsList(
    collections: Vec<CollectionResponse>,
    total: u64,
    page: u32,
    pages: u32,
    on_page_change: impl Fn(u32) + Clone + Send + 'static,
) -> impl IntoView {
    if collections.is_empty() {
        return view! {
            <Card>
                <div class="py-12 text-center">
                    <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto text-muted-foreground mb-4">
                        <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
                    </svg>
                    <h3 class="heading-4 mb-1">"No collections yet"</h3>
                    <p class="text-muted-foreground">"Create your first collection to start organizing documents."</p>
                </div>
            </Card>
        }
        .into_any();
    }

    view! {
        <Card>
            <Table>
                <TableHeader>
                    <TableRow>
                        <TableHead>"Name"</TableHead>
                        <TableHead>"Description"</TableHead>
                        <TableHead>"Documents"</TableHead>
                        <TableHead>"Created"</TableHead>
                        <TableHead>"Actions"</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {collections
                        .into_iter()
                        .map(|collection| {
                            let id = collection.collection_id.clone();
                            let id_link = id.clone();
                            let name = collection.name.clone();
                            let description = collection.description.clone().unwrap_or_default();
                            let doc_count = collection.document_count;
                            let created = format_date(&collection.created_at);

                            let badge_variant = if doc_count > 0 { BadgeVariant::Success } else { BadgeVariant::Secondary };

                            view! {
                                <TableRow>
                                    <TableCell>
                                        <a
                                            href=format!("/collections/{}", id_link)
                                            class="font-medium hover:underline"
                                        >
                                            {name}
                                        </a>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground truncate max-w-xs block">
                                            {if description.is_empty() { "-".to_string() } else { description }}
                                        </span>
                                    </TableCell>
                                    <TableCell>
                                        <Badge variant=badge_variant>
                                            {doc_count.to_string()}
                                        </Badge>
                                    </TableCell>
                                    <TableCell>
                                        <span class="text-muted-foreground text-sm">{created}</span>
                                    </TableCell>
                                    <TableCell>
                                        <a
                                            href=format!("/collections/{}", id)
                                            class="text-sm text-primary hover:underline"
                                        >
                                            "View"
                                        </a>
                                    </TableCell>
                                </TableRow>
                            }
                        })
                        .collect::<Vec<_>>()}
                </TableBody>
            </Table>

            // Pagination
            {if pages > 1 {
                let on_page_change_prev = on_page_change.clone();
                let _on_page_change_next = on_page_change.clone();
                Some(view! {
                    <div class="flex items-center justify-between border-t px-4 py-3">
                        <div class="text-sm text-muted-foreground">
                            {format!("Page {} of {} ({} total)", page, pages, total)}
                        </div>
                        <div class="flex items-center gap-2">
                            <button
                                class="inline-flex items-center justify-center rounded-md border border-input bg-background px-3 py-1 text-sm hover:bg-accent disabled:opacity-50"
                                disabled=move || page <= 1
                                on:click=move |_| on_page_change_prev(page.saturating_sub(1))
                            >
                                "Previous"
                            </button>
                            <button
                                class="inline-flex items-center justify-center rounded-md border border-input bg-background px-3 py-1 text-sm hover:bg-accent disabled:opacity-50"
                                disabled=move || page >= pages
                                on:click=move |_| _on_page_change_next(page + 1)
                            >
                                "Next"
                            </button>
                        </div>
                    </div>
                })
            } else {
                None
            }}
        </Card>
    }
    .into_any()
}

/// Collection detail page
#[component]
pub fn CollectionDetail() -> impl IntoView {
    let params = use_params_map();

    // Get collection ID from URL
    let collection_id = Memo::new(move |_| params.get().get("id").unwrap_or_default());

    // Delete confirmation state
    let show_delete_confirm = RwSignal::new(false);
    let deleting = RwSignal::new(false);
    let notifications = use_notifications();
    let show_add_dialog = RwSignal::new(false);

    // Trigger for refetch
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);

    // Fetch collection details
    let (collection, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = collection_id.get();
        let _trigger = refetch_trigger.get();
        async move { client.get_collection(&id).await }
    });

    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    // Delete handler
    let on_delete = {
        let notifications = notifications.clone();
        move |_| {
            let id = collection_id.get();
            deleting.set(true);
            let notifications = notifications.clone();

            let client = Arc::new(ApiClient::new());
            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_collection(&id).await {
                    Ok(_) => {
                        // Navigate back to collections list
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href("/collections");
                        }
                    }
                    Err(e) => {
                        notifications.error(
                            "Delete failed",
                            &format!("Failed to delete collection: {}", e),
                        );
                        let _ = deleting.try_set(false);
                        let _ = show_delete_confirm.try_set(false);
                    }
                }
            });
        }
    };

    // Remove document handler creator
    let create_remove_handler = {
        let notifications = notifications.clone();
        move |doc_id: String| {
            let coll_id = collection_id.get();
            let client = Arc::new(ApiClient::new());
            let notifications = notifications.clone();

            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .remove_document_from_collection(&coll_id, &doc_id)
                    .await
                {
                    Ok(_) => {
                        refetch();
                    }
                    Err(e) => {
                        notifications.error(
                            "Remove failed",
                            &format!("Failed to remove document: {}", e),
                        );
                    }
                }
            });
        }
    };

    view! {
        <PageScaffold
            title="Collection Details"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Collections", "/collections"),
                PageBreadcrumbItem::current(collection_id.get()),
            ]
        >
            <PageScaffoldActions slot>
                <RefreshButton
                    on_click=Callback::new({
                        let refetch = refetch;
                        move |_| refetch()
                    })
                />
                <Button
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| show_add_dialog.set(true))
                >
                    "Add Documents"
                </Button>
                <Button
                    variant=ButtonVariant::Destructive
                    on_click=Callback::new(move |_| show_delete_confirm.set(true))
                >
                    "Delete"
                </Button>
            </PageScaffoldActions>

            // Main content
            {
                let remove_handler = create_remove_handler.clone();
                let refetch = refetch;
                view! {
                    <AsyncBoundary
                        state=collection
                        render=move |data| {
                            let handler = remove_handler.clone();
                            view! {
                                <CollectionDetailContent
                                    collection=data.clone()
                                    remove_document=handler
                                    on_add_documents=Callback::new(move |_| show_add_dialog.set(true))
                                />
                                <AddDocumentsDialog
                                    open=show_add_dialog
                                    collection_id=data.collection_id.clone()
                                    existing_documents=data.documents.iter().map(|d| d.document_id.clone()).collect()
                                    on_added=Callback::new(move |_| refetch())
                                />
                            }
                        }
                    />
                }
            }

            // Delete confirmation dialog
            <ConfirmationDialog
                open=show_delete_confirm
                title="Delete Collection"
                description="Are you sure you want to delete this collection? This action cannot be undone. Documents will not be deleted, only removed from the collection."
                severity=ConfirmationSeverity::Destructive
                confirm_text="Delete"
                on_confirm=Callback::new(on_delete)
                loading=Signal::derive(move || deleting.get())
            />
        </PageScaffold>
    }
}

/// Collection detail content component
#[component]
fn CollectionDetailContent<F>(
    collection: CollectionDetailResponse,
    remove_document: F,
    on_add_documents: Callback<()>,
) -> impl IntoView
where
    F: Fn(String) + Clone + Send + 'static,
{
    // Clone values upfront to avoid move issues
    let name = collection.name.clone();
    let description = collection
        .description
        .clone()
        .unwrap_or_else(|| "No description".to_string());
    let collection_id = collection.collection_id.clone();
    let collection_id_display = collection_id.clone();
    let collection_id_usage = collection_id.clone();
    let document_count = collection.document_count;
    let created = format_date(&collection.created_at);
    let updated = collection
        .updated_at
        .as_ref()
        .map(|d| format_date(d))
        .unwrap_or_else(|| "-".to_string());

    view! {
        <div class="grid gap-6 md:grid-cols-3">
            // Info card
            <Card title="Information".to_string()>
                <div class="space-y-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Name"</p>
                        <p class="font-medium">{name}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Description"</p>
                        <p class="font-medium">{description}</p>
                    </div>
                    <CopyableId
                        id=collection_id_display
                        label="Collection ID".to_string()
                        truncate=32
                    />
                </div>
            </Card>

            // Stats card
            <Card title="Statistics".to_string()>
                <div class="space-y-4">
                    <div>
                        <p class="text-sm text-muted-foreground">"Documents"</p>
                        <p class="text-2xl font-bold">{document_count}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Created"</p>
                        <p class="font-medium">{created}</p>
                    </div>
                    <div>
                        <p class="text-sm text-muted-foreground">"Updated"</p>
                        <p class="font-medium">{updated}</p>
                    </div>
                </div>
            </Card>

            // Usage card
            <Card title="Usage".to_string()>
                <div class="space-y-4">
                    <p class="text-sm text-muted-foreground">
                        "Use this collection ID in inference requests to enable RAG:"
                    </p>
                    <div class="rounded-md bg-muted p-3 font-mono text-sm break-all">
                        {format!("collection_id: \"{}\"", collection_id_usage)}
                    </div>
                </div>
            </Card>
        </div>

        // Documents table
        <Card title="Documents".to_string() class="mt-6".to_string()>
            {if collection.documents.is_empty() {
                view! {
                    <div class="py-8 text-center">
                        <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" class="mx-auto text-muted-foreground mb-3">
                            <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                            <polyline points="14 2 14 8 20 8"/>
                        </svg>
                        <p class="text-muted-foreground">"No documents in this collection"</p>
                        <p class="text-sm text-muted-foreground mt-1">
                            "Add documents to enable RAG-enabled inference."
                        </p>
                        <div class="mt-4 flex justify-center">
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(move |_| on_add_documents.run(()))
                            >
                                "Add Documents"
                            </Button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! {
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"Name"</TableHead>
                                <TableHead>"Size"</TableHead>
                                <TableHead>"Status"</TableHead>
                                <TableHead>"Added"</TableHead>
                                <TableHead>"Actions"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {collection.documents.clone().into_iter().map(|doc| {
                                let doc_id = doc.document_id.clone();
                                let doc_id_remove = doc_id.clone();
                                let name = doc.name.clone();
                                let size = format_bytes(doc.size_bytes);
                                let status = doc.status.clone();
                                let added = format_date(&doc.added_at);
                                let remove_document = remove_document.clone();

                                let status_variant = match status.as_str() {
                                    "indexed" => BadgeVariant::Success,
                                    "pending" => BadgeVariant::Warning,
                                    "error" => BadgeVariant::Destructive,
                                    _ => BadgeVariant::Secondary,
                                };

                                view! {
                                    <TableRow>
                                        <TableCell>
                                            <div>
                                                <p class="font-medium">{name}</p>
                                                <CopyableId id=doc_id.clone() truncate=24 />
                                            </div>
                                        </TableCell>
                                        <TableCell>{size}</TableCell>
                                        <TableCell>
                                            <Badge variant=status_variant>{status}</Badge>
                                        </TableCell>
                                        <TableCell>
                                            <span class="text-muted-foreground text-sm">{added}</span>
                                        </TableCell>
                                        <TableCell>
                                            <button
                                                class="text-sm text-destructive hover:underline"
                                                on:click=move |_| remove_document(doc_id_remove.clone())
                                            >
                                                "Remove"
                                            </button>
                                        </TableCell>
                                    </TableRow>
                                }
                            }).collect::<Vec<_>>()}
                        </TableBody>
                    </Table>
                }.into_any()
            }}
        </Card>
    }
}

#[component]
fn AddDocumentsDialog(
    open: RwSignal<bool>,
    collection_id: String,
    existing_documents: Vec<String>,
    on_added: Callback<()>,
) -> impl IntoView {
    let alive = use_scope_alive();
    let existing_set: Arc<HashSet<String>> =
        Arc::new(existing_documents.into_iter().collect::<HashSet<_>>());

    let status_filter = RwSignal::new("indexed".to_string());
    let search_query = RwSignal::new(String::new());
    let (page, set_page) = signal(1u32);
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);
    let selected_ids = RwSignal::new(Vec::<String>::new());
    let adding = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);

    let (documents, _) = use_api_resource(move |client: Arc<ApiClient>| {
        let is_open = open.get();
        let page = page.get();
        let status_value = status_filter.get();
        let _trigger = refetch_trigger.get();
        async move {
            if !is_open {
                return Ok(DocumentListResponse {
                    schema_version: String::new(),
                    data: Vec::new(),
                    total: 0,
                    page: 1,
                    limit: 20,
                    pages: 1,
                });
            }
            let status = if status_value.is_empty() {
                None
            } else {
                Some(status_value)
            };
            let params = DocumentListParams {
                status,
                page: Some(page),
                limit: Some(20),
            };
            client.list_documents(Some(&params)).await
        }
    });

    // Refetch and reset selection when dialog opens or filter changes
    Effect::new(move || {
        if open.get() {
            selected_ids.set(Vec::new());
            error_msg.set(None);
            set_refetch_trigger.update(|t| *t += 1);
        }
    });

    Effect::new(move || {
        let _ = status_filter.get();
        set_page.set(1);
        set_refetch_trigger.update(|t| *t += 1);
    });

    let toggle_selected = {
        move |doc_id: String, checked: bool| {
            selected_ids.update(|ids| {
                if checked {
                    if !ids.contains(&doc_id) {
                        ids.push(doc_id);
                    }
                } else {
                    ids.retain(|id| id != &doc_id);
                }
            });
        }
    };

    let add_selected = Callback::new({
        let collection_id = collection_id.clone();
        let alive = alive.clone();
        move |_| {
            let ids = selected_ids.get();
            if ids.is_empty() {
                error_msg.set(Some("Select at least one document to add.".into()));
                return;
            }
            adding.set(true);
            error_msg.set(None);
            let client = Arc::new(ApiClient::new());
            let on_added = on_added;
            let open = open;
            let selected_ids = selected_ids;
            let collection_id = collection_id.clone();
            let alive = alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut failures = Vec::new();
                for doc_id in ids.iter() {
                    if let Err(e) = client
                        .add_document_to_collection(&collection_id, doc_id)
                        .await
                    {
                        failures.push((doc_id.clone(), e.to_string()));
                    }
                }

                if failures.is_empty() {
                    let _ = selected_ids.try_set(Vec::new());
                    let _ = open.try_set(false);
                    if alive.load(std::sync::atomic::Ordering::SeqCst) {
                        on_added.run(());
                    }
                } else {
                    let first = failures.first().map(|(_, e)| e.clone()).unwrap_or_default();
                    let _ = error_msg.try_set(Some(format!(
                        "Failed to add {} document(s): {}",
                        failures.len(),
                        first
                    )));
                }
                let _ = adding.try_set(false);
            });
        }
    });

    let selected_count = Signal::derive(move || selected_ids.get().len());

    view! {
        <Dialog
            open=open
            title="Add Documents"
            description="Select indexed documents to include in this collection."
            size=crate::components::DialogSize::Lg
        >
            <div class="space-y-4 py-2">
                <div class="grid gap-3 md:grid-cols-2">
                    <Select
                        value=status_filter
                        options=vec![
                            ("indexed".to_string(), "Indexed".to_string()),
                            ("processing".to_string(), "Processing".to_string()),
                            ("failed".to_string(), "Failed".to_string()),
                            ("".to_string(), "All Statuses".to_string()),
                        ]
                        class="w-full".to_string()
                    />
                    <Input
                        value=search_query
                        label="Search".to_string()
                        placeholder="Filter by name or ID".to_string()
                    />
                </div>

                {move || {
                    match documents.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-6">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            let search = search_query.get().to_lowercase();
                            let existing = existing_set.clone();
                            let DocumentListResponse {
                                data: docs,
                                page: current_page,
                                pages: total_pages,
                                ..
                            } = data;
                            let filtered = docs
                                .into_iter()
                                .filter(|doc| !existing.contains(&doc.document_id))
                                .filter(|doc| {
                                    if search.is_empty() {
                                        true
                                    } else {
                                        doc.name.to_lowercase().contains(&search)
                                            || doc.document_id.to_lowercase().contains(&search)
                                    }
                                })
                                .collect::<Vec<_>>();

                            if filtered.is_empty() {
                                return view! {
                                    <div class="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
                                        "No eligible documents found."
                                    </div>
                                }.into_any();
                            }

                            view! {
                                <div class="space-y-3">
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead class="w-12">""</TableHead>
                                                <TableHead>"Name"</TableHead>
                                                <TableHead>"Status"</TableHead>
                                                <TableHead>"Size"</TableHead>
                                                <TableHead>"Created"</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {filtered.into_iter().map(|doc| {
                                                let doc_id = doc.document_id.clone();
                                                let doc_id_for_toggle = doc_id.clone();
                                                let doc_id_for_selected = doc_id.clone();
                                                let is_selected = Signal::derive({
                                                    move || selected_ids.get().contains(&doc_id_for_selected)
                                                });
                                                let status_variant = match doc.status.as_str() {
                                                    "indexed" => BadgeVariant::Success,
                                                    "processing" => BadgeVariant::Secondary,
                                                    "failed" => BadgeVariant::Destructive,
                                                    _ => BadgeVariant::Secondary,
                                                };

                                                view! {
                                                    <TableRow>
                                                        <TableCell>
                                                            <Checkbox
                                                                checked=is_selected
                                                                on_change=Callback::new({
                                                                    let toggle_selected = toggle_selected;
                                                                    move |checked| toggle_selected(doc_id_for_toggle.clone(), checked)
                                                                })
                                                            />
                                                        </TableCell>
                                                        <TableCell>
                                                            <div>
                                                                <p class="font-medium">{doc.name.clone()}</p>
                                                                <CopyableId id=doc_id.clone() truncate=24 />
                                                            </div>
                                                        </TableCell>
                                                        <TableCell>
                                                            <Badge variant=status_variant>{doc.status.clone()}</Badge>
                                                        </TableCell>
                                                        <TableCell>{format_bytes(doc.size_bytes)}</TableCell>
                                                        <TableCell>
                                                            <span class="text-sm text-muted-foreground">
                                                                {format_date(&doc.created_at)}
                                                            </span>
                                                        </TableCell>
                                                    </TableRow>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </TableBody>
                                    </Table>

                                    {if total_pages > 1 {
                                        Some(view! {
                                            <div class="flex items-center justify-between text-sm text-muted-foreground">
                                                <span>{format!("Page {} of {}", current_page, total_pages)}</span>
                                                <div class="flex items-center gap-2">
                                                    <Button
                                                        variant=ButtonVariant::Outline
                                                        size=ButtonSize::Sm
                                                        disabled=Signal::derive(move || current_page <= 1)
                                                        on_click=Callback::new(move |_| set_page.set(current_page.saturating_sub(1)))
                                                    >
                                                        "Previous"
                                                    </Button>
                                                    <Button
                                                        variant=ButtonVariant::Outline
                                                        size=ButtonSize::Sm
                                                        disabled=Signal::derive(move || current_page >= total_pages)
                                                        on_click=Callback::new(move |_| set_page.set((current_page + 1).min(total_pages)))
                                                    >
                                                        "Next"
                                                    </Button>
                                                </div>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }}
                                </div>
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

                {move || error_msg.get().map(|err| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                        {err}
                    </div>
                })}
            </div>

            <div class="flex items-center justify-between gap-2">
                <span class="text-sm text-muted-foreground">
                    {move || format!("Selected: {}", selected_count.get())}
                </span>
                <div class="flex items-center gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| open.set(false))
                        disabled=Signal::derive(move || adding.get())
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        loading=Signal::derive(move || adding.get())
                        disabled=Signal::derive(move || adding.get() || selected_count.get() == 0)
                        on_click=add_selected
                    >
                        "Add Selected"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}
