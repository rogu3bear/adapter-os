//! Chunked upload support for large dataset files
//!
//! Provides infrastructure for:
//! - Resumable uploads with chunk tracking
//! - Parallel chunk processing
//! - Compression support (gzip/zip)
//! - Memory-efficient streaming
//! - Resume token generation and validation

use anyhow::{anyhow, Context, Result};
use adapteros_secure_fs::path_policy::{canonicalize_strict, canonicalize_strict_in_allowed_roots};
use adapteros_secure_fs::traversal::check_path_traversal;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self as std_fs, File as StdFile, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use zip::ZipArchive;

/// Minimum chunk size for segmented uploads (1MB)
pub const MIN_CHUNK_SIZE: usize = 1024 * 1024;

/// Default chunk size for segmented uploads (10MB)
pub const DEFAULT_CHUNK_SIZE: usize = 10 * 1024 * 1024;

/// Maximum chunk size (100MB)
pub const MAX_CHUNK_SIZE: usize = 100 * 1024 * 1024;

/// Timeout for incomplete uploads (1 hour)
/// Reduced from 24 hours to limit memory and disk usage from stale sessions.
pub const UPLOAD_TIMEOUT_SECS: u64 = 3600;

/// Interval for background cleanup task (5 minutes)
/// Reduced from 1 hour for more aggressive cleanup of expired sessions.
const CLEANUP_INTERVAL_SECS: u64 = 300;

/// Metadata for a chunked upload session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    /// Unique session identifier
    pub session_id: String,
    /// File being uploaded
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size for this upload
    pub chunk_size: usize,
    /// Content type of the file
    pub content_type: String,
    /// Chunks already received (chunk_index -> hash)
    pub received_chunks: HashMap<usize, String>,
    /// Temporary directory for chunks
    pub temp_dir: PathBuf,
    /// When the session was created
    pub created_at: std::time::SystemTime,
    /// Compression format if applicable (gzip, zip, none)
    pub compression: CompressionFormat,
    /// Is this upload resumed from a previous session?
    pub is_resumed: bool,
    /// Optional workspace ID for tenant isolation
    pub workspace_id: Option<String>,
}

/// Compression format for uploads
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CompressionFormat {
    None,
    Gzip,
    Zip,
}

impl CompressionFormat {
    pub fn from_content_type(ct: &str) -> Self {
        match ct {
            t if t.contains("gzip") => CompressionFormat::Gzip,
            t if t.contains("zip") => CompressionFormat::Zip,
            _ => CompressionFormat::None,
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            CompressionFormat::None => "",
            CompressionFormat::Gzip => ".gz",
            CompressionFormat::Zip => ".zip",
        }
    }
}

/// Resume token for resuming interrupted uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeToken {
    /// Session ID to resume
    pub session_id: String,
    /// Next expected chunk index
    pub next_chunk: usize,
    /// Current hash state (hex string)
    pub hash_state: String,
}

/// Result of chunk upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkUploadResult {
    /// Session ID
    pub session_id: String,
    /// Chunk index that was uploaded
    pub chunk_index: usize,
    /// Hash of this chunk
    pub chunk_hash: String,
    /// Total chunks received
    pub chunks_received: usize,
    /// Is upload complete?
    pub is_complete: bool,
    /// Resume token for resuming from next chunk (if not complete)
    pub resume_token: Option<ResumeToken>,
}

/// In-memory upload session manager
pub struct UploadSessionManager {
    sessions: Arc<RwLock<HashMap<String, UploadSession>>>,
    max_sessions: usize,
}

