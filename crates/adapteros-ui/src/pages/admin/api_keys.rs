//! API Keys section component with SplitPanel list-detail layout.
//!
//! LM Studio-style API key management.

use crate::api::{report_error_with_toast, ApiClient, ApiKeyInfo, CreateApiKeyRequest};
use crate::components::{
    use_split_panel_selection_state, Badge, BadgeVariant, Button, ButtonVariant, Card,
    ConfirmationDialog, ConfirmationSeverity, ErrorDisplay, FormDialog, FormField, Input,
    SkeletonTable, SplitPanel, SplitRatio, Table, TableBody, TableCell, TableHead, TableHeader,
    TableRow,
};
use crate::hooks::{use_api_resource, use_polling, LoadingState};
use crate::signals::{use_notifications, use_refetch_signal, RefetchTopic};
use crate::utils::format_date;
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

/// API Keys section - LM Studio-style API key management with SplitPanel detail view.
#[component]
pub fn ApiKeysSection() -> impl IntoView {
    let notifications = use_notifications();
    let sel = use_split_panel_selection_state();
    let selected_id = sel.selected_id;

    // Fetch API keys from API
    let (keys, refetch) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_api_keys().await });

    // Periodic polling (30s)
    let refetch_poll = refetch;
    let _cancel_poll = use_polling(30_000, move || {
        refetch_poll.run(());
        async {}
    });

    // SSE-driven refetch
    let api_keys_counter = use_refetch_signal(RefetchTopic::ApiKeys);
    Effect::new(move || {
        let _ = api_keys_counter.get();
        refetch.run(());
    });

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

    // Derived: active keys
    let active_keys = Signal::derive(move || -> Vec<ApiKeyInfo> {
        match keys.try_get().unwrap_or(LoadingState::Idle) {
            LoadingState::Loaded(data) => data
                .api_keys
                .into_iter()
                .filter(|k| k.revoked_at.is_none())
                .collect(),
            _ => Vec::new(),
        }
    });

    // Create key handler
    let on_create = Callback::new(move |_| {
        let name = new_key_name.try_get().unwrap_or_default();
        if name.trim().is_empty() {
            create_error.set(Some("Name is required".to_string()));
            return;
        }

        creating.set(true);
        create_error.set(None);

        let scopes = selected_scopes.try_get().unwrap_or_default();
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
                        Some("/admin?tab=keys"),
                        true,
                    );
                    create_error.set(Some(e.user_message()));
                }
            }
            creating.set(false);
        });
    });

    // Copy to clipboard using wasm-bindgen
    let on_copy = move |_| {
        if let Some(token) = created_key_token.try_get().flatten() {
            spawn_local(async move {
                if copy_to_clipboard(&token).await {
                    copied.set(true);
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
        if let Some(id) = revoke_target_id.try_get().flatten() {
            revoking_id.set(Some(id.clone()));
            show_revoke_confirm.set(false);

            let notifications = notifications.clone();
            spawn_local(async move {
                let client = ApiClient::new();
                match client.revoke_api_key(&id).await {
                    Ok(_) => {
                        notifications.success("API key revoked", "The API key has been revoked.");
                        refetch.run(());
                    }
                    Err(e) => {
                        report_error_with_toast(
                            &e,
                            "Failed to revoke API key",
                            Some("/admin?tab=keys"),
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
        let mut current = selected_scopes.try_get().unwrap_or_default();
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
                if let Some(token) = created_key_token.try_get().flatten() {
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
                                            {move || if copied.try_get().unwrap_or(false) { "Copied!" } else { "Copy" }}
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

            // Create dialog (FormDialog with focus trapping, Escape to close)
            <FormDialog
                open=show_create_dialog
                title="Generate New API Key"
                submit_label="Generate Key"
                loading=Signal::from(creating)
                on_submit=on_create
                on_cancel=Callback::new(move |_| {
                    create_error.set(None);
                })
            >
                <div class="space-y-3">
                    <FormField label="Key Name" name="key_name">
                        <Input
                            value=new_key_name
                            placeholder="e.g., Production API, Development Key".to_string()
                        />
                    </FormField>

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
                                            let is_selected = selected_scopes.try_get().unwrap_or_default().contains(&scope_for_check);
                                            if is_selected {
                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                            } else {
                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-muted text-muted-foreground hover:bg-muted/80 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
                                            }
                                        }
                                        type="button"
                                        aria-pressed={
                                            let scope = scope_for_toggle.clone();
                                            move || selected_scopes.try_get().unwrap_or_default().contains(&scope)
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
                    if let Some(error) = create_error.try_get().flatten() {
                        view! {
                            <div class="text-sm text-destructive">{error}</div>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
            </FormDialog>

            // Keys list with SplitPanel
            {move || {
                match keys.try_get().unwrap_or(LoadingState::Idle) {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <Card>
                                <SkeletonTable rows=3 columns=4/>
                            </Card>
                        }.into_any()
                    }
                    LoadingState::Loaded(_) => {
                        let keys_list = active_keys.get();
                        if keys_list.is_empty() && !show_create_dialog.try_get().unwrap_or(false) {
                            view! {
                                <Card>
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
                                </Card>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-4">
                                    <div class="flex items-center justify-between">
                                        <span class="text-sm text-muted-foreground">
                                            {move || {
                                                let count = active_keys.get().len();
                                                format!("{} active key{}", count, if count == 1 { "" } else { "s" })
                                            }}
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

                                    <SplitPanel
                                        has_selection=sel.has_selection
                                        on_close=sel.on_close
                                        back_label="Back to API Keys"
                                        ratio=SplitRatio::TwoFifthsThreeFifths
                                        list_panel=move || {
                                            let on_select = sel.on_select;
                                            view! {
                                                <Card>
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
                                                            {move || {
                                                                active_keys.get().into_iter().map(|key| {
                                                                    let key_id = key.id.clone();
                                                                    let key_id_click = key.id.clone();
                                                                    let key_id_for_revoke = key.id.clone();
                                                                    let key_name_for_revoke = key.name.clone();
                                                                    let is_revoking = Signal::derive({
                                                                        let kid = key.id.clone();
                                                                        move || revoking_id.try_get().flatten() == Some(kid.clone())
                                                                    });

                                                                    let key_id_key = key_id_click.clone();

                                                                    view! {
                                                                        <tr
                                                                            class="border-b transition-colors hover:bg-muted/50 cursor-pointer"
                                                                            class:bg-muted=move || selected_id.try_get().flatten().as_ref() == Some(&key_id)
                                                                            on:click=move |_| on_select.run(key_id_click.clone())
                                                                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                                                                if e.key() == "Enter" || e.key() == " " {
                                                                                    e.prevent_default();
                                                                                    on_select.run(key_id_key.clone());
                                                                                }
                                                                            }
                                                                            role="button"
                                                                            tabindex=0
                                                                        >
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
                                                                                    disabled=is_revoking
                                                                                >
                                                                                    {move || if is_revoking.try_get().unwrap_or(false) { "Revoking..." } else { "Revoke" }}
                                                                                </Button>
                                                                            </TableCell>
                                                                        </tr>
                                                                    }
                                                                }).collect::<Vec<_>>()
                                                            }}
                                                        </TableBody>
                                                    </Table>
                                                </Card>
                                            }
                                        }
                                        detail_panel=move || {
                                            view! {
                                                {move || {
                                                    let kid = selected_id.get();
                                                    kid.and_then(|id| {
                                                        active_keys.get().into_iter().find(|k| k.id == id)
                                                    }).map(|key| {
                                                        view! {
                                                            <ApiKeyDetailPanel
                                                                api_key=key
                                                                on_close=move || selected_id.set(None)
                                                            />
                                                        }
                                                    })
                                                }}
                                            }
                                        }
                                    />
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

            // Revoke confirmation dialog
            {move || {
                let name = revoke_target_name.try_get().unwrap_or_default();
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
                        loading=Signal::derive(move || revoking_id.try_get().flatten().is_some())
                    />
                }
            }}
        </div>
    }
}

/// Detail panel for a selected API key.
#[component]
fn ApiKeyDetailPanel(api_key: ApiKeyInfo, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let created = format_date(&api_key.created_at);
    let is_revoked = api_key.revoked_at.is_some();
    let status_text = if is_revoked { "Revoked" } else { "Active" };
    let status_variant = if is_revoked {
        BadgeVariant::Destructive
    } else {
        BadgeVariant::Default
    };

    view! {
        <div class="space-y-4">
            // Header with close button
            <div class="flex items-center justify-between">
                <h2 class="heading-3">"API Key Details"</h2>
                <button
                    class="text-muted-foreground hover:text-foreground"
                    on:click=move |_| on_close()
                    aria-label="Close"
                >
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="24"
                        height="24"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M18 6 6 18"/>
                        <path d="m6 6 12 12"/>
                    </svg>
                </button>
            </div>

            <Card>
                <div class="space-y-4">
                    // Name + status
                    <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2">
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/>
                            </svg>
                            <h3 class="heading-4">{api_key.name.clone()}</h3>
                        </div>
                        <Badge variant=status_variant>{status_text}</Badge>
                    </div>

                    // Scopes
                    <div>
                        <span class="text-sm font-medium">"Permissions"</span>
                        <div class="flex gap-1 flex-wrap mt-1">
                            {api_key.scopes.clone().into_iter().map(|scope| {
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
                    </div>

                    // Info grid
                    <div class="grid grid-cols-2 gap-3 text-sm">
                        <div>
                            <span class="text-muted-foreground">"Created"</span>
                            <p class="font-medium">{created}</p>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Status"</span>
                            <p class="font-medium">{status_text}</p>
                        </div>
                        <div>
                            <span class="text-muted-foreground">"Key ID"</span>
                            <p class="font-mono text-xs">{api_key.id.clone()}</p>
                        </div>
                    </div>

                    // Revoked info
                    {api_key.revoked_at.clone().map(|revoked| {
                        view! {
                            <div class="text-sm">
                                <span class="text-muted-foreground">"Revoked at: "</span>
                                <span class="font-medium">{format_date(&revoked)}</span>
                            </div>
                        }
                    })}
                </div>
            </Card>
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
