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

pub use async_state::{ErrorDisplay, EmptyState, LoadingDisplay, PageHeader, RefreshButton, DetailRow};
pub use auth::{AuthProvider, ProtectedRoute};
pub use button::{Button, ButtonVariant, ButtonSize};
pub use card::Card;
pub use chat_dock::{ChatDock, ChatDockPanel, NarrowChatDock, MobileChatOverlay};
pub use dialog::Dialog;
pub use input::{Input, Textarea};
pub use layout::{Header, Sidebar, Shell, TopBar, Taskbar};
pub use spinner::Spinner;
pub use start_menu::{StartMenu, StartButton, MenuItem, MenuItemState, MenuGroup};
pub use status::{Badge, BadgeVariant, StatusIndicator, StatusColor};
pub use table::{Table, TableHeader, TableBody, TableRow, TableHead, TableCell};
pub use toggle::{Toggle, Select};
pub use trace_viewer::{TraceViewer, TraceButton, TracePanel};