impl UploadSessionManager {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_sessions,
        }
    }

    /// Create a new upload session
    pub async fn create_session(
        &self,
        file_name: String,
        total_size: u64,
        content_type: String,
        chunk_size: usize,
        temp_base_dir: &Path,
    ) -> Result<UploadSession> {
        self.create_session_with_workspace(
            file_name,
            total_size,
            content_type,
            chunk_size,
            temp_base_dir,
            None,
        )
        .await
    }

    /// Create a new upload session with optional workspace ID for tenant isolation
    pub async fn create_session_with_workspace(
        &self,
        file_name: String,
        total_size: u64,
        content_type: String,
        chunk_size: usize,
        temp_base_dir: &Path,
        workspace_id: Option<String>,
    ) -> Result<UploadSession> {
        let session_id = Uuid::now_v7().to_string();
        let temp_dir = temp_base_dir.join(&session_id);

        fs::create_dir_all(&temp_dir)
            .await
            .context("Failed to create chunk temporary directory")?;

        let compression = CompressionFormat::from_content_type(&content_type);

        let session = UploadSession {
            session_id: session_id.clone(),
            file_name,
            total_size,
            chunk_size,
            content_type,
            received_chunks: HashMap::new(),
            temp_dir,
            created_at: std::time::SystemTime::now(),
            compression,
            is_resumed: false,
            workspace_id,
        };

        let mut sessions = self.sessions.write().await;
        if sessions.len() >= self.max_sessions {
            return Err(anyhow!(
                "Maximum concurrent upload sessions ({}) reached",
                self.max_sessions
            ));
        }

        sessions.insert(session_id.clone(), session.clone());
        Ok(session)
    }

    /// Get an existing upload session
    pub async fn get_session(&self, session_id: &str) -> Result<UploadSession> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("Upload session {} not found or expired", session_id))
    }

    /// Update session with received chunk (with lock to prevent race during cleanup)
    pub async fn add_chunk(
        &self,
        session_id: &str,
        chunk_index: usize,
        chunk_hash: String,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Upload session {} not found", session_id))?;

        session.received_chunks.insert(chunk_index, chunk_hash);
        Ok(())
    }

    /// Start background cleanup task that runs every 5 minutes to remove expired sessions
    /// Returns a JoinHandle that can be used to cancel the task
    pub fn start_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                match self.cleanup_expired().await {
                    Ok(count) if count > 0 => {
                        info!("Cleanup task removed {} expired upload sessions", count);
                    }
                    Err(e) => {
                        warn!("Cleanup task failed: {}", e);
                    }
                    _ => {}
                }
            }
        })
    }

    /// Check if upload is complete
    pub async fn is_upload_complete(&self, session_id: &str) -> Result<bool> {
        let session = self.get_session(session_id).await?;
        let expected_chunks = session.total_size.div_ceil(session.chunk_size as u64);
        Ok(session.received_chunks.len() == expected_chunks as usize)
    }

    /// Remove completed session and clean up its temporary directory
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(session_id) {
            // Explicitly clean up temporary directory on session removal
            if session.temp_dir.exists() {
                if let Err(e) = fs::remove_dir_all(&session.temp_dir).await {
                    warn!(
                        session_id = %session_id,
                        temp_dir = ?session.temp_dir,
                        error = %e,
                        "Failed to clean up session temp directory"
                    );
                } else {
                    debug!(
                        session_id = %session_id,
                        temp_dir = ?session.temp_dir,
                        "Cleaned up session temp directory"
                    );
                }
            }
        }
        Ok(())
    }

    /// Clean up expired sessions and their temporary directories
    pub async fn cleanup_expired(&self) -> Result<usize> {
        // Collect expired sessions under lock, then release lock before I/O
        let expired_sessions: Vec<(String, UploadSession)> = {
            let mut sessions = self.sessions.write().await;
            let now = std::time::SystemTime::now();

            let expired_ids: Vec<String> = sessions
                .iter()
                .filter_map(
                    |(id, session)| match now.duration_since(session.created_at) {
                        Ok(duration) if duration.as_secs() > UPLOAD_TIMEOUT_SECS => {
                            Some(id.clone())
                        }
                        _ => None,
                    },
                )
                .collect();

            expired_ids
                .into_iter()
                .filter_map(|id| sessions.remove(&id).map(|s| (id, s)))
                .collect()
        };

        let count = expired_sessions.len();

        // Clean up temp directories outside the lock to avoid blocking
        for (session_id, session) in expired_sessions {
            if session.temp_dir.exists() {
                match fs::remove_dir_all(&session.temp_dir).await {
                    Ok(()) => {
                        info!(
                            session_id = %session_id,
                            temp_dir = ?session.temp_dir,
                            age_secs = Self::get_session_age(&session),
                            "Cleaned up expired upload session and temp directory"
                        );
                    }
                    Err(e) => {
                        warn!(
                            session_id = %session_id,
                            temp_dir = ?session.temp_dir,
                            error = %e,
                            "Failed to clean up expired session temp directory"
                        );
                    }
                }
            } else {
                info!(
                    session_id = %session_id,
                    age_secs = Self::get_session_age(&session),
                    "Cleaned up expired upload session (no temp directory)"
                );
            }
        }

        Ok(count)
    }

    /// List all active sessions (for monitoring)
    pub async fn list_sessions(&self) -> Vec<UploadSession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Get the maximum number of allowed concurrent sessions
    pub fn max_sessions(&self) -> usize {
        self.max_sessions
    }

    /// Get the age of a session in seconds
    pub fn get_session_age(session: &UploadSession) -> u64 {
        std::time::SystemTime::now()
            .duration_since(session.created_at)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Check if a session has expired
    pub fn is_session_expired(session: &UploadSession) -> bool {
        Self::get_session_age(session) > UPLOAD_TIMEOUT_SECS
    }

    /// Retry a chunk by replacing its hash (for failed/corrupted chunks)
    pub async fn retry_chunk(
        &self,
        session_id: &str,
        chunk_index: usize,
        chunk_hash: String,
    ) -> Result<Option<String>> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Upload session {} not found", session_id))?;

        let previous_hash = session.received_chunks.insert(chunk_index, chunk_hash);
        Ok(previous_hash)
    }
}

