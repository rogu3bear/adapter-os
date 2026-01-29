//! Galaxy loader for zero-copy adapter bundles.
//!
//! A Galaxy file is a contiguous bundle of `.aos` archives with a small header
//! that lists adapter IDs, offsets, and lengths. The layout is:
//! - 8 bytes magic `GLXYAOS1`
//! - u16 version (little endian)
//! - u16 entry count
//! - u32 header size (start of first payload, page aligned)
//! - Repeated entries:
//!   - u64 offset (page aligned, relative to file start)
//!   - u64 length
//!   - u16 adapter_id length
//!   - u16 reserved (0)
//!   - adapter_id UTF-8 bytes
//! - Zero padding up to `header_size`, then concatenated `.aos` payloads.
//!
//! All offsets are validated against the current system page size so each
//! adapter starts at a page boundary (16KB on M4, 4KB elsewhere).

use adapteros_aos::{open_aos, BackendTag, SegmentView};
use adapteros_core::{AosError, Result};
use memmap2::Mmap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs::File;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

const GALAXY_MAGIC: &[u8; 8] = b"GLXYAOS1";
const GALAXY_VERSION: u16 = 1;
const GALAXY_HEADER_FIXED: usize = 16;
const GALAXY_ENTRY_FIXED: usize = 20;

#[derive(Clone)]
pub struct GalaxyLoader {
    page_size: usize,
    galaxies: Arc<RwLock<HashMap<PathBuf, Arc<GalaxyMap>>>>,
    standalones: Arc<RwLock<HashMap<PathBuf, Arc<StandaloneMapping>>>>,
}

#[derive(Debug, Clone)]
pub struct AdapterView {
    pub payload_range: Range<usize>,
    pub manifest_range: Range<usize>,
    pub adapter_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub enum AdapterSource {
    Galaxy { path: PathBuf },
    Standalone { path: PathBuf },
}

#[derive(Debug, Clone)]
#[allow(private_interfaces)]
pub enum AdapterBacking {
    Galaxy(Arc<GalaxyMap>),
    Standalone(Arc<StandaloneMapping>),
}

#[allow(dead_code)]
impl AdapterBacking {
    pub fn galaxy_id(&self) -> Option<String> {
        match self {
            AdapterBacking::Galaxy(map) => Some(map.path.to_string_lossy().to_string()),
            AdapterBacking::Standalone(_) => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            AdapterBacking::Galaxy(map) => map.path.as_path(),
            AdapterBacking::Standalone(map) => map.path.as_path(),
        }
    }

    pub fn is_galaxy(&self) -> bool {
        matches!(self, AdapterBacking::Galaxy(_))
    }

