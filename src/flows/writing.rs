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
use crate::core::render::{RenderConfig, Renderer};

/// Run the writing flow
pub fn run_writing(
    root: &Path,
    anchor_id: &str,
    max_items: usize,
    config: RenderConfig,
) -> Result<()> {
    let result_set = gather_writing_evidence(root, anchor_id, max_items)?;

    let renderer = Renderer::with_config(config);
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
        // Smart keyword extraction: supports both English and Chinese
        let keywords = extract_keywords(&content, 5);

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

/// Check if a word is a common English word that shouldn't be searched
pub fn is_common_word(word: &str) -> bool {
    const COMMON: &[&str] = &[
        "the", "and", "that", "this", "with", "from", "have", "been", "were", "will", "would",
        "could", "should", "about", "which", "their", "there", "these", "those", "other", "into",
        "some", "than", "then", "when", "what", "where", "while", "after", "before", "between",
        "through", "during", "without", "within", "also", "just", "only", "very", "more", "most",
        "such", "each", "every", "both", "many", "much", "any", "all", "own", "same",
    ];
    COMMON.contains(&word.to_lowercase().as_str())
}

/// Check if a character is CJK (Chinese/Japanese/Korean)
#[inline]
fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)      // CJK Unified Ideographs
        || (0x3400..=0x4DBF).contains(&cp)  // CJK Extension A
        || (0x3000..=0x303F).contains(&cp)  // CJK Symbols and Punctuation
        || (0x3040..=0x309F).contains(&cp)  // Hiragana
        || (0x30A0..=0x30FF).contains(&cp)  // Katakana
        || (0xAC00..=0xD7AF).contains(&cp)  // Hangul Syllables
        || (0xFF00..=0xFFEF).contains(&cp) // Fullwidth Forms
}

/// Check if a CJK character is a common stop word (punctuation, particles)
#[inline]
fn is_cjk_stop_char(c: char) -> bool {
    // Common Chinese punctuation and particles
    const CJK_STOPS: &[char] = &[
        '的', '了', '是', '在', '我', '有', '和', '就', '不', '人', '都', '一', '这', '中', '大',
        '为', '上', '个', '到', '说', '们', '会', '着', '也', '很', '把', '那', '你', '他', '她',
        '它', '与', '及', '或', '等', '之', '于', '而', '以', '其',
        // Punctuation (using Unicode escapes for problematic chars)
        '，', '。', '！', '？', '、', '；', '：', '"', '"', '（', '）', '【', '】', '《', '》', '—',
        '…', '·',
    ];
    CJK_STOPS.contains(&c)
}

/// Extract keywords from text, supporting both English and CJK content
pub fn extract_keywords(text: &str, max_keywords: usize) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Extract English words (at least 4 chars, not common)
    let english_words: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 4)
        .filter(|w| !is_common_word(w))
        .collect();

    for word in english_words {
        let lower = word.to_lowercase();
        if !seen.contains(&lower) && keywords.len() < max_keywords {
            seen.insert(lower.clone());
            keywords.push(word.to_string());
        }
    }

    // Extract CJK n-grams (2-4 character phrases)
    let cjk_chars: Vec<char> = text
        .chars()
        .filter(|c| is_cjk_char(*c) && !is_cjk_stop_char(*c))
        .collect();

    // Extract 2-grams, 3-grams, and 4-grams
    for n in [3, 2, 4] {
        if cjk_chars.len() >= n {
            for window in cjk_chars.windows(n) {
                let ngram: String = window.iter().collect();
                if !seen.contains(&ngram) && keywords.len() < max_keywords {
                    // Skip if it's mostly stop characters
                    let stop_count = window.iter().filter(|c| is_cjk_stop_char(**c)).count();
                    if stop_count < n / 2 {
                        seen.insert(ngram.clone());
                        keywords.push(ngram);
                    }
                }
            }
        }
    }

    keywords.truncate(max_keywords);
    keywords
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

    #[test]
    fn test_extract_keywords_english() {
        let text = "This is a function that implements the core logic";
        let keywords = extract_keywords(text, 3);
        assert!(keywords.contains(&"function".to_string()));
        assert!(keywords.contains(&"implements".to_string()));
        assert!(keywords.contains(&"logic".to_string()) || keywords.contains(&"core".to_string()));
    }

    #[test]
    fn test_extract_keywords_chinese() {
        let text = "这是一个关于上下文准备工具的说明文档";
        let keywords = extract_keywords(text, 5);
        // Should extract n-grams like "上下文", "准备工具", "说明文档"
        assert!(!keywords.is_empty());
        assert!(keywords.iter().any(|k| k.chars().all(|c| is_cjk_char(c))));
    }

    #[test]
    fn test_extract_keywords_mixed() {
        let text = "mise 是一个上下文准备工具 for AI agents";
        let keywords = extract_keywords(text, 5);
        // Should have both English and Chinese keywords
        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_is_cjk_char() {
        assert!(is_cjk_char('中'));
        assert!(is_cjk_char('文'));
        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char('1'));
    }
}
