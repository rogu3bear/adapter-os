//! AOS Archive Writer
//!
//! Creates single-file `.aos` archives with a manifest and one or more
//! backend-specific segments described by an index in the header.
//!
//! See docs/AOS_FORMAT.md for full specification.

use adapteros_core::{AosError, B3Hash, Result};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::info;

/// Magic bytes identifying an AOS archive (4 bytes)
pub const AOS_MAGIC: [u8; 4] = *b"AOS2";

/// Current header size in bytes (64-byte aligned for cache efficiency)
pub const HEADER_SIZE: usize = 64;

/// Fixed size for each segment index entry
pub const INDEX_ENTRY_SIZE: usize = 80;

/// Bit 0: segment index present (required for AOS2)
pub const HAS_INDEX_FLAG: u32 = 0x1;

/// Compute the truncated scope hash used in the segment index.
pub fn compute_scope_hash(scope_path: &str) -> [u8; 16] {
    let full = B3Hash::hash(scope_path.as_bytes());
    let mut truncated = [0u8; 16];
    truncated.copy_from_slice(&full.as_bytes()[..16]);
    truncated
}

/// Backend tags for multi-backend UMA artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendTag {
    Canonical,
    Mlx,
    Metal,
    Coreml,
}

impl BackendTag {
    pub fn as_u16(self) -> u16 {
        match self {
            BackendTag::Canonical => 0,
            BackendTag::Mlx => 1,
            BackendTag::Metal => 2,
            BackendTag::Coreml => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BackendTag::Canonical => "canonical",
            BackendTag::Mlx => "mlx",
            BackendTag::Metal => "metal",
            BackendTag::Coreml => "coreml",
        }
    }
}

impl TryFrom<u16> for BackendTag {
    type Error = AosError;

    fn try_from(value: u16) -> Result<Self> {
        match value {
            0 => Ok(BackendTag::Canonical),
            1 => Ok(BackendTag::Mlx),
            2 => Ok(BackendTag::Metal),
            3 => Ok(BackendTag::Coreml),
            other => Err(AosError::Validation(format!(
                "Unknown backend tag: {}",
                other
            ))),
        }
    }
}

/// AOS Writer Options
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// Include signature (default: true)
    pub include_signature: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            include_signature: true,
        }
    }
}

#[derive(Debug, Clone)]
struct PendingSegment {
    segment_id: u32,
    backend_tag: BackendTag,
    scope_hash: [u8; 16],
    weights_hash: B3Hash,
    payload: Vec<u8>,
}

/// AOS Archive Writer
///
/// Creates .aos archive files with a 64-byte fixed header + segment index.
pub struct AosWriter {
    #[allow(dead_code)]
    options: WriteOptions,
    segments: Vec<PendingSegment>,
}

/// Parsed AOS archive header
///
/// ## Binary Layout (64 bytes)
/// ```text
/// | Offset | Size | Field                              |
/// |--------|------|------------------------------------|
/// | 0      | 4    | Magic: "AOS2"                      |
/// | 4      | 4    | Flags (u32 LE)                     |
/// | 8      | 8    | Index offset (u64 LE)              |
/// | 16     | 8    | Index size (u64 LE)                |
/// | 24     | 8    | Manifest offset (u64 LE)           |
/// | 32     | 8    | Manifest size (u64 LE)             |
/// | 40     | 24   | Reserved (padding)                 |
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AosHeader {
    /// Flags (bit 0 must be set to indicate presence of an index)
    pub flags: u32,
    /// Offset to segment index
    pub index_offset: u64,
    /// Size of segment index in bytes
    pub index_size: u64,
    /// Offset to manifest JSON
    pub manifest_offset: u64,
    /// Size of manifest JSON in bytes
    pub manifest_size: u64,
}

/// Parsed descriptor for a segment stored in the archive.
#[derive(Debug, Clone)]
pub struct SegmentDescriptor {
    pub segment_id: u32,
    pub backend_tag: BackendTag,
    pub scope_hash: [u8; 16],
    pub weights_hash: B3Hash,
    pub offset: usize,
    pub len: usize,
}