    pub fn slice<'a>(&'a self, range: &Range<usize>) -> &'a [u8] {
        match self {
            AdapterBacking::Galaxy(map) => &map.mmap[range.clone()],
            AdapterBacking::Standalone(map) => &map.mmap[range.clone()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterLoadOutcome {
    pub view: AdapterView,
    pub backing: AdapterBacking,
    pub source: AdapterSource,
    pub reused_mapping: bool,
    pub cached_view: bool,
}

impl AdapterLoadOutcome {
    pub fn payload(&self) -> &[u8] {
        self.backing.slice(&self.view.payload_range)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct GalaxyMap {
    path: PathBuf,
    mmap: Arc<Mmap>,
    header_size: usize,
    page_size: usize,
    entries: HashMap<String, GalaxyEntry>,
    views: RwLock<HashMap<String, AdapterView>>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct StandaloneMapping {
    path: PathBuf,
    mmap: Arc<Mmap>,
    view: RwLock<Option<AdapterView>>,
}

#[derive(Debug, Clone)]
struct GalaxyEntry {
    offset: usize,
    len: usize,
}

#[derive(Debug)]
struct ParsedGalaxyHeader {
    entries: Vec<GalaxyEntryWithId>,
    header_size: usize,
}

#[derive(Debug, Clone)]
struct GalaxyEntryWithId {
    entry: GalaxyEntry,
    adapter_id: String,
}

impl Default for GalaxyLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl GalaxyLoader {
    pub fn new() -> Self {
        let page_size = system_page_size().max(4096);
        Self {
            page_size,
            galaxies: Arc::new(RwLock::new(HashMap::new())),
            standalones: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Page size used for alignment checks (exposed for tests).
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Load an adapter from either a Galaxy bundle or its individual `.aos` file.
    /// Prefers Galaxy bundles for zero-copy reuse and falls back to the single file.
    pub fn load_adapter(
        &self,
        adapter_id: &str,
        adapter_path: &Path,
    ) -> Result<AdapterLoadOutcome> {
        let mut last_err = None;

        for galaxy_path in self.candidate_galaxy_paths(adapter_path) {
            match self.load_from_galaxy(adapter_id, &galaxy_path) {
                Ok(outcome) => {
                    info!(
                        adapter_id = %adapter_id,
                        galaxy = %galaxy_path.display(),
                        cached_map = outcome.reused_mapping,
                        cached_view = outcome.cached_view,
                        "Galaxy hit"
                    );
                    return Ok(outcome);
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        let outcome = self.load_standalone(adapter_id, adapter_path)?;
        if let Some(err) = last_err {
            match &err {
                AosError::NotFound(_) => debug!(
                    adapter_id = %adapter_id,
                    path = %adapter_path.display(),
                    error = %err,
                    "Galaxy miss; using individual load"
                ),
                _ => warn!(
                    adapter_id = %adapter_id,
                    path = %adapter_path.display(),
                    error = %err,
                    "Galaxy candidate failed, falling back to individual load"
                ),
            }
        } else {
            debug!(
                adapter_id = %adapter_id,
                path = %adapter_path.display(),
                "Galaxy miss; loading individual .aos"
            );
        }
        Ok(outcome)
    }

    fn load_from_galaxy(&self, adapter_id: &str, galaxy_path: &Path) -> Result<AdapterLoadOutcome> {
        let (map, reused) = self.galaxy_for_path(galaxy_path)?;
        if !map.entries.contains_key(adapter_id) {
            return Err(AosError::NotFound(format!(
                "Galaxy at '{}' does not contain adapter '{}'",
                galaxy_path.display(),
                adapter_id
            )));
        }
        let (view, cached_view) = map.view_for(adapter_id)?;
        Ok(AdapterLoadOutcome {
            view,
            backing: AdapterBacking::Galaxy(map.clone()),
            source: AdapterSource::Galaxy {
                path: galaxy_path.to_path_buf(),
            },
            reused_mapping: reused,
            cached_view,
        })
    }

    fn load_standalone(&self, adapter_id: &str, adapter_path: &Path) -> Result<AdapterLoadOutcome> {
        let (map, reused) = self.standalone_for_path(adapter_path)?;
        let (view, cached_view) = map.view(adapter_id)?;
        Ok(AdapterLoadOutcome {
            view,
            backing: AdapterBacking::Standalone(map.clone()),
            source: AdapterSource::Standalone {
                path: adapter_path.to_path_buf(),
            },
            reused_mapping: reused,
            cached_view,
        })
    }

    fn galaxy_for_path(&self, path: &Path) -> Result<(Arc<GalaxyMap>, bool)> {
        if let Some(existing) = self.galaxies.read().get(path).cloned() {
            return Ok((existing, true));
        }
        let map = Arc::new(GalaxyMap::load(path, self.page_size)?);
        self.galaxies
            .write()
            .insert(path.to_path_buf(), map.clone());
        Ok((map, false))
    }

    fn standalone_for_path(&self, path: &Path) -> Result<(Arc<StandaloneMapping>, bool)> {
        if let Some(existing) = self.standalones.read().get(path).cloned() {
            return Ok((existing, true));
        }

        let file = File::open(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to open adapter at '{}': {}",
                path.display(),
                e
            ))
        })?;
        // SAFETY: File is successfully opened and valid. Mmap::map creates a memory mapping.
        // The file must remain unchanged while mapped; the Arc<Mmap> ensures the mapping
        // outlives any references to it. StandaloneMapping owns both the mapping and metadata.
        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| {
                AosError::Io(format!(
                    "Failed to mmap adapter at '{}': {}",
                    path.display(),
                    e
                ))
            })?
        };
        let mapping = Arc::new(StandaloneMapping {
            path: path.to_path_buf(),
            mmap: Arc::new(mmap),
            view: RwLock::new(None),
        });
        self.standalones
            .write()
            .insert(path.to_path_buf(), mapping.clone());
        Ok((mapping, false))
    }

    fn candidate_galaxy_paths(&self, adapter_path: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let preferred = adapter_path.with_extension("galaxy");
        if preferred.exists() {
            paths.push(preferred);
        }

        if let Some(parent) = adapter_path.parent() {
            if let Ok(read_dir) = std::fs::read_dir(parent) {
                for entry in read_dir.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "galaxy") && !paths.contains(&path)
                    {
                        paths.push(path);
                    }
                }
            }
        }
        paths
    }
}

impl GalaxyMap {
    fn load(path: &Path, page_size: usize) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to open galaxy bundle '{}': {}",
                path.display(),
                e
            ))
        })?;
        // SAFETY: File is successfully opened and valid. Mmap::map creates a memory mapping.
        // The galaxy bundle is read-only and the GalaxyMap owns the mapping for its lifetime.
        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| {
                AosError::Io(format!(
                    "Failed to mmap galaxy bundle '{}': {}",
                    path.display(),
                    e
                ))
            })?
        };
        let header = parse_galaxy_header(&mmap, page_size)?;
        let entries: HashMap<String, GalaxyEntry> = header
            .entries
            .into_iter()
            .map(|e| (e.adapter_id, e.entry))
            .collect();

        Ok(Self {
            path: path.to_path_buf(),
            mmap: Arc::new(mmap),
            header_size: header.header_size,
            page_size,
            entries,
            views: RwLock::new(HashMap::new()),
        })
    }

    fn view_for(&self, adapter_id: &str) -> Result<(AdapterView, bool)> {
        if let Some(view) = self.views.read().get(adapter_id) {
            return Ok((view.clone(), true));
        }

        let entry = self.entries.get(adapter_id).ok_or_else(|| {
            AosError::NotFound(format!(
                "Adapter '{}' not found in galaxy '{}'",
                adapter_id,
                self.path.display()
            ))
        })?;

        let aos_slice = &self.mmap[entry.offset..entry.offset + entry.len];
        let file_view = open_aos(aos_slice)?;
        let segment = select_segment(&file_view)?;
        let payload_range = absolute_range(aos_slice, segment.payload, entry.offset)?;
        let manifest_range = absolute_range(aos_slice, file_view.manifest_bytes, entry.offset)?;

        let view = AdapterView {
            payload_range,
            manifest_range,
            adapter_range: entry.offset..entry.offset + entry.len,
        };
        self.views
            .write()
            .insert(adapter_id.to_string(), view.clone());
        Ok((view, false))
    }
}

