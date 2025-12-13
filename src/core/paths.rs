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

    #[test]
    fn test_make_relative() {
        let root = Path::new("/project");
        let path = Path::new("/project/src/main.rs");
        assert_eq!(make_relative(path, root), Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_make_relative_not_under_root() {
        let root = Path::new("/project");
        let path = Path::new("/other/file.rs");
        assert_eq!(make_relative(path, root), None);
    }

    #[test]
    fn test_make_relative_same_as_root() {
        let root = Path::new("/project");
        let path = Path::new("/project");
        assert_eq!(make_relative(path, root), Some("".to_string()));
    }

    #[test]
    fn test_join_normalized() {
        let base = Path::new("/project");
        let result = join_normalized(base, "src/main.rs");
        assert!(result.to_string_lossy().contains("src"));
        assert!(result.to_string_lossy().contains("main.rs"));
    }

    #[test]
    fn test_is_hidden_empty_filename() {
        // Path with no filename component
        assert!(!is_hidden(Path::new("/")));
    }

    #[test]
    fn test_is_within_root() {
        let temp = tempfile::tempdir().unwrap();
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let file = subdir.join("file.txt");
        std::fs::write(&file, "test").unwrap();

        assert!(is_within_root(&file, temp.path()));
    }

    #[test]
    fn test_is_within_root_outside() {
        let temp1 = tempfile::tempdir().unwrap();
        let temp2 = tempfile::tempdir().unwrap();
        let file = temp1.path().join("file.txt");
        std::fs::write(&file, "test").unwrap();

        // file in temp1 should not be within temp2
        assert!(!is_within_root(&file, temp2.path()));
    }

    #[test]
    fn test_normalize_path_nested() {
        let path = Path::new("a/b/c/d.rs");
        assert_eq!(normalize_path(path), "a/b/c/d.rs");
    }
}
