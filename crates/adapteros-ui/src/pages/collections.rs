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
    report_error_with_toast, ApiClient, CollectionDetailResponse, CollectionDocumentInfo,
    CollectionResponse, CreateCollectionRequest, DocumentListParams, DocumentListResponse,
};
use crate::components::{
    async_state::{AsyncBoundary, AsyncBoundaryWithErrorRender},
    loaded_signal, Badge, BadgeVariant, Button, ButtonLink, ButtonSize, ButtonVariant, Card,
    Checkbox, Column, ConfirmationDialog, ConfirmationSeverity, CopyableId, DataTable, Dialog,
    EmptyStateVariant, ErrorDisplay, FormField, Input, ListEmptyCard, PageBreadcrumbItem,
    PageScaffold, PageScaffoldActions, PaginationControls, RefreshButton, Select, Spinner, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow, Textarea,
};
use crate::hooks::{use_api, use_api_resource, use_scope_alive, LoadingState};
use crate::utils::{format_bytes, format_date};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashSet;
use std::sync::Arc;

/// Collections list page
#[component]
pub fn Collections() -> impl IntoView {
    let client = use_api();
    let navigate = use_navigate();

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
        let client = Arc::clone(&client);
        let navigate = navigate.clone();
        move |_| {
            let name = new_name.try_get().unwrap_or_default();
            let description = new_description.try_get().unwrap_or_default();

            if name.trim().is_empty() {
                create_error.set(Some("Name is required".to_string()));
                return;
            }

            set_creating.set(true);
            create_error.set(None);

            let refetch = refetch;
            let client = Arc::clone(&client);
            let navigate = navigate.clone();
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
                    Ok(created) => {
                        let _ = show_create_dialog.try_set(false);
                        let _ = new_name.try_set(String::new());
                        let _ = new_description.try_set(String::new());
                        refetch();
                        navigate(
                            &format!("/collections/{}", created.collection_id),
                            Default::default(),
                        );
                    }
                    Err(e) => {
                        let _ = create_error.try_set(Some(e.user_message()));
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
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Data", "/collections"),
                PageBreadcrumbItem::current("Collections"),
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
                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                >
                    "New Collection"
                </Button>
            </PageScaffoldActions>

            // Main content
            <AsyncBoundary
                state=collections
                on_retry=Callback::new(move |_| refetch())
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
                    <FormField label="Name" name="collection-name" required=true>
                        <Input
                            value=new_name
                            placeholder="My Collection".to_string()
                        />
                    </FormField>
                    <FormField label="Description" name="collection-description">
                        <Textarea
                            value=new_description
                            placeholder="A collection of documents for...".to_string()
                        />
                    </FormField>

                    // Error display
                    {move || create_error.try_get().flatten().map(|e| view! {
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
    on_page_change: impl Fn(u32) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    if collections.is_empty() {
        return view! {
            <ListEmptyCard
                title="No collections yet"
                description="Create your first collection to start organizing documents."
                variant=EmptyStateVariant::Empty
            />
        }
        .into_any();
    }

    let columns: Vec<Column<CollectionResponse>> = vec![
        Column::custom("Name", |c: &CollectionResponse| {
            let id = c.collection_id.clone();
            let name = c.name.clone();
            view! {
                <a href=format!("/collections/{}", id) class="font-medium hover:underline">
                    {name}
                </a>
            }
        }),
        Column::custom("Description", |c: &CollectionResponse| {
            let desc = c.description.clone().unwrap_or_default();
            view! {
                <span class="text-muted-foreground truncate max-w-xs block">
                    {if desc.is_empty() { "-".to_string() } else { desc }}
                </span>
            }
        }),
        Column::custom("Documents", |c: &CollectionResponse| {
            let count = c.document_count;
            let variant = if count > 0 {
                BadgeVariant::Success
            } else {
                BadgeVariant::Secondary
            };
            view! { <Badge variant=variant>{count.to_string()}</Badge> }
        }),
        Column::custom("Created", |c: &CollectionResponse| {
            let created = format_date(&c.created_at);
            view! { <span class="text-muted-foreground text-sm">{created}</span> }
        }),
        Column::custom("Actions", |c: &CollectionResponse| {
            let id = c.collection_id.clone();
            view! {
                <a href=format!("/collections/{}", id) class="text-sm text-primary hover:underline">
                    "View"
                </a>
            }
        }),
    ];

    let data = loaded_signal(Signal::derive({
        let collections = collections.clone();
        move || collections.clone()
    }));

    view! {
        <DataTable
            data=data
            columns=columns
            empty_title="No collections yet"
            empty_description="Create your first collection to start organizing documents."
        />

        // Pagination
        {if pages > 1 {
            let on_page_change_prev = on_page_change.clone();
            let on_page_change_next = on_page_change.clone();
            Some(view! {
                <PaginationControls
                    current_page=page as usize
                    total_pages=pages as usize
                    total_items=total as usize
                    class="border-t px-4 py-3".to_string()
                    on_prev=Callback::new(move |_| on_page_change_prev(page.saturating_sub(1)))
                    on_next=Callback::new(move |_| on_page_change_next((page + 1).min(pages)))
                />
            })
        } else {
            None
        }}
    }
    .into_any()
}

/// Collection detail page
#[component]
pub fn CollectionDetail() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let client = use_api();

    // Get collection ID from URL
    let collection_id = Memo::new(move |_| {
        params
            .try_get()
            .unwrap_or_default()
            .get("id")
            .unwrap_or_default()
    });

    // Delete confirmation state
    let show_delete_confirm = RwSignal::new(false);
    let deleting = RwSignal::new(false);
    let show_add_dialog = RwSignal::new(false);

    // Trigger for refetch
    let (refetch_trigger, set_refetch_trigger) = signal(0u32);

    // Fetch collection details
    let (collection, _refetch) = use_api_resource(move |client: Arc<ApiClient>| {
        let id = collection_id.try_get().unwrap_or_default();
        let _trigger = refetch_trigger.try_get().unwrap_or_default();
        async move { client.get_collection(&id).await }
    });

    let refetch = move || set_refetch_trigger.update(|t| *t += 1);

    // Delete handler
    let on_delete = {
        let navigate = navigate.clone();
        let client = Arc::clone(&client);
        move |_| {
            let id = collection_id.try_get().unwrap_or_default();
            deleting.set(true);

            let navigate = navigate.clone();
            let client = Arc::clone(&client);
            wasm_bindgen_futures::spawn_local(async move {
                match client.delete_collection(&id).await {
                    Ok(_) => {
                        let _ = deleting.try_set(false);
                        let _ = show_delete_confirm.try_set(false);
                        navigate("/collections", Default::default());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to delete collection",
                            Some("/collections"),
                            true,
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
        let client = Arc::clone(&client);
        move |doc_id: String| {
            let coll_id = collection_id.try_get().unwrap_or_default();
            let client = Arc::clone(&client);

            wasm_bindgen_futures::spawn_local(async move {
                match client
                    .remove_document_from_collection(&coll_id, &doc_id)
                    .await
                {
                    Ok(_) => {
                        refetch();
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to remove document",
                            Some("/collections"),
                            true,
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
                PageBreadcrumbItem::new("Data", "/collections"),
                PageBreadcrumbItem::new("Collections", "/collections"),
                PageBreadcrumbItem::current(collection_id.try_get().unwrap_or_default()),
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
                    <AsyncBoundaryWithErrorRender
                        state=collection
                        on_retry=Callback::new(move |_| refetch())
                        render=move |data| {
                            let handler = remove_handler.clone();
                            view! {
                                <CollectionDetailContent
                                    collection=data.clone()
                                    remove_document=handler
                                />
                                <AddDocumentsDialog
                                    open=show_add_dialog
                                    collection_id=data.collection_id.clone()
                                    existing_documents=data.documents.iter().map(|d| d.document_id.clone()).collect()
                                    on_added=Callback::new(move |_| refetch())
                                />
                            }
                        }
                        render_error=move |e, retry| {
                            if e.is_not_found() {
                                view! {
                                    <div class="flex min-h-[40vh] flex-col items-center justify-center px-4">
                                        <Card class="p-8 max-w-md w-full text-center">
                                            <div class="text-4xl font-bold text-muted-foreground mb-2">"404"</div>
                                            <h2 class="heading-3 mb-2">"Collection not found"</h2>
                                            <p class="text-muted-foreground mb-6">
                                                "This collection may have been deleted or doesn't exist."
                                            </p>
                                            <ButtonLink
                                                href="/collections"
                                                variant=ButtonVariant::Primary
                                                size=ButtonSize::Md
                                            >
                                                "View all collections"
                                            </ButtonLink>
                                        </Card>
                                    </div>
                                }
                                    .into_any()
                            } else {
                                match retry {
                                    Some(retry_cb) => {
                                        view! { <ErrorDisplay error=e on_retry=retry_cb/> }.into_any()
                                    }
                                    None => view! { <ErrorDisplay error=e/> }.into_any(),
                                }
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
                loading=Signal::derive(move || deleting.try_get().unwrap_or(false))
            />
        </PageScaffold>
    }
}

/// Collection detail content component
#[component]
fn CollectionDetailContent<F>(
    collection: CollectionDetailResponse,
    remove_document: F,
) -> impl IntoView
where
    F: Fn(String) + Clone + Send + Sync + 'static,
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
    let collection_display_name = collection.display_name.clone().unwrap_or_default();
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
                        display_name=collection_display_name
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
            {
                let remove_fn: Arc<dyn Fn(String) + Send + Sync> = Arc::new(remove_document);
                let remove_for_col = Arc::clone(&remove_fn);

                let columns: Vec<Column<CollectionDocumentInfo>> = vec![
                    Column::custom("Name", |doc: &CollectionDocumentInfo| {
                        let name = doc.name.clone();
                        let doc_id = doc.document_id.clone();
                        view! {
                            <div>
                                <p class="font-medium">{name}</p>
                                <CopyableId id=doc_id truncate=24 />
                            </div>
                        }
                    }),
                    Column::custom("Size", |doc: &CollectionDocumentInfo| {
                        view! { <span>{format_bytes(doc.size_bytes)}</span> }
                    }),
                    Column::custom("Status", |doc: &CollectionDocumentInfo| {
                        let status = doc.status.clone();
                        let variant = match status.as_str() {
                            "indexed" => BadgeVariant::Success,
                            "pending" => BadgeVariant::Warning,
                            "error" => BadgeVariant::Destructive,
                            _ => BadgeVariant::Secondary,
                        };
                        view! { <Badge variant=variant>{status}</Badge> }
                    }),
                    Column::custom("Added", |doc: &CollectionDocumentInfo| {
                        let added = format_date(&doc.added_at);
                        view! { <span class="text-muted-foreground text-sm">{added}</span> }
                    }),
                    Column::custom("Actions", move |doc: &CollectionDocumentInfo| {
                        let doc_id = doc.document_id.clone();
                        let remove = Arc::clone(&remove_for_col);
                        view! {
                            <button
                                class="text-sm text-destructive hover:underline"
                                on:click=move |_| remove(doc_id.clone())
                            >
                                "Remove"
                            </button>
                        }
                    }),
                ];

                let docs = collection.documents.clone();
                let data = loaded_signal(Signal::derive(move || docs.clone()));

                view! {
                    <DataTable
                        data=data
                        columns=columns
                        card=false
                        empty_title="No documents in this collection"
                        empty_description="Use Add Documents above to add indexed documents and enable RAG-enabled inference."
                    />
                }
                .into_any()
            }
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
    let client = use_api();
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
        let is_open = open.try_get().unwrap_or(false);
        let page = page.try_get().unwrap_or(1);
        let status_value = status_filter.try_get().unwrap_or_default();
        let search_value = search_query.try_get().unwrap_or_default();
        let _trigger = refetch_trigger.try_get().unwrap_or_default();
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

            if search_value.trim().is_empty() {
                let params = DocumentListParams {
                    status,
                    page: Some(page),
                    limit: Some(20),
                };
                client.list_documents(Some(&params)).await
            } else {
                // Search mode needs a complete eligible set; page-local filtering
                // creates false "no results" states.
                let search_limit = 100_u32;
                let first_page_params = DocumentListParams {
                    status: status.clone(),
                    page: Some(1),
                    limit: Some(search_limit),
                };
                let first_page = client.list_documents(Some(&first_page_params)).await?;
                let schema_version = first_page.schema_version.clone();
                let total = first_page.total;
                let total_pages = first_page.pages.max(1);
                let mut all_docs = first_page.data;

                for current_page in 2..=total_pages {
                    let page_params = DocumentListParams {
                        status: status.clone(),
                        page: Some(current_page),
                        limit: Some(search_limit),
                    };
                    let response = client.list_documents(Some(&page_params)).await?;
                    all_docs.extend(response.data);
                }

                Ok(DocumentListResponse {
                    schema_version,
                    data: all_docs,
                    total,
                    page: 1,
                    limit: search_limit,
                    pages: 1,
                })
            }
        }
    });

    // Refetch and reset selection when dialog opens or filter changes
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        if is_open {
            let _ = selected_ids.try_set(Vec::new());
            let _ = error_msg.try_set(None);
            let _ = set_refetch_trigger.try_update(|t| *t += 1);
        }
    });

    Effect::new(move || {
        let Some(_) = status_filter.try_get() else {
            return;
        };
        let _ = set_page.try_set(1);
        let _ = set_refetch_trigger.try_update(|t| *t += 1);
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
        let client = Arc::clone(&client);
        move |_| {
            let ids = selected_ids.try_get().unwrap_or_default();
            if ids.is_empty() {
                error_msg.set(Some("Select at least one document to add.".into()));
                return;
            }
            adding.set(true);
            error_msg.set(None);
            let client = Arc::clone(&client);
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
                        failures.push((doc_id.clone(), e.user_message()));
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

    let selected_count = Signal::derive(move || selected_ids.try_get().unwrap_or_default().len());

    view! {
        <Dialog
            open=open
            title="Add Documents"
            description="Select indexed documents to include in this collection."
            size=crate::components::DialogSize::Lg
        >
            <div class="space-y-4 py-2">
                <div class="grid gap-3 md:grid-cols-2">
                    <FormField label="Status" name="doc-status-filter">
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
                    </FormField>
                    <FormField label="Search" name="doc-search">
                        <Input
                            value=search_query
                            placeholder="Filter by name or ID".to_string()
                        />
                    </FormField>
                </div>

                {move || {
                    match documents.try_get().unwrap_or(LoadingState::Idle) {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-6">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            let search = search_query.try_get().unwrap_or_default().to_lowercase();
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
                                                    move || selected_ids.try_get().unwrap_or_default().contains(&doc_id_for_selected)
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
                                            <PaginationControls
                                                current_page=current_page as usize
                                                total_pages=total_pages as usize
                                                on_prev=Callback::new(move |_| set_page.set(current_page.saturating_sub(1)))
                                                on_next=Callback::new(move |_| set_page.set((current_page + 1).min(total_pages)))
                                            />
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
                                    <p class="text-destructive">{e.user_message()}</p>
                                </div>
                            }.into_any()
                        }
                    }
                }}

                {move || error_msg.try_get().flatten().map(|err| view! {
                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                        {err}
                    </div>
                })}
            </div>

            <div class="flex items-center justify-between gap-2">
                <span class="text-sm text-muted-foreground">
                    {move || format!("Selected: {}", selected_count.try_get().unwrap_or(0))}
                </span>
                <div class="flex items-center gap-2">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| open.set(false))
                        disabled=Signal::derive(move || adding.try_get().unwrap_or(false))
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        loading=Signal::derive(move || adding.try_get().unwrap_or(false))
                        disabled=Signal::derive(move || adding.try_get().unwrap_or(false) || selected_count.try_get().unwrap_or(0) == 0)
                        on_click=add_selected
                    >
                        "Add Selected"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}
