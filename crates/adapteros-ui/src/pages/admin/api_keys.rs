//! API Keys section component
//!
//! LM Studio-style API key management.

use crate::api::{report_error_with_toast, ApiClient, ApiKeyInfo, CreateApiKeyRequest};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmationDialog, ConfirmationSeverity,
    ErrorDisplay, Input, Spinner, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::utils::format_date;
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

/// API Keys section - LM Studio-style API key management
#[component]
pub fn ApiKeysSection() -> impl IntoView {
    // Fetch API keys from API
    let (keys, refetch) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_api_keys().await });

    // Dialog state
    let show_create_dialog = RwSignal::new(false);
    let new_key_name = RwSignal::new(String::new());
    let selected_scopes = RwSignal::new(vec!["viewer".to_string()]);
    let creating = RwSignal::new(false);
    let create_error = RwSignal::new(Option::<String>::None);

    // Created key display (shown once after creation)
    let created_key_token = RwSignal::new(Option::<String>::None);
    let copied = RwSignal::new(false);

    // Revoking state
    let revoking_id = RwSignal::new(Option::<String>::None);
    let show_revoke_confirm = RwSignal::new(false);
    let revoke_target_id = RwSignal::new(Option::<String>::None);
    let revoke_target_name = RwSignal::new(String::new());

    // Create key handler
    let on_create = move |_| {
        let name = new_key_name.get();
        if name.trim().is_empty() {
            create_error.set(Some("Name is required".to_string()));
            return;
        }

        creating.set(true);
        create_error.set(None);

        let scopes = selected_scopes.get();
        let request = CreateApiKeyRequest {
            name: name.trim().to_string(),
            scopes,
        };

        spawn_local(async move {
            let client = ApiClient::new();
            match client.create_api_key(&request).await {
                Ok(response) => {
                    created_key_token.set(Some(response.token));
                    new_key_name.set(String::new());
                    selected_scopes.set(vec!["viewer".to_string()]);
                    show_create_dialog.set(false);
                    refetch.run(());
                }
                Err(e) => {
                    report_error_with_toast(
                        &e,
                        "Failed to create API key",
                        Some("/admin/api-keys"),
                        true,
                    );
                    create_error.set(Some(e.user_message()));
                }
            }
            creating.set(false);
        });
    };

    // Copy to clipboard using wasm-bindgen
    let on_copy = move |_| {
        if let Some(token) = created_key_token.get() {
            spawn_local(async move {
                if copy_to_clipboard(&token).await {
                    copied.set(true);
                    // Reset after 2 seconds
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
            });
        }
    };

    // Revoke key - request confirmation first
    let request_revoke = move |id: String, name: String| {
        revoke_target_id.set(Some(id));
        revoke_target_name.set(name);
        show_revoke_confirm.set(true);
    };

    // Execute revoke after confirmation
    let do_revoke = Callback::new(move |_| {
        if let Some(id) = revoke_target_id.get() {
            revoking_id.set(Some(id.clone()));
            show_revoke_confirm.set(false);

            spawn_local(async move {
                let client = ApiClient::new();
                match client.revoke_api_key(&id).await {
                    Ok(_) => {
                        refetch.run(());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to revoke API key",
                            Some("/admin/api-keys"),
                            true,
                        );
                    }
                }
                revoking_id.set(None);
            });
        }
    });

    // Toggle scope selection
    let toggle_scope = move |scope: String| {
        let mut current = selected_scopes.get();
        if current.contains(&scope) {
            current.retain(|s| s != &scope);
        } else {
            current.push(scope);
        }
        if current.is_empty() {
            current.push("viewer".to_string());
        }
        selected_scopes.set(current);
    };

    view! {
        <div class="space-y-4">
            // Created key display (show once after creation)
            {move || {
                if let Some(token) = created_key_token.get() {
                    view! {
                        <div class="glass-panel border-status-success/50 bg-status-success/10 p-4 rounded-lg">
                            <div class="flex items-start gap-3">
                                <div class="rounded-full bg-status-success/20 p-2">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-status-success" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                        <polyline points="20 6 9 17 4 12"/>
                                    </svg>
                                </div>
                                <div class="flex-1 min-w-0">
                                    <h4 class="font-medium text-status-success">"API Key Created"</h4>
                                    <p class="text-sm text-muted-foreground mt-1">
                                        "Copy this key now. You won't be able to see it again."
                                    </p>
                                    <div class="mt-3 flex items-center gap-2">
                                        <code class="flex-1 bg-background/50 px-3 py-2 rounded font-mono text-sm truncate">
                                            {token.clone()}
                                        </code>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=Callback::new(on_copy)
                                        >
                                            {move || if copied.get() { "Copied!" } else { "Copy" }}
                                        </Button>
                                    </div>
                                </div>
                                <button
                                    class="text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                    aria-label="Close"
                                    type="button"
                                    on:click=move |_| created_key_token.set(None)
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                        <line x1="18" y1="6" x2="6" y2="18"/>
                                        <line x1="6" y1="6" x2="18" y2="18"/>
                                    </svg>
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}

            // Create dialog
            {move || {
                if show_create_dialog.get() {
                    view! {
                        <Card>
                            <div class="space-y-4">
                                <div class="flex items-center justify-between">
                                    <h3 class="heading-4">"Generate New API Key"</h3>
                                    <button
                                        class="text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
                                        aria-label="Close"
                                        type="button"
                                        on:click=move |_| {
                                            show_create_dialog.set(false);
                                            create_error.set(None);
                                        }
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                            <line x1="18" y1="6" x2="6" y2="18"/>
                                            <line x1="6" y1="6" x2="18" y2="18"/>
                                        </svg>
                                    </button>
                                </div>

                                <div class="space-y-3">
                                    <Input
                                        value=new_key_name
                                        label="Key Name".to_string()
                                        placeholder="e.g., Production API, Development Key".to_string()
                                    />

                                    <div>
                                        <label class="text-sm font-medium mb-2 block">"Permissions"</label>
                                        <div class="flex flex-wrap gap-2">
                                            {["admin", "operator", "viewer"].into_iter().map(|scope| {
                                                let scope_str = scope.to_string();
                                                let scope_for_check = scope_str.clone();
                                                let scope_for_toggle = scope_str.clone();
                                                view! {
                                                    <button
                                                        class=move || {
                                                            let is_selected = selected_scopes.get().contains(&scope_for_check);
                                                            if is_selected {
                                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                                            } else {
                                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-muted text-muted-foreground hover:bg-muted/80 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                                            }
                                                        }
                                                        type="button"
                                                        aria-pressed={
                                                            let scope = scope_for_toggle.clone();
                                                            move || selected_scopes.get().contains(&scope)
                                                        }
                                                        on:click={
                                                            let scope = scope_for_toggle.clone();
                                                            move |_| toggle_scope(scope.clone())
                                                        }
                                                    >
                                                        {scope_str.clone()}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                        <p class="text-xs text-muted-foreground mt-1">
                                            "Select the permission levels this key should have"
                                        </p>
                                    </div>
                                </div>

                                {move || {
                                    if let Some(error) = create_error.get() {
                                        view! {
                                            <div class="text-sm text-destructive">{error}</div>
                                        }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}

                                <div class="flex justify-end gap-2 pt-2">
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        on_click=Callback::new(move |_| {
                                            show_create_dialog.set(false);
                                            create_error.set(None);
                                        })
                                    >
                                        "Cancel"
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Primary
                                        on_click=Callback::new(on_create)
                                        disabled=creating.get()
                                    >
                                        {move || if creating.get() { "Generating..." } else { "Generate Key" }}
                                    </Button>
                                </div>
                            </div>
                        </Card>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}

            // Keys list
            <Card>
                {move || {
                    match keys.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            let active_keys: Vec<ApiKeyInfo> = data.api_keys.into_iter()
                                .filter(|k| k.revoked_at.is_none())
                                .collect();

                            if active_keys.is_empty() && !show_create_dialog.get() {
                                view! {
                                    <div class="py-8 text-center">
                                        <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-8 w-8 text-muted-foreground"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="1.5"
                                            >
                                                <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/>
                                            </svg>
                                        </div>
                                        <h3 class="heading-4 mb-2">"No API Keys"</h3>
                                        <p class="text-muted-foreground max-w-md mx-auto mb-4">
                                            "Generate API keys for programmatic access to the adapterOS API."
                                        </p>
                                        <Button
                                            variant=ButtonVariant::Primary
                                            on_click=Callback::new(move |_| show_create_dialog.set(true))
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                <line x1="12" y1="5" x2="12" y2="19"/>
                                                <line x1="5" y1="12" x2="19" y2="12"/>
                                            </svg>
                                            "Generate New Key"
                                        </Button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        <div class="flex items-center justify-between mb-4">
                                            <span class="text-sm text-muted-foreground">
                                                {format!("{} active key{}", active_keys.len(), if active_keys.len() == 1 { "" } else { "s" })}
                                            </span>
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    on_click=refetch.as_callback()
                                                >
                                                    "Refresh"
                                                </Button>
                                                <Button
                                                    variant=ButtonVariant::Primary
                                                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                        <line x1="12" y1="5" x2="12" y2="19"/>
                                                        <line x1="5" y1="12" x2="19" y2="12"/>
                                                    </svg>
                                                    "Generate New Key"
                                                </Button>
                                            </div>
                                        </div>
                                        <Table>
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>"Name"</TableHead>
                                                    <TableHead>"Permissions"</TableHead>
                                                    <TableHead>"Created"</TableHead>
                                                    <TableHead class="text-right".to_string()>"Actions"</TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {active_keys.into_iter().map(|key| {
                                                    let key_id = key.id.clone();
                                                    let key_id_for_revoke = key.id.clone();
                                                    let key_name_for_revoke = key.name.clone();
                                                    let is_revoking = move || revoking_id.get() == Some(key_id.clone());

                                                    view! {
                                                        <TableRow>
                                                            <TableCell>
                                                                <div class="flex items-center gap-2">
                                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/>
                                                                    </svg>
                                                                    <span class="font-medium">{key.name.clone()}</span>
                                                                </div>
                                                            </TableCell>
                                                            <TableCell>
                                                                <div class="flex gap-1 flex-wrap">
                                                                    {key.scopes.clone().into_iter().map(|scope| {
                                                                        let variant = match scope.as_str() {
                                                                            "admin" => BadgeVariant::Destructive,
                                                                            "operator" => BadgeVariant::Warning,
                                                                            _ => BadgeVariant::Secondary,
                                                                        };
                                                                        view! {
                                                                            <Badge variant=variant>{scope}</Badge>
                                                                        }
                                                                    }).collect::<Vec<_>>()}
                                                                </div>
                                                            </TableCell>
                                                            <TableCell>
                                                                <span class="text-sm text-muted-foreground">
                                                                    {format_date(&key.created_at)}
                                                                </span>
                                                            </TableCell>
                                                            <TableCell class="text-right".to_string()>
                                                                <Button
                                                                    variant=ButtonVariant::Ghost
                                                                    on_click=Callback::new({
                                                                        let id = key_id_for_revoke.clone();
                                                                        let name = key_name_for_revoke.clone();
                                                                        move |_| request_revoke(id.clone(), name.clone())
                                                                    })
                                                                    disabled=is_revoking()
                                                                >
                                                                    {move || if is_revoking() { "Revoking..." } else { "Revoke" }}
                                                                </Button>
                                                            </TableCell>
                                                        </TableRow>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </TableBody>
                                        </Table>
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <ErrorDisplay
                                    error=e
                                    on_retry=refetch.as_callback()
                                />
                            }.into_any()
                        }
                    }
                }}
            </Card>

            // Revoke confirmation dialog
            {move || {
                let name = revoke_target_name.get();
                let description = format!(
                    "Revoke '{}'? Applications using this key will immediately lose access.",
                    name,
                );
                view! {
                    <ConfirmationDialog
                        open=show_revoke_confirm
                        title="Revoke API Key"
                        description=description
                        severity=ConfirmationSeverity::Destructive
                        confirm_text="Revoke"
                        typed_confirmation=name
                        on_confirm=do_revoke
                        on_cancel=Callback::new(move |_| {
                            show_revoke_confirm.set(false);
                        })
                        loading=Signal::derive(move || revoking_id.get().is_some())
                    />
                }
            }}
        </div>
    }
}

/// Copy text to clipboard using the Clipboard API
async fn copy_to_clipboard(text: &str) -> bool {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };

    let navigator = window.navigator();

    // Get clipboard from navigator using JS reflection
    let clipboard = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .ok()
        .filter(|v| !v.is_undefined());

    let clipboard = match clipboard {
        Some(c) => c,
        None => return false,
    };

    // Call writeText method
    let write_text_fn =
        match js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")) {
            Ok(f) => f,
            Err(_) => return false,
        };

    let write_text_fn = match write_text_fn.dyn_ref::<js_sys::Function>() {
        Some(f) => f,
        None => return false,
    };

    let promise = match write_text_fn.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let promise = match promise.dyn_into::<js_sys::Promise>() {
        Ok(p) => p,
        Err(_) => return false,
    };

    JsFuture::from(promise).await.is_ok()
}
