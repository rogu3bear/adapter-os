//! Workspace layout primitives.

use leptos::prelude::*;

/// Two-column layout ratios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoColumnRatio {
    OneTwo,
    TwoOne,
    OneOne,
}

impl TwoColumnRatio {
    fn class(&self) -> &'static str {
        match self {
            Self::OneTwo => "workspace-two-col--1-2",
            Self::TwoOne => "workspace-two-col--2-1",
            Self::OneOne => "workspace-two-col--1-1",
        }
    }
}

/// Generic workspace wrapper.
#[component]
pub fn Workspace(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=format!("workspace {}", class)>
            {children()}
        </div>
    }
}

/// Workspace column with optional sticky header behavior.
#[component]
pub fn WorkspaceColumn(
    #[prop(optional)] sticky_header: bool,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let sticky_class = if sticky_header {
        "workspace-column workspace-column--sticky-header"
    } else {
        "workspace-column"
    };

    view! {
        <div class=format!("{} {}", sticky_class, class)>
            {children()}
        </div>
    }
}

/// Workspace header block.
#[component]
pub fn WorkspaceHeader(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=format!("workspace-header {}", class)>
            {children()}
        </div>
    }
}

/// Workspace panel block.
#[component]
pub fn WorkspacePanel(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=format!("workspace-panel {}", class)>
            {children()}
        </div>
    }
}

/// Two-column workspace layout.
#[component]
pub fn WorkspaceTwoColumn(
    #[prop(optional)] ratio: Option<TwoColumnRatio>,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let ratio_class = ratio.unwrap_or(TwoColumnRatio::OneTwo).class();
    view! {
        <div class=format!("workspace-two-col {} {}", ratio_class, class)>
            {children()}
        </div>
    }
}

/// Three-column workspace layout.
#[component]
pub fn WorkspaceThreeColumn(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("workspace-three-col {}", class)>
            {children()}
        </div>
    }
}

/// Flexible workspace grid layout.
#[component]
pub fn WorkspaceGrid(
    #[prop(optional)] cols_sm: Option<u8>,
    #[prop(optional)] cols_md: Option<u8>,
    #[prop(optional)] cols_lg: Option<u8>,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let mut classes = vec!["workspace-grid".to_string(), class];

    if let Some(cols) = cols_sm {
        classes.push(format!("workspace-grid--cols-sm-{}", cols));
    }
    if let Some(cols) = cols_md {
        classes.push(format!("workspace-grid--cols-md-{}", cols));
    }
    if let Some(cols) = cols_lg {
        classes.push(format!("workspace-grid--cols-{}", cols));
    }

    view! {
        <div class=classes.join(" ")>
            {children()}
        </div>
    }
}
