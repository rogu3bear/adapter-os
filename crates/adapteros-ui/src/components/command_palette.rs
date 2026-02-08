//! Command Palette component
//!
//! A Ctrl+K modal for keyboard-first navigation and actions.

use crate::components::search_results::{SearchEmptyState, SearchResultsList};
use crate::search::{RecentItem, SearchAction};
use crate::signals::search::use_search;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

/// Command Palette modal component
#[component]
pub fn CommandPalette() -> impl IntoView {
    let search = use_search();
    let navigate = use_navigate();
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Clone for closures
    let search_for_close = search.clone();
    let search_for_keydown = search.clone();
    let search_for_select = search.clone();
    let search_for_effect = search.clone();

    // Auto-focus input when opened
    Effect::new(move || {
        if search_for_effect.command_palette_open.get() {
            // Prefetch entities when palette opens
            search_for_effect.prefetch_entities();

            // Focus input after a brief delay to ensure DOM is ready
            let input_ref = input_ref;
            set_timeout_simple(
                move || {
                    if let Some(input) = input_ref.get() {
                        let _ = input.focus();
                    }
                },
                50,
            );
        }
    });

    // Handle result selection
    let on_select = {
        let navigate = navigate.clone();
        let search = search_for_select.clone();
        Callback::new(move |result: crate::search::SearchResult| {
            // Record to recent items
            let recent_item = RecentItem::new(
                match result.result_type {
                    crate::search::SearchResultType::Page => crate::search::RecentItemType::Page,
                    crate::search::SearchResultType::Adapter => {
                        crate::search::RecentItemType::Adapter
                    }
                    crate::search::SearchResultType::Model => crate::search::RecentItemType::Model,
                    crate::search::SearchResultType::Worker => {
                        crate::search::RecentItemType::Worker
                    }
                    crate::search::SearchResultType::Stack => {
                        crate::search::RecentItemType::Adapter
                    }
                    crate::search::SearchResultType::Action => {
                        crate::search::RecentItemType::Action
                    }
                },
                &result.id,
                &result.title,
                result.path().unwrap_or(""),
            );
            search.record_recent(recent_item);

            // Execute action
            match &result.action {
                SearchAction::Navigate(path) => {
                    navigate(path, Default::default());
                }
                SearchAction::Execute(command) => {
                    execute_command(command);
                }
            }

            // Close palette
            search.close();
        })
    };

    let search_for_when = search.clone();
    let search_for_backdrop = search_for_close.clone();
    let search_for_keydown_view = search_for_keydown.clone();
    let navigate_for_keydown = navigate.clone();
    let search_for_value = search.clone();
    let search_for_input = search.clone();
    let search_for_clear_outer = search.clone();

    view! {
        <Show when=move || search_for_when.command_palette_open.get()>
            // Backdrop
            <div
                class="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm"
                style="animation: fadeIn 100ms ease-out"
                on:click={
                    let search = search_for_backdrop.clone();
                    move |_| search.close()
                }
            />

            // Palette container
            <div
                class="fixed left-1/2 top-1/4 -translate-x-1/2 z-50 w-full max-w-xl"
                style="animation: slideDown 150ms ease-out"
                role="dialog"
                aria-modal="true"
                aria-label="Command palette"
            >
                <div
                    class="mx-4 rounded-lg border bg-popover text-popover-foreground shadow-2xl overflow-hidden"
                    on:click=|e| e.stop_propagation()
                    on:keydown={
                        let search = search_for_keydown_view.clone();
                        let navigate = navigate_for_keydown.clone();
                        move |ev: KeyboardEvent| {
                            match ev.key().as_str() {
                                "Escape" => {
                                    ev.prevent_default();
                                    search.close();
                                }
                                "ArrowUp" | "k" if ev.ctrl_key() => {
                                    ev.prevent_default();
                                    search.select_prev();
                                }
                                "ArrowDown" | "j" if ev.ctrl_key() => {
                                    ev.prevent_default();
                                    search.select_next();
                                }
                                "Enter" => {
                                    ev.prevent_default();
                                    if let Some(result) = search.selected_result() {
                                        // Record to recent
                                        let recent_item = RecentItem::new(
                                            match result.result_type {
                                                crate::search::SearchResultType::Page => crate::search::RecentItemType::Page,
                                                crate::search::SearchResultType::Adapter => crate::search::RecentItemType::Adapter,
                                                crate::search::SearchResultType::Model => crate::search::RecentItemType::Model,
                                                crate::search::SearchResultType::Worker => crate::search::RecentItemType::Worker,
                                                crate::search::SearchResultType::Stack => crate::search::RecentItemType::Adapter,
                                                crate::search::SearchResultType::Action => crate::search::RecentItemType::Action,
                                            },
                                            &result.id,
                                            &result.title,
                                            result.path().unwrap_or(""),
                                        );
                                        search.record_recent(recent_item);

                                        // Execute
                                        match &result.action {
                                            SearchAction::Navigate(path) => {
                                                navigate(path, Default::default());
                                            }
                                            SearchAction::Execute(command) => {
                                                execute_command(command);
                                            }
                                        }
                                        search.close();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                >
                    // Search input
                    <div class="flex items-center gap-3 border-b px-4 py-3">
                        <svg
                            xmlns="http://www.w3.org/2000/svg"
                            class="h-5 w-5 text-muted-foreground shrink-0"
                            fill="none"
                            viewBox="0 0 24 24"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
                        </svg>
                        <input
                            node_ref=input_ref
                            type="text"
                            class="flex-1 bg-transparent text-base outline-none placeholder:text-muted-foreground"
                            placeholder="Search pages, adapters, actions..."
                            aria-label="Search pages, adapters, and actions"
                            prop:value={
                                let search = search_for_value.clone();
                                move || search.query.get()
                            }
                            on:input={
                                let search = search_for_input.clone();
                                move |ev| {
                                    let value = event_target_value(&ev);
                                    search.search_debounced(value);
                                }
                            }
                            on:blur={
                                let search = search_for_input.clone();
                                move |_| {
                                    // Flush pending debounce on blur so users see results before dialog closes
                                    let value = search.query.get_untracked();
                                    search.search_immediate(value);
                                }
                            }
                        />
                        {
                            let search_for_clear = search_for_clear_outer.clone();
                            move || {
                            if !search_for_clear.query.get().is_empty() {
                                let search = search_for_clear.clone();
                                Some(view! {
                                    <button
                                        class="p-1 rounded hover:bg-muted text-muted-foreground"
                                        aria-label="Clear search"
                                        on:click=move |_| {
                                            search.query.set(String::new());
                                            search.results.set(Vec::new());
                                        }
                                    >
                                        <svg
                                            xmlns="http://www.w3.org/2000/svg"
                                            class="h-4 w-4"
                                            fill="none"
                                            viewBox="0 0 24 24"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            aria-hidden="true"
                                        >
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                        </svg>
                                    </button>
                                })
                            } else {
                                None
                            }
                        }}
                    </div>

                    // Results or empty state
                    {
                        let search_for_results = search.clone();
                        let on_select_for_results = on_select;
                        move || {
                            let query = search_for_results.query.get();
                            let results = search_for_results.results.get();

                            if query.is_empty() {
                                // Show recent items when query is empty
                                let recent = search_for_results.recent_items();
                                if recent.is_empty() {
                                    view! { <SearchEmptyState show_recent_hint=false/> }.into_any()
                                } else {
                                    view! {
                                        <RecentItemsList
                                            items=recent
                                            on_select=on_select_for_results
                                        />
                                    }.into_any()
                                }
                            } else if results.is_empty() {
                                view! {
                                    <div class="px-4 py-8 text-center text-sm text-muted-foreground">
                                        "No results for \""{query.clone()}"\""
                                    </div>
                                }.into_any()
                            } else {
                                let search = search_for_results.clone();
                                view! {
                                    <SearchResultsList
                                        results=Signal::derive(move || search.results.get())
                                        selected_index=Signal::derive(move || search.selected_index.get())
                                        on_select=on_select_for_results
                                    />
                                }.into_any()
                            }
                        }
                    }

                    // Footer with keyboard hints
                    <div class="flex items-center justify-between border-t px-4 py-2 text-xs text-muted-foreground">
                        <div class="flex items-center gap-4">
                            <span class="flex items-center gap-1">
                                <kbd class="px-1 py-0.5 rounded bg-muted border border-border font-mono">"↑↓"</kbd>
                                " navigate"
                            </span>
                            <span class="flex items-center gap-1">
                                <kbd class="px-1 py-0.5 rounded bg-muted border border-border font-mono">"↵"</kbd>
                                " select"
                            </span>
                            <span class="flex items-center gap-1">
                                <kbd class="px-1 py-0.5 rounded bg-muted border border-border font-mono">"esc"</kbd>
                                " close"
                            </span>
                        </div>
                        <span class="hidden sm:block text-muted-foreground/70">
                            "adapterOS"
                        </span>
                    </div>
                </div>
            </div>
        </Show>
    }
}

/// Recent items list (shown when query is empty)
#[component]
fn RecentItemsList(
    items: Vec<RecentItem>,
    on_select: Callback<crate::search::SearchResult>,
) -> impl IntoView {
    view! {
        <div class="max-h-80 overflow-y-auto py-2">
            <div class="flex items-center gap-2 px-3 py-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-3.5 w-3.5"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"/>
                </svg>
                <span>"Recent"</span>
            </div>
            <div class="space-y-0.5">
                {items.into_iter().map(|item| {
                    let item_clone = item.clone();
                    let item_label = item.label.clone();
                    view! {
                        <button
                            type="button"
                            class="flex w-full items-center gap-3 px-3 py-2 mx-1 rounded-md hover:bg-muted/50 cursor-pointer text-left"
                            aria-label={format!("Go to {}", item_label)}
                            on:click=move |_| {
                                // Convert recent item to search result for selection
                                let result = crate::search::SearchResult {
                                    id: item_clone.id.clone(),
                                    result_type: match item_clone.item_type {
                                        crate::search::RecentItemType::Page => crate::search::SearchResultType::Page,
                                        crate::search::RecentItemType::Adapter => crate::search::SearchResultType::Adapter,
                                        crate::search::RecentItemType::Model => crate::search::SearchResultType::Model,
                                        crate::search::RecentItemType::Worker => crate::search::SearchResultType::Worker,
                                        crate::search::RecentItemType::Action => crate::search::SearchResultType::Action,
                                    },
                                    title: item_clone.label.clone(),
                                    subtitle: item_clone.subtitle.clone(),
                                    score: 1.0,
                                    action: crate::search::SearchAction::Navigate(item_clone.path.clone()),
                                    shortcut: None,
                                };
                                on_select.run(result);
                            }
                        >
                            <div class="flex h-8 w-8 items-center justify-center rounded-md bg-muted shrink-0">
                                <svg
                                    xmlns="http://www.w3.org/2000/svg"
                                    class="h-4 w-4 text-muted-foreground"
                                    fill="none"
                                    viewBox="0 0 24 24"
                                    stroke="currentColor"
                                    stroke-width="2"
                                >
                                    <path stroke-linecap="round" stroke-linejoin="round" d=item.item_type.icon_path()/>
                                </svg>
                            </div>
                            <div class="flex-1 min-w-0">
                                <p class="text-sm font-medium truncate">{item.label.clone()}</p>
                                {item.subtitle.as_ref().map(|subtitle| {
                                    view! {
                                        <p class="text-xs text-muted-foreground truncate">{subtitle.clone()}</p>
                                    }
                                })}
                            </div>
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

/// Execute a command by key
fn execute_command(command: &str) {
    match command {
        "toggle-chat" => {
            // Will be handled by chat context
            web_sys::console::log_1(&"Command: toggle-chat".into());
        }
        "toggle-theme" => {
            // Toggle between light/dark
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(html) = document.document_element() {
                    let current = html.class_list();
                    if current.contains("dark") {
                        let _ = current.remove_1("dark");
                    } else {
                        let _ = current.add_1("dark");
                    }
                }
            }
        }
        "new-chat" => {
            web_sys::console::log_1(&"Command: new-chat".into());
        }
        "refresh" => {
            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        }
        "logout" => {
            web_sys::console::log_1(&"Command: logout".into());
        }
        _ => {
            web_sys::console::log_1(&format!("Unknown command: {}", command).into());
        }
    }
}

// Simple timeout helper
#[cfg(target_arch = "wasm32")]
fn set_timeout_simple<F: FnOnce() + 'static>(f: F, ms: i32) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let closure = Closure::once_into_js(f);
    let Some(window) = web_sys::window() else {
        tracing::error!("set_timeout_simple: no window object available");
        return;
    };
    let _ =
        window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}
