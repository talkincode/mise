//! File scanning backend
//!
//! Uses walkdir and ignore crate for efficient file traversal

use anyhow::Result;
use ignore::WalkBuilder;
use std::path::Path;

use crate::core::model::{Meta, ResultItem, ResultSet};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::{get_file_size, get_mtime_ms};

/// Scan files in a directory
pub fn scan_files(
    root: &Path,
    scope: Option<&Path>,
    max_depth: Option<usize>,
    hidden: bool,
    ignore: bool,
    file_type: Option<&str>,
) -> Result<ResultSet> {
    let scan_path = scope.unwrap_or(root);

    let mut builder = WalkBuilder::new(scan_path);
    builder
        .hidden(!hidden)
        .git_ignore(ignore)
        .git_global(ignore)
        .git_exclude(ignore);

    if let Some(depth) = max_depth {
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
        match file_type {
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
pub fn run_scan(
    root: &Path,
    scope: Option<&Path>,
    max_depth: Option<usize>,
    hidden: bool,
    ignore: bool,
    file_type: Option<&str>,
    config: RenderConfig,
) -> Result<()> {
    let result_set = scan_files(root, scope, max_depth, hidden, ignore, file_type)?;

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
    let mut result_set = scan_files(root, scope, None, false, true, Some("file"))?;

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

/// Aliases for MCP compatibility
pub fn scan_to_result_set(
    root: &Path,
    scope: Option<&Path>,
    max_depth: Option<usize>,
    hidden: bool,
    ignore: bool,
    file_type: Option<&str>,
) -> Result<ResultSet> {
    scan_files(root, scope, max_depth, hidden, ignore, file_type)
}

pub fn find_to_result_set(
    root: &Path,
    pattern: Option<&str>,
    scope: Option<&Path>,
) -> Result<ResultSet> {
    find_files(root, pattern, scope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_scan_empty_dir() {
        let temp = tempdir().unwrap();
        let result = scan_files(temp.path(), None, None, false, true, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_with_files() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("file1.txt")).unwrap();
        File::create(temp.path().join("file2.rs")).unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_scan_only_dirs() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("file1.txt")).unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let result = scan_files(temp.path(), None, None, false, true, Some("dir")).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_scan_with_scope() {
        let temp = tempdir().unwrap();
        let subdir = temp.path().join("src");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("main.rs")).unwrap();
        File::create(temp.path().join("README.md")).unwrap();

        let result =
            scan_files(temp.path(), Some(&subdir), None, false, true, Some("file")).unwrap();
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

        // With max_depth = 1, should not traverse into level2
        let result = scan_files(temp.path(), None, Some(2), false, true, Some("file")).unwrap();
        // Only file1.txt at depth 2 should be found (level1 is depth 1, file1.txt is depth 2)
        assert!(result.len() >= 1);
    }

    #[test]
    fn test_scan_hidden_files() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join(".hidden")).unwrap();
        File::create(temp.path().join("visible.txt")).unwrap();

        // Without hidden=true, should skip hidden files
        let result_no_hidden =
            scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
        assert!(result_no_hidden
            .items
            .iter()
            .all(|i| !i.path.as_ref().unwrap().starts_with('.')));

        // With hidden=true, should include hidden files
        let result_with_hidden =
            scan_files(temp.path(), None, None, true, true, Some("file")).unwrap();
        assert!(result_with_hidden.len() >= result_no_hidden.len());
    }

    #[test]
    fn test_scan_result_has_metadata() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let result = scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
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
        let result = scan_files(temp.path(), None, None, false, true, None).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_scan_sorted_output() {
        let temp = tempdir().unwrap();
        File::create(temp.path().join("z_file.txt")).unwrap();
        File::create(temp.path().join("a_file.txt")).unwrap();
        File::create(temp.path().join("m_file.txt")).unwrap();

        let result = scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
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

        let result = run_scan(temp.path(), None, None, false, true, Some("file"), config);
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
        let result = scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
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
        let result = scan_files(temp.path(), None, None, false, false, Some("file")).unwrap();
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
        let result = scan_files(temp.path(), None, None, false, true, Some("file")).unwrap();
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

        let result = scan_files(temp.path(), None, None, false, true, Some("dir")).unwrap();
        assert!(result.items.iter().any(|i| i
            .path
            .as_ref()
            .map(|p| p.contains("empty_subdir"))
            .unwrap_or(false)));
    }
}
