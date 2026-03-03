//! Chat page surface modules.
//!
//! Split into route entrypoints + focused submodules so the chat surface can
//! evolve without one monolithic file.

pub mod attachments;
pub mod composer;
pub mod conversation;
pub mod formatters;
pub mod session_list;
pub mod status_banners;
pub mod target_selector;
mod workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSurfaceMode {
    QuickStart,
    SessionDetail,
    History,
}

pub use workspace::ChatSession;
pub use workspace::{Chat, ChatHistory, ChatSessionEquivalent};