/// Handles chunk writing with streaming and hashing
pub struct ChunkWriter {
    file: File,
    hasher: Hasher,
    bytes_written: u64,
}

impl ChunkWriter {
    pub async fn new(path: &Path) -> Result<Self> {
        let file = File::create(path)
            .await
            .context("Failed to create chunk file")?;

        Ok(Self {
            file,
            hasher: Hasher::new(),
            bytes_written: 0,
        })
    }

    /// Write chunk data with simultaneous hashing
    pub async fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        self.file
            .write_all(data)
            .await
            .context("Failed to write chunk data")?;
        self.hasher.update(data);
        self.bytes_written += data.len() as u64;
        Ok(())
    }

    /// Flush and return hash
    pub async fn finalize(mut self) -> Result<String> {
        self.file
            .flush()
            .await
            .context("Failed to flush chunk file")?;
        self.file
            .sync_all()
            .await
            .context("Failed to sync chunk file")?;
        Ok(self.hasher.finalize().to_hex().to_string())
    }
}

/// Assembles chunks into final file
pub struct ChunkAssembler {
    output_path: PathBuf,
    chunk_dir: PathBuf,
    /// Chunk size used for validation (reserved for future integrity checks)
    #[allow(dead_code)]
    chunk_size: usize,
    expected_chunks: usize,
    /// Compression format for decompression after assembly (reserved for future feature)
    #[allow(dead_code)]
    compression: CompressionFormat,
}

impl ChunkAssembler {
    pub fn new(
        output_path: PathBuf,
        chunk_dir: PathBuf,
        chunk_size: usize,
        total_size: u64,
        compression: CompressionFormat,
    ) -> Self {
        let expected_chunks = total_size.div_ceil(chunk_size as u64) as usize;
        Self {
            output_path,
            chunk_dir,
            chunk_size,
            expected_chunks,
            compression,
        }
    }

    /// Assemble all chunks into final file using streaming reads to avoid OOM
    pub async fn assemble(&self) -> Result<(String, u64)> {
        debug!(
            "Assembling {} chunks into {}",
            self.expected_chunks,
            self.output_path.display()
        );

        let mut output_file = File::create(&self.output_path)
            .await
            .context("Failed to create output file")?;

        let mut final_hasher = Hasher::new();
        let mut total_bytes = 0u64;

        // Bounded buffer for streaming reads (10MB to prevent OOM)
        const STREAM_BUFFER_SIZE: usize = 10 * 1024 * 1024;
        let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];

        // Read chunks in order using streaming to avoid loading entire chunk into memory
        for i in 0..self.expected_chunks {
            let chunk_path = self.chunk_dir.join(format!("chunk_{:08}", i));

            // Open chunk file for streaming read
            let mut chunk_file = match File::open(&chunk_path).await {
                Ok(file) => file,
                Err(e) => {
                    error!("Failed to open chunk {}: {}", i, e);
                    return Err(anyhow!("Missing chunk {} during assembly", i));
                }
            };

            // Stream chunk to output file using bounded buffer
            loop {
                let n = chunk_file
                    .read(&mut buffer)
                    .await
                    .context(format!("Failed to read chunk {}", i))?;

                if n == 0 {
                    break;
                }

                // Write to output and update hash
                output_file
                    .write_all(&buffer[..n])
                    .await
                    .context(format!("Failed to write chunk {} to output", i))?;
                final_hasher.update(&buffer[..n]);
                total_bytes += n as u64;
            }

            // Clean up chunk file
            let _ = fs::remove_file(&chunk_path).await;
        }

