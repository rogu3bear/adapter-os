use super::workspace::MAX_CHAT_DATASET_MESSAGES;
use crate::api::{api_base_url, report_error_with_toast, ApiClient};
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChatEmptyConversationState, ChatSessionRowShell, Checkbox,
    ConfirmationDialog, ConfirmationSeverity, Input,
};
use crate::signals::{use_chat, use_settings, ChatSessionMeta, ChatSessionsManager};
use crate::utils::format_relative_time;
use adapteros_api_types::training::ChatMessageInput;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

#[component]
pub(super) fn ChatEmptyWorkspace() -> impl IntoView {
    let navigate = use_navigate();
    let (_, chat_action) = use_chat();
    let create_session = {
        let navigate = navigate.clone();
        let action = chat_action.clone();
        Callback::new(move |open_add_files: bool| {
            let navigate = navigate.clone();
            let action = action.clone();
            // Optimistic navigation: make the URL/session stable immediately, then swap to the
            // server-issued session id once created. This prevents "New Chat" from appearing
            // non-responsive during cold starts or transient backend delays.
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = if open_add_files {
                format!("/chat/s/{}?add_files=1", placeholder_id)
            } else {
                format!("/chat/s/{}", placeholder_id)
            };
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                // Create the session in the backend first; inference streaming requires
                // a server-issued session id.
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Conversation".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        // Clean up placeholder (if untouched) before switching to the real id.
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let path = if open_add_files {
                            format!("/chat/s/{}?add_files=1", session_id)
                        } else {
                            format!("/chat/s/{}", session_id)
                        };
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        report_error_with_toast(
                            &e,
                            "Failed to create chat session",
                            Some("/chat"),
                            false,
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };
    let start_chat = { Callback::new(move |_: ()| create_session.run(false)) };
    let add_files = { Callback::new(move |_: ()| create_session.run(true)) };
    let go_to_adapters = {
        Callback::new(move |_: ()| {
            navigate("/adapters", Default::default());
        })
    };

    view! {
        <ChatEmptyConversationState
            on_start_chat=start_chat
            on_add_files=add_files
            on_browse_adapters=go_to_adapters
        />
    }
}

// ---------------------------------------------------------------------------
// SessionListPanel - left sidebar with session list, search, and actions
// ---------------------------------------------------------------------------

/// Session list panel for the workspace sidebar
#[component]
pub(super) fn SessionListPanel(
    /// Currently selected session ID for highlighting
    selected_id: Signal<Option<String>>,
    /// Active sessions (reactive)
    #[prop(into)]
    sessions: Signal<Vec<ChatSessionMeta>>,
    /// Archived sessions (reactive)
    #[prop(into)]
    archived_sessions: Signal<Vec<ChatSessionMeta>>,
    /// Sidebar mode toggle (active vs archived)
    show_archived: RwSignal<bool>,
    /// Callback when a session is archived (passes session ID)
    on_archive: Callback<String>,
    /// Callback when a session is restored from archive (passes session ID)
    on_unarchive: Callback<String>,
    /// Callback when a session is deleted (passes deleted session ID)
    on_delete: Callback<String>,
) -> impl IntoView {
    let navigate = use_navigate();
    let search_query = RwSignal::new(String::new());
    let (chat_state, chat_action) = use_chat();

    // Filtered sessions based on search
    let filtered_sessions = Memo::new(move |_| {
        let query = search_query.try_get().unwrap_or_default().to_lowercase();
        let all = if show_archived.try_get().unwrap_or(false) {
            archived_sessions.try_get().unwrap_or_default()
        } else {
            sessions.try_get().unwrap_or_default()
        };
        if query.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|s| {
                    s.title.to_lowercase().contains(&query)
                        || s.preview.to_lowercase().contains(&query)
                })
                .collect()
        }
    });

    // Check if dock has unsaved messages
    let dock_has_messages =
        Memo::new(move |_| !chat_state.try_get().unwrap_or_default().messages.is_empty());
    let dock_message_count =
        Memo::new(move |_| chat_state.try_get().unwrap_or_default().messages.len());

    // Create new session
    let create_session = {
        let navigate = navigate.clone();
        let action = chat_action.clone();
        Callback::new(move |_: ()| {
            let navigate = navigate.clone();
            let action = action.clone();
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = format!("/chat/s/{}", placeholder_id);
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Conversation".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let path = format!("/chat/s/{}", session_id);
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        report_error_with_toast(
                            &e,
                            "Failed to create chat session",
                            Some("/chat"),
                            false,
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };

    // Save dock messages to a new session
    let save_dock_and_navigate = {
        let action = chat_action.clone();
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            let state = chat_state.get_untracked();
            let action = action.clone();
            let navigate = navigate.clone();
            let placeholder_id = format!("ses-{}", uuid::Uuid::new_v4().simple());
            let placeholder_path = format!("/chat/s/{}", placeholder_id);
            navigate(&placeholder_path, Default::default());
            wasm_bindgen_futures::spawn_local(async move {
                let name = generate_readable_id("session", "chat");
                match action
                    .create_backend_session(name, Some("New Conversation".to_string()))
                    .await
                {
                    Ok(session_id) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        let session = ChatSessionsManager::session_from_state(&session_id, &state);
                        ChatSessionsManager::save_session(&session);
                        action.clear_messages();
                        let path = format!("/chat/s/{}", session_id);
                        navigate(
                            &path,
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    Err(e) => {
                        ChatSessionsManager::prune_placeholder_session(&placeholder_id);
                        report_error_with_toast(
                            &e,
                            "Failed to save dock messages to session",
                            Some("/chat"),
                            false,
                        );
                        navigate(
                            "/chat",
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                }
            });
        })
    };

    // Multi-select state for chat-to-training flow
    let selected_training_session_ids = RwSignal::new(Vec::<String>::new());
    let creating_training_dataset = RwSignal::new(false);
    let training_dataset_error = RwSignal::new(Option::<String>::None);
    let selected_training_count = Signal::derive(move || {
        selected_training_session_ids
            .try_get()
            .unwrap_or_default()
            .len()
    });

    let toggle_training_session = Callback::new(move |(session_id, checked): (String, bool)| {
        selected_training_session_ids.update(|ids| {
            if checked {
                if !ids.contains(&session_id) {
                    ids.push(session_id);
                }
            } else {
                ids.retain(|id| id != &session_id);
            }
        });
        training_dataset_error.set(None);
    });

    let clear_training_selection = Callback::new(move |_: ()| {
        selected_training_session_ids.set(Vec::new());
        training_dataset_error.set(None);
    });

    let learn_and_generate_adapter = {
        let navigate = navigate.clone();
        Callback::new(move |_: ()| {
            if creating_training_dataset.try_get().unwrap_or(false) {
                return;
            }
            let selected_ids = selected_training_session_ids.try_get().unwrap_or_default();
            if selected_ids.is_empty() {
                training_dataset_error
                    .set(Some("Select one or more chat sessions first.".to_string()));
                return;
            }

            creating_training_dataset.set(true);
            training_dataset_error.set(None);
            let navigate = navigate.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let mut combined_messages: Vec<ChatMessageInput> = selected_ids
                    .iter()
                    .filter_map(|id| ChatSessionsManager::load_session(id))
                    .flat_map(|session| {
                        session
                            .messages
                            .into_iter()
                            .filter_map(|msg| {
                                let content = msg.content.trim().to_string();
                                if content.is_empty() {
                                    None
                                } else {
                                    Some(ChatMessageInput {
                                        role: msg.role,
                                        content,
                                        timestamp: Some(msg.timestamp),
                                    })
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();

                if combined_messages.is_empty() {
                    training_dataset_error.set(Some(
                        "No messages found in the selected sessions.".to_string(),
                    ));
                    creating_training_dataset.set(false);
                    return;
                }

                if combined_messages.len() > MAX_CHAT_DATASET_MESSAGES {
                    training_dataset_error.set(Some(format!(
                        "Selected chats exceed {} messages. Choose fewer chats and try again.",
                        MAX_CHAT_DATASET_MESSAGES
                    )));
                    creating_training_dataset.set(false);
                    return;
                }

                combined_messages.sort_by(|a, b| {
                    a.timestamp
                        .as_deref()
                        .unwrap_or_default()
                        .cmp(b.timestamp.as_deref().unwrap_or_default())
                });

                let dataset_name = format!(
                    "chat-learning-{}",
                    crate::utils::now_utc().format("%Y%m%d_%H%M%S")
                );
                let provenance_session_id = if selected_ids.len() == 1 {
                    selected_ids.first().cloned()
                } else {
                    None
                };
                let client = ApiClient::with_base_url(api_base_url());

                match client
                    .create_dataset_from_chat(
                        combined_messages,
                        Some(dataset_name),
                        provenance_session_id,
                    )
                    .await
                {
                    Ok(resp) => {
                        selected_training_session_ids.set(Vec::new());
                        creating_training_dataset.set(false);
                        let path = format!(
                            "/training?open_wizard=1&dataset_id={}&return_to=/chat",
                            resp.dataset_id
                        );
                        navigate(&path, Default::default());
                    }
                    Err(e) => {
                        creating_training_dataset.set(false);
                        training_dataset_error.set(Some(e.user_message()));
                        report_error_with_toast(
                            &e,
                            "Failed to prepare training data from selected chats",
                            Some("/chat"),
                            false,
                        );
                    }
                }
            });
        })
    };

    // Delete confirmation state
    let pending_delete_id = RwSignal::new(Option::<String>::None);
    let show_delete_confirm = RwSignal::new(false);
    let active_count = Memo::new(move |_| sessions.try_get().unwrap_or_default().len());
    let archived_count = Memo::new(move |_| archived_sessions.try_get().unwrap_or_default().len());
    let pending_delete_title = Memo::new(move |_| {
        let id = pending_delete_id.try_get().flatten().unwrap_or_default();
        if id.is_empty() {
            return String::new();
        }
        sessions
            .try_get()
            .unwrap_or_default()
            .into_iter()
            .chain(archived_sessions.try_get().unwrap_or_default())
            .find(|s| s.id == id)
            .map(|s| s.title)
            .unwrap_or_default()
    });

    let request_delete = Callback::new(move |id: String| {
        pending_delete_id.set(Some(id));
        show_delete_confirm.set(true);
    });

    let confirm_delete = move |_| {
        if let Some(id) = pending_delete_id.try_get().flatten() {
            selected_training_session_ids.update(|ids| ids.retain(|sid| sid != &id));
            on_delete.run(id);
        }
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    let cancel_delete = move |_| {
        pending_delete_id.set(None);
        show_delete_confirm.set(false);
    };

    view! {
        <div class="flex flex-col h-full">
            // Header with search and new button
            <div class="p-3 space-y-2 border-b border-border shrink-0">
                <div class="flex items-center justify-between">
                    <h2 class="text-sm font-semibold">"Conversations"</h2>
                    <button
                        class="btn btn-primary btn-sm inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                        on:click=move |_| create_session.run(())
                        title="New conversation"
                        aria-label="New Conversation"
                        data-testid="chat-sidebar-new-session"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4"/>
                        </svg>
                        "New Conversation"
                    </button>
                </div>
                <div class="grid grid-cols-2 gap-1 rounded-lg bg-muted/40 p-1">
                    <button
                        class=move || format!(
                            "btn btn-ghost btn-sm px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if !show_archived.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(false); }
                        aria-label="Show active sessions"
                        data-testid="chat-sidebar-toggle-active"
                    >
                        {move || format!("Active ({})", active_count.try_get().unwrap_or(0))}
                    </button>
                    <button
                        class=move || format!(
                            "btn btn-ghost btn-sm px-2 py-1 text-xs font-medium rounded-md transition-colors {}",
                            if show_archived.try_get().unwrap_or(false) {
                                "bg-background text-foreground shadow-sm"
                            } else {
                                "text-muted-foreground hover:text-foreground"
                            }
                        )
                        on:click=move |_| { show_archived.set(true); }
                        aria-label="Show archived sessions"
                        data-testid="chat-sidebar-toggle-archived"
                    >
                        {move || format!("Archived ({})", archived_count.try_get().unwrap_or(0))}
                    </button>
                </div>
                <Show when=move || { selected_training_count.try_get().unwrap_or(0) > 0 || creating_training_dataset.try_get().unwrap_or(false) }>
                    <div class="flex items-center gap-1.5">
                        <button
                            class=move || format!(
                                "btn btn-outline btn-sm flex-1 inline-flex items-center justify-center gap-1.5 px-2 py-1.5 text-xs font-semibold rounded-md border transition-colors {}",
                                if creating_training_dataset.try_get().unwrap_or(false) {
                                    "border-border text-muted-foreground bg-muted/30 cursor-not-allowed"
                                } else {
                                    "border-primary/30 text-primary bg-primary/5 hover:bg-primary/10"
                                }
                            )
                            disabled=move || creating_training_dataset.try_get().unwrap_or(false)
                            on:click=move |_| learn_and_generate_adapter.run(())
                            title="Create an adapter from selected conversations"
                            aria-label="Create Adapter From Selected Conversations"
                            data-testid="chat-sidebar-learn"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v4m0 10v4M3 12h4m10 0h4M5.6 5.6l2.8 2.8m7.2 7.2 2.8 2.8m0-12.8-2.8 2.8m-7.2 7.2-2.8 2.8"/>
                            </svg>
                            {move || {
                                if creating_training_dataset.try_get().unwrap_or(false) {
                                    "Preparing..."
                                } else {
                                    let count = selected_training_count.try_get().unwrap_or(0);
                                    if count == 1 { "Create Adapter (1)" } else { "Create Adapter" }
                                }
                            }}
                        </button>
                        <button
                            class="btn btn-ghost btn-xs px-1.5 py-1 text-2xs text-muted-foreground hover:text-foreground rounded"
                            on:click=move |_| clear_training_selection.run(())
                            data-testid="chat-sidebar-learn-clear"
                            aria-label="Clear training selection"
                        >
                            "Clear"
                        </button>
                    </div>
                    {move || {
                        training_dataset_error
                            .try_get()
                            .flatten()
                            .map(|err| {
                                view! {
                                    <p class="text-2xs text-destructive">{err}</p>
                                }
                            })
                    }}
                </Show>
                <div data-testid="chat-sidebar-search">
                    <Input
                        value=search_query
                        placeholder="Search sessions...".to_string()
                    />
                </div>
            </div>

            // Continue from dock banner
            {move || {
                if dock_has_messages.try_get().unwrap_or(false) {
                    Some(view! {
                        <div class="px-3 py-2 border-b border-primary/20 bg-primary/5 shrink-0">
                            <div class="flex items-center justify-between gap-2">
                                <div class="min-w-0">
                                    <p class="text-xs font-medium truncate">"Continue this draft"</p>
                                    <p class="text-2xs text-muted-foreground">
                                        {move || format!("{} messages", dock_message_count.try_get().unwrap_or(0))}
                                    </p>
                                </div>
                                <button
                                    class="btn btn-outline btn-sm shrink-0 px-2 py-1 text-xs font-medium rounded border border-primary/30 text-primary hover:bg-primary/10 transition-colors"
                                    on:click=move |_| save_dock_and_navigate.run(())
                                    data-testid="chat-sidebar-continue"
                                >
                                    "Save & Open"
                                </button>
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Session list (scrollable)
            <div class="flex-1 overflow-y-auto">
                {move || {
                    let showing_archived = show_archived.try_get().unwrap_or(false);
                    let list = filtered_sessions.try_get().unwrap_or_default();
                    if list.is_empty() {
                        let create_session_empty = create_session;
                        let has_search = !search_query.try_get().unwrap_or_default().is_empty();
                        let message = if has_search {
                            "No matching conversations"
                        } else if showing_archived {
                            "No archived conversations"
                        } else {
                            "No conversations yet"
                        };
                        view! {
                            <div class="p-6 text-center space-y-2">
                                <p class="text-xs text-muted-foreground">{message}</p>
                                {if has_search {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| search_query.set(String::new()))
                                        >
                                            "Clear search"
                                        </Button>
                                    }.into_any()
                                } else if showing_archived {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Ghost
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| {
                                                show_archived.set(false);
                                            })
                                        >
                                            "View active sessions"
                                        </Button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Button
                                            variant=ButtonVariant::Secondary
                                            size=ButtonSize::Sm
                                            on_click=Callback::new(move |_| create_session_empty.run(()))
                                        >
                                            "New conversation"
                                        </Button>
                                    }.into_any()
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="divide-y divide-border">
                                {list.into_iter().map(|session| {
                                    let id = session.id.clone();
                                    let is_selected = {
                                        let id = id.clone();
                                        Signal::derive(move || selected_id.try_get().flatten().as_deref() == Some(id.as_str()))
                                    };
                                    let training_selected = {
                                        let id = id.clone();
                                        Signal::derive(move || {
                                            selected_training_session_ids
                                                .try_get()
                                                .unwrap_or_default()
                                                .contains(&id)
                                        })
                                    };
                                    let delete_handler = request_delete;
                                    let archive_handler = on_archive;
                                    let unarchive_handler = on_unarchive;
                                    let training_toggle_handler = toggle_training_session;
                                    let archive_id = id.clone();
                                    let unarchive_id = id.clone();
                                    let delete_id = id.clone();
                                    let training_id = id.clone();
                                    let archive_callback = if showing_archived {
                                        None
                                    } else {
                                        Some(Callback::new(move |_: ()| {
                                            archive_handler.run(archive_id.clone());
                                        }))
                                    };
                                    let unarchive_callback = if showing_archived {
                                        Some(Callback::new(move |_: ()| {
                                            unarchive_handler.run(unarchive_id.clone());
                                        }))
                                    } else {
                                        None
                                    };
                                    view! {
                                        <SessionListItem
                                            session=session
                                            selected=is_selected
                                            training_selected=training_selected
                                            on_training_select_change=Callback::new(move |checked| {
                                                training_toggle_handler.run((training_id.clone(), checked));
                                            })
                                            on_archive=archive_callback
                                            on_unarchive=unarchive_callback
                                            on_delete=Callback::new(move |_: ()| {
                                                delete_handler.run(delete_id.clone());
                                            })
                                            is_archived=showing_archived
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Delete confirmation dialog
            {move || {
                let title = pending_delete_title.try_get().unwrap_or_default();
                let description = format!(
                    "Are you sure you want to delete '{}'? This action cannot be undone.",
                    title,
                );
                view! {
                    <ConfirmationDialog
                        open=show_delete_confirm
                        title="Delete Session"
                        description=description
                        severity=ConfirmationSeverity::Destructive
                        confirm_text="Delete"
                        typed_confirmation=title
                        on_confirm=Callback::new(confirm_delete)
                        on_cancel=Callback::new(cancel_delete)
                    />
                }
            }}
        </div>
    }
}

/// Session list item in the workspace sidebar
#[component]
fn SessionListItem(
    session: ChatSessionMeta,
    /// Whether this session is currently selected
    #[prop(into)]
    selected: Signal<bool>,
    /// Whether this session is selected for "Create Adapter from Selection"
    #[prop(into)]
    training_selected: Signal<bool>,
    /// Callback for selecting this session as training input
    on_training_select_change: Callback<bool>,
    on_archive: Option<Callback<()>>,
    on_unarchive: Option<Callback<()>>,
    on_delete: Callback<()>,
    /// Whether this item is in the archived list (controls overflow menu options)
    #[prop(default = false)]
    is_archived: bool,
) -> impl IntoView {
    let settings = use_settings();
    let id = session.id.clone();
    let href = format!("/chat/s/{}", id);
    let updated_at = session.updated_at.clone();
    let message_count = session.message_count;
    let session_title = session.title.clone();
    let session_preview = session.preview.clone();
    let training_aria_label = format!("Select '{}' to create an adapter", session_title.clone());
    let archive_action = on_archive;
    let unarchive_action = on_unarchive;

    // Overflow menu state
    let show_overflow = RwSignal::new(false);
    let overflow_trigger_ref = NodeRef::<leptos::html::Button>::new();
    let overflow_menu_ref = NodeRef::<leptos::html::Div>::new();
    let show_permanent_delete_confirm = RwSignal::new(false);
    let hard_delete_session_id = id.clone();

    // Focus first menu item whenever overflow menu opens.
    {
        Effect::new(move |_| {
            if !show_overflow.try_get().unwrap_or(false) {
                return;
            }
            gloo_timers::callback::Timeout::new(0, move || {
                if let Some(menu) = overflow_menu_ref.get() {
                    if let Ok(Some(first_item)) = menu.query_selector(r#"[role="menuitem"]"#) {
                        if let Ok(first_item) = first_item.dyn_into::<web_sys::HtmlElement>() {
                            let _ = first_item.focus();
                        }
                    }
                }
            })
            .forget();
        });
    }

    let handle_overflow_keydown = Callback::new({
        #[allow(clippy::redundant_locals)]
        let overflow_menu_ref = overflow_menu_ref;
        #[allow(clippy::redundant_locals)]
        let overflow_trigger_ref = overflow_trigger_ref;
        move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                show_overflow.set(false);
                if let Some(trigger) = overflow_trigger_ref.get() {
                    let _ = trigger.focus();
                }
            }
            "ArrowDown" | "ArrowUp" => {
                ev.prevent_default();
                ev.stop_propagation();
                let Some(menu) = overflow_menu_ref.get() else {
                    return;
                };
                let Ok(items) = menu.query_selector_all(r#"[role="menuitem"]"#) else {
                    return;
                };
                let len = items.length();
                if len == 0 {
                    return;
                }
                let active = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .and_then(|el| el.dyn_into::<web_sys::Node>().ok());

                let mut current_idx: i32 = -1;
                for idx in 0..len {
                    let Some(node) = items.item(idx) else {
                        continue;
                    };
                    if active
                        .as_ref()
                        .is_some_and(|active_node| node.is_same_node(Some(active_node)))
                    {
                        current_idx = idx as i32;
                        break;
                    }
                }

                let next_idx = if ev.key() == "ArrowDown" {
                    if current_idx < 0 {
                        0
                    } else {
                        ((current_idx + 1) as u32) % len
                    }
                } else if current_idx < 0 {
                    len - 1
                } else {
                    ((current_idx - 1 + len as i32) as u32) % len
                };

                if let Some(node) = items.item(next_idx) {
                    if let Ok(item) = node.dyn_into::<web_sys::HtmlElement>() {
                        let _ = item.focus();
                    }
                }
            }
            _ => {}
        }
    });

    let row_class = Signal::derive(move || {
        format!(
            "chat-session-row {}",
            if selected.try_get().unwrap_or(false) {
                "chat-session-row--active"
            } else {
                ""
            }
        )
    });

    view! {
        <ChatSessionRowShell class=row_class>
            <div
                class="chat-session-row-checkbox"
                data-testid="chat-session-training-checkbox"
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    ev.stop_propagation();
                }
            >
                <Checkbox
                    checked=training_selected
                    on_change=Callback::new(move |checked| on_training_select_change.run(checked))
                    aria_label=training_aria_label
                />
            </div>

            <a href=href class="chat-session-row-link">
                <h3 class="chat-session-row-title">{session_title}</h3>

                {if !session_preview.is_empty() {
                    let preview = session_preview.clone();
                    Some(view! {
                        <p class="chat-session-row-preview">{preview}</p>
                    })
                } else {
                    None
                }}

                <div class="chat-session-row-meta">
                    {move || {
                        let show_timestamps = settings
                            .try_get()
                            .map(|s| s.show_timestamps)
                            .unwrap_or(true);
                        if show_timestamps {
                            view! {
                                <>
                                    <span>{format_relative_time(&updated_at)}</span>
                                    <span>"·"</span>
                                </>
                            }
                            .into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}
                    <span>{format!("{} msgs", message_count)}</span>
                </div>
            </a>

            <div class="chat-session-row-actions">
                {archive_action.map(|archive| view! {
                    <button
                        class="btn btn-ghost btn-icon-sm chat-session-row-action"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            archive.run(());
                        }
                        title="Archive session"
                        aria-label="Archive session"
                        data-testid="chat-session-archive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 7h18M5 7v10a2 2 0 002 2h10a2 2 0 002-2V7M9 11h6"/>
                        </svg>
                    </button>
                })}
                {unarchive_action.map(|unarchive| view! {
                    <button
                        class="btn btn-ghost btn-icon-sm chat-session-row-action"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            unarchive.run(());
                        }
                        title="Restore session"
                        aria-label="Restore session"
                        data-testid="chat-session-unarchive"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 12a8 8 0 118 8M4 12V8m0 4h4"/>
                        </svg>
                    </button>
                })}
                <button
                    class="btn btn-ghost btn-icon-sm chat-session-row-action chat-session-row-action--destructive"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        on_delete.run(());
                    }
                    title="Delete session"
                    aria-label="Delete session"
                    data-testid="chat-session-delete"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                    </svg>
                </button>

                <div class="relative">
                    <button
                        node_ref=overflow_trigger_ref
                        class="btn btn-ghost btn-icon-sm chat-session-row-action chat-session-row-action--muted"
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            ev.stop_propagation();
                            show_overflow.update(|v| *v = !*v);
                        }
                        title="More actions"
                        aria-label="More actions"
                        aria-haspopup="menu"
                        attr:aria-expanded=move || show_overflow.try_get().unwrap_or(false).to_string()
                        data-testid="chat-session-overflow"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 20 20">
                            <circle cx="10" cy="4" r="1.5"/>
                            <circle cx="10" cy="10" r="1.5"/>
                            <circle cx="10" cy="16" r="1.5"/>
                        </svg>
                    </button>
                    {move || {
                        if !show_overflow.try_get().unwrap_or(false) {
                            return view! {}.into_any();
                        }
                        view! {
                            <div
                                class="fixed inset-0 z-40"
                                on:click=move |_| {
                                    show_overflow.set(false);
                                    if let Some(trigger) = overflow_trigger_ref.get() {
                                        let _ = trigger.focus();
                                    }
                                }
                            />
                            <div
                                node_ref=overflow_menu_ref
                                class="absolute right-0 top-full z-50 mt-1 w-40 rounded-md border border-border bg-background shadow-lg py-1"
                                role="menu"
                                on:keydown=move |ev| handle_overflow_keydown.run(ev)
                            >
                                {is_archived.then(|| view! {
                                    <div class="border-t border-border my-1" role="separator" />
                                    <button
                                        class="btn btn-ghost btn-sm w-full text-left px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors"
                                        role="menuitem"
                                        on:click=move |ev: web_sys::MouseEvent| {
                                            ev.stop_propagation();
                                            show_overflow.set(false);
                                            show_permanent_delete_confirm.set(true);
                                        }
                                    >
                                        "Permanent Delete"
                                    </button>
                                })}
                            </div>
                        }.into_any()
                    }}
                </div>
            </div>

            {move || {
                if show_permanent_delete_confirm.try_get().unwrap_or(false) {
                    let delete_id = hard_delete_session_id.clone();
                    Some(view! {
                        <ConfirmationDialog
                            open=show_permanent_delete_confirm
                            title="Permanently Delete Session"
                            description="This will permanently remove the session and all its data. This cannot be undone."
                            severity=ConfirmationSeverity::Destructive
                            confirm_text="Permanently Delete"
                            on_confirm=Callback::new(move |_| {
                                let id = delete_id.clone();
                                let on_delete = on_delete;
                                show_permanent_delete_confirm.set(false);
                                wasm_bindgen_futures::spawn_local(async move {
                                    let client = ApiClient::with_base_url(api_base_url());
                                    match client.hard_delete_session(&id).await {
                                        Ok(_) => on_delete.run(()),
                                        Err(e) => {
                                            report_error_with_toast(&e, "Failed to permanently delete session", Some("/chat"), false);
                                        }
                                    }
                                });
                            })
                            on_cancel=Callback::new(move |_| show_permanent_delete_confirm.set(false))
                        />
                    })
                } else { None }
            }}
        </ChatSessionRowShell>
    }
}

pub(super) fn generate_readable_id(_prefix: &str, _slug_source: &str) -> String {
    adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string()
}
