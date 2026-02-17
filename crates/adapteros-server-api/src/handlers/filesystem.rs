//! Filesystem browser handler
//!
//! Provides directory listing within allowed roots (var/, adapters, datasets, documents).
//! All paths are canonicalized and jailed to prevent traversal attacks.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use adapteros_api_types::filesystem::{
    EntryType, FileBrowseEntry, FileBrowseRequest, FileBrowseResponse, FileContentRequest,
    FileContentResponse, WriteFileContentRequest, WriteFileContentResponse,
};
use adapteros_core::AosError;
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use axum::extract::{Extension, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use std::path::PathBuf;

const MAX_EDITOR_FILE_BYTES: u64 = 10 * 1024 * 1024;

async fn allowed_roots(state: &AppState) -> Vec<PathBuf> {
    // Keep this intentionally tight: only browse storage roots the control plane already uses.
    let mut roots = vec![];

    let var_dir = adapteros_core::resolve_var_dir();
    roots.push(var_dir);

    if let Ok(cfg) = state.config.read() {
        let paths = &cfg.paths;
        roots.extend([
            PathBuf::from(&paths.artifacts_root),
            PathBuf::from(&paths.bundles_root),
            PathBuf::from(&paths.adapters_root),
            PathBuf::from(&paths.plan_dir),
            PathBuf::from(&paths.datasets_root),
            PathBuf::from(&paths.documents_root),
        ]);
    }

    if let Ok(repositories) = state.db.list_git_repositories().await {
        roots.extend(
            repositories
                .into_iter()
                .map(|repo| PathBuf::from(repo.path))
                .collect::<Vec<_>>(),
        );
    }

    roots
        .into_iter()
        .filter(|r| r.exists() && r.is_dir())
        .filter_map(|r| std::fs::canonicalize(&r).ok())
        .collect()
}

#[utoipa::path(
    get,
    path = "/v1/filesystem/browse",
    params(
        ("path" = String, Query, description = "Directory path to browse"),
        ("show_hidden" = Option<bool>, Query, description = "Show hidden files"),
    ),
    responses(
        (status = 200, description = "Directory listing", body = FileBrowseResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Access denied"),
    ),
    tag = "filesystem"
)]
pub async fn browse_filesystem(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<FileBrowseRequest>,
) -> Result<
    impl IntoResponse,
    (
        axum::http::StatusCode,
        Json<adapteros_api_types::ErrorResponse>,
    ),
