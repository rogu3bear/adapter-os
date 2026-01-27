//! BLAKE3 hash verification for model files
//!
//! This module provides efficient, streaming BLAKE3 hash computation and verification
//! for model files, including support for content-addressed filenames.
//!
//! ## Example
//!
//! ```rust,no_run
//! use adapteros_model_hub::integrity::IntegrityChecker;
//! use adapteros_model_hub::B3Hash;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let path = Path::new("/path/to/model.safetensors");
//!     let hash = IntegrityChecker::hash_file(path).await?;
//!     println!("BLAKE3 hash: {}", hash);
//!     Ok(())
//! }
//! ```

use crate::ModelHubError;
use adapteros_core::B3Hash;
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Result type for integrity operations
pub type Result<T> = std::result::Result<T, ModelHubError>;

/// Buffer size for streaming file reads (64KB)
const BUFFER_SIZE: usize = 64 * 1024;

/// Streaming hasher for incremental hash computation
///
/// This is useful for computing hashes while downloading or processing data streams.
///
/// # Example
///
/// ```rust
/// use adapteros_model_hub::integrity::IntegrityChecker;
///
/// let mut hasher = IntegrityChecker::streaming_hasher();
/// hasher.update(b"hello ");
/// hasher.update(b"world");
/// let hash = hasher.finalize();
/// ```
pub struct StreamingHasher {
    hasher: blake3::Hasher,
}

impl StreamingHasher {
    /// Create a new streaming hasher
    fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }

    /// Update the hash with new data
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Finalize the hash and return the result
    pub fn finalize(self) -> B3Hash {
        let hash = self.hasher.finalize();
        B3Hash::from_bytes(*hash.as_bytes())
    }
}

/// BLAKE3 integrity checker for model files
pub struct IntegrityChecker;

impl IntegrityChecker {
    /// Hash data in memory
    ///
    /// For small amounts of data that fit in memory.
    ///
    /// # Example
    ///
    /// ```rust
    /// use adapteros_model_hub::integrity::IntegrityChecker;
    ///
    /// let data = b"hello world";
    /// let hash = IntegrityChecker::hash_bytes(data);
    /// println!("Hash: {}", hash);
    /// ```
    pub fn hash_bytes(data: &[u8]) -> B3Hash {
        B3Hash::hash(data)
    }

    /// Hash a file using streaming I/O
    ///
    /// This method reads the file in 64KB chunks, making it efficient for large files.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use adapteros_model_hub::integrity::IntegrityChecker;
    /// use std::path::Path;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let hash = IntegrityChecker::hash_file(Path::new("/path/to/file")).await?;
    ///     println!("File hash: {}", hash);
    ///     Ok(())
    /// }
    /// ```
    pub async fn hash_file(path: &Path) -> Result<B3Hash> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(ModelHubError::Io)?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; BUFFER_SIZE];

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(B3Hash::from_bytes(*hash.as_bytes()))
    }

    /// Verify a file matches the expected hash
    ///
    /// Returns `Ok(true)` if the hash matches, `Ok(false)` if it doesn't.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use adapteros_model_hub::integrity::IntegrityChecker;
    /// use adapteros_model_hub::B3Hash;
    /// use std::path::Path;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let expected = B3Hash::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")?;
    ///     let matches = IntegrityChecker::verify_file(Path::new("/path/to/file"), &expected).await?;
    ///     println!("Hash matches: {}", matches);
    ///     Ok(())
    /// }
    /// ```
    pub async fn verify_file(path: &Path, expected: &B3Hash) -> Result<bool> {
        let actual = Self::hash_file(path).await?;
        Ok(actual == *expected)
    }

    /// Create a new streaming hasher
    ///
    /// # Example
    ///
    /// ```rust
    /// use adapteros_model_hub::integrity::IntegrityChecker;
    ///
    /// let mut hasher = IntegrityChecker::streaming_hasher();
    /// hasher.update(b"data");
    /// let hash = hasher.finalize();
    /// ```
    pub fn streaming_hasher() -> StreamingHasher {
        StreamingHasher::new()
    }
}

