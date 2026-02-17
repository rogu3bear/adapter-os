//! Filesystem browsing types shared between server and UI.

use serde::{Deserialize, Serialize};

/// Request to browse a filesystem directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileBrowseRequest {
    /// Absolute path to browse
    pub path: String,
    /// Include hidden files (dotfiles)
    #[serde(default)]
    pub show_hidden: bool,
}

/// Request to read file content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileContentRequest {
    /// Absolute file path to read
    pub path: String,
}

/// Response from browsing a filesystem directory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileBrowseResponse {
    /// Schema version
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Canonical path that was browsed
    pub path: String,
    /// Parent directory path (None if at an allowed root)
    pub parent_path: Option<String>,
    /// Directory entries
    pub entries: Vec<FileBrowseEntry>,
    /// Allowed root directories the user can browse
    pub allowed_roots: Vec<String>,
}

/// Response with file content payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileContentResponse {
    /// Schema version
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Canonical path that was read
    pub path: String,
    /// UTF-8 file content
    pub content: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Last modified time (ISO 8601)
    pub modified_at: Option<String>,
    /// MIME type guess
    pub mime_type: Option<String>,
    /// Language hint for editors
    pub language: Option<String>,
    /// Total line count
    pub line_count: u32,
    /// Whether file is readonly in editor
    #[serde(default)]
    pub readonly: bool,
}

/// Request to write file content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WriteFileContentRequest {
    /// Absolute file path to write
    pub path: String,
    /// UTF-8 content to write
    pub content: String,
}

/// Response after writing file content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WriteFileContentResponse {
    /// Schema version
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Canonical path that was written
    pub path: String,
    /// Written byte size
    pub size_bytes: u64,
    /// Last modified time (ISO 8601)
    pub modified_at: Option<String>,
}

/// A single filesystem entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileBrowseEntry {
    /// File/directory name
    pub name: String,
    /// Full absolute path
    pub path: String,
    /// Entry type
    pub entry_type: EntryType,
    /// File size in bytes (None for directories)
    pub size_bytes: Option<u64>,
    /// Last modified time (ISO 8601)
    pub modified_at: Option<String>,
    /// MIME type guess (None for directories)
    pub mime_type: Option<String>,
}

/// Type of filesystem entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}