/// Borrowed view of a segment payload.
#[derive(Debug, Clone)]
pub struct SegmentView<'a> {
    pub segment_id: u32,
    pub backend_tag: BackendTag,
    pub scope_hash: [u8; 16],
    pub payload: &'a [u8],
}

/// Borrowed view of an .aos file.
#[derive(Debug, Clone)]
pub struct AosFileView<'a> {
    pub manifest_bytes: &'a [u8],
    pub segments: Vec<SegmentView<'a>>,
}

impl AosWriter {
    /// Create a new AOS writer
    pub fn new() -> Self {
        Self {
            options: WriteOptions::default(),
            segments: Vec::new(),
        }
    }

    /// Create with custom options
    pub fn with_options(options: WriteOptions) -> Self {
        Self {
            options,
            segments: Vec::new(),
        }
    }

    /// Add a segment to the archive. Returns the assigned segment_id.
    pub fn add_segment(
        &mut self,
        backend_tag: BackendTag,
        scope_path: Option<String>,
        bytes: &[u8],
    ) -> Result<u32> {
        let segment_id = self.segments.len() as u32;
        let weights_hash = B3Hash::hash(bytes);
        let scope_hash = scope_path
            .as_deref()
            .map(compute_scope_hash)
            .unwrap_or([0u8; 16]);

        self.segments.push(PendingSegment {
            segment_id,
            backend_tag,
            scope_hash,
            weights_hash,
            payload: bytes.to_vec(),
        });

        Ok(segment_id)
    }

    /// Clear all pending segments (builder reset)
    pub fn clear_segments(&mut self) {
        self.segments.clear();
    }

    /// Write adapter archive to file using the queued segments
    pub fn write_archive<P, M>(&self, output_path: P, manifest: &M) -> Result<u64>
    where
        P: AsRef<Path>,
        M: Serialize,
    {
        let output_path = output_path.as_ref();
        if self.segments.is_empty() {
            return Err(AosError::Validation(
                "Cannot write .aos archive without segments".to_string(),
            ));
        }

        if !self
            .segments
            .iter()
            .any(|seg| seg.backend_tag == BackendTag::Canonical)
        {
            return Err(AosError::Validation(
                "Cannot write .aos archive without canonical segment".to_string(),
            ));
        }

        let manifest_value: Value = serde_json::to_value(manifest)?;
        let scope_path = manifest_value
            .get("metadata")
            .and_then(|v| v.get("scope_path"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AosError::Validation(
                    "Corrupted / needs retrain: missing scope_path in manifest metadata"
                        .to_string(),
                )
            })?;
        let manifest_scope_hash = compute_scope_hash(scope_path);

        // Serialize manifest to JSON
        let manifest_json = serde_json::to_vec_pretty(&manifest_value)?;

        // Layout
        let index_offset = HEADER_SIZE as u64;
        let index_size = (self.segments.len() as u64) * INDEX_ENTRY_SIZE as u64;
        let mut current_offset = index_offset + index_size;

        // Build index bytes and capture payload offsets
        let mut index_bytes = Vec::with_capacity(index_size as usize);
        for segment in &self.segments {
            let offset = current_offset;
            let len = segment.payload.len() as u64;
            current_offset = current_offset
                .checked_add(len)
                .ok_or_else(|| AosError::Validation("Segment offsets overflow".to_string()))?;

            let actual_hash = B3Hash::hash(&segment.payload);
            if actual_hash != segment.weights_hash {
                return Err(AosError::Validation(format!(
                    "Corrupted / needs retrain: segment {} hash mismatch before write",
                    segment.segment_id
                )));
            }

            if segment.scope_hash != [0u8; 16] && segment.scope_hash != manifest_scope_hash {
                return Err(AosError::Validation(
                    "Corrupted / needs retrain: segment scope hash does not match manifest scope_path"
                        .to_string(),
                ));
            }

            let mut entry = [0u8; INDEX_ENTRY_SIZE];
            entry[0..4].copy_from_slice(&segment.segment_id.to_le_bytes());
            entry[4..6].copy_from_slice(&segment.backend_tag.as_u16().to_le_bytes());
            // entry[6..8] reserved zeros
            entry[8..16].copy_from_slice(&offset.to_le_bytes());
            entry[16..24].copy_from_slice(&len.to_le_bytes());
            entry[24..40].copy_from_slice(&manifest_scope_hash);
            entry[40..72].copy_from_slice(actual_hash.as_bytes());
            // entry[72..80] reserved zeros
            index_bytes.extend_from_slice(&entry);
        }

        let manifest_offset = current_offset;
        let manifest_size = manifest_json.len() as u64;
        let total_size = manifest_offset + manifest_size;

        // Write archive
        let mut file = File::create(output_path)
            .map_err(|e| AosError::Io(format!("Failed to create archive: {}", e)))?;

        // Write 64-byte header
        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&AOS_MAGIC);
        header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
        header[8..16].copy_from_slice(&index_offset.to_le_bytes());
        header[16..24].copy_from_slice(&index_size.to_le_bytes());
        header[24..32].copy_from_slice(&manifest_offset.to_le_bytes());
        header[32..40].copy_from_slice(&manifest_size.to_le_bytes());

