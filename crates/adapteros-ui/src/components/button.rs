//! Button component

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
            Self::Primary => "bg-primary text-primary-foreground hover:bg-primary/90",
            Self::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80",
            Self::Outline => {
                "border border-input bg-background hover:bg-accent hover:text-accent-foreground"
            }
            Self::Ghost => "hover:bg-accent hover:text-accent-foreground",
            Self::Destructive => {
                "bg-destructive text-destructive-foreground hover:bg-destructive/90"
            }
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
            Self::Sm => "h-8 px-3 text-xs",
            Self::Md => "h-10 px-4 py-2",
            Self::Lg => "h-12 px-8",
            Self::Icon => "h-10 w-10",
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
    #[prop(optional)] on_click: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let base_class = "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50";

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
            on:click=move |_| {
                if let Some(ref cb) = on_click {
                    cb.run(());
                }
            }
        >
            {move || {
                if loading {
                    view! {
                        <svg
                            class="animate-spin h-4 w-4"
                            xmlns="http://www.w3.org/2000/svg"
                            fill="none"
                            viewBox="0 0 24 24"
                        >
                            <circle
                                class="opacity-25"
                                cx="12"
                                cy="12"
                                r="10"
                                stroke="currentColor"
                                stroke-width="4"
                            ></circle>
                            <path
                                class="opacity-75"
                                fill="currentColor"
                                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                            ></path>
                        </svg>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
            {children()}
        </button>
    }
}
