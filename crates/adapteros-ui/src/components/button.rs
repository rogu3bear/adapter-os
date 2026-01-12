//! Button component

use super::spinner::{Spinner, SpinnerSize};
use leptos::prelude::*;

/// Button variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
}

impl ButtonVariant {
    fn class(&self) -> &'static str {
        match self {
            Self::Primary => "btn-primary",
            Self::Secondary => "btn-secondary",
            Self::Outline => "btn-outline",
            Self::Ghost => "btn-ghost",
            Self::Destructive => "btn-destructive",
        }
    }
}

/// Button sizes
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
    Icon,
}

impl ButtonSize {
    fn class(&self) -> &'static str {
        match self {
            Self::Sm => "btn-sm",
            Self::Md => "btn-md",
            Self::Lg => "btn-lg",
            Self::Icon => "btn-icon",
        }
    }
}

/// Button component
#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] loading: bool,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: Option<String>,
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let base_class = "btn";

    let full_class = format!(
        "{} {} {} {}",
        base_class,
        variant.class(),
        size.class(),
        class
    );

    view! {
        <button
            class=full_class
            disabled=disabled || loading
            aria-label=aria_label
            on:click=move |_| {
                if let Some(ref cb) = on_click {
                    cb.run(());
                }
            }
        >
            {move || {
                if loading {
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
