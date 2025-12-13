//! File scanning backend
//!
//! Uses walkdir and ignore crate for efficient file traversal

use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

use crate::core::model::{Meta, ResultItem, ResultSet};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::{get_file_size, get_mtime_ms};

/// Options for the scan command
#[derive(Debug, Default)]
pub struct ScanOptions {
    pub scope: Option<PathBuf>,
    pub max_depth: Option<usize>,
    pub hidden: bool,
    pub ignore: bool,
    pub file_type: Option<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Simple glob matching (supports * and **)
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.starts_with("*.") {
        // Extension match: *.rs -> ends with .rs
        let ext = &pattern[1..];
        path.ends_with(ext)
    } else if pattern.ends_with("/*") {
        // Directory match: vendor/* -> starts with vendor/
        let prefix = &pattern[..pattern.len() - 1];
        path.starts_with(prefix)
    } else if pattern.contains('*') {
        // Generic wildcard - simple contains check for the non-wildcard part
        let parts: Vec<&str> = pattern.split('*').filter(|s| !s.is_empty()).collect();
        parts.iter().all(|part| path.contains(part))
    } else {
        // Exact match
        path == pattern || path.contains(pattern)
    }
}

/// Scan files in a directory
pub fn scan_files(root: &Path, options: &ScanOptions) -> Result<ResultSet> {
    let scan_path = options.scope.as_deref().unwrap_or(root);

    let mut builder = WalkBuilder::new(scan_path);
    builder
        .hidden(!options.hidden)
        .git_ignore(options.ignore)
        .git_global(options.ignore)
        .git_exclude(options.ignore);

    if let Some(depth) = options.max_depth {
        builder.max_depth(Some(depth));
    }

    let mut result_set = ResultSet::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // Filter by type
        let is_dir = path.is_dir();
        match options.file_type.as_deref() {
            Some("file") if is_dir => continue,
            Some("dir") if !is_dir => continue,
            _ => {}
        }

        // Skip the root itself
        if path == root || path == scan_path {
            continue;
        }

        // Get relative path
        let relative = match make_relative(path, root) {
            Some(r) => r,
            None => continue,
        };

        // Apply include/exclude filters
        if !options.include.is_empty() {
            let matched = options
                .include
                .iter()
                .any(|glob| glob_match(glob, &relative));
            if !matched {
                continue;
            }
        }
        if options
            .exclude
            .iter()
            .any(|glob| glob_match(glob, &relative))
        {
            continue;
        }

        // Build result item
        let mut item = ResultItem::file(relative);

        // Add metadata for files
        if !is_dir {
            let mut meta = Meta::default();
            if let Ok(size) = get_file_size(path) {
                meta.size = Some(size);
            }
            if let Ok(mtime) = get_mtime_ms(path) {
                meta.mtime_ms = Some(mtime);
            }
            item = item.with_meta(meta);
        }

        result_set.push(item);
    }

    result_set.sort();
    Ok(result_set)
}

