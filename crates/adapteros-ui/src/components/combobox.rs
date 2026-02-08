//! Combobox component
//!
//! A text input with dropdown suggestions for auto-complete functionality.
//! Supports keyboard navigation, filtering, and selection.

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;

/// A single option in the combobox dropdown
#[derive(Clone, Debug, PartialEq)]
pub struct ComboboxOption {
    /// The value to use when selected
    pub value: String,
    /// The display label
    pub label: String,
    /// Optional secondary text (e.g., format, description)
    pub description: Option<String>,
}

impl ComboboxOption {
    /// Create a new option with just value and label
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    /// Create a new option with description
    pub fn with_description(
        value: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: Some(description.into()),
        }
    }
}

/// Combobox component with auto-suggest dropdown
#[component]
pub fn Combobox(
    /// The current value (two-way bound)
    #[prop(into)]
    value: RwSignal<String>,
    /// Available options to filter and select from
    #[prop(into)]
    options: Signal<Vec<ComboboxOption>>,
    /// Placeholder text
    #[prop(optional, into)]
    placeholder: String,
    /// Label for the input
    #[prop(optional, into)]
    label: Option<String>,
    /// Optional ID for the input element
    #[prop(optional, into)]
    id: Option<String>,
    /// Whether the input is disabled
    #[prop(optional, into)]
    disabled: Signal<bool>,
    /// Additional CSS classes
    #[prop(optional, into)]
    class: String,
    /// Callback when an option is selected from the dropdown
    #[prop(optional)]
    on_select: Option<Callback<ComboboxOption>>,
    /// Error message to display
    #[prop(optional, into)]
    error: Option<String>,
    /// Whether to allow free text (not just from options)
    #[prop(optional)]
    allow_free_text: bool,
) -> impl IntoView {
    let input_ref = NodeRef::<leptos::html::Input>::new();
    let dropdown_open = RwSignal::new(false);
    let selected_index = RwSignal::new(0i32);

    // Use StoredValue for IDs to avoid move issues
    let input_id = id.unwrap_or_else(|| format!("combobox-{}", uuid::Uuid::new_v4()));
    let input_id_store = StoredValue::new(input_id);
    let listbox_id_store = StoredValue::new(format!(
        "{}-listbox",
        input_id_store.with_value(|id| id.clone())
    ));

    // Filter options based on current input value
    let filtered_options = Memo::new(move |_| {
        let query = value.get().to_lowercase();
        let all_options = options.get();

        if query.is_empty() {
            all_options
        } else {
            all_options
                .into_iter()
                .filter(|opt| {
                    opt.label.to_lowercase().contains(&query)
                        || opt.value.to_lowercase().contains(&query)
                        || opt
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&query))
                            .unwrap_or(false)
                })
                .collect()
        }
    });

    // Reset selected index when filtered options change
    Effect::new(move || {
        let _ = filtered_options.get();
        selected_index.set(0);
    });

    // Handle keyboard navigation
    let on_keydown = move |ev: KeyboardEvent| {
        let opts = filtered_options.get();
        let len = opts.len() as i32;

        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                if !dropdown_open.get() {
                    dropdown_open.set(true);
                } else if len > 0 {
                    selected_index.update(|idx| {
                        *idx = (*idx + 1).min(len - 1);
                    });
                    let listbox_id = listbox_id_store.with_value(|id| id.clone());
                    scroll_option_into_view(&listbox_id, selected_index.get_untracked() as usize);
                }
            }
            "ArrowUp" => {
                ev.prevent_default();
                if dropdown_open.get() && len > 0 {
                    selected_index.update(|idx| {
                        *idx = (*idx - 1).max(0);
                    });
                    let listbox_id = listbox_id_store.with_value(|id| id.clone());
                    scroll_option_into_view(&listbox_id, selected_index.get_untracked() as usize);
                }
            }
            "Enter" => {
                if dropdown_open.get() {
                    ev.prevent_default();
                    let idx = selected_index.get() as usize;
                    if let Some(opt) = opts.get(idx) {
                        value.set(opt.value.clone());
                        dropdown_open.set(false);
                        if let Some(ref cb) = on_select {
                            cb.run(opt.clone());
                        }
                    }
                }
            }
            "Escape" => {
                if dropdown_open.get() {
                    ev.prevent_default();
                    dropdown_open.set(false);
                }
            }
            "Tab" => {
                // Close dropdown on tab, allow default behavior
                dropdown_open.set(false);
            }
            _ => {}
        }
    };

    // Handle input changes
    let on_input = move |ev: web_sys::Event| {
        let input_value = event_target_value(&ev);
        value.set(input_value);
        dropdown_open.set(true);
    };

    // Handle focus
    let on_focus = move |_| {
        dropdown_open.set(true);
    };

    // Handle blur - delay to allow click on dropdown
    let on_blur = move |_| {
        // Use a timeout to allow click events on dropdown items to fire first
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let closure = Closure::once_into_js(move || {
                dropdown_open.set(false);
            });
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    closure.unchecked_ref(),
                    150,
                );
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            dropdown_open.set(false);
        }
    };

    // Handle option click
    let on_select_clone = on_select;
    let select_option = move |opt: ComboboxOption| {
        value.set(opt.value.clone());
        dropdown_open.set(false);
        if let Some(ref input) = input_ref.get() {
            let _ = input.focus();
        }
        if let Some(ref cb) = on_select_clone {
            cb.run(opt);
        }
    };

    // Build CSS classes
    let base_class = "input";
    let state_class = if error.is_some() { "input-error" } else { "" };
    let full_class = format!("{} {} {}", base_class, state_class, class);

    // Check if current value matches any option (for validation indicator)
    let is_valid_selection = Memo::new(move |_| {
        let current = value.get();
        if current.is_empty() {
            return allow_free_text;
        }
        let opts = options.get();
        opts.iter().any(|opt| opt.value == current) || allow_free_text
    });

    view! {
        <div class="relative w-full">
            {label.map(|l| {
                let for_id = input_id_store.with_value(|id| id.clone());
                view! {
                    <label class="label mb-1.5 block" for=for_id>
                        {l}
                    </label>
                }
            })}

            <div class="relative">
                <input
                    node_ref=input_ref
                    id=input_id_store.with_value(|id| id.clone())
                    type="text"
                    class=full_class
                    placeholder=placeholder
                    disabled=move || disabled.get()
                    autocomplete="off"
                    role="combobox"
                    aria-expanded=move || dropdown_open.get().to_string()
                    aria-controls=listbox_id_store.with_value(|id| id.clone())
                    aria-autocomplete="list"
                    aria-activedescendant=move || {
                        if dropdown_open.get() {
                            let input_id = input_id_store.with_value(|id| id.clone());
                            Some(format!("{}-option-{}", input_id, selected_index.get()))
                        } else {
                            None
                        }
                    }
                    prop:value=move || value.get()
                    on:input=on_input
                    on:focus=on_focus
                    on:blur=on_blur
                    on:keydown=on_keydown
                />

                // Dropdown chevron
                <button
                    type="button"
                    tabindex="-1"
                    class="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground"
                    aria-label="Toggle dropdown"
                    on:mousedown=move |ev| {
                        ev.prevent_default(); // Prevent blur
                        dropdown_open.update(|open| *open = !*open);
                        if let Some(input) = input_ref.get() {
                            let _ = input.focus();
                        }
                    }
                >
                    <svg
                        class=move || format!(
                            "h-4 w-4 transition-transform {}",
                            if dropdown_open.get() { "rotate-180" } else { "" }
                        )
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                    </svg>
                </button>

                // Valid selection checkmark
                {move || {
                    if is_valid_selection.get() && !value.get().is_empty() {
                        Some(view! {
                            <span class="absolute right-8 top-1/2 -translate-y-1/2 text-success">
                                <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                                </svg>
                            </span>
                        })
                    } else {
                        None
                    }
                }}
            </div>

            // Dropdown list
            <Show when=move || dropdown_open.get() && !filtered_options.get().is_empty()>
                {
                    let listbox_id_inner = listbox_id_store.with_value(|id| id.clone());
                    view! {
                        <ul
                            id=listbox_id_inner
                            role="listbox"
                            class="combobox-dropdown absolute z-50 mt-1 w-full max-h-60 overflow-auto rounded-md border border-border bg-popover shadow-lg"
                        >
                            <For
                                each=move || filtered_options.get().into_iter().enumerate()
                                key=|(idx, opt)| format!("{}-{}", idx, opt.value.clone())
                                children={
                                    let select_option = select_option;
                                    move |(idx, opt)| {
                                        let opt_for_click = opt.clone();
                                        let opt_for_view = opt.clone();
                                        let is_selected = move || selected_index.get() == idx as i32;
                                        let option_id = input_id_store.with_value(|id| format!("{}-option-{}", id, idx));

                                        view! {
                                            <li
                                                id=option_id
                                                role="option"
                                                aria-selected=move || is_selected().to_string()
                                                class=move || format!(
                                                    "combobox-option cursor-pointer px-3 py-2 text-sm {}",
                                                    if is_selected() { "bg-accent text-accent-foreground" } else { "hover:bg-muted" }
                                                )
                                                on:mousedown={
                                                    let opt = opt_for_click.clone();
                                                    move |ev: web_sys::MouseEvent| {
                                                        ev.prevent_default(); // Prevent blur
                                                        select_option(opt.clone());
                                                    }
                                                }
                                                on:mouseenter=move |_| {
                                                    selected_index.set(idx as i32);
                                                }
                                            >
                                                <div class="font-medium">{opt_for_view.label.clone()}</div>
                                                {opt_for_view.description.clone().map(|desc| view! {
                                                    <div class="text-xs text-muted-foreground">{desc}</div>
                                                })}
                                            </li>
                                        }
                                    }
                                }
                            />
                        </ul>
                    }
                }
            </Show>

            // Empty state when dropdown is open but no matches
            <Show when=move || dropdown_open.get() && filtered_options.get().is_empty() && !value.get().is_empty()>
                <div class="absolute z-50 mt-1 w-full rounded-md border border-border bg-popover p-3 text-sm text-muted-foreground shadow-lg">
                    "No matching options"
                </div>
            </Show>

            // Error message
            {error.map(|e| view! {
                <p class="form-field-error mt-1.5" role="alert">{e}</p>
            })}
        </div>
    }
}