        file.write_all(&header)
            .map_err(|e| AosError::Io(format!("Failed to write header: {}", e)))?;
        file.write_all(&index_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write index: {}", e)))?;

        for segment in &self.segments {
            file.write_all(&segment.payload)
                .map_err(|e| AosError::Io(format!("Failed to write segment: {}", e)))?;
        }

        file.write_all(&manifest_json)
            .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;
        file.flush()
            .map_err(|e| AosError::Io(format!("Failed to flush archive: {}", e)))?;

        info!(
            path = %output_path.display(),
            total_size = total_size,
            segments = self.segments.len(),
            index_size = index_size,
            manifest_size = manifest_size,
            "AOS archive written with indexed segments"
        );

        Ok(total_size)
    }

    /// Read and validate archive header
    ///
    /// Returns the parsed AosHeader with all offset and length fields.
    pub fn read_header<P: AsRef<Path>>(path: P) -> Result<AosHeader> {
        use std::io::Read;

        let mut file = File::open(path.as_ref())
            .map_err(|e| AosError::Io(format!("Failed to open archive: {}", e)))?;

        let mut header = [0u8; HEADER_SIZE];
        file.read_exact(&mut header)
            .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

        // Reuse in-memory parser so validation stays consistent
        AosWriter::parse_header_bytes(&header)
    }

    /// Parse and validate a header from an in-memory byte slice.
    pub fn parse_header_bytes(bytes: &[u8]) -> Result<AosHeader> {
        if bytes.len() < HEADER_SIZE {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: file too small for AOS2 header".to_string(),
            ));
        }

        if bytes[0..4] != AOS_MAGIC {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: invalid AOS magic".to_string(),
            ));
        }

        let flags = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let index_offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let index_size = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let manifest_offset = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let manifest_size = u64::from_le_bytes(bytes[32..40].try_into().unwrap());

        if index_offset != HEADER_SIZE as u64 {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: invalid header layout (index offset)".to_string(),
            ));
        }

        if bytes[40..HEADER_SIZE].iter().any(|b| *b != 0) {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: reserved header bytes non-zero".to_string(),
            ));
        }

        Ok(AosHeader {
            flags,
            index_offset,
            index_size,
            manifest_offset,
            manifest_size,
        })
    }
}