        output_file
            .flush()
            .await
            .context("Failed to flush output file")?;
        output_file
            .sync_all()
            .await
            .context("Failed to sync output file")?;

        let final_hash = final_hasher.finalize().to_hex().to_string();
        info!(
            "Assembled file {} ({} bytes, hash: {})",
            self.output_path.display(),
            total_bytes,
            final_hash
        );

        Ok((final_hash, total_bytes))
    }
}

/// Decompresses uploaded archives
pub struct CompressionHandler;

impl CompressionHandler {
    /// Decompress gzip file to directory
    pub async fn decompress_gzip(input_path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>> {
        let input_file = StdFile::open(input_path).context("Failed to open gzip file")?;
        let decoder = flate2::read::GzDecoder::new(input_file);
        let mut archive = tar::Archive::new(decoder);

        extract_tar_entries(&mut archive, output_dir).context("Failed to decompress gzip archive")
    }

    /// Decompress zip file to directory
    pub async fn decompress_zip(input_path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>> {
        std_fs::create_dir_all(output_dir).context("Failed to create output directory")?;
        let canonical_output_dir =
            canonicalize_strict(output_dir).context("Failed to canonicalize output directory")?;
        let allowed_roots = [canonical_output_dir.clone()];

        let file = StdFile::open(input_path).context("Failed to open zip file")?;
        let mut archive = ZipArchive::new(file).context("Failed to parse zip archive")?;

        let mut files = Vec::new();
        let mut total_bytes: u64 = 0;
        let mut file_count: usize = 0;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .context(format!("Failed to read zip entry {}", i))?;

            if is_zip_symlink(&entry) {
                return Err(anyhow!(
                    "Zip entry is a symlink and was rejected: {}",
                    entry.name()
                ));
            }

            let entry_path = entry.enclosed_name().map(|p| p.to_path_buf()).ok_or_else(|| {
                let name = entry.name().to_string();
                error!(
                    original = %name,
                    canonical = "<unavailable>",
                    "Zip entry path rejected"
                );
                anyhow!("Zip entry contains invalid path: {}", name)
            })?;
            validate_archive_entry_path(&entry_path, entry.name())?;

            let output_path = canonical_output_dir.join(&entry_path);
            if entry.is_dir() {
                std_fs::create_dir_all(&output_path)
                    .context("Failed to create output directory")?;
                canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                    .context("Zip entry path rejected")?;
                continue;
            }

            file_count += 1;
            if file_count > crate::handlers::datasets::MAX_FILE_COUNT {
                return Err(anyhow!(
                    "Zip archive exceeds maximum file count of {}",
                    crate::handlers::datasets::MAX_FILE_COUNT
                ));
            }
            total_bytes = total_bytes.saturating_add(entry.size());
            if total_bytes > crate::handlers::datasets::MAX_TOTAL_SIZE as u64 {
                return Err(anyhow!(
                    "Zip archive exceeds maximum size of {} bytes",
                    crate::handlers::datasets::MAX_TOTAL_SIZE
                ));
            }

            if let Some(parent) = output_path.parent() {
                std_fs::create_dir_all(parent).context("Failed to create output directory")?;
                canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
                    .context("Zip entry path rejected")?;
            }

            if output_path.exists() {
                let metadata = std_fs::symlink_metadata(&output_path).context(format!(
                    "Failed to get metadata for: {}",
                    output_path.display()
                ))?;
                if metadata.file_type().is_symlink() {
                    return Err(anyhow!(
                        "Zip entry path is a symlink, rejecting: {}",
                        output_path.display()
                    ));
                }
            }

            let mut output_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)
                .context("Failed to create output file")?;

            std::io::copy(&mut entry, &mut output_file)
                .context("Failed to write decompressed data")?;

            canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                .context("Zip entry path rejected")?;
            files.push(output_path);
        }

        Ok(files)
    }

