//! Reusable UI components
//!
//! Headless-style components using the Liquid Glass design system.

pub mod action_card;
pub mod actions_overflow;
pub mod adapter_bar;
pub mod adapter_detail_panel;
pub mod adapter_lifecycle_controls;
pub mod async_state;
pub mod auth;
pub mod breadcrumb_trail;
pub mod button;
pub mod card;
pub mod chat;
pub mod checkbox;
pub mod combobox;
pub mod command_palette;
pub mod confirmation_dialog;
pub mod copyable_id;
pub mod danger_zone;
pub mod data_table;
pub mod detail_grid;
pub mod diag_diff;
pub mod dialog;
pub mod document_upload_dialog;
pub mod error_boundary;
pub mod form_dialog;
pub mod form_field;
pub mod glass_toggle;
pub mod global_search;
pub mod hash_display;
pub mod icons;
pub mod inference_banner;
pub mod inference_guidance;
pub mod inline_banner;
pub mod input;
pub mod layout;
pub mod lifecycle_transition_dialog;
pub mod link;
pub mod markdown;
pub mod notification_provider;
pub mod offline_banner;
pub mod onboarding;
pub mod pagination_controls;
pub mod progress_rail;
pub mod progress_stages;
pub mod provenance_badge;
pub mod responsive;
pub mod search_results;
pub mod skeleton;
pub mod spinner;
pub mod split_panel;
pub mod split_panel_state;
pub mod status;
pub mod status_center;
pub mod table;
pub mod tabs;
pub mod toast;
pub mod toggle;
pub mod trace_viewer;
pub mod workspace;