impl StandaloneMapping {
    fn view(&self, adapter_id: &str) -> Result<(AdapterView, bool)> {
        if let Some(view) = self.view.read().clone() {
            return Ok((view, true));
        }

        let aos_slice = &self.mmap[..];
        let file_view = open_aos(aos_slice)?;
        let segment = select_segment(&file_view)?;
        let payload_range = absolute_range(aos_slice, segment.payload, 0)?;
        let manifest_range = absolute_range(aos_slice, file_view.manifest_bytes, 0)?;

        let view = AdapterView {
            payload_range,
            manifest_range,
            adapter_range: 0..self.mmap.len(),
        };
        *self.view.write() = Some(view.clone());
        debug!(
            adapter_id = %adapter_id,
            path = %self.path.display(),
            "Parsed standalone adapter"
        );
        Ok((view, false))
    }
}

fn select_segment<'a>(
    file_view: &'a adapteros_aos::AosFileView<'a>,
) -> Result<&'a SegmentView<'a>> {
    file_view
        .segments
        .iter()
        .find(|seg| seg.backend_tag == BackendTag::Coreml)
        .or_else(|| {
            file_view
                .segments
                .iter()
                .find(|seg| seg.backend_tag == BackendTag::Canonical)
        })
        .ok_or_else(|| {
            AosError::Validation("Missing CoreML or canonical segment in adapter".to_string())
        })
}

fn absolute_range(parent: &[u8], slice: &[u8], parent_offset: usize) -> Result<Range<usize>> {
    let parent_ptr = parent.as_ptr() as usize;
    let slice_ptr = slice.as_ptr() as usize;
    if slice_ptr < parent_ptr {
        return Err(AosError::Validation(
            "Segment slice lies before parent mapping".to_string(),
        ));
    }
    let rel = slice_ptr - parent_ptr;
    if rel + slice.len() > parent.len() {
        return Err(AosError::Validation(
            "Segment slice extends past parent mapping".to_string(),
        ));
    }
    let start = parent_offset + rel;
    Ok(start..start + slice.len())
}

