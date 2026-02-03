use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::icons::IconCopy;

#[component]
pub fn CopyableId(
    id: String,
    #[prop(optional)] label: String,
    #[prop(optional)] legacy_id: String,
    #[prop(optional)] truncate: usize,
) -> impl IntoView {
    let copied = RwSignal::new(false);
    let label = if label.is_empty() { None } else { Some(label) };
    let legacy_id = if legacy_id.is_empty() {
        None
    } else {
        Some(legacy_id)
    };
    let truncate = if truncate == 0 { None } else { Some(truncate) };
    let display_id = match truncate {
        Some(len) if id.len() > len => format!("{}…", &id[..len]),
        _ => id.clone(),
    };

    let id_for_title = id.clone();
    let on_copy = move |_| {
        let id = id.clone();
        let copied = copied.clone();
        spawn_local(async move {
            if copy_to_clipboard(&id).await {
                copied.set(true);
                let copied_reset = copied.clone();
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
            {legacy_id.as_ref().map(|legacy| view! {
                <div class="text-xs text-muted-foreground font-mono" title=legacy.clone()>
                    {format!("legacy: {}", legacy)}
                </div>
            })}
        </div>
    }
}

async fn copy_to_clipboard(text: &str) -> bool {
    let window = web_sys::window().unwrap();
    let navigator = window.navigator();
    let clipboard = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .ok();
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
