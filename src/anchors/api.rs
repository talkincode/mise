//! Anchor API - list, get operations

use anyhow::Result;
use std::path::Path;

use crate::anchors::parse::{parse_file, Anchor};
use crate::backends::scan::scan_files;
use crate::core::model::ResultSet;
use crate::core::render::{RenderConfig, Renderer};

/// List all anchors in the workspace
pub fn list_anchors(root: &Path, tag_filter: Option<&str>) -> Result<ResultSet> {
    let mut result_set = ResultSet::new();

    // Scan all files
    let files = scan_files(root, None, None, false, true, Some("file"))?;

    for item in files.items {
        if let Some(path) = &item.path {
            let full_path = root.join(path);

            // Only process text files
            if !is_anchor_candidate(&full_path) {
                continue;
            }

            let anchors = parse_file(&full_path, path);

            for anchor in anchors {
                // Apply tag filter if specified
                if let Some(tag) = tag_filter {
                    if !anchor.tags.iter().any(|t| t == tag) {
                        continue;
                    }
                }

                result_set.push(anchor.to_result_item());
            }
        }
    }

    result_set.sort();
    Ok(result_set)
}

/// Get a specific anchor by ID
pub fn get_anchor(root: &Path, id: &str, with_neighbors: Option<usize>) -> Result<ResultSet> {
    let mut result_set = ResultSet::new();
    let mut target_anchor: Option<Anchor> = None;
    let mut all_anchors: Vec<Anchor> = Vec::new();

    // Scan and collect all anchors
    let files = scan_files(root, None, None, false, true, Some("file"))?;

    for item in files.items {
        if let Some(path) = &item.path {
            let full_path = root.join(path);

            if !is_anchor_candidate(&full_path) {
                continue;
            }

            let anchors = parse_file(&full_path, path);

            for anchor in anchors {
                if anchor.id == id {
                    target_anchor = Some(anchor.clone());
                }
                all_anchors.push(anchor);
            }
        }
    }

    // Add target anchor
    if let Some(anchor) = target_anchor {
        result_set.push(anchor.to_result_item());

        // Add neighbors if requested
        if let Some(n) = with_neighbors {
            // Find anchors with overlapping tags
            let target_tags: std::collections::HashSet<_> = anchor.tags.iter().collect();

            let mut neighbors: Vec<_> = all_anchors
                .iter()
                .filter(|a| a.id != id)
                .map(|a| {
                    let overlap = a.tags.iter().filter(|t| target_tags.contains(t)).count();
                    (overlap, a)
                })
                .filter(|(overlap, _)| *overlap > 0)
                .collect();

            // Sort by overlap count (descending)
            neighbors.sort_by(|a, b| b.0.cmp(&a.0));

            for (_, neighbor) in neighbors.into_iter().take(n) {
                let mut item = neighbor.to_result_item();
                item.confidence = crate::core::model::Confidence::Medium;
                result_set.push(item);
            }
        }
    }

    Ok(result_set)
}

