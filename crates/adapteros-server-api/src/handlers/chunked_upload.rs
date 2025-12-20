//! Chunked upload support for large dataset files
//!
//! Provides infrastructure for:
//! - Resumable uploads with chunk tracking
//! - Parallel chunk processing
//! - Compression support (gzip/zip)
//! - Memory-efficient streaming
//! - Resume token generation and validation

use anyhow::{anyhow, Context, Result};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Timeout for incomplete uploads (24 hours)
pub const UPLOAD_TIMEOUT_SECS: u64 = 86400;

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

    /// Start background cleanup task that runs every hour to remove expired sessions
    /// Returns a JoinHandle that can be used to cancel the task
    pub fn start_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // 1 hour
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
        let expected_chunks =
            (session.total_size + (session.chunk_size as u64 - 1)) / (session.chunk_size as u64);
        Ok(session.received_chunks.len() == expected_chunks as usize)
    }

    /// Remove completed session
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let mut sessions = self.sessions.write().await;
        let now = std::time::SystemTime::now();

        let expired: Vec<String> = sessions
            .iter()
            .filter_map(
                |(id, session)| match now.duration_since(session.created_at) {
                    Ok(duration) if duration.as_secs() > UPLOAD_TIMEOUT_SECS => Some(id.clone()),
                    _ => None,
                },
            )
            .collect();

        let count = expired.len();
        for session_id in expired {
            if let Some(session) = sessions.remove(&session_id) {
                // Clean up temporary files
                let _ = std::fs::remove_dir_all(&session.temp_dir);
                warn!("Cleaned up expired upload session {}", session_id);
            }
        }

        Ok(count)
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
        let expected_chunks =
            ((total_size + (chunk_size as u64 - 1)) / (chunk_size as u64)) as usize;
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
        let input_file = std::fs::File::open(input_path).context("Failed to open gzip file")?;
        let decoder = flate2::read::GzDecoder::new(input_file);
        let mut archive = tar::Archive::new(decoder);

        archive
            .unpack(output_dir)
            .context("Failed to decompress gzip archive")?;

        // List extracted files
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(output_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }

        Ok(files)
    }

    /// Decompress zip file to directory
    pub async fn decompress_zip(input_path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>> {
        let file = std::fs::File::open(input_path).context("Failed to open zip file")?;
        let mut archive = ZipArchive::new(file).context("Failed to parse zip archive")?;

        let mut files = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .context(format!("Failed to read zip entry {}", i))?;

            if !file.is_file() {
                continue;
            }

            // Security: Validate entry name to prevent path traversal attacks
            let entry_name = file.name();
            if entry_name.contains("..") || Path::new(entry_name).is_absolute() {
                return Err(anyhow!(
                    "Zip entry contains invalid path (path traversal attempt): {}",
                    entry_name
                ));
            }

            let output_path = output_dir.join(entry_name);

            // Security: Verify resolved path stays within output directory
            // Use lexical comparison since output_path may not exist yet
            let canonical_output_dir = output_dir
                .canonicalize()
                .context("Failed to canonicalize output directory")?;
            if let Ok(canonical_output) = output_path.canonicalize() {
                if !canonical_output.starts_with(&canonical_output_dir) {
                    return Err(anyhow!(
                        "Zip entry would extract outside target directory: {}",
                        entry_name
                    ));
                }
            } else {
                // File doesn't exist yet, check the parent directory
                if let Some(parent) = output_path.parent() {
                    if parent.exists() {
                        let canonical_parent = parent
                            .canonicalize()
                            .context("Failed to canonicalize parent directory")?;
                        if !canonical_parent.starts_with(&canonical_output_dir) {
                            return Err(anyhow!(
                                "Zip entry would extract outside target directory: {}",
                                entry_name
                            ));
                        }
                    }
                }
            }

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .context("Failed to create output directory")?;
            }

            let mut output_file = File::create(&output_path)
                .await
                .context("Failed to create output file")?;

            let mut buffer = vec![0; 8192];
            loop {
                let n = file.read(&mut buffer).context("Failed to read zip entry")?;
                if n == 0 {
                    break;
                }
                output_file
                    .write_all(&buffer[..n])
                    .await
                    .context("Failed to write decompressed data")?;
            }

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
        };

        assert_eq!(session.session_id, "test-123");
        assert_eq!(session.file_name, "data.jsonl");
        assert_eq!(session.total_size, 100_000_000);
    }
}