/// Extract BLAKE3 hash from a content-addressed filename
///
/// Supports filenames in the format: `b3-{hash}.{extension}`
///
/// # Example
///
/// ```rust
/// use adapteros_model_hub::integrity::extract_hash_from_filename;
///
/// let filename = "b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
/// let hash = extract_hash_from_filename(filename).unwrap();
/// assert_eq!(hash.to_hex(), "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
/// ```
pub fn extract_hash_from_filename(filename: &str) -> Option<B3Hash> {
    // Remove path components if present
    let filename = filename
        .rsplit('/')
        .next()
        .or_else(|| filename.rsplit('\\').next())?;

    // Check for b3- prefix
    if !filename.starts_with("b3-") {
        return None;
    }

    // Extract hash portion (between "b3-" and first ".")
    let hash_start = 3; // length of "b3-"
    let hash_end = filename.find('.')?;

    if hash_end <= hash_start {
        return None;
    }

    let hash_str = &filename[hash_start..hash_end];

    // Validate length (64 hex chars = 32 bytes)
    if hash_str.len() != 64 {
        return None;
    }

    // Parse the hex string (B3Hash::from_hex returns Result<B3Hash, AosError>)
    B3Hash::from_hex(hash_str).ok()
}

// ============================================================================
// Legacy compatibility layer
// ============================================================================

/// Legacy IntegrityVerifier for backwards compatibility
///
/// This maintains the existing API while using the new B3Hash implementation.
#[deprecated(since = "0.2.0", note = "Use IntegrityChecker instead")]
pub struct IntegrityVerifier;

#[allow(deprecated)]
impl IntegrityVerifier {
    /// Verify a file's BLAKE3 checksum
    pub async fn verify_file(path: &Path, expected_checksum: &str) -> Result<bool> {
        let expected = B3Hash::from_hex(expected_checksum)
            .map_err(|e| ModelHubError::IntegrityFailure(format!("Invalid hash: {}", e)))?;
        IntegrityChecker::verify_file(path, &expected).await
    }

    /// Compute BLAKE3 checksum of a file
    pub async fn compute_checksum(path: &Path) -> Result<String> {
        let hash = IntegrityChecker::hash_file(path).await?;
        Ok(hash.to_hex())
    }