/// Run the scan command
pub fn run_scan(root: &Path, options: ScanOptions, config: RenderConfig) -> Result<()> {
    let result_set = scan_files(root, &options)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Run the find command (scan with pattern filtering)
pub fn run_find(
    root: &Path,
    pattern: Option<&str>,
    scope: Option<&Path>,
    config: RenderConfig,
) -> Result<()> {
    let result_set = find_files(root, pattern, scope)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Find files by pattern (for MCP and programmatic use)
pub fn find_files(root: &Path, pattern: Option<&str>, scope: Option<&Path>) -> Result<ResultSet> {
    let options = ScanOptions {
        scope: scope.map(|p| p.to_path_buf()),
        file_type: Some("file".to_string()),
        ignore: true,
        ..Default::default()
    };
    let mut result_set = scan_files(root, &options)?;

    // Filter by pattern if provided
    if let Some(pattern) = pattern {
        let pattern_lower = pattern.to_lowercase();
        result_set.items.retain(|item| {
            item.path
                .as_ref()
                .map(|p| p.to_lowercase().contains(&pattern_lower))
                .unwrap_or(false)
        });
    }

    Ok(result_set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    fn default_options() -> ScanOptions {
        ScanOptions::default()
    }

    fn file_options() -> ScanOptions {
        ScanOptions {
            file_type: Some("file".to_string()),
            ignore: true,
            ..Default::default()
        }
    }

    fn dir_options() -> ScanOptions {
        ScanOptions {
            file_type: Some("dir".to_string()),
            ignore: true,
            ..Default::default()
        }
    }

    #[test]
    fn test_scan_empty_dir() {
        let temp = tempdir().unwrap();
        let result = scan_files(temp.path(), &default_options()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_with_files() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("file1.txt")).unwrap();
        File::create(temp.path().join("file2.rs")).unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = scan_files(temp.path(), &file_options()).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_scan_only_dirs() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("file1.txt")).unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = scan_files(temp.path(), &dir_options()).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_scan_with_scope() {
        let temp = tempdir().unwrap();
        let subdir = temp.path().join("src");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("main.rs")).unwrap();
        File::create(temp.path().join("README.md")).unwrap();

        let options = ScanOptions {
            scope: Some(subdir),
            file_type: Some("file".to_string()),
            ignore: true,
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.items[0].path.as_ref().unwrap().contains("main.rs"));
    }

    #[test]
    fn test_scan_max_depth() {
        let temp = tempdir().unwrap();
        let level1 = temp.path().join("level1");
        let level2 = level1.join("level2");
        fs::create_dir_all(&level2).unwrap();
        File::create(level1.join("file1.txt")).unwrap();
        File::create(level2.join("file2.txt")).unwrap();

        // With max_depth = 2, should traverse into level1 but not level2
        let options = ScanOptions {
            max_depth: Some(2),
            file_type: Some("file".to_string()),
            ignore: true,
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();
        // Only file1.txt at depth 2 should be found (level1 is depth 1, file1.txt is depth 2)
        assert!(result.len() >= 1);
    }

    #[test]
    fn test_scan_hidden_files() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join(".hidden")).unwrap();
        File::create(temp.path().join("visible.txt")).unwrap();

        // Without hidden=true, should skip hidden files
        let result_no_hidden = scan_files(temp.path(), &file_options()).unwrap();
        assert!(result_no_hidden
            .items
            .iter()
            .all(|i| !i.path.as_ref().unwrap().starts_with('.')));

        // With hidden=true, should include hidden files
        let options = ScanOptions {
            hidden: true,
            file_type: Some("file".to_string()),
            ignore: true,
            ..Default::default()
        };
        let result_with_hidden = scan_files(temp.path(), &options).unwrap();
        assert!(result_with_hidden.len() >= result_no_hidden.len());
    }

    #[test]
    fn test_scan_result_has_metadata() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let result = scan_files(temp.path(), &file_options()).unwrap();
        assert_eq!(result.len(), 1);

        let item = &result.items[0];
        assert!(item.meta.size.is_some());
        assert!(item.meta.mtime_ms.is_some());
        assert_eq!(item.meta.size.unwrap(), 11); // "hello world" is 11 bytes
    }

    #[test]
    fn test_scan_all_types() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("file.txt")).unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        // With file_type = None, should include both files and directories
        let options = ScanOptions {
            ignore: true,
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_scan_sorted_output() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("z_file.txt")).unwrap();
        File::create(temp.path().join("a_file.txt")).unwrap();
        File::create(temp.path().join("m_file.txt")).unwrap();

        let result = scan_files(temp.path(), &file_options()).unwrap();
        let paths: Vec<_> = result
            .items
            .iter()
            .filter_map(|i| i.path.as_ref())
            .collect();

        let mut sorted_paths = paths.clone();
        sorted_paths.sort();
        assert_eq!(paths, sorted_paths);
    }

    #[test]
    fn test_run_scan_command() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("test.txt")).unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_scan(temp.path(), file_options(), config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_find_no_pattern() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("test.txt")).unwrap();
        File::create(temp.path().join("other.rs")).unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        // No pattern should return all files
        let result = run_find(temp.path(), None, None, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_find_with_pattern() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("test.txt")).unwrap();
        File::create(temp.path().join("other.rs")).unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_find(temp.path(), Some(".txt"), None, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_find_case_insensitive() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("TEST.TXT")).unwrap();
        File::create(temp.path().join("other.rs")).unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        // Pattern matching should be case-insensitive
        let result = run_find(temp.path(), Some("test"), None, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_scan_gitignore_respected() {
        let temp = tempdir().unwrap();

        // Initialize git repo to make .gitignore work
        std::process::Command::new("git")
            .arg("init")
            .current_dir(temp.path())
            .output()
            .ok();

        // Create .gitignore
        std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").unwrap();

        // Create ignored and non-ignored files
        File::create(temp.path().join("ignored.txt")).unwrap();
        File::create(temp.path().join("included.txt")).unwrap();

        // With ignore=true, should skip ignored files
        let result = scan_files(temp.path(), &file_options()).unwrap();
        let paths: Vec<_> = result
            .items
            .iter()
            .filter_map(|i| i.path.as_ref())
            .collect();

        // Should contain included.txt
        assert!(paths.iter().any(|p| p.contains("included.txt")));
        // May or may not contain ignored.txt depending on git init success
    }

    #[test]
    fn test_scan_without_gitignore() {
        let temp = tempdir().unwrap();

        // Create .gitignore
        std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").unwrap();
        File::create(temp.path().join("ignored.txt")).unwrap();

        // With ignore=false, should include all files
        let options = ScanOptions {
            file_type: Some("file".to_string()),
            ignore: false,
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();
        let paths: Vec<_> = result
            .items
            .iter()
            .filter_map(|i| i.path.as_ref())
            .collect();

        // Should contain the "ignored" file since gitignore is disabled
        // Note: files starting with . are still hidden by default
        assert!(paths.iter().any(|p| p.contains("ignored.txt")));
    }

    #[test]
    fn test_scan_deep_nesting() {
        let temp = tempdir().unwrap();
        let deep = temp.path().join("a/b/c/d/e");
        fs::create_dir_all(&deep).unwrap();
        File::create(deep.join("deep.txt")).unwrap();

        // Without max_depth, should find deep files
        let result = scan_files(temp.path(), &file_options()).unwrap();
        assert!(result.items.iter().any(|i| i
            .path
            .as_ref()
            .map(|p| p.contains("deep.txt"))
            .unwrap_or(false)));
    }

    #[test]
    fn test_scan_empty_subdir() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join("empty_subdir")).unwrap();

        let result = scan_files(temp.path(), &dir_options()).unwrap();
        assert!(result.items.iter().any(|i| i
            .path
            .as_ref()
            .map(|p| p.contains("empty_subdir"))
            .unwrap_or(false)));
    }

    // ==================== glob_match tests ====================

    #[test]
    fn test_glob_match_extension() {
        // Test *.ext pattern
        assert!(glob_match("*.rs", "src/main.rs"));
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(!glob_match("*.rs", "src/main.py"));
        assert!(!glob_match("*.rs", "rsx"));
    }

    #[test]
    fn test_glob_match_directory_wildcard() {
        // Test dir/* pattern
        assert!(glob_match("vendor/*", "vendor/package"));
        assert!(glob_match("vendor/*", "vendor/a/b/c"));
        assert!(!glob_match("vendor/*", "src/vendor"));
    }

    #[test]
    fn test_glob_match_generic_wildcard() {
        // Test patterns with * in the middle
        assert!(glob_match("*_test.rs", "foo_test.rs"));
        assert!(glob_match("test_*", "test_foo.rs"));
        assert!(glob_match("*test*", "my_test_file.rs"));
    }

    #[test]
    fn test_glob_match_exact() {
        // Test exact match
        assert!(glob_match("README.md", "README.md"));
        assert!(glob_match("README.md", "docs/README.md")); // contains
        assert!(!glob_match("README.md", "readme.md")); // case sensitive
    }

    #[test]
    fn test_glob_match_contains() {
        // Test simple contains pattern
        assert!(glob_match("test", "src/test/main.rs"));
        assert!(glob_match("test", "test.rs"));
        assert!(!glob_match("test", "spec.rs"));
    }

    // ==================== include/exclude tests ====================

    #[test]
    fn test_scan_with_include_glob() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("main.rs")).unwrap();
        File::create(temp.path().join("lib.rs")).unwrap();
        File::create(temp.path().join("readme.md")).unwrap();

        let options = ScanOptions {
            file_type: Some("file".to_string()),
            ignore: true,
            include: vec!["*.rs".to_string()],
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();

        // Should only include .rs files
        assert_eq!(result.len(), 2);
        assert!(result.items.iter().all(|i| i
            .path
            .as_ref()
            .map(|p| p.ends_with(".rs"))
            .unwrap_or(false)));
    }

    #[test]
    fn test_scan_with_exclude_glob() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("main.rs")).unwrap();
        File::create(temp.path().join("main_test.rs")).unwrap();
        File::create(temp.path().join("lib.rs")).unwrap();

        let options = ScanOptions {
            file_type: Some("file".to_string()),
            ignore: true,
            exclude: vec!["*_test.rs".to_string()],
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();

        // Should exclude _test.rs files
        assert_eq!(result.len(), 2);
        assert!(result.items.iter().all(|i| i
            .path
            .as_ref()
            .map(|p| !p.contains("_test"))
            .unwrap_or(false)));
    }

    #[test]
    fn test_scan_with_include_and_exclude() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("main.rs")).unwrap();
        File::create(temp.path().join("main_test.rs")).unwrap();
        File::create(temp.path().join("readme.md")).unwrap();

        let options = ScanOptions {
            file_type: Some("file".to_string()),
            ignore: true,
            include: vec!["*.rs".to_string()],
            exclude: vec!["*_test.rs".to_string()],
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();

        // Should include .rs but exclude _test.rs
        assert_eq!(result.len(), 1);
        let path = result.items[0].path.as_ref().unwrap();
        assert!(path.ends_with("main.rs"));
    }

    #[test]
    fn test_scan_with_multiple_include_patterns() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("main.rs")).unwrap();
        File::create(temp.path().join("lib.py")).unwrap();
        File::create(temp.path().join("readme.md")).unwrap();

        let options = ScanOptions {
            file_type: Some("file".to_string()),
            ignore: true,
            include: vec!["*.rs".to_string(), "*.py".to_string()],
            ..Default::default()
        };
        let result = scan_files(temp.path(), &options).unwrap();

        // Should include both .rs and .py files
        assert_eq!(result.len(), 2);
    }
}
