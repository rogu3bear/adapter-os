//! Workspace layout primitives
//!
//! CSS Grid-based layout components for building responsive workspaces.
//! Designed for flexible, multi-column layouts with independent scrolling.

use leptos::prelude::*;

/// Column ratio variants for two-column layouts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TwoColumnRatio {
    /// 1:2 ratio (narrow left, wide right)
    OneToTwo,
    /// 2:1 ratio (wide left, narrow right)
    TwoToOne,
    /// 1:1 ratio (equal width)
    #[default]
    OneToOne,
}

impl TwoColumnRatio {
    /// Get the CSS grid-template-columns value
    pub fn grid_columns(&self) -> &'static str {
        match self {
            TwoColumnRatio::OneToTwo => "1fr 2fr",
            TwoColumnRatio::TwoToOne => "2fr 1fr",
            TwoColumnRatio::OneToOne => "1fr 1fr",
        }
    }
}

/// Main workspace wrapper component
///
/// Provides the outer container for workspace layouts.
/// Uses CSS Grid and fills available height with flex-1.
///
/// # Example
///
/// ```ignore
/// view! {
///     <Workspace>
///         <WorkspaceTwoColumn ratio=TwoColumnRatio::OneToTwo>
///             <WorkspaceColumn slot:left>
///                 <SidebarContent/>
///             </WorkspaceColumn>
///             <WorkspaceColumn slot:right>
///                 <MainContent/>
///             </WorkspaceColumn>
///         </WorkspaceTwoColumn>
///     </Workspace>
/// }
/// ```
#[component]
pub fn Workspace(
    /// Child content (typically layout components)
    children: Children,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let class = format!("workspace {}", class.unwrap_or_default());

    view! {
        <div class=class>
            {children()}
        </div>
    }
}

/// Single column layout component
///
/// Provides a scrollable column with optional sticky header support.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceColumn sticky_header=true>
///         <h2>"Header"</h2>
///         <div>"Scrollable content..."</div>
///     </WorkspaceColumn>
/// }
/// ```
#[component]
pub fn WorkspaceColumn(
    /// Child content
    children: Children,
    /// Enable sticky header behavior
    #[prop(optional)]
    sticky_header: bool,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let base_class = if sticky_header {
        "workspace-column workspace-column--sticky-header"
    } else {
        "workspace-column"
    };
    let class = format!("{} {}", base_class, class.unwrap_or_default());

    view! {
        <div class=class>
            {children()}
        </div>
    }
}

/// Two-column layout component
///
/// Creates a responsive two-column grid with configurable ratio.
/// Collapses to single column on mobile/tablet.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceTwoColumn ratio=TwoColumnRatio::OneToTwo>
///         <div>"Left column"</div>
///         <div>"Right column"</div>
///     </WorkspaceTwoColumn>
/// }
/// ```
#[component]
pub fn WorkspaceTwoColumn(
    /// Child content (expects two child elements)
    children: Children,
    /// Column ratio variant
    #[prop(default = TwoColumnRatio::OneToOne)]
    ratio: TwoColumnRatio,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let ratio_class = match ratio {
        TwoColumnRatio::OneToTwo => "workspace-two-col--1-2",
        TwoColumnRatio::TwoToOne => "workspace-two-col--2-1",
        TwoColumnRatio::OneToOne => "workspace-two-col--1-1",
    };
    let class = format!(
        "workspace-two-col {} {}",
        ratio_class,
        class.unwrap_or_default()
    );

    view! {
        <div class=class>
            {children()}
        </div>
    }
}

/// Three-column dashboard layout component
///
/// Creates a responsive three-column grid suitable for dashboards.
/// Collapses to two columns on tablet, single column on mobile.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceThreeColumn>
///         <div>"Navigation"</div>
///         <div>"Main content"</div>
///         <div>"Chat dock"</div>
///     </WorkspaceThreeColumn>
/// }
/// ```
#[component]
pub fn WorkspaceThreeColumn(
    /// Child content (expects three child elements)
    children: Children,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let class = format!("workspace-three-col {}", class.unwrap_or_default());

    view! {
        <div class=class>
            {children()}
        </div>
    }
}

/// Configurable grid layout component
///
/// Creates a responsive grid with configurable column count.
/// Supports different column counts at different breakpoints.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceGrid cols=4 cols_md=2 cols_sm=1 gap="1rem">
///         <Card>"Item 1"</Card>
///         <Card>"Item 2"</Card>
///         <Card>"Item 3"</Card>
///         <Card>"Item 4"</Card>
///     </WorkspaceGrid>
/// }
/// ```
#[component]
pub fn WorkspaceGrid(
    /// Child content
    children: Children,
    /// Number of columns at desktop+ (default: 3)
    #[prop(default = 3)]
    cols: u8,
    /// Number of columns at tablet (default: 2)
    #[prop(default = 2)]
    cols_md: u8,
    /// Number of columns at mobile (default: 1)
    #[prop(default = 1)]
    cols_sm: u8,
    /// Gap between items (CSS value, default: 1rem)
    #[prop(default = "1rem".to_string(), into)]
    gap: String,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let cols_class = format!("workspace-grid--cols-{}", cols);
    let cols_md_class = format!("workspace-grid--cols-md-{}", cols_md);
    let cols_sm_class = format!("workspace-grid--cols-sm-{}", cols_sm);
    let class = format!(
        "workspace-grid {} {} {} {}",
        cols_class,
        cols_md_class,
        cols_sm_class,
        class.unwrap_or_default()
    );

    view! {
        <div
            class=class
            style=format!("--workspace-grid-gap: {}", gap)
        >
            {children()}
        </div>
    }
}

/// Workspace panel component for containing content within columns
///
/// Provides consistent padding and styling for panel content.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceColumn>
///         <WorkspacePanel>
///             <h2>"Panel Title"</h2>
///             <p>"Panel content..."</p>
///         </WorkspacePanel>
///     </WorkspaceColumn>
/// }
/// ```
#[component]
pub fn WorkspacePanel(
    /// Child content
    children: Children,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let class = format!("workspace-panel {}", class.unwrap_or_default());

    view! {
        <div class=class>
            {children()}
        </div>
    }
}

/// Workspace header component for sticky column headers
///
/// Use within a WorkspaceColumn with sticky_header=true.
///
/// # Example
///
/// ```ignore
/// view! {
///     <WorkspaceColumn sticky_header=true>
///         <WorkspaceHeader>
///             <h2>"Section Title"</h2>
///         </WorkspaceHeader>
///         <div>"Scrollable content..."</div>
///     </WorkspaceColumn>
/// }
/// ```
#[component]
pub fn WorkspaceHeader(
    /// Child content
    children: Children,
    /// Optional additional CSS classes
    #[prop(optional, into)]
    class: Option<String>,
) -> impl IntoView {
    let class = format!("workspace-header {}", class.unwrap_or_default());

    view! {
        <div class=class>
            {children()}
        </div>
    }
}
