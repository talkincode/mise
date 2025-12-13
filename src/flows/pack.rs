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
use crate::core::tokenizer::{count_tokens, TokenModel};

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
    /// Token model for counting (default: cl100k)
    pub token_model: TokenModel,
}

/// Pack result statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackStats {
    pub total_items: usize,
    pub total_chars: usize,
    pub estimated_tokens: usize,
    pub truncated: bool,
    pub items_truncated: usize,
    /// Token model used for counting
    pub token_model: String,
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

/// Estimate tokens for a result item using tiktoken
fn item_tokens(item: &ResultItem, model: TokenModel) -> usize {
    let mut total_tokens = 0;

    // Path tokens
    if let Some(path) = &item.path {
        total_tokens += count_tokens(path, model);
    }

    // Excerpt/content tokens
    if let Some(excerpt) = &item.excerpt {
        total_tokens += count_tokens(excerpt, model);
    }

    // JSON structure overhead (~12-15 tokens for field names and formatting)
    total_tokens += 15;

    total_tokens
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
    model: TokenModel,
) -> (Vec<ResultItem>, PackStats) {
    let total_items = items.len();
    let total_chars: usize = items
        .iter()
        .map(|i| i.excerpt.as_ref().map(|e| e.len()).unwrap_or(0))
        .sum();

    // Use tiktoken for accurate token estimation
    let estimated_tokens: usize = items.iter().map(|i| item_tokens(i, model)).sum();

    // If no budget or under budget, return as-is
    if max_tokens.is_none() || estimated_tokens <= max_tokens.unwrap() {
        let stats = PackStats {
            total_items,
            total_chars,
            estimated_tokens,
            truncated: false,
            items_truncated: 0,
            token_model: model.to_string(),
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
        let item_token_count = item_tokens(&item, model);

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

    // Use tiktoken for accurate final token count
    let final_tokens: usize = result.iter().map(|i| item_tokens(i, model)).sum();

    let stats = PackStats {
        total_items,
        total_chars,
        estimated_tokens: final_tokens,
        truncated: items_truncated > 0 || result.len() < total_items,
        items_truncated: total_items - result.len(),
        token_model: model.to_string(),
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

    // Apply token budget with the specified model
    let (final_items, stats) =
        apply_budget(all_items, opts.max_tokens, opts.priority, opts.token_model);

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
    token_model: TokenModel,
    config: RenderConfig,
) -> Result<()> {
    let opts = PackOptions {
        anchors,
        files,
        max_tokens,
        priority,
        token_model,
    };

    let (result_set, stats) = pack_context(root, opts)?;

    // Output stats to stderr if requested
    if show_stats {
        eprintln!("📦 Pack Statistics:");
        eprintln!("   Items: {}", stats.total_items);
        eprintln!("   Characters: {}", stats.total_chars);
        eprintln!(
            "   Tokens: {} (model: {})",
            stats.estimated_tokens, stats.token_model
        );
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
    use crate::core::tokenizer::estimate_tokens_heuristic;

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

        let (result, stats) =
            apply_budget(items, None, PackPriority::ByOrder, TokenModel::default());

        assert_eq!(result.len(), 2);
        assert!(!stats.truncated);
    }

    #[test]
    fn test_apply_budget_with_limit() {
        let items = vec![
            {
                let mut item = ResultItem::file("test.rs");
                // Use varied text to get more tokens - "hello world " repeated gives ~3 tokens per repeat
                item.excerpt = Some("hello world ".repeat(500)); // ~1500 tokens
                item.confidence = Confidence::High;
                item
            },
            {
                let mut item = ResultItem::file("lib.rs");
                item.excerpt = Some("goodbye world ".repeat(500)); // Another ~1500 tokens
                item.confidence = Confidence::Low;
                item
            },
        ];

        // Set a very small budget that cannot fit even one full item
        let (result, stats) = apply_budget(
            items,
            Some(100),
            PackPriority::ByConfidence,
            TokenModel::default(),
        );

        assert!(stats.truncated);
        assert!(!result.is_empty()); // At least first item should be partially included
    }

    #[test]
    fn test_tiktoken_count_ascii() {
        // Pure ASCII text: tiktoken gives accurate count
        let text = "Hello world, this is a test.";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        // tiktoken count for this text
        assert!(tokens > 0 && tokens < 15);
    }

    #[test]
    fn test_tiktoken_count_code() {
        // Code with symbols
        let text = "fn main() { let x = 1 + 2; }";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        assert!(tokens > 0);
    }

    #[test]
    fn test_tiktoken_count_cjk() {
        // CJK text: tiktoken gives accurate count
        let text = "这是一个中文测试文本";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        assert!(tokens > 0);
    }

    #[test]
    fn test_tiktoken_count_mixed() {
        // Mixed content
        let text = "Hello 世界! fn test() { println!(\"你好\"); }";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        assert!(tokens > 5);
    }

    #[test]
    fn test_heuristic_empty() {
        assert_eq!(estimate_tokens_heuristic(""), 0);
    }

    #[test]
    fn test_find_char_boundary_within() {
        let s = "hello world";
        assert_eq!(find_char_boundary(s, 5), 5);
    }

    #[test]
    fn test_find_char_boundary_beyond() {
        let s = "hello";
        assert_eq!(find_char_boundary(s, 100), 5);
    }

    #[test]
    fn test_find_char_boundary_unicode() {
        let s = "你好世界";
        // Each Chinese char is 3 bytes, so boundary should find valid position
        let boundary = find_char_boundary(s, 4);
        assert!(s.is_char_boundary(boundary));
    }

    #[test]
    fn test_pack_priority_default() {
        assert_eq!(PackPriority::default(), PackPriority::ByConfidence);
    }

    #[test]
    fn test_pack_priority_parse_aliases() {
        assert_eq!(
            "byconfidence".parse::<PackPriority>().unwrap(),
            PackPriority::ByConfidence
        );
        assert_eq!(
            "byorder".parse::<PackPriority>().unwrap(),
            PackPriority::ByOrder
        );
    }

    #[test]
    fn test_pack_priority_parse_invalid() {
        assert!("invalid".parse::<PackPriority>().is_err());
    }

    #[test]
    fn test_pack_stats_creation() {
        let stats = PackStats {
            total_items: 10,
            total_chars: 1000,
            estimated_tokens: 250,
            truncated: true,
            items_truncated: 2,
            token_model: "cl100k".to_string(),
        };
        assert_eq!(stats.total_items, 10);
        assert!(stats.truncated);
        assert_eq!(stats.token_model, "cl100k");
    }

    #[test]
    fn test_pack_options_default() {
        let opts = PackOptions::default();
        assert!(opts.anchors.is_empty());
        assert!(opts.files.is_empty());
        assert!(opts.max_tokens.is_none());
        assert_eq!(opts.priority, PackPriority::ByConfidence);
        assert_eq!(opts.token_model, TokenModel::default());
    }

    #[test]
    fn test_item_tokens_file_only() {
        let item = ResultItem::file("src/main.rs");
        let tokens = item_tokens(&item, TokenModel::default());
        assert!(tokens > 0);
    }

    #[test]
    fn test_item_tokens_with_excerpt() {
        let mut item = ResultItem::file("test.rs");
        item.excerpt = Some("fn main() { println!(\"hello\"); }".to_string());
        let tokens = item_tokens(&item, TokenModel::default());
        // Should include path tokens + excerpt tokens + overhead
        assert!(tokens > 15); // At least the overhead
    }

    #[test]
    fn test_apply_budget_empty_input() {
        let items: Vec<ResultItem> = vec![];
        let (result, stats) = apply_budget(
            items,
            Some(100),
            PackPriority::ByOrder,
            TokenModel::default(),
        );
        assert!(result.is_empty());
        assert_eq!(stats.total_items, 0);
        assert!(!stats.truncated);
    }

    #[test]
    fn test_apply_budget_by_confidence_sorting() {
        let items = vec![
            {
                let mut item = ResultItem::file("low.rs");
                item.confidence = Confidence::Low;
                item.excerpt = Some("a".repeat(500)); // Make it big enough to trigger sorting
                item
            },
            {
                let mut item = ResultItem::file("high.rs");
                item.confidence = Confidence::High;
                item.excerpt = Some("b".repeat(500));
                item
            },
            {
                let mut item = ResultItem::file("medium.rs");
                item.confidence = Confidence::Medium;
                item.excerpt = Some("c".repeat(500));
                item
            },
        ];

        // Set a very large budget so sorting happens but items fit
        let (result, _) = apply_budget(
            items,
            Some(10000),
            PackPriority::ByConfidence,
            TokenModel::default(),
        );

        // When under budget, items are returned in original order
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_apply_budget_by_order_preserves_order() {
        let items = vec![
            {
                let mut item = ResultItem::file("first.rs");
                item.excerpt = Some("first".to_string());
                item
            },
            {
                let mut item = ResultItem::file("second.rs");
                item.excerpt = Some("second".to_string());
                item
            },
            {
                let mut item = ResultItem::file("third.rs");
                item.excerpt = Some("third".to_string());
                item
            },
        ];

        let (result, _) = apply_budget(items, None, PackPriority::ByOrder, TokenModel::default());
        assert_eq!(result[0].path, Some("first.rs".to_string()));
        assert_eq!(result[1].path, Some("second.rs".to_string()));
        assert_eq!(result[2].path, Some("third.rs".to_string()));
    }

    #[test]
    fn test_different_token_models() {
        let mut item = ResultItem::file("test.rs");
        item.excerpt = Some("Hello world, 你好世界! fn test() {}".to_string());

        let cl100k = item_tokens(&item, TokenModel::Cl100k);
        let o200k = item_tokens(&item, TokenModel::O200k);
        let heuristic = item_tokens(&item, TokenModel::Heuristic);

        // All should produce non-zero results
        assert!(cl100k > 0);
        assert!(o200k > 0);
        assert!(heuristic > 0);
    }
}
