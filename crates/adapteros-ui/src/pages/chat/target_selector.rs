use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{use_chat, ChatTarget};
use leptos::prelude::*;

#[derive(Debug, Clone, Default)]
struct TargetOptions {
    models: Vec<(String, String)>,   // (id, name)
    policies: Vec<(String, String)>, // (cpid, display_name)
    loading: bool,
    error: Option<String>,
}

#[component]
pub(super) fn ChatTargetSelector(#[prop(optional)] inline: bool) -> impl IntoView {
    let (chat_state, chat_action) = use_chat();
    let show_dropdown = RwSignal::new(false);
    let options = RwSignal::new(TargetOptions::default());
    let has_loaded = RwSignal::new(false);

    let (system_status, _) = use_system_status();
    let active_model_name =
        Signal::derive(
            move || match system_status.try_get().unwrap_or(LoadingState::Idle) {
                LoadingState::Loaded(ref status) => status
                    .kernel
                    .as_ref()
                    .and_then(|k| k.model.as_ref())
                    .and_then(|m| m.model_id.clone()),
                _ => None,
            },
        );

    let toggle_dropdown = move |_| {
        show_dropdown.update(|v| *v = !*v);
    };

    let select_target = {
        let action = chat_action.clone();
        move |target: ChatTarget| {
            action.set_target(target);
            show_dropdown.set(false);
        }
    };

    Effect::new(move |prev_open: Option<bool>| {
        let Some(is_open) = show_dropdown.try_get() else {
            return prev_open.unwrap_or(false);
        };
        if let Some(was_open) = prev_open {
            if was_open && !is_open {
                let _ = has_loaded.try_set(false);
            }
        }
        is_open
    });

    Effect::new(move || {
        if show_dropdown.try_get().unwrap_or(false) && !has_loaded.try_get().unwrap_or(true) {
            has_loaded.set(true);
            options.update(|o| {
                o.loading = true;
                o.error = None;
            });

            wasm_bindgen_futures::spawn_local(async move {
                let client = crate::api::ApiClient::with_base_url(crate::api::api_base_url());

                let models_fut = client.list_models();
                let policies_fut = client.list_policies();

                let (models_res, policies_res) = futures::join!(models_fut, policies_fut);

                let mut errors: Vec<String> = Vec::new();

                let _ = options.try_update(|o| {
                    o.loading = false;

                    match models_res {
                        Ok(resp) => {
                            let mut model_rows = resp.models;
                            model_rows.sort_by(|a, b| {
                                let a_coreml = a.backend.as_deref() == Some("coreml");
                                let b_coreml = b.backend.as_deref() == Some("coreml");
                                a_coreml
                                    .cmp(&b_coreml)
                                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                                    .then_with(|| a.id.cmp(&b.id))
                            });
                            o.models = model_rows
                                .into_iter()
                                .map(|m| {
                                    let mut label = if m.name.trim() != m.id {
                                        format!("{} ({})", m.name, m.id)
                                    } else {
                                        m.name.clone()
                                    };
                                    if let Some(q) = m.quantization.as_deref() {
                                        if !q.trim().is_empty() {
                                            label.push_str(&format!(" • {}", q));
                                        }
                                    }
                                    if let Some(backend) = m.backend.as_deref() {
                                        if !backend.trim().is_empty() {
                                            if backend == "coreml" {
                                                label.push_str(" • CoreML");
                                            } else {
                                                label.push_str(&format!(
                                                    " • {}",
                                                    backend.to_uppercase()
                                                ));
                                            }
                                        }
                                    }
                                    (m.id, label)
                                })
                                .collect();
                        }
                        Err(e) => {
                            let msg = format!("Models: {}", e);
                            web_sys::console::warn_1(&msg.clone().into());
                            errors.push(msg);
                        }
                    }

                    match policies_res {
                        Ok(policies) => {
                            o.policies = policies
                                .into_iter()
                                .map(|p| {
                                    let display = p
                                        .cpid
                                        .replace('-', " ")
                                        .split_whitespace()
                                        .map(|w| {
                                            let mut chars = w.chars();
                                            match chars.next() {
                                                Some(first) => {
                                                    first.to_uppercase().chain(chars).collect()
                                                }
                                                None => String::new(),
                                            }
                                        })
                                        .collect::<Vec<String>>()
                                        .join(" ");
                                    (p.cpid, display)
                                })
                                .collect();
                        }
                        Err(e) => {
                            let msg = format!("Policies: {}", e);
                            web_sys::console::warn_1(&msg.clone().into());
                            errors.push(msg);
                        }
                    }

                    if !errors.is_empty() {
                        o.error = Some(format!("Failed to load: {}", errors.join(", ")));
                    }
                });
            });
        }
    });

    let container_class = if inline {
        "relative"
    } else {
        "relative border-b px-4 py-2"
    };
    let button_class = if inline {
        "flex items-center gap-2 rounded-md border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted transition-colors"
    } else {
        "flex w-full items-center justify-between rounded-md border bg-background px-3 py-2 text-sm hover:bg-muted transition-colors"
    };
    let dropdown_class = if inline {
        "absolute left-0 top-full z-50 mt-1 min-w-[200px] rounded-md border border-border bg-popover shadow-lg max-h-80 overflow-y-auto"
    } else {
        "absolute left-4 right-4 top-full z-50 mt-1 rounded-md border bg-popover shadow-lg max-h-80 overflow-y-auto"
    };

    view! {
        <div class=container_class>
            <button
                class=button_class
                on:click=toggle_dropdown
                data-testid=move || if inline { Some("chat-target-selector".to_string()) } else { None }
            >
                {move || {
                    let model = active_model_name.try_get().flatten();
                    let label = chat_state.get().target.display_name_with_model(model.as_deref());
                    if inline {
                        view! {
                            <>
                                <span class="text-muted-foreground text-xs">"Target:"</span>
                                <span class="font-medium truncate min-w-[140px] max-w-[220px]">{label}</span>
                            </>
                        }
                        .into_any()
                    } else {
                        view! { <span class="truncate">{label}</span> }.into_any()
                    }
                }}
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class=if inline {
                        "h-4 w-4 text-muted-foreground flex-shrink-0"
                    } else {
                        "h-4 w-4 text-muted-foreground"
                    }
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/>
                </svg>
            </button>

            {move || {
                if inline && show_dropdown.get() {
                    Some(view! {
                        <div
                            class="fixed inset-0 z-40"
                            on:click=move |_| show_dropdown.set(false)
                        />
                    })
                } else {
                    None
                }
            }}

            {move || {
                if show_dropdown.get() {
                    let select = select_target.clone();
                    let opts = options.get();

                    view! {
                        <div
                            class=dropdown_class
                            data-testid=move || if inline { Some("chat-target-dropdown".to_string()) } else { None }
                        >
                            <div class="p-1">
                                <ChatTargetOption
                                    target=ChatTarget::Default
                                    label="Auto".to_string()
                                    on_select=select.clone()
                                />

                                {opts.error.as_ref().map(|e| view! {
                                    <div class="px-2 py-2 text-xs text-destructive bg-destructive/10 rounded mx-1 my-1">
                                        {e.clone()}
                                    </div>
                                })}

                                {if opts.loading {
                                    Some(view! {
                                        <div class="px-2 py-3 text-center text-sm text-muted-foreground">
                                            <span class="animate-pulse">"Loading options..."</span>
                                        </div>
                                    })
                                } else {
                                    None
                                }}

                                {if !opts.models.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Models"</div>
                                        {opts.models.iter().map(|(id, name)| {
                                            let target = ChatTarget::Model(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <ChatTargetOption
                                                    target=target
                                                    label=label
                                                    on_select=select
                                                />
                                            }
                                        }).collect::<Vec<_>>()}
                                    })
                                } else {
                                    None
                                }}

                                {if !opts.policies.is_empty() {
                                    let select = select.clone();
                                    Some(view! {
                                        <div class="my-1 border-t"/>
                                        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">"Policy Packs"</div>
                                        {opts.policies.iter().map(|(id, name)| {
                                            let target = ChatTarget::PolicyPack(id.clone());
                                            let label = name.clone();
                                            let select = select.clone();
                                            view! {
                                                <ChatTargetOption
                                                    target=target
                                                    label=label
                                                    on_select=select
                                                />
                                            }
                                        }).collect::<Vec<_>>()}
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn ChatTargetOption<F>(target: ChatTarget, label: String, on_select: F) -> impl IntoView
where
    F: Fn(ChatTarget) + Clone + 'static,
{
    let target_clone = target.clone();
    let select = on_select.clone();

    view! {
        <button
            class="flex w-full items-center rounded-sm px-2 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground transition-colors"
            on:click=move |_| {
                select(target_clone.clone());
            }
        >
            {label}
        </button>
    }
}
