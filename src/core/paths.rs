//! Path normalization utilities
//!
//! Ensures all paths are normalized to use '/' as separator and are relative to root.

use std::path::{Path, PathBuf};

/// Normalize a path to use '/' as separator (for cross-platform consistency)
pub fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Make a path relative to the root directory
pub fn make_relative(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(normalize_path)
}

/// Join paths and normalize
#[allow(dead_code)]
pub fn join_normalized(base: &Path, relative: &str) -> PathBuf {
    base.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
}

/// Check if a path is hidden (starts with '.')
#[allow(dead_code)]
pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// Get the .mise cache directory for a given root
pub fn cache_dir(root: &Path) -> PathBuf {
    root.join(".mise")
}

/// Validate that a path is within the root directory (prevent path traversal)
#[allow(dead_code)]
pub fn is_within_root(path: &Path, root: &Path) -> bool {
    path.canonicalize()
        .ok()
        .and_then(|p| root.canonicalize().ok().map(|r| p.starts_with(r)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let path = Path::new("src/main.rs");
        assert_eq!(normalize_path(path), "src/main.rs");
    }

    #[test]
    fn test_is_hidden() {
        assert!(is_hidden(Path::new(".git")));
        assert!(is_hidden(Path::new(".gitignore")));
        assert!(!is_hidden(Path::new("src")));
        assert!(!is_hidden(Path::new("main.rs")));
    }

    #[test]
    fn test_cache_dir() {
        let root = Path::new("/project");
        assert_eq!(cache_dir(root), PathBuf::from("/project/.mise"));
    }
}
