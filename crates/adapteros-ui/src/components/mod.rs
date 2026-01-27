//! Reusable UI components
//!
//! Headless-style components with Tailwind CSS styling.

pub mod adapter_bar;
pub mod adapter_detail_panel;
pub mod async_state;
pub mod auth;
pub mod button;
pub mod card;
pub mod charts;
pub mod chat_dock;
pub mod command_palette;
pub mod confirmation_dialog;
pub mod danger_zone;
pub mod data_table;
pub mod diag_diff;
pub mod dialog;
pub mod form_dialog;
pub mod form_field;
pub mod glass_toggle;
pub mod global_search;
pub mod icons;
pub mod input;
pub mod layout;
pub mod notification_provider;
pub mod offline_banner;
pub mod responsive;
pub mod search_results;
pub mod skeleton;
pub mod spinner;
pub mod split_panel;
pub mod start_menu;
pub mod status;
pub mod status_center;
pub mod table;
pub mod tabs;
pub mod telemetry_overlay;
pub mod toast;
pub mod toggle;
pub mod trace_viewer;
pub mod version_skew_banner;
pub mod virtual_list;
pub mod workspace;

pub use adapter_bar::{
    AdapterBar, AdapterChipState, AdapterHeat, AdapterMagnet, SuggestedAdapterView,
    SuggestedAdaptersBar,
};
pub use adapter_detail_panel::{AdapterDetailPanel, AdapterSuggestionContext};
pub use async_state::{
    AsyncBoundary, AsyncBoundaryWithEmpty, Breadcrumb, DetailRow, EmptyState, EmptyStateVariant,
    ErrorDisplay, LoadingDisplay, PageHeader, RefreshButton,
};
pub use auth::{AuthProvider, ProtectedRoute};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::Card;
pub use chat_dock::{ChatDock, ChatDockPanel, MobileChatOverlay, NarrowChatDock};
pub use command_palette::CommandPalette;
pub use confirmation_dialog::{ConfirmationDialog, ConfirmationSeverity, SimpleConfirmDialog};
pub use danger_zone::{DangerZone, DangerZoneItem, InfoBanner, WarningBanner};
pub use data_table::{Column, DataTable};
pub use diag_diff::DiffResults;
pub use dialog::Dialog;
pub use form_dialog::{FormDialog, StepFormDialog};
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
pub use skeleton::{
    Skeleton, SkeletonAvatar, SkeletonButton, SkeletonCard, SkeletonTable, SkeletonText,
    SkeletonVariant,
};
pub use spinner::Spinner;
pub use split_panel::{SplitMode, SplitPanel, SplitRatio};
pub use start_menu::{MenuGroup, MenuItem, MenuItemState, StartButton, StartMenu};
pub use status::{
    BackendStatus, BackendStatusBadge, BackendStatusIndicator, Badge, BadgeVariant, StatusColor,
    StatusIndicator,
};
pub use table::{
    SortDirection, Table, TableBody, TableCell, TableHead, TableHeadSortable, TableHeader, TableRow,
};
pub use tabs::{TabButton, TabButtonEnum, TabNav, TabNavEnum, TabPanel, TabPanelEnum};
pub use toggle::{Select, Toggle};
pub use trace_viewer::{TokenDecisions, TraceButton, TraceDetailStandalone, TracePanel, TraceViewer, TraceViewerWithData};
pub use version_skew_banner::VersionSkewBanner;
pub use workspace::{
    TwoColumnRatio, Workspace, WorkspaceColumn, WorkspaceGrid, WorkspaceHeader, WorkspacePanel,
    WorkspaceThreeColumn, WorkspaceTwoColumn,
};

// Virtual list components for efficient large list rendering
pub use virtual_list::{CappedList, VirtualList, VirtualListConfig, VirtualTableBody};

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

// Telemetry Overlay (Ctrl+Shift+T toggle)
pub use telemetry_overlay::TelemetryOverlay;

// Icon components (centralized SVG icons)
pub use icons::{
    IconArrowLeft, IconCheck, IconCheckCircle, IconChevronDown, IconChevronLeft, IconChevronRight,
    IconChevronUp, IconCog, IconCopy, IconDocument, IconDotsHorizontal, IconDotsVertical, IconEdit,
    IconError, IconExternalLink, IconEye, IconEyeOff, IconFolder, IconHome, IconInfo, IconLogout,
    IconMenu, IconMinus, IconPause, IconPlay, IconPlus, IconRefresh, IconSearch, IconServer,
    IconSpinner, IconStop, IconTrash, IconUser, IconWarning, IconX,
};