> {
    // Directory listings expose server runtime paths; keep it operator/admin-only.
    require_permission(&claims, Permission::WorkspaceResourceManage)?;

    let roots = allowed_roots(&state).await;
    let root_strings: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();

    let requested = if params.path.is_empty() {
        roots
            .first()
            .cloned()
            .ok_or_else(|| ApiError::bad_request("No browseable directories configured"))?
    } else {
        PathBuf::from(&params.path)
    };

    let canonical =
        canonicalize_strict_in_allowed_roots(&requested, &roots).map_err(|e| match e {
            AosError::NotFound(_) => {
                ApiError::bad_request(format!("Path not found: {}", requested.display()))
            }
            AosError::Validation(_) => ApiError::forbidden("Path is outside allowed directories"),
            AosError::Config(_) => ApiError::internal("No browseable directories configured"),
            _ => ApiError::internal(format!("Failed to canonicalize path: {e}")),
        })?;
    if !canonical.is_dir() {
        return Err(ApiError::bad_request("Path is not a directory").into());
    }

    let parent_path = canonical
        .parent()
        .and_then(|p| canonicalize_strict_in_allowed_roots(p, &roots).ok())
        .map(|p| p.display().to_string());

    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(&canonical)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read directory: {e}")))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read entry: {e}")))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && !params.show_hidden {
            continue;
        }
        let entry_path = entry.path();
        // Use symlink_metadata so we don't follow links outside roots (we'll still display them).
        let metadata = match tokio::fs::symlink_metadata(&entry_path).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let entry_type = if metadata.is_dir() {
            EntryType::Directory
        } else if metadata.file_type().is_symlink() {
            EntryType::Symlink
        } else {
            EntryType::File
        };
        let size_bytes = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };
        let modified_at = metadata.modified().ok().map(|t| {
            // `SystemTime` -> chrono is fallible; keep it best-effort.
            chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
        });
        let mime_type = if metadata.is_file() {
            mime_from_extension(&name)
        } else {
            None
        };
        entries.push(FileBrowseEntry {
            name,
            path: entry_path.display().to_string(),
            entry_type,
            size_bytes,
            modified_at,
            mime_type,
        });
    }

    entries.sort_by(|a, b| {
        let a_is_dir = matches!(a.entry_type, EntryType::Directory);
        let b_is_dir = matches!(b.entry_type, EntryType::Directory);
        b_is_dir
            .cmp(&a_is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Json(FileBrowseResponse {
        schema_version: adapteros_api_types::schema_version(),
        path: canonical.display().to_string(),
        parent_path,
        entries,
        allowed_roots: root_strings,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/filesystem/content",
    params(
        ("path" = String, Query, description = "File path to read"),
    ),
    responses(
        (status = 200, description = "File content", body = FileContentResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Access denied"),
    ),
    tag = "filesystem"
)]
pub async fn read_file_content(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<FileContentRequest>,
) -> Result<
    impl IntoResponse,
    (
        axum::http::StatusCode,
        Json<adapteros_api_types::ErrorResponse>,
    ),
> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;

    let roots = allowed_roots(&state).await;
    let requested = PathBuf::from(&params.path);
    let canonical =
        canonicalize_strict_in_allowed_roots(&requested, &roots).map_err(|e| match e {
            AosError::NotFound(_) => {
                ApiError::bad_request(format!("Path not found: {}", requested.display()))
            }
            AosError::Validation(_) => ApiError::forbidden("Path is outside allowed directories"),
            AosError::Config(_) => ApiError::internal("No browseable directories configured"),
            _ => ApiError::internal(format!("Failed to canonicalize path: {e}")),
        })?;

    if !canonical.is_file() {
        return Err(ApiError::bad_request("Path is not a file").into());
    }

    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read file: {e}")))?;
    if (bytes.len() as u64) > MAX_EDITOR_FILE_BYTES {
        return Err(ApiError::bad_request("File exceeds editor size limit (10MB)").into());
    }
    let content =
        String::from_utf8(bytes).map_err(|_| ApiError::bad_request("File is not UTF-8 text"))?;

    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read file metadata: {e}")))?;
    let modified_at = metadata
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let language = language_from_extension(name);
    let line_count = content.lines().count() as u32;

    Ok(Json(FileContentResponse {
        schema_version: adapteros_api_types::schema_version(),
        path: canonical.display().to_string(),
        content,
        size_bytes: metadata.len(),
        modified_at,
        mime_type: mime_from_extension(name),
        language,
        line_count,
        readonly: false,
    }))
}

#[utoipa::path(
    put,
    path = "/v1/filesystem/content",
    request_body = WriteFileContentRequest,
    responses(
        (status = 200, description = "File written", body = WriteFileContentResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Access denied"),
    ),
    tag = "filesystem"
)]
pub async fn write_file_content(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<WriteFileContentRequest>,
) -> Result<
    impl IntoResponse,
    (
        axum::http::StatusCode,
        Json<adapteros_api_types::ErrorResponse>,
    ),
> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;

    let roots = allowed_roots(&state).await;
    let requested = PathBuf::from(&request.path);
    let canonical =
        canonicalize_strict_in_allowed_roots(&requested, &roots).map_err(|e| match e {
            AosError::NotFound(_) => {
                ApiError::bad_request(format!("Path not found: {}", requested.display()))
            }
            AosError::Validation(_) => ApiError::forbidden("Path is outside allowed directories"),
            AosError::Config(_) => ApiError::internal("No browseable directories configured"),
            _ => ApiError::internal(format!("Failed to canonicalize path: {e}")),
        })?;

    if !canonical.is_file() {
        return Err(ApiError::bad_request("Path is not a file").into());
    }

    let tmp_path =
        canonical.with_extension(format!("aos.tmp.{}", chrono::Utc::now().timestamp_millis()));
    tokio::fs::write(&tmp_path, request.content.as_bytes())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to stage file write: {e}")))?;
    tokio::fs::rename(&tmp_path, &canonical)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to finalize file write: {e}")))?;

    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read file metadata: {e}")))?;
    let modified_at = metadata
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());

    Ok(Json(WriteFileContentResponse {
        schema_version: adapteros_api_types::schema_version(),
        path: canonical.display().to_string(),
        size_bytes: metadata.len(),
        modified_at,
    }))
}

fn mime_from_extension(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    let mime = match ext.as_str() {
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "sh" => "text/x-shellscript",
        "sqlite3" | "db" => "application/x-sqlite3",
        "bin" | "gguf" | "safetensors" => "application/octet-stream",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        _ => return None,
    };
    Some(mime.to_string())
}

fn language_from_extension(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next()?.to_lowercase();
    let language = match ext.as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "css" => "css",
        "html" | "htm" => "html",
        "py" => "python",
        "go" => "go",
        "sh" => "shell",
        _ => return None,
    };
    Some(language.to_string())
}
