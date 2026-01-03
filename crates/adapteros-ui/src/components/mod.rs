//! Reusable UI components
//!
//! Headless-style components with Tailwind CSS styling.

pub mod async_state;
pub mod auth;
pub mod button;
pub mod card;
pub mod chat_dock;
pub mod dialog;
pub mod input;
pub mod layout;
pub mod spinner;
pub mod start_menu;
pub mod status;
pub mod table;
pub mod toggle;
pub mod trace_viewer;

pub use async_state::{
    DetailRow, EmptyState, ErrorDisplay, LoadingDisplay, PageHeader, RefreshButton,
};
pub use auth::{AuthProvider, ProtectedRoute};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::Card;
pub use chat_dock::{ChatDock, ChatDockPanel, MobileChatOverlay, NarrowChatDock};
pub use dialog::Dialog;
pub use input::{Input, Textarea};
pub use layout::{Header, Shell, Sidebar, Taskbar, TopBar};
pub use spinner::Spinner;
pub use start_menu::{MenuGroup, MenuItem, MenuItemState, StartButton, StartMenu};
pub use status::{
    BackendStatus, BackendStatusBadge, BackendStatusIndicator, Badge, BadgeVariant, StatusColor,
    StatusIndicator,
};
pub use table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow};
pub use toggle::{Select, Toggle};
pub use trace_viewer::{TraceButton, TracePanel, TraceViewer};
