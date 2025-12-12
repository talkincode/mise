//! Context packing flow - Bundle anchors and files for AI context
//!
//! Combines multiple anchors and files into a single context package
//! with optional token budget control.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::anchors::api::get_anchor;
use crate::core::model::{Confidence, Kind, Meta, Range, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};

/// Priority mode for truncation when over budget
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackPriority {
    /// Prioritize by confidence level (high > medium > low)
    #[default]
    ByConfidence,
    /// Keep items in the order they were specified
    ByOrder,
}

impl std::str::FromStr for PackPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "confidence" | "byconfidence" => Ok(PackPriority::ByConfidence),
            "order" | "byorder" => Ok(PackPriority::ByOrder),
            _ => Err(format!("Unknown priority mode: {}", s)),
        }
    }
}

/// Options for pack command
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Anchor IDs to include
    pub anchors: Vec<String>,
    /// File paths to include
    pub files: Vec<String>,
    /// Maximum tokens (estimated as chars / 4)
    pub max_tokens: Option<usize>,
    /// Priority mode for truncation
    pub priority: PackPriority,
}

/// Pack result statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackStats {
    pub total_items: usize,
    pub total_chars: usize,
    pub estimated_tokens: usize,
    pub truncated: bool,
    pub items_truncated: usize,
}

/// Find a valid UTF-8 character boundary at or before the given byte index
fn find_char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }

    // Start from max_bytes and work backwards to find a char boundary
    let mut pos = max_bytes;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Estimate token count from character count
/// Uses a simple heuristic: ~4 characters per token for code
fn estimate_tokens(chars: usize) -> usize {
    (chars + 3) / 4 // Round up
}

/// Estimate tokens for a result item
fn item_tokens(item: &ResultItem) -> usize {
    let mut chars = 0;

    // Path
    if let Some(path) = &item.path {
        chars += path.len();
    }

    // Excerpt/content
    if let Some(excerpt) = &item.excerpt {
        chars += excerpt.len();
    }

    // Some overhead for JSON structure
    chars += 50;

    estimate_tokens(chars)
}

/// Collect anchor content
fn collect_anchors(root: &Path, anchor_ids: &[String]) -> Result<Vec<ResultItem>> {
    let mut items = Vec::new();

    for anchor_id in anchor_ids {
        match get_anchor(root, anchor_id, None) {
            Ok(result_set) => {
                for item in result_set.items {
                    items.push(item);
                }
            }
            Err(e) => {
                // Add an error item for missing anchor
                let mut error_item = ResultItem::file(format!("anchor:{}", anchor_id));
                error_item.kind = Kind::Anchor;
                error_item.confidence = Confidence::Low;
                error_item.excerpt = Some(format!("Error: {}", e));
                items.push(error_item);
            }
        }
    }

    Ok(items)
}

/// Collect file content
fn collect_files(root: &Path, file_paths: &[String]) -> Result<Vec<ResultItem>> {
    let mut items = Vec::new();

    for file_path in file_paths {
        let full_path = root.join(file_path);

        if !full_path.exists() {
            // Add error item for missing file
            let mut error_item = ResultItem::file(file_path);
            error_item.kind = Kind::File;
            error_item.confidence = Confidence::Low;
            error_item.excerpt = Some("Error: File not found".to_string());
            items.push(error_item);
            continue;
        }

        // Read file content
        match fs::read_to_string(&full_path) {
            Ok(content) => {
                let line_count = content.lines().count() as u32;
                let range = Range::lines(1, line_count.max(1));

                let mut item = ResultItem::extract(file_path.clone(), range, content);
                item.kind = Kind::File;
                item.confidence = Confidence::High;
                item.source_mode = SourceMode::Scan;

                // Add file metadata
                if let Ok(metadata) = full_path.metadata() {
                    item.meta = Meta {
                        size: Some(metadata.len()),
                        ..Default::default()
                    };
                }

                items.push(item);
            }
            Err(e) => {
                let mut error_item = ResultItem::file(file_path);
                error_item.kind = Kind::File;
                error_item.confidence = Confidence::Low;
                error_item.excerpt = Some(format!("Error reading file: {}", e));
                items.push(error_item);
            }
        }
    }

    Ok(items)
}

