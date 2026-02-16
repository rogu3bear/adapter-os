//! Button component

use super::spinner::{Spinner, SpinnerSize};
use leptos::prelude::*;
use web_sys::MouseEvent;

/// Button variants (Glass-Integrated Flat design)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ButtonVariant {
    /// Main action - semi-transparent primary with blur
    #[default]
    Primary,
    /// Supporting action - Tier 1 glass, subtle presence
    Secondary,
    /// Transparent with glass border, fills on hover
    Outline,
    /// Invisible until hover, then glass fades in
    Ghost,
    /// Semi-transparent red with glow on hover
    Destructive,
    /// Text-only, no glass effects
    Link,
}

impl ButtonVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Primary => "btn-primary",
            Self::Secondary => "btn-secondary",
            Self::Outline => "btn-outline",
            Self::Ghost => "btn-ghost",
            Self::Destructive => "btn-destructive",
            Self::Link => "btn-link",
        }
    }
}

/// Button sizes
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ButtonSize {
    /// Extra compact size for very dense controls.
    Xs,
    /// Compact size for dense UIs
    Sm,
    /// Default size
    #[default]
    Md,
    /// Prominent actions
    Lg,
    /// Square icon button
    Icon,
    /// Smaller square icon button
    IconSm,
}

impl ButtonSize {
    fn class(&self) -> &'static str {
        match self {
            Self::Xs => "btn-xs",
            Self::Sm => "btn-sm",
            Self::Md => "btn-md",
            Self::Lg => "btn-lg",
            Self::Icon => "btn-icon",
            Self::IconSm => "btn-icon-sm",
        }
    }
}

/// Native button `type` attribute semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ButtonType {
    /// Default safe action inside or outside forms.
    #[default]
    Button,
    /// Triggers parent form submission.
    Submit,
    /// Triggers parent form reset.
    Reset,
}

fn compose_button_class(variant: ButtonVariant, size: ButtonSize, class: &str) -> String {
    format!("btn {} {} {}", variant.class(), size.class(), class)
}

impl ButtonType {
    fn attr(&self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Submit => "submit",
            Self::Reset => "reset",
        }
    }
}

/// Button component
///
/// Supports both static and reactive `disabled` and `loading` props via `Signal<bool>`.
/// When `loading` is true, a spinner is shown and the button is disabled.
#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] loading: Signal<bool>,
    #[prop(optional, into)] class: String,
    #[prop(optional)] button_type: ButtonType,
    #[prop(optional, into)] aria_label: String,
    #[prop(optional, into)] data_testid: Option<String>,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let full_class = compose_button_class(variant, size, &class);

    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <button
            class=full_class
            type=button_type.attr()
            disabled=move || disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false)
            aria-label=move || (!aria_label.is_empty()).then(|| aria_label.clone())
            data-testid=move || data_testid.clone()
            on:click=move |_| {
                if let Some(ref cb) = on_click {
                    cb.run(());
                }
            }
        >
            {move || {
                if loading.try_get().unwrap_or(false) {
                    view! {
                        <Spinner size=SpinnerSize::Sm class="mr-2".to_string()/>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
            {children()}
        </button>
    }
}

/// Link styled as a button for navigation actions.
#[component]
pub fn ButtonLink(
    #[prop(into)] href: String,
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] target: Option<String>,
    #[prop(optional, into)] rel: Option<String>,
    #[prop(optional, into)] aria_label: Option<String>,
    #[prop(optional, into)] data_testid: Option<String>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let full_class = compose_button_class(variant, size, &class);
    let final_rel = if target.as_deref() == Some("_blank") {
        Some(rel.unwrap_or_else(|| "noopener noreferrer".to_string()))
    } else {
        rel
    };
    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <a
            href=href
            class=full_class
            target=target
            rel=final_rel
            aria-label=aria_label
            data-testid=move || data_testid.clone()
            on:click=move |ev| {
                if let Some(ref cb) = on_click {
                    cb.run(ev);
                }
            }
        >
            {children()}
        </a>
    }
}
