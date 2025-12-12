//! Anchor API - list, get operations

use anyhow::Result;
use std::path::Path;

use crate::anchors::parse::{parse_file, Anchor};
use crate::backends::scan::scan_files;
use crate::core::model::ResultSet;
use crate::core::render::{OutputFormat, Renderer};

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
pub fn run_list(root: &Path, tag: Option<&str>, format: OutputFormat) -> Result<()> {
    let result_set = list_anchors(root, tag)?;

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Run anchor get command
pub fn run_get(
    root: &Path,
    id: &str,
    with_neighbors: Option<usize>,
    format: OutputFormat,
) -> Result<()> {
    let result_set = get_anchor(root, id, with_neighbors)?;

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
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
}