pub use action_card::{ActionCard, ActionCardVariant};
pub use actions_overflow::{ActionsOverflow, ActionsOverflowItem};
pub use adapter_bar::{
    AdapterBar, AdapterChipState, AdapterHeat, AdapterMagnet, AdapterManageDialog,
    ChatAdaptersRegion, SuggestedAdapterView, SuggestedAdaptersBar,
};
pub use adapter_detail_panel::{AdapterDetailPanel, AdapterSuggestionContext};
pub use adapter_lifecycle_controls::AdapterLifecycleControls;
pub use async_state::{
    AsyncBoundary, AsyncBoundaryWithEmpty, AsyncBoundaryWithErrorRender, DetailGridRow, DetailRow,
    EmptyState, EmptyStateVariant, ErrorDisplay, LoadingDisplay, NotFoundSurface, PageHeader,
    RefreshButton,
};
pub use auth::{AuthProvider, ProtectedRoute};
pub use breadcrumb_trail::{humanize_segment, BreadcrumbTrail};
pub use button::{Button, ButtonLink, ButtonSize, ButtonType, ButtonVariant};
pub use card::Card;
pub use chat::{
    ChatEmptyConversationState, ChatHeaderControls, ChatQuickStartCard, ChatSessionListShell,
    ChatSessionRowShell, ChatUnavailableEntry, ChatWorkspaceLayout,
};
pub use checkbox::Checkbox;
pub use combobox::{Combobox, ComboboxOption, ModelCombobox};
pub use command_palette::CommandPalette;
pub use confirmation_dialog::{
    ConfirmationDialog, ConfirmationSeverity, ImpactItem, SimpleConfirmDialog,
};
pub use copyable_id::CopyableId;
pub use danger_zone::{AlertBanner, BannerVariant, DangerZone, DangerZoneItem};
pub use data_table::*;
pub use detail_grid::{DetailGrid, DetailItem};
pub use diag_diff::DiffResults;
pub use dialog::{Dialog, DialogSize};
pub use document_upload_dialog::DocumentUploadDialog;
pub use error_boundary::{InlineErrorBoundary, RouteErrorBoundary};
pub use form_dialog::{FormDialog, StepFormDialog};
pub use form_field::{FormField, HelpTooltip, LabelWithHelp};
pub use glass_toggle::GlassThemeToggle;
pub use global_search::{GlobalSearchBox, SearchTriggerButton};
pub use hash_display::HashDisplay;
pub use inference_banner::InferenceBanner;
pub use inline_banner::{InlineErrorBanner, InlineWarningBanner};
pub use input::{Input, Textarea};
pub use layout::{
    BreadcrumbItem, BreadcrumbItem as PageBreadcrumbItem, PageScaffold, PageScaffoldActions,
    PageScaffoldInspector, PageScaffoldPrimaryAction, PageScaffoldStatus, Shell, ShellDispatch,
    SidebarNav, SidebarState, TopBar,
};
pub use lifecycle_transition_dialog::{LifecycleTransitionDialog, LifecycleTransitionInfo};
pub use link::{Link, LinkVariant};
pub use markdown::{Markdown, MarkdownStream};
pub use offline_banner::OfflineBanner;
pub use onboarding::{
    OnboardingActionPanel, OnboardingContainer, OnboardingHeader, OnboardingProgressStep,
    OnboardingProgressStepper, OnboardingReadinessChecklist, ReadinessCheckItem,
};
pub use pagination_controls::PaginationControls;
pub use progress_rail::ProgressRail;
pub use progress_stages::{InlineProgress, ProgressController, ProgressStage, ProgressStages};
pub use provenance_badge::ProvenanceBadge;
pub use responsive::{
    use_breakpoint, use_is_desktop_or_larger, use_is_mobile, use_is_tablet_or_smaller, Breakpoint,
};
pub use search_results::{SearchEmptyState, SearchResultsList};
pub use skeleton::{
    Skeleton, SkeletonAvatar, SkeletonButton, SkeletonCard, SkeletonDetailSection,
    SkeletonPageHeader, SkeletonStatsGrid, SkeletonTable, SkeletonText, SkeletonVariant,
};
pub use spinner::Spinner;
pub use split_panel::{SplitMode, SplitPanel, SplitRatio};
pub use split_panel_state::{
    publish_route_selection, use_split_panel_selection_state, SplitPanelSelectionState,
};
pub use status::{
    BackendStatus, BackendStatusBadge, BackendStatusIndicator, Badge, BadgeVariant, StatusColor,
    StatusIconBox, StatusIndicator, StatusVariant, WorkerStatusBadge,
};
pub use table::{
    SortDirection, Table, TableBody, TableCell, TableHead, TableHeadSortable, TableHeader, TableRow,
};
pub use tabs::{TabButton, TabNav, TabPanel};
pub use toggle::{Select, Toggle};
pub use trace_viewer::{
    TokenDecisions, TokenDecisionsPaged, TraceButton, TraceDetailStandalone, TracePanel,
    TraceViewer, TraceViewerWithData,
};
pub use workspace::{
    TwoColumnRatio, Workspace, WorkspaceColumn, WorkspaceGrid, WorkspaceTwoColumn,
};

// Notification system components
pub use notification_provider::NotificationProvider;
pub use toast::{ToastContainer, ToastItem};

// Status Center components (Ctrl+Shift+S panel)
pub use status_center::{
    CombinedStatus, StatusCenterPanel, StatusCenterProvider, StatusDivider, StatusItem,
    StatusItemAvailability, StatusItemMemory, StatusItemSeverity, StatusLoadingState,
    StatusSection, StatusSectionBadgeVariant, StatusSectionLabel,
};

// Icon components (centralized SVG icons)
pub use icons::{
    IconArrowLeft, IconCheck, IconCheckCircle, IconChevronDown, IconChevronLeft, IconChevronRight,
    IconChevronUp, IconCog, IconCopy, IconDocument, IconDotsHorizontal, IconDotsVertical, IconEdit,
    IconError, IconExternalLink, IconEye, IconEyeOff, IconFolder, IconHome, IconInfo, IconLogout,
    IconMenu, IconMinus, IconPause, IconPlay, IconPlus, IconRefresh, IconSearch, IconServer,
    IconSpinner, IconStop, IconTrash, IconUser, IconWarning, IconX,
};
