//! Cache reader - Smart cache access with fallback to live scan
//!
//! Provides a unified interface for accessing files and anchors data,
//! preferring cache when valid, falling back to live scan when needed.

use anyhow::Result;
use std::path::Path;

use crate::anchors::api::list_anchors;
use crate::anchors::parse::{parse_file, Anchor};
use crate::backends::scan::{scan_files, ScanOptions};
use crate::cache::store::{is_cache_valid, read_cache_jsonl, ANCHORS_CACHE, FILES_CACHE};
use crate::core::model::ResultSet;
use crate::core::paths::cache_dir;

/// Get files list, preferring cache if valid
pub fn get_files_cached(root: &Path) -> Result<ResultSet> {
    // Try cache first
    if is_cache_valid(root) {
        let cache = cache_dir(root);
        if let Ok(items) = read_cache_jsonl(&cache, FILES_CACHE) {
            let mut result_set = ResultSet::new();
            for item in items {
                result_set.push(item);
            }
            return Ok(result_set);
        }
    }

    // Fall back to live scan
    let options = ScanOptions {
        file_type: Some("file".to_string()),
        ignore: true,
        ..Default::default()
    };
    scan_files(root, &options)
}

/// Get anchors list, preferring cache if valid
#[allow(dead_code)]
pub fn get_anchors_cached(root: &Path) -> Result<ResultSet> {
    // Try cache first
    if is_cache_valid(root) {
        let cache = cache_dir(root);
        if let Ok(items) = read_cache_jsonl(&cache, ANCHORS_CACHE) {
            let mut result_set = ResultSet::new();
            for item in items {
                result_set.push(item);
            }
            return Ok(result_set);
        }
    }

    // Fall back to live list
    list_anchors(root, None)
}

/// Get all anchors as parsed Anchor structs (more useful for flows)
/// This needs to parse files since cache only stores ResultItems
pub fn get_all_anchors_parsed(root: &Path) -> Result<Vec<(String, Anchor)>> {
    let files = get_files_cached(root)?;
    let mut all_anchors = Vec::new();

    for file_item in files.items {
        if let Some(path) = &file_item.path {
            let full_path = root.join(path);
            if full_path.exists() {
                let anchors = parse_file(&full_path, path);
                for anchor in anchors {
                    all_anchors.push((path.clone(), anchor));
                }
            }
        }
    }

    Ok(all_anchors)
}

/// Get anchors for a specific file
#[allow(dead_code)]
pub fn get_file_anchors(root: &Path, file_path: &str) -> Vec<Anchor> {
    let full_path = root.join(file_path);
    if full_path.exists() {
        parse_file(&full_path, file_path)
    } else {
        Vec::new()
    }
}

/// Find anchor by ID across all files
pub fn find_anchor_by_id(root: &Path, anchor_id: &str) -> Result<Option<(String, Anchor)>> {
    let files = get_files_cached(root)?;

    for file_item in files.items {
        if let Some(path) = &file_item.path {
            let full_path = root.join(path);
            if full_path.exists() {
                let anchors = parse_file(&full_path, path);
                for anchor in anchors {
                    if anchor.id == anchor_id {
                        return Ok(Some((path.clone(), anchor)));
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Find anchors by tag
#[allow(dead_code)]
pub fn find_anchors_by_tag(root: &Path, tag: &str) -> Result<Vec<(String, Anchor)>> {
    let files = get_files_cached(root)?;
    let mut results = Vec::new();

    for file_item in files.items {
        if let Some(path) = &file_item.path {
            let full_path = root.join(path);
            if full_path.exists() {
                let anchors = parse_file(&full_path, path);
                for anchor in anchors {
                    if anchor.tags.contains(&tag.to_string()) {
                        results.push((path.clone(), anchor));
                    }
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_files_cached_no_cache() {
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.txt"), "hello").unwrap();

        let result = get_files_cached(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert!(!files.items.is_empty());
    }

    #[test]
    fn test_get_anchors_cached_no_cache() {
        let temp = tempdir().unwrap();
        let content = "<!--Q:begin id=test v=1-->\nContent\n<!--Q:end id=test-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let result = get_anchors_cached(temp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_all_anchors_parsed() {
        let temp = tempdir().unwrap();
        let content = "<!--Q:begin id=anchor1 tags=tag1 v=1-->\nContent\n<!--Q:end id=anchor1-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let result = get_all_anchors_parsed(temp.path());
        assert!(result.is_ok());
        let anchors = result.unwrap();
        assert!(!anchors.is_empty());
        assert_eq!(anchors[0].1.id, "anchor1");
    }

    #[test]
    fn test_find_anchor_by_id() {
        let temp = tempdir().unwrap();
        let content = "<!--Q:begin id=target v=1-->\nContent\n<!--Q:end id=target-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let result = find_anchor_by_id(temp.path(), "target");
        assert!(result.is_ok());
        let found = result.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().1.id, "target");
    }

    #[test]
    fn test_find_anchor_by_id_not_found() {
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.md"), "no anchors here").unwrap();

        let result = find_anchor_by_id(temp.path(), "nonexistent");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_find_anchors_by_tag() {
        let temp = tempdir().unwrap();
        let content = r#"<!--Q:begin id=a1 tags=chapter v=1-->
A
<!--Q:end id=a1-->
<!--Q:begin id=a2 tags=chapter,intro v=1-->
B
<!--Q:end id=a2-->
<!--Q:begin id=a3 tags=other v=1-->
C
<!--Q:end id=a3-->
"#;
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let result = find_anchors_by_tag(temp.path(), "chapter");
        assert!(result.is_ok());
        let anchors = result.unwrap();
        assert_eq!(anchors.len(), 2);
    }

    #[test]
    fn test_get_file_anchors() {
        let temp = tempdir().unwrap();
        let content = "<!--Q:begin id=test v=1-->\nContent\n<!--Q:end id=test-->\n";
        std::fs::write(temp.path().join("doc.md"), content).unwrap();

        let anchors = get_file_anchors(temp.path(), "doc.md");
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].id, "test");
    }

    #[test]
    fn test_get_file_anchors_nonexistent() {
        let temp = tempdir().unwrap();
        let anchors = get_file_anchors(temp.path(), "nonexistent.md");
        assert!(anchors.is_empty());
    }
}
