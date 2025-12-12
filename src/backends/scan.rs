//! File scanning backend
//!
//! Uses walkdir and ignore crate for efficient file traversal

use anyhow::Result;
use ignore::WalkBuilder;
use std::path::Path;

use crate::core::model::{Meta, ResultItem, ResultSet};
use crate::core::paths::make_relative;
use crate::core::render::{OutputFormat, Renderer};
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
    format: OutputFormat,
) -> Result<()> {
    let result_set = scan_files(root, scope, max_depth, hidden, ignore, file_type)?;

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Run the find command (scan with pattern filtering)
pub fn run_find(
    root: &Path,
    pattern: Option<&str>,
    scope: Option<&Path>,
    format: OutputFormat,
) -> Result<()> {
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

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
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
}
