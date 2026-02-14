//! SplitPanel component for responsive list-detail layouts.
//!
//! Provides a consistent pattern for pages with list/detail split views:
//! - Desktop (>=1024px): Two-column layout with list left, detail right
//! - Tablet/Mobile (<1024px): Stacked layout with "Back to list" button

use crate::components::responsive::use_is_tablet_or_smaller;
use leptos::prelude::*;

/// Ratio for the two-column desktop layout.
#[derive(Clone, Copy, Default)]
pub enum SplitRatio {
    /// 50/50 split (w-1/2 each)
    #[default]
    Half,
    /// 1/3 list, 2/3 detail
    ThirdTwoThirds,
    /// 2/5 list, 3/5 detail
    TwoFifthsThreeFifths,
}

impl SplitRatio {
    fn list_class(&self) -> &'static str {
        match self {
            SplitRatio::Half => "w-1/2",
            SplitRatio::ThirdTwoThirds => "w-1/3",
            SplitRatio::TwoFifthsThreeFifths => "w-2/5",
        }
    }

    fn detail_class(&self) -> &'static str {
        match self {
            SplitRatio::Half => "w-1/2",
            SplitRatio::ThirdTwoThirds => "w-2/3",
            SplitRatio::TwoFifthsThreeFifths => "w-3/5",
        }
    }
}

/// Layout mode for the split panel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SplitMode {
    /// Two-column layout (desktop)
    Desktop,
    /// Stacked layout (tablet/mobile)
    Stacked,
}

/// Responsive split panel for list-detail layouts.
///
/// On desktop, renders as a two-column layout.
/// On tablet/mobile, renders stacked with a "Back to list" button when detail is open.
///
/// # Example
/// ```ignore
/// <SplitPanel
///     has_selection=Signal::derive(move || selected_id.get().is_some())
///     on_close=Callback::new(move |_| selected_id.set(None))
///     back_label="Back to Jobs"
///     list_panel=move || view! { <JobList/> }
///     detail_panel=move || view! { <JobDetail/> }
/// />
/// ```
#[component]
pub fn SplitPanel<LV, DV, LF, DF>(
    /// Whether a detail item is selected.
    has_selection: Signal<bool>,
    /// Callback when user closes the detail panel (via back button).
    on_close: Callback<()>,
    /// Label for the back button (e.g., "Back to Jobs"). Defaults to "Back to list".
    #[prop(optional, into)]
    back_label: String,
    /// Column ratio for desktop layout.
    #[prop(default = SplitRatio::default())]
    ratio: SplitRatio,
    /// Function that produces the list panel content.
    list_panel: LF,
    /// Function that produces the detail panel content (rendered when has_selection is true).
    detail_panel: DF,
) -> impl IntoView
where
    LV: IntoView + 'static,
    DV: IntoView + 'static,
    LF: Fn() -> LV + Clone + Send + Sync + 'static,
    DF: Fn() -> DV + Clone + Send + Sync + 'static,
{
    let is_stacked = use_is_tablet_or_smaller();

    // Determine the current mode
    let mode = Memo::new(move |_| {
        if is_stacked.get() {
            SplitMode::Stacked
        } else {
            SplitMode::Desktop
        }
    });

    // Debug logging for mode changes
    #[cfg(debug_assertions)]
    {
        Effect::new(move |prev_mode: Option<SplitMode>| {
            let Some(current) = mode.try_get() else {
                return prev_mode.unwrap_or(SplitMode::Desktop);
            };
            if prev_mode != Some(current) {
                web_sys::console::log_1(
                    &format!("[layout] split_panel mode: {:?}", current).into(),
                );
            }
            current
        });
    }

    let back_text = if back_label.is_empty() {
        "Back to list".to_string()
    } else {
        back_label
    };

    view! {
        {move || {
            let current_mode = mode.get();
            let selected = has_selection.get();
            let list_fn = list_panel.clone();
            let detail_fn = detail_panel.clone();
            let back_text = back_text.clone();
            let on_close = on_close;

            match current_mode {
                SplitMode::Desktop => {
                    // Desktop: two-column layout
                    let list_class = if selected {
                        format!("{} pr-4 min-w-0 box-border", ratio.list_class())
                    } else {
                        "flex-1 pr-4 min-w-0 box-border".to_string()
                    };

                    view! {
                        <div class="flex min-w-0">
                            // List panel
                            <div class=list_class>
                                {list_fn()}
                            </div>

                            // Detail panel (when selected)
                            {selected.then(move || {
                                let detail_class =
                                    format!("{} border-l px-4 min-w-0 box-border", ratio.detail_class());
                                view! {
                                    <div class=detail_class role="complementary" aria-label="Detail panel">
                                        {detail_fn()}
                                    </div>
                                }
                            })}
                        </div>
                    }.into_any()
                }
                SplitMode::Stacked => {
                    // Stacked: show list OR detail with back button
                    if selected {
                        view! {
                            <div class="space-y-4 min-w-0">
                                // Back button with 44px minimum touch target for mobile accessibility
                                <button
                                    class="split-panel-back-btn"
                                    on:click=move |_| on_close.run(())
                                    aria-label=back_text.clone()
                                >
                                    <BackArrowIcon/>
                                    <span>{back_text.clone()}</span>
                                </button>

                                // Detail content
                                {detail_fn()}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="min-w-0">
                                {list_fn()}
                            </div>
                        }.into_any()
                    }
                }
            }
        }}
    }
}

/// Back arrow icon for stacked mode.
#[component]
fn BackArrowIcon() -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <path d="M19 12H5"/>
            <path d="m12 19-7-7 7-7"/>
        </svg>
    }
}
