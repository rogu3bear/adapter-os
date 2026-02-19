use super::{Button, ButtonSize, ButtonVariant};
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

#[component]
pub fn CodeEditor(
    active_path: Signal<Option<String>>,
    content: Signal<String>,
    is_loading: Signal<bool>,
    on_content_change: Callback<String>,
    on_save: Callback<String>,
) -> impl IntoView {
    let container_ref = NodeRef::<html::Div>::new();
    let uses_wrapper = RwSignal::new(false);

    Effect::new(move || {
        let Some(container) = container_ref.get() else {
            return;
        };

        let Some(wrapper) = codemirror_wrapper() else {
            uses_wrapper.set(false);
            return;
        };

        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("value"),
            &JsValue::from_str(&content.get()),
        );
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("language"),
            &JsValue::from_str(&guess_language(active_path.get().as_deref().unwrap_or(""))),
        );

        if let Ok(create_fn) = js_sys::Reflect::get(&wrapper, &JsValue::from_str("create"))
            .and_then(|f| f.dyn_into::<js_sys::Function>())
        {
            let _ = create_fn.call2(&wrapper, container.as_ref(), &options);
            uses_wrapper.set(true);
        } else {
            uses_wrapper.set(false);
        }
    });

    Effect::new(move || {
        let Some(container) = container_ref.get() else {
            return;
        };

        let path = active_path.get().unwrap_or_default();
        if let Some(wrapper) = codemirror_wrapper() {
            if let Ok(set_value_fn) = js_sys::Reflect::get(&wrapper, &JsValue::from_str("setValue"))
                .and_then(|f| f.dyn_into::<js_sys::Function>())
            {
                let _ = set_value_fn.call2(
                    &wrapper,
                    container.as_ref(),
                    &JsValue::from_str(&content.get()),
                );
            }
            if let Ok(set_language_fn) =
                js_sys::Reflect::get(&wrapper, &JsValue::from_str("setLanguage"))
                    .and_then(|f| f.dyn_into::<js_sys::Function>())
            {
                let _ = set_language_fn.call2(
                    &wrapper,
                    container.as_ref(),
                    &JsValue::from_str(&guess_language(&path)),
                );
            }
        }
    });

    let on_save_click = {
        let on_save = on_save.clone();
        let on_content_change = on_content_change.clone();
        move |_| {
            if let (Some(wrapper), Some(container)) = (codemirror_wrapper(), container_ref.get()) {
                if let Ok(get_value_fn) =
                    js_sys::Reflect::get(&wrapper, &JsValue::from_str("getValue"))
                        .and_then(|f| f.dyn_into::<js_sys::Function>())
                {
                    if let Ok(value) = get_value_fn.call1(&wrapper, container.as_ref()) {
                        if let Some(text) = value.as_string() {
                            on_content_change.run(text.clone());
                            on_save.run(text);
                            return;
                        }
                    }
                }
            }
            on_save.run(content.get_untracked());
        }
    };

    view! {
        <div class="files-editor">
            <div class="files-editor-toolbar">
                <div class="files-editor-path">{move || active_path.get().unwrap_or_else(|| "No file selected".to_string())}</div>
                <Button
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Sm
                    disabled=Signal::derive(move || is_loading.get() || active_path.get().is_none())
                    on_click=Callback::new(on_save_click)
                >
                    "Save"
                </Button>
            </div>
            <div class="files-editor-body">
                <div class="files-editor-codemirror" node_ref=container_ref class:hidden=move || !uses_wrapper.get()></div>
                <textarea
                    class="files-editor-fallback"
                    class:hidden=move || uses_wrapper.get()
                    prop:value=move || content.get()
                    on:input=move |ev| on_content_change.run(event_target_value(&ev))
                    placeholder="Select a file to edit"
                    aria_label="Code editor"
                ></textarea>
            </div>
        </div>
    }
}

fn codemirror_wrapper() -> Option<JsValue> {
    let window = web_sys::window()?;
    js_sys::Reflect::get(&window, &JsValue::from_str("AdapterOSCodeMirror")).ok()
}

fn guess_language(path: &str) -> String {
    if path.ends_with(".rs") {
        "rust".to_string()
    } else if path.ends_with(".ts") || path.ends_with(".tsx") {
        "typescript".to_string()
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "javascript".to_string()
    } else if path.ends_with(".json") {
        "json".to_string()
    } else if path.ends_with(".md") {
        "markdown".to_string()
    } else if path.ends_with(".toml") {
        "toml".to_string()
    } else {
        "text".to_string()
    }
}