fn parse_galaxy_header(bytes: &[u8], page_size: usize) -> Result<ParsedGalaxyHeader> {
    if bytes.len() < GALAXY_HEADER_FIXED {
        return Err(AosError::Validation(
            "Galaxy bundle too small for header".to_string(),
        ));
    }
    if &bytes[..8] != GALAXY_MAGIC {
        return Err(AosError::Validation("Invalid galaxy magic".to_string()));
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != GALAXY_VERSION {
        return Err(AosError::Validation(format!(
            "Unsupported galaxy version {} (expected {})",
            version, GALAXY_VERSION
        )));
    }

    let entry_count = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
    let header_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;

    let mut cursor = GALAXY_HEADER_FIXED;
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        if cursor + GALAXY_ENTRY_FIXED > bytes.len() {
            return Err(AosError::Validation(
                "Galaxy header truncated while reading entries".to_string(),
            ));
        }
        let offset = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap()) as usize;
        let len = u64::from_le_bytes(bytes[cursor + 8..cursor + 16].try_into().unwrap()) as usize;
        let id_len =
            u16::from_le_bytes(bytes[cursor + 16..cursor + 18].try_into().unwrap()) as usize;
        cursor += GALAXY_ENTRY_FIXED;

        let id_end = cursor
            .checked_add(id_len)
            .ok_or_else(|| AosError::Validation("Galaxy adapter id length overflow".to_string()))?;
        if id_end > bytes.len() {
            return Err(AosError::Validation(
                "Galaxy header truncated while reading adapter ids".to_string(),
            ));
        }
        let adapter_id = std::str::from_utf8(&bytes[cursor..id_end])
            .map_err(|e| AosError::Validation(format!("Galaxy adapter id not UTF-8: {e}")))?;
        cursor = id_end;

        if !offset.is_multiple_of(page_size) {
            return Err(AosError::Validation(format!(
                "Adapter '{}' not page aligned (offset {}, page {})",
                adapter_id, offset, page_size
            )));
        }
        entries.push(GalaxyEntryWithId {
            entry: GalaxyEntry { offset, len },
            adapter_id: adapter_id.to_string(),
        });
    }

    let aligned_cursor = align_up(cursor, page_size);
    let header_size = header_size.max(aligned_cursor);
    if header_size > bytes.len() {
        return Err(AosError::Validation(
            "Galaxy header size exceeds file length".to_string(),
        ));
    }

    for entry in &entries {
        let end = entry
            .entry
            .offset
            .checked_add(entry.entry.len)
            .ok_or_else(|| AosError::Validation("Galaxy entry size overflow".to_string()))?;
        if entry.entry.offset < header_size || end > bytes.len() {
            return Err(AosError::Validation(format!(
                "Galaxy entry for '{}' is out of bounds",
                entry.adapter_id
            )));
        }
    }

    Ok(ParsedGalaxyHeader {
        entries,
        header_size,
    })
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

