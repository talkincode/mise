//! Writing flow - Gather evidence for writing tasks
//!
//! Steps:
//! 1. Get the specified anchor as primary evidence (high confidence)
//! 2. Find related anchors by shared tags (medium confidence)
//! 3. Use ripgrep to find additional relevant content (low/medium confidence)

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::anchors::api::get_anchor;
use crate::anchors::parse::parse_file;
use crate::backends::rg::run_rg;
use crate::backends::scan::scan_files;
use crate::core::model::{Confidence, ResultSet};
use crate::core::render::{OutputFormat, Renderer};

/// Run the writing flow
pub fn run_writing(
    root: &Path,
    anchor_id: &str,
    max_items: usize,
    format: OutputFormat,
) -> Result<()> {
    let result_set = gather_writing_evidence(root, anchor_id, max_items)?;

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Gather evidence for a writing task
pub fn gather_writing_evidence(
    root: &Path,
    anchor_id: &str,
    max_items: usize,
) -> Result<ResultSet> {
    let mut result_set = ResultSet::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // Step 1: Get the primary anchor (high confidence)
    let primary = get_anchor(root, anchor_id, None)?;

    let mut primary_tags: Vec<String> = Vec::new();
    let mut primary_content: Option<String> = None;

    for item in primary.items {
        if let Some(path) = &item.path {
            seen_paths.insert(path.clone());
        }
        primary_content = item.excerpt.clone();

        // Extract tags from the anchor (we need to re-parse to get tags)
        // For now, we'll use the content for keyword extraction
        result_set.push(item);
    }

    // Find the anchor to get its tags
    let files = scan_files(root, None, None, false, true, Some("file"))?;
    for file_item in files.items {
        if let Some(path) = &file_item.path {
            let full_path = root.join(path);
            let anchors = parse_file(&full_path, path);

            for anchor in anchors {
                if anchor.id == anchor_id {
                    primary_tags = anchor.tags.clone();
                    break;
                }
            }
        }
        if !primary_tags.is_empty() {
            break;
        }
    }

    // Step 2: Find related anchors by shared tags (medium confidence)
    if !primary_tags.is_empty() {
        let files = scan_files(root, None, None, false, true, Some("file"))?;
        let mut related_count = 0;

        'outer: for file_item in files.items {
            if let Some(path) = &file_item.path {
                if seen_paths.contains(path) {
                    continue;
                }

                let full_path = root.join(path);
                let anchors = parse_file(&full_path, path);

                for anchor in anchors {
                    if anchor.id == anchor_id {
                        continue;
                    }

                    // Check for shared tags
                    let shared_tags: Vec<_> = anchor
                        .tags
                        .iter()
                        .filter(|t| primary_tags.contains(t))
                        .collect();

                    if !shared_tags.is_empty() {
                        let mut item = anchor.to_result_item();
                        item.confidence = Confidence::Medium;

                        if let Some(path) = &item.path {
                            seen_paths.insert(path.clone());
                        }

                        result_set.push(item);
                        related_count += 1;

                        if related_count >= max_items / 2 {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    // Step 3: Search for additional content using ripgrep (low confidence)
    // Extract keywords from primary content for searching
    if let Some(content) = primary_content {
        // Simple keyword extraction: find significant words
        let keywords: Vec<_> = content
            .split_whitespace()
            .filter(|w| w.len() > 4)
            .filter(|w| !is_common_word(w))
            .take(3)
            .collect();

        if !keywords.is_empty() {
            let pattern = keywords.join("|");
            let search_results = run_rg(root, &pattern, &[] as &[&Path])?;

            let mut search_count = 0;
            for mut item in search_results.items {
                if let Some(path) = &item.path {
                    if seen_paths.contains(path) {
                        continue;
                    }
                    seen_paths.insert(path.clone());
                }

                item.confidence = Confidence::Low;
                result_set.push(item);
                search_count += 1;

                if search_count >= max_items / 2 {
                    break;
                }
            }
        }
    }

    Ok(result_set)
}

/// Check if a word is a common word that shouldn't be searched
fn is_common_word(word: &str) -> bool {
    const COMMON: &[&str] = &[
        "the", "and", "that", "this", "with", "from", "have", "been", "were", "will", "would",
        "could", "should", "about", "which", "their", "there", "these", "those", "other", "into",
        "some", "than", "then", "when", "what", "where", "while", "after", "before", "between",
        "through", "during", "without", "within",
    ];
    COMMON.contains(&word.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_common_word() {
        assert!(is_common_word("the"));
        assert!(is_common_word("The"));
        assert!(is_common_word("about"));
        assert!(!is_common_word("function"));
        assert!(!is_common_word("implement"));
    }
}
