//! Reusable UI components
//!
//! Headless-style components with Tailwind CSS styling.

pub mod async_state;
pub mod auth;
pub mod button;
pub mod card;
pub mod charts;
pub mod chat_dock;
pub mod command_palette;
pub mod confirmation_dialog;
pub mod danger_zone;
pub mod dialog;
pub mod form_field;
pub mod glass_toggle;
pub mod global_search;
pub mod input;
pub mod layout;
pub mod notification_provider;
pub mod offline_banner;
pub mod responsive;
pub mod search_results;
pub mod spinner;
pub mod start_menu;
pub mod status;
pub mod status_center;
pub mod table;
pub mod toast;
pub mod toggle;
pub mod trace_viewer;
pub mod workspace;

pub use async_state::{
    Breadcrumb, DetailRow, EmptyState, ErrorDisplay, LoadingDisplay, PageHeader, RefreshButton,
};
pub use auth::{AuthProvider, ProtectedRoute};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::Card;
pub use chat_dock::{ChatDock, ChatDockPanel, MobileChatOverlay, NarrowChatDock};
pub use command_palette::CommandPalette;
pub use confirmation_dialog::{ConfirmationDialog, ConfirmationSeverity, SimpleConfirmDialog};
pub use danger_zone::{DangerZone, DangerZoneItem, InfoBanner, WarningBanner};
pub use dialog::Dialog;
pub use form_field::{FormField, HelpTooltip, LabelWithHelp};
pub use glass_toggle::GlassThemeToggle;
pub use global_search::{GlobalSearchBox, SearchTriggerButton};
pub use input::{Input, Textarea};
pub use layout::{Header, Shell, Sidebar, Taskbar, TopBar};
pub use offline_banner::OfflineBanner;
pub use responsive::{
    use_breakpoint, use_is_desktop_or_larger, use_is_mobile, use_is_tablet_or_smaller, Breakpoint,
};
pub use search_results::{SearchEmptyState, SearchResultsList};
pub use spinner::Spinner;
pub use start_menu::{MenuGroup, MenuItem, MenuItemState, StartButton, StartMenu};
pub use status::{
    BackendStatus, BackendStatusBadge, BackendStatusIndicator, Badge, BadgeVariant, StatusColor,
    StatusIndicator,
};
pub use table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow};
pub use toggle::{Select, Toggle};
pub use trace_viewer::{TraceButton, TracePanel, TraceViewer};
pub use workspace::{
    TwoColumnRatio, Workspace, WorkspaceColumn, WorkspaceGrid, WorkspaceHeader, WorkspacePanel,
    WorkspaceThreeColumn, WorkspaceTwoColumn,
};

// Chart components (Liquid Glass Charts - PRD-UI-101)
pub use charts::{
    ChartPoint, DataSeries, HeatmapData, LineChart, MiniHeatmap, MiniLineChart, Sparkline,
    SparklineMetric, StatusHeatmap, TimeSeriesData, WorkerStatus,
};

// Notification system components
pub use notification_provider::NotificationProvider;
pub use toast::{ToastContainer, ToastItem};

// Status Center components (Ctrl+Shift+S panel)
pub use status_center::{
    CombinedStatus, StatusCenter, StatusCenterPanel, StatusDivider, StatusItem,
    StatusItemAvailability, StatusItemMemory, StatusItemSeverity, StatusLoadingState,
    StatusSection, StatusSectionBadgeVariant, StatusSectionLabel,
};