fn system_page_size() -> usize {
    // SAFETY: sysconf returns a long; fallback to 4K if unavailable.
    unsafe {
        let size = libc::sysconf(libc::_SC_PAGESIZE);
        if size > 0 {
            size as usize
        } else {
            4096
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_aos::{AosWriter, BackendTag, WriteOptions};
    use serde_json::json;
    use tempfile::tempdir;

    fn write_aos(path: &Path, adapter_id: &str, payload: &[u8]) -> Result<()> {
        let mut writer = AosWriter::with_options(WriteOptions {
            include_signature: false,
        });
        writer.add_segment(
            BackendTag::Canonical,
            Some("tests/scope".to_string()),
            payload,
        )?;
        let manifest = json!({
            "adapter_id": adapter_id,
            "version": "1.0.0",
            "rank": 1,
            "alpha": 1.0,
            "base_model": "test",
            "target_modules": [],
            "metadata": { "scope_path": "tests/scope" }
        });
        writer.write_archive(path, &manifest)?;
        Ok(())
    }

    fn build_galaxy(path: &Path, entries: Vec<(String, Vec<u8>)>, page_size: usize) -> Result<()> {
        let mut cursor = GALAXY_HEADER_FIXED;
        let mut payloads = Vec::new();
        for (adapter_id, payload) in &entries {
            cursor += GALAXY_ENTRY_FIXED + adapter_id.len();
            payloads.push((adapter_id, payload));
        }
        let header_size = align_up(cursor, page_size);

        let mut file_bytes = Vec::with_capacity(header_size);
        file_bytes.extend_from_slice(GALAXY_MAGIC);
        file_bytes.extend_from_slice(&GALAXY_VERSION.to_le_bytes());
        file_bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        file_bytes.extend_from_slice(&(header_size as u32).to_le_bytes());

        let mut current = header_size;
        let mut offsets = Vec::new();
        for (adapter_id, payload) in &entries {
            let aligned = align_up(current, page_size);
            let len = payload.len() as u64;
            file_bytes.extend_from_slice(&(aligned as u64).to_le_bytes());
            file_bytes.extend_from_slice(&len.to_le_bytes());
            file_bytes.extend_from_slice(&(adapter_id.len() as u16).to_le_bytes());
            file_bytes.extend_from_slice(&0u16.to_le_bytes());
            file_bytes.extend_from_slice(adapter_id.as_bytes());
            offsets.push(aligned);
            current = aligned + payload.len();
        }

        file_bytes.resize(header_size, 0);
        for ((_, payload), offset) in entries.iter().zip(offsets.iter()) {
            let target = *offset;
            if file_bytes.len() < target {
                file_bytes.resize(target, 0);
            }
            file_bytes.extend_from_slice(payload);
        }

        std::fs::write(path, file_bytes).map_err(|e| {
            AosError::Io(format!(
                "Failed to write galaxy '{}' fixture: {}",
                path.display(),
                e
            ))
        })
    }

    #[test]
    fn parses_galaxy_header_and_views_offsets() {
        let dir = tempdir().unwrap();
        let a_path = dir.path().join("a.aos");
        let b_path = dir.path().join("b.aos");
        write_aos(&a_path, "a", b"aaa").unwrap();
        write_aos(&b_path, "b", b"bbbb").unwrap();

        let a_bytes = std::fs::read(&a_path).unwrap();
        let b_bytes = std::fs::read(&b_path).unwrap();
        let galaxy_path = dir.path().join("cluster.galaxy");
        let loader = GalaxyLoader::new();
        let page_size = loader.page_size();
        build_galaxy(
            &galaxy_path,
            vec![
                ("a".to_string(), a_bytes.clone()),
                ("b".to_string(), b_bytes.clone()),
            ],
            page_size,
        )
        .unwrap();

        let from_galaxy = loader
            .load_adapter("b", &b_path)
            .expect("loads from galaxy");

        assert!(matches!(from_galaxy.source, AdapterSource::Galaxy { .. }));
        let file_view = adapteros_aos::open_aos(&b_bytes).unwrap();
        let canonical = file_view
            .segments
            .iter()
            .find(|seg| seg.backend_tag == BackendTag::Canonical)
            .unwrap();
        assert_eq!(from_galaxy.payload(), canonical.payload);
        assert_eq!(from_galaxy.view.adapter_range.start % page_size, 0);
    }

    #[test]
    fn falls_back_when_galaxy_missing() {
        let dir = tempdir().unwrap();
        let aos_path = dir.path().join("solo.aos");
        write_aos(&aos_path, "solo", b"payload").unwrap();

        let loader = GalaxyLoader::new();
        let outcome = loader
            .load_adapter("solo", &aos_path)
            .expect("loads standalone");
        assert!(matches!(outcome.source, AdapterSource::Standalone { .. }));
        assert!(!outcome.backing.is_galaxy());
    }

    #[test]
    fn misaligned_galaxy_falls_back_to_standalone() {
        let dir = tempdir().unwrap();
        let aos_path = dir.path().join("aligned.aos");
        write_aos(&aos_path, "aligned", b"abc").unwrap();
        let bytes = std::fs::read(&aos_path).unwrap();
        let galaxy_path = dir.path().join("bad.galaxy");

        // Force misalignment by using tiny page size during build.
        build_galaxy(
            &galaxy_path,
            vec![("aligned".to_string(), bytes)],
            8, // intentionally too small so loader (real page size) fails alignment
        )
        .unwrap();

        let loader = GalaxyLoader::new();
        let outcome = loader
            .load_adapter("aligned", &aos_path)
            .expect("falls back to standalone");
        assert!(matches!(outcome.source, AdapterSource::Standalone { .. }));
    }
}
