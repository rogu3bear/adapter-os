//! Copyable ID component with clipboard interaction.
//!
//! Displays a (potentially truncated) identifier with a copy-to-clipboard button.
//! Shows "Copied" feedback for 1.2 seconds after a successful copy. Supports optional
//! labels and word aliases.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::icons::IconCopy;

#[component]
pub fn CopyableId(
    id: String,
    #[prop(optional)] label: String,
    #[prop(optional)] display_name: String,
    #[prop(optional)] truncate: usize,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    let label = if label.is_empty() { None } else { Some(label) };
    let display_name_opt = if display_name.is_empty() {
        None
    } else {
        Some(display_name)
    };
    let truncate = if truncate == 0 { None } else { Some(truncate) };

    // When display_name (word alias) is provided, show it as primary text.
    // Otherwise fall back to TypedId::short() via adapteros_id, or truncate.
    let display_id = if let Some(ref alias) = display_name_opt {
        alias.clone()
    } else {
        let short = adapteros_id::short_id(&id);
        match truncate {
            Some(len) if short.len() > len => format!("{}…", &short[..len]),
            _ => short,
        }
    };

    let id_for_title = id.clone();
    let on_copy = move |_| {
        let id = id.clone();
        let copied = copied;
        spawn_local(async move {
            if copy_to_clipboard(&id).await {
                copied.set(true);
                let copied_reset = copied;
                leptos::task::spawn_local(async move {
                    gloo_timers::future::TimeoutFuture::new(1200).await;
                    copied_reset.set(false);
                });
            }
        });
    };

    view! {
        <div class="flex flex-col gap-1">
            {label.as_ref().map(|label| view! {
                <span class="text-xs text-muted-foreground">{label.clone()}</span>
            })}
            <div class="flex items-center gap-2">
                <span class="font-mono text-xs" title=id_for_title.clone()>{display_id}</span>
                <button
                    class="text-muted-foreground hover:text-foreground transition-colors"
                    title="Copy ID"
                    aria-label="Copy ID to clipboard"
                    on:click=on_copy
                >
                    <IconCopy class="h-3 w-3"/>
                </button>
                {move || {
                    if copied.get() {
                        view! { <span class="text-xs text-muted-foreground">"Copied"</span> }
                    } else {
                        view! { <span class="sr-only">""</span> }
                    }
                }}
            </div>
        </div>
    }
}

async fn copy_to_clipboard(text: &str) -> bool {
    // Defensive: window() may be None in non-browser environments (SSR, tests, workers)
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    let clipboard =
        js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard")).ok();
    let Some(clipboard) = clipboard else {
        return false;
    };
    let write_text_fn =
        js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")).ok();
    let Some(write_text_fn) = write_text_fn else {
        return false;
    };
    let Ok(write_text_fn) = write_text_fn.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let promise = match write_text_fn.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let promise = js_sys::Promise::resolve(&promise);
    wasm_bindgen_futures::JsFuture::from(promise).await.is_ok()
}