/// Apply token budget and truncate if necessary
fn apply_budget(
    items: Vec<ResultItem>,
    max_tokens: Option<usize>,
    priority: PackPriority,
) -> (Vec<ResultItem>, PackStats) {
    let total_items = items.len();
    let total_chars: usize = items
        .iter()
        .map(|i| i.excerpt.as_ref().map(|e| e.len()).unwrap_or(0))
        .sum();
    let estimated_tokens = estimate_tokens(total_chars);

    // If no budget or under budget, return as-is
    if max_tokens.is_none() || estimated_tokens <= max_tokens.unwrap() {
        let stats = PackStats {
            total_items,
            total_chars,
            estimated_tokens,
            truncated: false,
            items_truncated: 0,
        };
        return (items, stats);
    }

    let budget = max_tokens.unwrap();

    // Sort items by priority if needed
    let mut sorted_items = items;
    if priority == PackPriority::ByConfidence {
        sorted_items.sort_by(|a, b| {
            // High confidence first
            let conf_order = |c: &Confidence| match c {
                Confidence::High => 0,
                Confidence::Medium => 1,
                Confidence::Low => 2,
            };
            conf_order(&a.confidence).cmp(&conf_order(&b.confidence))
        });
    }

    // Include items until we hit the budget
    let mut result = Vec::new();
    let mut current_tokens = 0;
    let mut items_truncated = 0;

    for item in sorted_items {
        let item_token_count = item_tokens(&item);

        if current_tokens + item_token_count <= budget {
            current_tokens += item_token_count;
            result.push(item);
        } else {
            // Try to include a truncated version of the item
            let remaining_tokens = budget.saturating_sub(current_tokens);
            let remaining_chars = remaining_tokens * 4;

            if remaining_chars > 100 {
                // Only include if we can fit at least 100 chars
                if let Some(excerpt) = &item.excerpt {
                    if excerpt.len() > remaining_chars {
                        let mut truncated_item = item.clone();
                        // Find a valid UTF-8 boundary for truncation
                        let truncate_at = find_char_boundary(excerpt, remaining_chars);
                        truncated_item.excerpt =
                            Some(format!("{}...[truncated]", &excerpt[..truncate_at]));
                        truncated_item.meta.truncated = true;
                        result.push(truncated_item);
                        items_truncated += 1;
                    } else {
                        result.push(item);
                    }
                }
            }
            break;
        }
    }

    let final_chars: usize = result
        .iter()
        .map(|i| i.excerpt.as_ref().map(|e| e.len()).unwrap_or(0))
        .sum();

    let stats = PackStats {
        total_items,
        total_chars,
        estimated_tokens: estimate_tokens(final_chars),
        truncated: items_truncated > 0 || result.len() < total_items,
        items_truncated: total_items - result.len(),
    };

    (result, stats)
}

/// Pack anchors and files into a context bundle
pub fn pack_context(root: &Path, opts: PackOptions) -> Result<(ResultSet, PackStats)> {
    let mut all_items = Vec::new();

    // Collect anchors first (higher priority)
    let anchor_items = collect_anchors(root, &opts.anchors)?;
    all_items.extend(anchor_items);

    // Then collect files
    let file_items = collect_files(root, &opts.files)?;
    all_items.extend(file_items);

    // Apply token budget
    let (final_items, stats) = apply_budget(all_items, opts.max_tokens, opts.priority);

    let mut result_set = ResultSet::new();
    for item in final_items {
        result_set.push(item);
    }

    Ok((result_set, stats))
}

/// Run the pack command
pub fn run_pack(
    root: &Path,
    anchors: Vec<String>,
    files: Vec<String>,
    max_tokens: Option<usize>,
    priority: PackPriority,
    show_stats: bool,
    config: RenderConfig,
) -> Result<()> {
    let opts = PackOptions {
        anchors,
        files,
        max_tokens,
        priority,
    };

    let (result_set, stats) = pack_context(root, opts)?;

    // Output stats to stderr if requested
    if show_stats {
        eprintln!("📦 Pack Statistics:");
        eprintln!("   Items: {}", stats.total_items);
        eprintln!("   Characters: {}", stats.total_chars);
        eprintln!("   Estimated tokens: {}", stats.estimated_tokens);
        if stats.truncated {
            eprintln!("   ⚠️  Truncated: {} items dropped", stats.items_truncated);
        }
        eprintln!();
    }

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(4), 1);
        assert_eq!(estimate_tokens(8), 2);
        assert_eq!(estimate_tokens(100), 25);
        assert_eq!(estimate_tokens(101), 26); // Rounds up
    }

    #[test]
    fn test_pack_priority_parse() {
        assert_eq!(
            "confidence".parse::<PackPriority>().unwrap(),
            PackPriority::ByConfidence
        );
        assert_eq!(
            "order".parse::<PackPriority>().unwrap(),
            PackPriority::ByOrder
        );
    }

    #[test]
    fn test_apply_budget_no_limit() {
        let items = vec![
            {
                let mut item = ResultItem::file("test.rs");
                item.excerpt = Some("fn main() {}".to_string());
                item
            },
            {
                let mut item = ResultItem::file("lib.rs");
                item.excerpt = Some("pub mod core;".to_string());
                item
            },
        ];

        let (result, stats) = apply_budget(items, None, PackPriority::ByOrder);

        assert_eq!(result.len(), 2);
        assert!(!stats.truncated);
    }

    #[test]
    fn test_apply_budget_with_limit() {
        let items = vec![
            {
                let mut item = ResultItem::file("test.rs");
                item.excerpt = Some("a".repeat(1000)); // 1000 chars = ~250 tokens
                item.confidence = Confidence::High;
                item
            },
            {
                let mut item = ResultItem::file("lib.rs");
                item.excerpt = Some("b".repeat(1000)); // Another 250 tokens
                item.confidence = Confidence::Low;
                item
            },
        ];

        // Only allow 300 tokens - first item uses 250, second item gets truncated
        // (50 remaining tokens = 200 chars, which is > 100 so it includes truncated version)
        let (result, stats) = apply_budget(items, Some(300), PackPriority::ByConfidence);

        assert_eq!(result.len(), 2); // Both included, second one truncated
        assert!(stats.truncated);
        assert!(result[1].meta.truncated); // Second item is truncated
    }
}