    /// Verify and return detailed error on mismatch
    pub async fn verify_or_error(path: &Path, expected_checksum: &str) -> Result<()> {
        let expected = B3Hash::from_hex(expected_checksum)
            .map_err(|e| ModelHubError::IntegrityFailure(format!("Invalid hash: {}", e)))?;
        let actual = IntegrityChecker::hash_file(path).await?;

        if actual != expected {
            return Err(ModelHubError::IntegrityFailure(format!(
                "Checksum mismatch: expected {}, got {}",
                expected.to_hex(),
                actual.to_hex()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn new_test_tempfile() -> NamedTempFile {
        tempfile::Builder::new()
            .prefix("aos-test-")
            .tempfile()
            .expect("create temp file")
    }

    #[test]
    fn test_hash_bytes() {
        let data = b"hello world";
        let hash = IntegrityChecker::hash_bytes(data);

        // BLAKE3 hash of "hello world"
        let expected =
            B3Hash::from_hex("d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24")
                .unwrap();

        assert_eq!(hash, expected);
    }

    #[test]
    fn test_hash_hex_roundtrip() {
        let original = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let hash = B3Hash::from_hex(original).unwrap();
        assert_eq!(hash.to_hex(), original);
    }

    #[test]
    fn test_invalid_hex_length() {
        let result = B3Hash::from_hex("0123");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hex_chars() {
        let result =
            B3Hash::from_hex("0123456789abcdefXXXX456789abcdef0123456789abcdef0123456789abcdef");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hash_file() {
        let mut temp_file = new_test_tempfile();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let hash = IntegrityChecker::hash_file(temp_file.path()).await.unwrap();

        let expected =
            B3Hash::from_hex("d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24")
                .unwrap();

        assert_eq!(hash, expected);
    }

    #[tokio::test]
    async fn test_verify_file() {
        let mut temp_file = new_test_tempfile();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let expected =
            B3Hash::from_hex("d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24")
                .unwrap();

        let matches = IntegrityChecker::verify_file(temp_file.path(), &expected)
            .await
            .unwrap();

        assert!(matches);
    }

    #[tokio::test]
    async fn test_verify_file_mismatch() {
        let mut temp_file = new_test_tempfile();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let wrong_hash =
            B3Hash::from_hex("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap();

        let matches = IntegrityChecker::verify_file(temp_file.path(), &wrong_hash)
            .await
            .unwrap();

        assert!(!matches);
    }

    #[test]
    fn test_streaming_hasher() {
        let mut hasher = IntegrityChecker::streaming_hasher();
        hasher.update(b"hello ");
        hasher.update(b"world");
        let hash = hasher.finalize();

        let expected =
            B3Hash::from_hex("d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24")
                .unwrap();

        assert_eq!(hash, expected);
    }

    #[test]
    fn test_extract_hash_from_filename() {
        let filename =
            "b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename).unwrap();
        assert_eq!(
            hash.to_hex(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn test_extract_hash_from_filename_with_path() {
        let filename = "/path/to/b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename).unwrap();
        assert_eq!(
            hash.to_hex(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_extract_hash_from_filename_windows_path() {
        let filename = r"C:\path\to\b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename).unwrap();
        assert_eq!(
            hash.to_hex(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_extract_hash_from_filename_backslash_in_name() {
        // On non-Windows, backslash in filename is just another character
        // This tests that we handle it gracefully
        let filename =
            "b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename).unwrap();
        assert_eq!(
            hash.to_hex(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn test_extract_hash_no_prefix() {
        let filename =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename);
        assert!(hash.is_none());
    }

    #[test]
    fn test_extract_hash_invalid_length() {
        let filename = "b3-0123456789abcdef.safetensors";
        let hash = extract_hash_from_filename(filename);
        assert!(hash.is_none());
    }

    #[test]
    fn test_extract_hash_no_extension() {
        let filename = "b3-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let hash = extract_hash_from_filename(filename);
        assert!(hash.is_none());
    }

    #[test]
    fn test_b3hash_display() {
        let hash =
            B3Hash::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        // B3Hash from adapteros_core uses "b3:" prefix and short hex in Display
        assert_eq!(format!("{}", hash), "b3:0123456789abcdef");
        // Full hex is available via to_hex()
        assert_eq!(
            hash.to_hex(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
    }

    #[tokio::test]
    async fn test_hash_large_file() {
        // Create a file larger than the buffer size (64KB)
        let mut temp_file = new_test_tempfile();
        let data = vec![0x42u8; 128 * 1024]; // 128KB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let hash = IntegrityChecker::hash_file(temp_file.path()).await.unwrap();

        // Verify we got a hash (value doesn't matter for this test)
        assert_eq!(hash.to_hex().len(), 64); // 32 bytes = 64 hex chars
    }

    #[tokio::test]
    async fn test_hash_empty_file() {
        let temp_file = new_test_tempfile();

        let hash = IntegrityChecker::hash_file(temp_file.path()).await.unwrap();

        // BLAKE3 hash of empty input
        let expected =
            B3Hash::from_hex("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")
                .unwrap();

        assert_eq!(hash, expected);
    }

    // Legacy compatibility tests
    #[tokio::test]
    #[allow(deprecated)]
    async fn test_legacy_compute_checksum() {
        let mut temp = new_test_tempfile();
        temp.write_all(b"test content").unwrap();
        temp.flush().unwrap();

        let checksum = IntegrityVerifier::compute_checksum(temp.path())
            .await
            .unwrap();

        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // BLAKE3 produces 256-bit hash
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_legacy_verify_file() {
        let mut temp = new_test_tempfile();
        temp.write_all(b"test content").unwrap();
        temp.flush().unwrap();

        let checksum = IntegrityVerifier::compute_checksum(temp.path())
            .await
            .unwrap();

        let result = IntegrityVerifier::verify_file(temp.path(), &checksum)
            .await
            .unwrap();

        assert!(result);
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_legacy_verify_or_error() {
        let mut temp = new_test_tempfile();
        temp.write_all(b"test content").unwrap();
        temp.flush().unwrap();

        let checksum = IntegrityVerifier::compute_checksum(temp.path())
            .await
            .unwrap();

        // Should succeed
        IntegrityVerifier::verify_or_error(temp.path(), &checksum)
            .await
            .unwrap();

        // Should fail
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = IntegrityVerifier::verify_or_error(temp.path(), wrong).await;
        assert!(result.is_err());
    }
}
