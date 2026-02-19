//! ActionsOverflow - compact dropdown for secondary page actions.

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::icons::IconDotsHorizontal;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// A static overflow action item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionsOverflowItem {
    /// Display label for the action.
    pub label: String,
    /// Navigation target.
    pub href: String,
}

impl ActionsOverflowItem {
    /// Create a new overflow action item.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
        }
    }
}

/// Secondary actions overflow menu.
#[component]
pub fn ActionsOverflow(
    /// Static or reactive list of overflow items.
    #[prop(into)]
    items: Signal<Vec<ActionsOverflowItem>>,
    /// Accessible label for the trigger button.
    #[prop(optional, into)]
    aria_label: String,
) -> impl IntoView {
    let (is_open, set_is_open) = signal(false);
    let container_ref = NodeRef::<leptos::html::Div>::new();

    // Match existing menu behavior: close on outside click/touch and Escape.
    let listeners_set = StoredValue::new(false);
    Effect::new(move || {
        if listeners_set.get_value() {
            return;
        }
        listeners_set.set_value(true);

        let is_open = is_open;
        let set_is_open = set_is_open;
        let container_ref = container_ref;

        let click_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            if !is_open.try_get_untracked().unwrap_or(false) {
                return;
            }
            let Some(target) = event.target() else {
                return;
            };
            let Ok(target_node) = target.dyn_into::<web_sys::Node>() else {
                return;
            };
            if let Some(container) = container_ref.get() {
                if container.contains(Some(&target_node)) {
                    return;
                }
            }
            let _ = set_is_open.try_set(false);
        }) as Box<dyn FnMut(_)>);

        let key_closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if !is_open.try_get_untracked().unwrap_or(false) {
                return;
            }
            if event.key() == "Escape" {
                let _ = set_is_open.try_set(false);
            }
        }) as Box<dyn FnMut(_)>);

        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let _ = document.add_event_listener_with_callback(
                "mousedown",
                click_closure.as_ref().unchecked_ref(),
            );
            let _ = document.add_event_listener_with_callback(
                "touchstart",
                click_closure.as_ref().unchecked_ref(),
            );
            let _ = document
                .add_event_listener_with_callback("keydown", key_closure.as_ref().unchecked_ref());
        }

        click_closure.forget();
        key_closure.forget();
    });

    let trigger_aria_label = if aria_label.trim().is_empty() {
        "More actions".to_string()
    } else {
        aria_label
    };

    view! {
        <Show when=move || !items.get().is_empty()>
            <div class="relative" node_ref=container_ref>
                <Button
                    variant=ButtonVariant::Outline
                    size=ButtonSize::Sm
                    aria_label=trigger_aria_label.clone()
                    on_click=Callback::new(move |_| {
                        set_is_open.update(|open| *open = !*open);
                    })
                >
                    <IconDotsHorizontal class="h-4 w-4".to_string() />
                </Button>

                <Show when=move || is_open.get()>
                    <div class="absolute right-0 top-full z-50 mt-1 min-w-44 rounded-lg border border-border bg-background shadow-lg">
                        <div class="p-1">
                            {move || {
                                items
                                    .get()
                                    .into_iter()
                                    .map(|item| {
                                        let label = item.label;
                                        let href = item.href;
                                        view! {
                                            <a
                                                href=href
                                                class="flex w-full items-center rounded px-3 py-2 text-sm transition-colors hover:bg-muted/50"
                                                on:click=move |_| set_is_open.set(false)
                                            >
                                                {label}
                                            </a>
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            }}
                        </div>
                    </div>
                </Show>
            </div>
        </Show>
    }
}