    /// Auto-detect and decompress based on content type
    pub async fn decompress(
        input_path: &Path,
        output_dir: &Path,
        compression: &CompressionFormat,
    ) -> Result<Vec<PathBuf>> {
        match compression {
            CompressionFormat::Gzip => Self::decompress_gzip(input_path, output_dir).await,
            CompressionFormat::Zip => Self::decompress_zip(input_path, output_dir).await,
            CompressionFormat::None => {
                // Not compressed, return as single file
                let file_name = input_path
                    .file_name()
                    .ok_or_else(|| anyhow!("Invalid file path"))?;
                let output_path = output_dir.join(file_name);
                fs::copy(input_path, &output_path)
                    .await
                    .context("Failed to copy file")?;
                Ok(vec![output_path])
            }
        }
    }
}

fn extract_tar_entries<R: Read>(
    archive: &mut tar::Archive<R>,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    std_fs::create_dir_all(output_dir).context("Failed to create output directory")?;
    let canonical_output_dir =
        canonicalize_strict(output_dir).context("Failed to canonicalize output directory")?;
    let allowed_roots = [canonical_output_dir.clone()];

    let mut files = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    for entry in archive.entries().context("Failed to read tar entries")? {
        let mut entry = entry.context("Failed to read tar entry")?;
        let entry_path = entry.path().context("Failed to read tar entry path")?;
        validate_archive_entry_path(&entry_path, &entry_path.to_string_lossy())?;

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(anyhow!(
                "Tar entry is a link and was rejected: {}",
                entry_path.display()
            ));
        }

        let output_path = canonical_output_dir.join(&entry_path);
        if entry_type.is_dir() {
            std_fs::create_dir_all(&output_path).context("Failed to create output directory")?;
            canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                .context("Tar entry path rejected")?;
            continue;
        }

        if !entry_type.is_file() {
            return Err(anyhow!(
                "Unsupported tar entry type for {}",
                entry_path.display()
            ));
        }

        file_count += 1;
        if file_count > crate::handlers::datasets::MAX_FILE_COUNT {
            return Err(anyhow!(
                "Tar archive exceeds maximum file count of {}",
                crate::handlers::datasets::MAX_FILE_COUNT
            ));
        }
        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > crate::handlers::datasets::MAX_TOTAL_SIZE as u64 {
            return Err(anyhow!(
                "Tar archive exceeds maximum size of {} bytes",
                crate::handlers::datasets::MAX_TOTAL_SIZE
            ));
        }

        if let Some(parent) = output_path.parent() {
            std_fs::create_dir_all(parent).context("Failed to create output directory")?;
            canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
                .context("Tar entry path rejected")?;
        }

        if output_path.exists() {
            let metadata = std_fs::symlink_metadata(&output_path).context(format!(
                "Failed to get metadata for: {}",
                output_path.display()
            ))?;
            if metadata.file_type().is_symlink() {
                return Err(anyhow!(
                    "Tar entry path is a symlink, rejecting: {}",
                    output_path.display()
                ));
            }
        }

        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .context("Failed to create output file")?;

        std::io::copy(&mut entry, &mut output_file).context("Failed to extract tar entry")?;
        canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
            .context("Tar entry path rejected")?;
        files.push(output_path);
    }

    Ok(files)
}

fn validate_archive_entry_path(entry_path: &Path, entry_name: &str) -> Result<()> {
    if entry_path.is_absolute() {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            "Archive entry path rejected (absolute)"
        );
        return Err(anyhow!("Archive entry path is absolute: {}", entry_name));
    }

    check_path_traversal(entry_path).map_err(|e| {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            error = %e,
            "Archive entry path rejected (traversal)"
        );
        anyhow!("Archive entry path rejected: {}", entry_name)
    })?;

    Ok(())
}

fn is_zip_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .map(|mode| (mode & 0o170000) == 0o120000)
        .unwrap_or(false)
}

/// Validates files before processing
pub struct FileValidator;