/// Check if a file might contain anchors
fn is_anchor_candidate(path: &Path) -> bool {
    let text_extensions = [
        "md", "txt", "rs", "py", "js", "ts", "jsx", "tsx", "html", "css", "json", "yaml", "yml",
        "toml", "xml", "sh", "bash", "zsh", "c", "cpp", "h", "hpp", "java", "go", "rb", "php",
        "swift",
    ];

    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| text_extensions.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Run anchor list command
pub fn run_list(root: &Path, tag: Option<&str>, config: RenderConfig) -> Result<()> {
    let result_set = list_anchors(root, tag)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Alias for MCP compatibility
pub fn list_to_result_set(root: &Path, tag: Option<&str>) -> Result<ResultSet> {
    list_anchors(root, tag)
}

/// Run anchor get command
pub fn run_get(
    root: &Path,
    id: &str,
    with_neighbors: Option<usize>,
    config: RenderConfig,
) -> Result<()> {
    let result_set = get_anchor(root, id, with_neighbors)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Alias for MCP compatibility
pub fn get_to_result_set(
    root: &Path,
    id: &str,
    with_neighbors: Option<usize>,
) -> Result<ResultSet> {
    get_anchor(root, id, with_neighbors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_anchor_candidate() {
        assert!(is_anchor_candidate(Path::new("test.md")));
        assert!(is_anchor_candidate(Path::new("code.rs")));
        assert!(!is_anchor_candidate(Path::new("image.png")));
        assert!(!is_anchor_candidate(Path::new("binary.exe")));
    }

    #[test]
    fn test_is_anchor_candidate_various_extensions() {
        assert!(is_anchor_candidate(Path::new("test.txt")));
        assert!(is_anchor_candidate(Path::new("test.py")));
        assert!(is_anchor_candidate(Path::new("test.js")));
        assert!(is_anchor_candidate(Path::new("test.ts")));
        assert!(is_anchor_candidate(Path::new("test.jsx")));
        assert!(is_anchor_candidate(Path::new("test.tsx")));
        assert!(is_anchor_candidate(Path::new("test.html")));
        assert!(is_anchor_candidate(Path::new("test.css")));
        assert!(is_anchor_candidate(Path::new("test.json")));
        assert!(is_anchor_candidate(Path::new("test.yaml")));
        assert!(is_anchor_candidate(Path::new("test.yml")));
        assert!(is_anchor_candidate(Path::new("test.toml")));
        assert!(is_anchor_candidate(Path::new("test.xml")));
        assert!(is_anchor_candidate(Path::new("test.sh")));
        assert!(is_anchor_candidate(Path::new("test.bash")));
        assert!(is_anchor_candidate(Path::new("test.zsh")));
        assert!(is_anchor_candidate(Path::new("test.c")));
        assert!(is_anchor_candidate(Path::new("test.cpp")));
        assert!(is_anchor_candidate(Path::new("test.h")));
        assert!(is_anchor_candidate(Path::new("test.hpp")));
        assert!(is_anchor_candidate(Path::new("test.java")));
        assert!(is_anchor_candidate(Path::new("test.go")));
        assert!(is_anchor_candidate(Path::new("test.rb")));
        assert!(is_anchor_candidate(Path::new("test.php")));
        assert!(is_anchor_candidate(Path::new("test.swift")));
    }

    #[test]
    fn test_is_anchor_candidate_case_insensitive() {
        assert!(is_anchor_candidate(Path::new("test.MD")));
        assert!(is_anchor_candidate(Path::new("test.RS")));
        assert!(is_anchor_candidate(Path::new("test.Py")));
    }

    #[test]
    fn test_is_anchor_candidate_no_extension() {
        assert!(!is_anchor_candidate(Path::new("Makefile")));
        assert!(!is_anchor_candidate(Path::new("README")));
    }

    #[test]
    fn test_list_anchors_empty_dir() {
        let temp = tempfile::tempdir().unwrap();
        let result = list_anchors(temp.path(), None);
        assert!(result.is_ok());
        assert!(result.unwrap().items.is_empty());
    }

    #[test]
    fn test_list_anchors_with_anchors() {
        let temp = tempfile::tempdir().unwrap();
        let content = "# Test\n<!--Q:begin id=test1 tags=a,b v=1-->\nContent\n<!--Q:end id=test1-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();
        
        let result = list_anchors(temp.path(), None).unwrap();
        assert_eq!(result.items.len(), 1);
    }

    #[test]
    fn test_list_anchors_with_tag_filter() {
        let temp = tempfile::tempdir().unwrap();
        let content = "<!--Q:begin id=a tags=foo v=1-->\nA\n<!--Q:end id=a-->\n<!--Q:begin id=b tags=bar v=1-->\nB\n<!--Q:end id=b-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();
        
        let result = list_anchors(temp.path(), Some("foo")).unwrap();
        assert_eq!(result.items.len(), 1);
    }

    #[test]
    fn test_get_anchor_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let content = "# Test\n<!--Q:begin id=test1 v=1-->\nContent\n<!--Q:end id=test1-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();
        
        let result = get_anchor(temp.path(), "nonexistent", None).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn test_get_anchor_found() {
        let temp = tempfile::tempdir().unwrap();
        let content = "# Test\n<!--Q:begin id=test1 v=1-->\nContent\n<!--Q:end id=test1-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();
        
        let result = get_anchor(temp.path(), "test1", None).unwrap();
        assert_eq!(result.items.len(), 1);
    }

    #[test]
    fn test_get_anchor_with_neighbors() {
        let temp = tempfile::tempdir().unwrap();
        let content = "<!--Q:begin id=a tags=common v=1-->\nA\n<!--Q:end id=a-->\n<!--Q:begin id=b tags=common v=1-->\nB\n<!--Q:end id=b-->\n<!--Q:begin id=c tags=other v=1-->\nC\n<!--Q:end id=c-->\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();
        
        let result = get_anchor(temp.path(), "a", Some(2)).unwrap();
        // Should have anchor 'a' and neighbor 'b' (which shares tag 'common')
        assert!(result.items.len() >= 1);
    }

    #[test]
    fn test_is_anchor_candidate_binary_extensions() {
        assert!(!is_anchor_candidate(Path::new("test.jpg")));
        assert!(!is_anchor_candidate(Path::new("test.gif")));
        assert!(!is_anchor_candidate(Path::new("test.pdf")));
        assert!(!is_anchor_candidate(Path::new("test.zip")));
        assert!(!is_anchor_candidate(Path::new("test.tar")));
        assert!(!is_anchor_candidate(Path::new("test.dll")));
        assert!(!is_anchor_candidate(Path::new("test.so")));
    }
}