impl Default for AosWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse segment descriptors from an in-memory archive, validating layout and hashes.
pub fn parse_segments(bytes: &[u8], header: &AosHeader) -> Result<Vec<SegmentDescriptor>> {
    if header.flags & HAS_INDEX_FLAG == 0 {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: missing segment index".to_string(),
        ));
    }

    let file_len = bytes.len();
    let index_offset = header.index_offset as usize;
    let index_size = header.index_size as usize;
    let manifest_offset = header.manifest_offset as usize;
    let manifest_size = header.manifest_size as usize;

    if index_offset < HEADER_SIZE {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index overlaps header".to_string(),
        ));
    }

    let index_end = index_offset.checked_add(index_size).ok_or_else(|| {
        AosError::Validation("Corrupted / needs retrain: index overflow".to_string())
    })?;
    if index_end > file_len {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index beyond file".to_string(),
        ));
    }

    if index_size % INDEX_ENTRY_SIZE != 0 {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index size not 80-byte aligned".to_string(),
        ));
    }

    let manifest_end = manifest_offset.checked_add(manifest_size).ok_or_else(|| {
        AosError::Validation("Corrupted / needs retrain: manifest overflow".to_string())
    })?;
    if manifest_end > file_len {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: manifest beyond file".to_string(),
        ));
    }
    if manifest_offset < index_end {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: manifest overlaps index/segments".to_string(),
        ));
    }

    let entry_count = index_size / INDEX_ENTRY_SIZE;
    let index_bytes = &bytes[index_offset..index_end];
    let mut segments = Vec::with_capacity(entry_count);

    for i in 0..entry_count {
        let entry_start = i * INDEX_ENTRY_SIZE;
        let entry = &index_bytes[entry_start..entry_start + INDEX_ENTRY_SIZE];
        let segment_id = u32::from_le_bytes(entry[0..4].try_into().unwrap());
        let backend_tag_raw = u16::from_le_bytes(entry[4..6].try_into().unwrap());
        let backend_tag = BackendTag::try_from(backend_tag_raw)?;
        let offset = u64::from_le_bytes(entry[8..16].try_into().unwrap()) as usize;
        let len = u64::from_le_bytes(entry[16..24].try_into().unwrap()) as usize;
        let mut scope_hash = [0u8; 16];
        scope_hash.copy_from_slice(&entry[24..40]);
        let mut weights_hash_bytes = [0u8; 32];
        weights_hash_bytes.copy_from_slice(&entry[40..72]);
        let weights_hash = B3Hash::from_bytes(weights_hash_bytes);

        let payload_end = offset.checked_add(len).ok_or_else(|| {
            AosError::Validation("Corrupted / needs retrain: segment overflow".to_string())
        })?;
        if offset < index_end {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment overlaps index".to_string(),
            ));
        }
        if payload_end > manifest_offset {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment overlaps manifest".to_string(),
            ));
        }
        if payload_end > file_len {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment beyond file".to_string(),
            ));
        }

        let payload = &bytes[offset..payload_end];
        if B3Hash::hash(payload) != weights_hash {
            return Err(AosError::Validation(format!(
                "Corrupted / needs retrain: segment {} hash mismatch",
                segment_id
            )));
        }

        segments.push(SegmentDescriptor {
            segment_id,
            backend_tag,
            scope_hash,
            weights_hash,
            offset,
            len,
        });
    }

    Ok(segments)
}

/// Open an in-memory AOS file, returning manifest bytes and borrowed segments.
pub fn open_aos<'a>(bytes: &'a [u8]) -> Result<AosFileView<'a>> {
    let header = AosWriter::parse_header_bytes(bytes)?;
    let descriptors = parse_segments(bytes, &header)?;

    let mut segments = Vec::with_capacity(descriptors.len());
    for desc in descriptors {
        let payload = &bytes[desc.offset..desc.offset + desc.len];
        segments.push(SegmentView {
            segment_id: desc.segment_id,
            backend_tag: desc.backend_tag,
            scope_hash: desc.scope_hash,
            payload,
        });
    }

    let manifest_start = header.manifest_offset as usize;
    let manifest_end = (header.manifest_offset + header.manifest_size) as usize;
    let manifest_bytes = &bytes[manifest_start..manifest_end];

    Ok(AosFileView {
        manifest_bytes,
        segments,
    })
}