impl FileValidator {
    /// Validate file format based on content and extension
    pub fn validate_format(file_path: &Path, format: &str) -> Result<()> {
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        match format {
            "jsonl" => {
                if !matches!(extension, "jsonl" | "ndjson" | "txt") {
                    return Err(anyhow!("JSONL file must have .jsonl or .ndjson extension"));
                }
            }
            "json" => {
                if !matches!(extension, "json" | "jsonl" | "ndjson") {
                    return Err(anyhow!("JSON file must have .json extension"));
                }
            }
            "csv" => {
                if !matches!(extension, "csv") {
                    return Err(anyhow!("CSV file must have .csv extension"));
                }
            }
            _ => {
                debug!("Skipping extension validation for format: {}", format);
            }
        }

        Ok(())
    }

    /// Quick content validation without full parse
    /// Returns detailed error messages with file name, line numbers, and encoding info
    pub async fn quick_validate(file_path: &Path, format: &str, max_bytes: usize) -> Result<()> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let metadata = fs::metadata(file_path)
            .await
            .context(format!("Failed to read file metadata for {}", file_name))?;

        if metadata.len() == 0 {
            return Err(anyhow!("File {} is empty (size: 0 bytes)", file_name));
        }

        // Read first chunk for validation
        let mut file = File::open(file_path)
            .await
            .context(format!("Failed to open file {}", file_name))?;

        let read_size = (metadata.len() as usize).min(max_bytes);
        let mut buffer = vec![0u8; read_size];
        let n = file
            .read(&mut buffer)
            .await
            .context(format!("Failed to read file {}", file_name))?;

        buffer.truncate(n);

        // Check encoding - use lossy conversion to detect invalid UTF-8
        let content = String::from_utf8_lossy(&buffer);
        if content.chars().any(|c| c == '\u{FFFD}') {
            // Found replacement character, indicates invalid UTF-8
            return Err(anyhow!(
                "File {} has invalid UTF-8 encoding (contains non-UTF-8 bytes)",
                file_name
            ));
        }

        match format {
            "jsonl" => {
                for (line_num, line) in content.lines().enumerate() {
                    if !line.trim().is_empty() {
                        match serde_json::from_str::<serde_json::Value>(line) {
                            Ok(_) => {}
                            Err(e) => {
                                // Try to extract column number from error (serde_json reports 0 when unavailable)
                                let column = e.column();
                                let col_info = if column > 0 {
                                    format!("line {}, column {}", line_num + 1, column)
                                } else {
                                    format!("line {}", line_num + 1)
                                };
                                return Err(anyhow!(
                                    "File {}: Invalid JSON at {}: {}",
                                    file_name,
                                    col_info,
                                    e
                                ));
                            }
                        }
                    }
                }
            }
            "json" => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(_) => {}
                Err(e) => {
                    let line = e.line();
                    let line_info = if line > 0 {
                        format!("line {}, column {}", line, e.column())
                    } else {
                        "unknown position".to_string()
                    };
                    return Err(anyhow!(
                        "File {}: Invalid JSON at {}: {}",
                        file_name,
                        line_info,
                        e
                    ));
                }
            },
            _ => {
                // No validation for other formats
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_format_detection() {
        assert_eq!(
            CompressionFormat::from_content_type("application/gzip"),
            CompressionFormat::Gzip
        );
        assert_eq!(
            CompressionFormat::from_content_type("application/zip"),
            CompressionFormat::Zip
        );
        assert_eq!(
            CompressionFormat::from_content_type("application/octet-stream"),
            CompressionFormat::None
        );
    }

    #[test]
    fn test_upload_session_creation() {
        let temp_root = std::path::PathBuf::from("var/tmp");
        std::fs::create_dir_all(&temp_root).unwrap();
        let temp_dir = tempfile::TempDir::new_in(&temp_root).unwrap();
        let session = UploadSession {
            session_id: "test-123".to_string(),
            file_name: "data.jsonl".to_string(),
            total_size: 100_000_000,
            chunk_size: 10_000_000,
            content_type: "application/jsonl".to_string(),
            received_chunks: HashMap::new(),
            temp_dir: temp_dir.path().to_path_buf(),
            created_at: std::time::SystemTime::now(),
            compression: CompressionFormat::None,
            is_resumed: false,
            workspace_id: Some("test-workspace".to_string()),
        };

        assert_eq!(session.session_id, "test-123");
        assert_eq!(session.file_name, "data.jsonl");
        assert_eq!(session.total_size, 100_000_000);
        assert_eq!(session.workspace_id, Some("test-workspace".to_string()));
    }
}
