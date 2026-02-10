//! Archive utilities (zip, tar, etc.)
//!
//! Small helpers that are reused across multiple crates to avoid copy/paste drift.

/// Returns true if a zip entry is a symlink (based on Unix mode bits).
#[inline]
pub fn is_zip_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .map(|mode| (mode & 0o170000) == 0o120000)
        .unwrap_or(false)
}
