//! Format detection for .aos files
//!
//! Detects whether a file is in ZIP format (v1) or AOS format (v2)
//! by examining magic bytes.

use adapteros_core::{AosError, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// AOS magic bytes (4-byte prefix)
const AOS_MAGIC: [u8; 4] = *b"AOS\x00";

/// Format version enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    /// ZIP-based format (v1)
    ZipV1,
    /// Memory-mappable AOS format (v2)
    AosV2,
}

/// Detect format version by examining magic bytes
pub fn detect_format<P: AsRef<Path>>(path: P) -> Result<FormatVersion> {
    let path = path.as_ref();
    let mut file = File::open(path)
        .map_err(|e| AosError::Io(format!("Failed to open file for format detection: {}", e)))?;

    // Get file size before attempting to read magic bytes
    let metadata = file
        .metadata()
        .map_err(|e| AosError::Io(format!("Failed to read file metadata: {}", e)))?;
    let file_size = metadata.len() as usize;

    // Need at least 4 bytes for ZIP format detection
    if file_size < 4 {
        return Err(AosError::Parse(format!(
            "File too short for format detection: {} bytes, need at least 4",
            file_size
        )));
    }

    // Read at least 4 bytes for format detection
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| AosError::Io(format!("Failed to read magic bytes: {}", e)))?;

    // Check for ZIP format (PK\x03\x04)
    if &magic == b"PK\x03\x04" {
        return Ok(FormatVersion::ZipV1);
    }

    // Check for AOS format if file is large enough (AOS\0\x00\x00\x00\x00)
    if file_size >= 8 {
        let mut remaining = [0u8; 4];
        file.read_exact(&mut remaining)
            .map_err(|e| AosError::Io(format!("Failed to read magic bytes: {}", e)))?;

        if magic == AOS_MAGIC && &remaining == b"\x00\x00\x00\x00" {
            return Ok(FormatVersion::AosV2);
        }
    }

    let read_bytes = if file_size >= 8 { 8 } else { 4 };
    Err(AosError::Parse(format!(
        "Unknown file format: expected ZIP (PK\\x03\\x04) or AOS (AOS\\0\\x00\\x00\\x00\\x00) magic bytes, got {:?} (file size: {} bytes, read {} bytes)",
        &magic,
        file_size,
        read_bytes
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn new_test_tempfile() -> NamedTempFile {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        NamedTempFile::new_in(&root).expect("create temp file")
    }

    #[test]
    fn test_detect_zip_format() {
        let mut file = new_test_tempfile();
        // Write only ZIP magic bytes (4 bytes minimum for ZIP detection)
        file.write_all(b"PK\x03\x04").unwrap();
        file.flush().unwrap();

        let format = detect_format(file.path()).unwrap();
        assert_eq!(format, FormatVersion::ZipV1);
    }

    #[test]
    fn test_detect_aos_format() {
        let mut file = new_test_tempfile();
        // Write AOS magic bytes
        file.write_all(b"AOS\x00\x00\x00\x00\x00").unwrap();
        file.flush().unwrap();

        let format = detect_format(file.path()).unwrap();
        assert_eq!(format, FormatVersion::AosV2);
    }

    #[test]
    fn test_detect_unknown_format() {
        let mut file = new_test_tempfile();
        file.write_all(b"UNKNOWN\x00").unwrap();
        file.flush().unwrap();

        let result = detect_format(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_empty_file() {
        let mut file = new_test_tempfile();
        // Write nothing - empty file
        file.flush().unwrap();

        let result = detect_format(file.path());
        assert!(result.is_err());
        let error = result.unwrap_err();
        match error {
            AosError::Parse(msg) => assert!(msg.contains("File too short for format detection")),
            _ => panic!("Expected Parse error, got {:?}", error),
        }
    }

    #[test]
    fn test_detect_unknown_4_byte_format() {
        let mut file = new_test_tempfile();
        // Write 4 bytes that don't match ZIP magic
        file.write_all(b"ABCD").unwrap();
        file.flush().unwrap();

        let result = detect_format(file.path());
        assert!(result.is_err());
        let error = result.unwrap_err();
        match error {
            AosError::Parse(msg) => assert!(msg.contains("Unknown file format")),
            _ => panic!("Expected Parse error, got {:?}", error),
        }
    }

    #[test]
    fn test_detect_aos_with_extra_data() {
        let mut file = new_test_tempfile();
        // Write AOS magic bytes followed by some extra data
        file.write_all(b"AOS\x00\x00\x00\x00\x00EXTRA").unwrap();
        file.flush().unwrap();

        let format = detect_format(file.path()).unwrap();
        assert_eq!(format, FormatVersion::AosV2);
    }
}