/// Deterministically select a segment by scope hash and backend preference.
pub fn select_segment<'a>(
    segments: &'a [SegmentDescriptor],
    scope_hash: [u8; 16],
    preferred_backend: Option<BackendTag>,
) -> Option<&'a SegmentDescriptor> {
    let scoped: Vec<&SegmentDescriptor> = segments
        .iter()
        .filter(|s| s.scope_hash == scope_hash)
        .collect();

    // Prefer explicit backend within the scoped set
    if let Some(preferred) = preferred_backend {
        if let Some(seg) = scoped.iter().find(|s| s.backend_tag == preferred) {
            return Some(*seg);
        }
    }

    // Fallback to canonical for the scoped set
    if let Some(seg) = scoped
        .iter()
        .find(|s| s.backend_tag == BackendTag::Canonical)
    {
        return Some(*seg);
    }

    // If no scoped segments match, allow canonical without scope match
    if let Some(seg) = segments
        .iter()
        .find(|s| s.backend_tag == BackendTag::Canonical)
    {
        return Some(seg);
    }

    // Final fallback: first available segment (deterministic by index order)
    segments.first()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestManifest {
        adapter_id: String,
        rank: u32,
        metadata: HashMap<String, String>,
    }

    fn fake_bytes(label: &str, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut seed = B3Hash::hash(label.as_bytes()).to_bytes().to_vec();
        while out.len() < len {
            out.extend_from_slice(&seed);
            seed = B3Hash::hash(&seed).to_bytes().to_vec();
        }
        out.truncate(len);
        out
    }

    #[test]
    fn test_write_and_read_archive_with_segments() -> Result<()> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let manifest = TestManifest {
            adapter_id: "test-adapter".to_string(),
            rank: 4,
            metadata: HashMap::from([(
                "scope_path".to_string(),
                "domain/group/scope/op".to_string(),
            )]),
        };

        let canonical_bytes = fake_bytes("canonical", 64);
        let mlx_bytes = fake_bytes("mlx", 32);
        let metal_bytes = fake_bytes("metal", 48);

        let mut writer = AosWriter::new();
        writer.add_segment(
            BackendTag::Canonical,
            Some("domain/group/scope/op".to_string()),
            &canonical_bytes,
        )?;
        writer.add_segment(BackendTag::Mlx, None, &mlx_bytes)?;
        writer.add_segment(BackendTag::Metal, None, &metal_bytes)?;

        let total_size = writer.write_archive(temp_file.path(), &manifest)?;

        // Verify header
        let header = AosWriter::read_header(temp_file.path())?;
        assert_eq!(header.flags & HAS_INDEX_FLAG, HAS_INDEX_FLAG);
        assert_eq!(header.index_offset, HEADER_SIZE as u64);
        assert_eq!(header.index_size, 3 * INDEX_ENTRY_SIZE as u64);
        assert!(header.manifest_size > 0);
        assert_eq!(header.manifest_offset + header.manifest_size, total_size);

        // Parse index manually
        let data = std::fs::read(temp_file.path()).unwrap();
        let index_bytes =
            &data[header.index_offset as usize..(header.index_offset + header.index_size) as usize];

        for (i, (expected_tag, expected_bytes)) in [
            (BackendTag::Canonical, &canonical_bytes),
            (BackendTag::Mlx, &mlx_bytes),
            (BackendTag::Metal, &metal_bytes),
        ]
        .iter()
        .enumerate()
        {
            let entry_start = i * INDEX_ENTRY_SIZE;
            let entry = &index_bytes[entry_start..entry_start + INDEX_ENTRY_SIZE];
            let segment_id = u32::from_le_bytes(entry[0..4].try_into().unwrap());
            let backend_tag_raw = u16::from_le_bytes(entry[4..6].try_into().unwrap());
            let offset = u64::from_le_bytes(entry[8..16].try_into().unwrap()) as usize;
            let len = u64::from_le_bytes(entry[16..24].try_into().unwrap()) as usize;
            let mut scope_hash = [0u8; 16];
            scope_hash.copy_from_slice(&entry[24..40]);
            let mut weights_hash = [0u8; 32];
            weights_hash.copy_from_slice(&entry[40..72]);

            assert_eq!(segment_id as usize, i);
            assert_eq!(
                BackendTag::try_from(backend_tag_raw).unwrap(),
                *expected_tag
            );
            assert_eq!(&data[offset..offset + len], *expected_bytes);
            let expected_hash = B3Hash::hash(expected_bytes);
            assert_eq!(expected_hash.as_bytes(), &weights_hash);

            if i == 0 {
                // canonical segment had a scope path
                assert_ne!(scope_hash, [0u8; 16]);
            }
        }

        Ok(())
    }

    #[test]
    fn test_magic_validation() {
        let temp_file = NamedTempFile::new().unwrap();

        // Write invalid magic bytes (file too small for full header)
        std::fs::write(temp_file.path(), b"BAD!").unwrap();

        let result = AosWriter::read_header(temp_file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid magic"));
    }

    #[test]
    fn test_header_size_is_64_bytes() {
        assert_eq!(HEADER_SIZE, 64);
    }
}
