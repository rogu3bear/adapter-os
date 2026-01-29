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
};
use crate::components::{
    async_state::AsyncBoundary, Badge, BadgeVariant, Button, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, Dialog, Input, Link, LinkVariant, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow, Textarea,
};
use crate::hooks::use_api_resource;
use crate::signals::use_notifications;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
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
        let refetch = refetch.clone();
        move |_| {
            let name = new_name.get();
            let description = new_description.get();

            if name.trim().is_empty() {
                create_error.set(Some("Name is required".to_string()));
                return;
            }

            set_creating.set(true);
            create_error.set(None);

            let refetch = refetch.clone();
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
                        show_create_dialog.set(false);
                        new_name.set(String::new());
                        new_description.set(String::new());
                        refetch();
                    }
                    Err(e) => {
                        create_error.set(Some(e.to_string()));
                    }
                }
                set_creating.set(false);
            });
        }
    };

    view! {
        <div class="space-y-6">
            // Header with title and actions
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="text-3xl font-bold tracking-tight">"Collections"</h1>
                    <p class="text-muted-foreground mt-1">
                        "Organize documents into collections for RAG-enabled inference"
                    </p>
                </div>
                <div class="flex items-center gap-2">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new({
                            let refetch = refetch.clone();
                            move |_| refetch()
                        })
                    >
                        "Refresh"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(move |_| show_create_dialog.set(true))
                    >
                        "New Collection"
                    </Button>
                </div>
            </div>

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
        </div>
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
                    <h3 class="text-lg font-medium mb-1">"No collections yet"</h3>
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
                        deleting.set(false);
                        show_delete_confirm.set(false);
                    }
                }
            });
        }
    };

    // Remove document handler creator
    let create_remove_handler = {
        let refetch = refetch.clone();
        let notifications = notifications.clone();
        move |doc_id: String| {
            let coll_id = collection_id.get();
            let client = Arc::new(ApiClient::new());
            let refetch = refetch.clone();
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
        <div class="space-y-6">
            // Header with back link
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <Link href="/collections" variant=LinkVariant::Muted>
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>
                        </svg>
                    </Link>
                    <h1 class="text-3xl font-bold tracking-tight">"Collection Details"</h1>
                </div>
                <div class="flex items-center gap-2">
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Callback::new({
                            let refetch = refetch.clone();
                            move |_| refetch()
                        })
                    >
                        "Refresh"
                    </Button>
                    <Button
                        variant=ButtonVariant::Destructive
                        on_click=Callback::new(move |_| show_delete_confirm.set(true))
                    >
                        "Delete"
                    </Button>
                </div>
            </div>

            // Main content
            {
                let remove_handler = create_remove_handler.clone();
                view! {
                    <AsyncBoundary
                        state=collection
                        render=move |data| {
                            let handler = remove_handler.clone();
                            view! {
                                <CollectionDetailContent collection=data remove_document=handler/>
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
        </div>
    }
}

/// Collection detail content component
#[component]
fn CollectionDetailContent<F>(
    collection: CollectionDetailResponse,
    remove_document: F,
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
                    <div>
                        <p class="text-sm text-muted-foreground">"Collection ID"</p>
                        <p class="font-mono text-sm">{collection_id_display}</p>
                    </div>
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
                        <p class="text-sm text-muted-foreground mt-1">"Add documents via the API to use RAG-enabled inference."</p>
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
                                                <p class="text-xs text-muted-foreground font-mono">{doc_id.clone()}</p>
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

/// Format an ISO date string to a human-readable format
fn format_date(iso_date: &str) -> String {
    // Simple formatting - just show date portion
    if let Some(date_part) = iso_date.split('T').next() {
        date_part.to_string()
    } else {
        iso_date.to_string()
    }
}

/// Format bytes to human-readable size
fn format_bytes(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