/// Scroll an option into view within the listbox
#[cfg(target_arch = "wasm32")]
fn scroll_option_into_view(listbox_id: &str, index: usize) {
    use wasm_bindgen::JsCast;

    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        let option_id = format!("{}-option-{}", listbox_id.replace("-listbox", ""), index);
        if let Some(element) = document.get_element_by_id(&option_id) {
            if let Ok(html_element) = element.dyn_into::<web_sys::HtmlElement>() {
                html_element.scroll_into_view_with_bool(false);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scroll_option_into_view(_listbox_id: &str, _index: usize) {}

/// Model combobox specialized for model selection
///
/// A convenience wrapper around Combobox that fetches and displays models.
#[component]
pub fn ModelCombobox(
    /// The selected model ID (two-way bound)
    #[prop(into)]
    value: RwSignal<String>,
    /// The model options (from API)
    #[prop(into)]
    models: Signal<Vec<crate::api::ModelWithStatsResponse>>,
    /// Placeholder text
    #[prop(optional, into)]
    placeholder: String,
    /// Label for the input (optional)
    #[prop(optional, into)]
    label: Option<String>,
    /// Whether the input is disabled
    #[prop(optional, into)]
    disabled: Signal<bool>,
    /// Callback when a model is selected
    #[prop(optional)]
    on_select: Option<Callback<crate::api::ModelWithStatsResponse>>,
) -> impl IntoView {
    // Convert models to combobox options
    let options = Memo::new(move |_| {
        models
            .get()
            .into_iter()
            .map(|m| {
                let desc = match (&m.format, &m.backend) {
                    (Some(f), Some(b)) => format!("{} / {}", f, b),
                    (Some(f), None) => f.clone(),
                    (None, Some(b)) => b.clone(),
                    (None, None) => String::new(),
                };
                ComboboxOption {
                    value: m.id.clone(),
                    label: m.name.clone(),
                    description: if desc.is_empty() { None } else { Some(desc) },
                }
            })
            .collect::<Vec<_>>()
    });

    // Find the full model when selected
    let on_option_select = move |opt: ComboboxOption| {
        if let Some(ref cb) = on_select {
            let all_models = models.get();
            if let Some(model) = all_models.iter().find(|m| m.id == opt.value) {
                cb.run(model.clone());
            }
        }
    };

    // Render with or without label based on whether it's provided
    match label {
        Some(l) => view! {
            <Combobox
                value=value
                options=Signal::derive(move || options.get())
                placeholder=placeholder
                label=l
                disabled=disabled
                on_select=Callback::new(on_option_select)
            />
        }
        .into_any(),
        None => view! {
            <Combobox
                value=value
                options=Signal::derive(move || options.get())
                placeholder=placeholder
                disabled=disabled
                on_select=Callback::new(on_option_select)
            />
        }
        .into_any(),
    }
}
