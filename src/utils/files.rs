//! Plain file-system helpers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Returns `true` if `path` exists.
#[must_use]
pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Reads `path` as UTF-8 text.
///
/// # Errors
///
/// Returns an error if the file cannot be read or is not UTF-8.
pub fn read(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

/// Writes `contents` to `path`, creating its parent folders first.
///
/// # Errors
///
/// Returns an error if a folder or the file cannot be written.
pub fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

/// Resolves `path` to an absolute path if it is an existing folder.
///
/// Returns `None` if it does not exist or is not a folder.
#[must_use]
pub fn resolve_dir(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().filter(|p| p.is_dir())
}
