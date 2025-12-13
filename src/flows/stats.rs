//! Statistics flow - Project statistics for writing projects
//!
//! Provides word count, character count, anchor statistics, and token estimates.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::anchors::parse::parse_file;
use crate::backends::scan::scan_files;
use crate::core::model::{Confidence, Kind, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};

/// Statistics for a single file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileStats {
    /// File path relative to root
    pub path: String,
    /// Total characters (including whitespace)
    pub chars: usize,
    /// Total characters (excluding whitespace)
    pub chars_no_space: usize,
    /// Word count (English words)
    pub words: usize,
    /// CJK character count
    pub cjk_chars: usize,
    /// Line count
    pub lines: usize,
    /// Estimated token count
    pub tokens: usize,
    /// Number of anchors in this file
    pub anchors: usize,
}

/// Project-wide statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectStats {
    /// Total files scanned
    pub total_files: usize,
    /// Total characters
    pub total_chars: usize,
    /// Total characters (excluding whitespace)
    pub total_chars_no_space: usize,
    /// Total words (English)
    pub total_words: usize,
    /// Total CJK characters
    pub total_cjk_chars: usize,
    /// Total lines
    pub total_lines: usize,
    /// Estimated total tokens
    pub total_tokens: usize,
    /// Total anchors
    pub total_anchors: usize,
    /// Anchor count by tag
    pub anchors_by_tag: HashMap<String, usize>,
    /// Per-file statistics (top files by size)
    pub file_stats: Vec<FileStats>,
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

/// Check if a character is a code symbol
#[inline]
fn is_code_symbol(c: char) -> bool {
    matches!(
        c,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '&'
            | '|'
            | '^'
            | '!'
            | '~'
            | '?'
            | ':'
            | ';'
            | ','
            | '.'
            | '@'
            | '#'
            | '$'
            | '\\'
            | '"'
            | '\''
            | '`'
    )
}

/// Estimate tokens using smart analysis
fn estimate_tokens_smart(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let mut ascii_chars = 0usize;
    let mut cjk_chars = 0usize;
    let mut other_unicode = 0usize;
    let mut whitespace = 0usize;
    let mut code_symbols = 0usize;

    for c in text.chars() {
        if c.is_ascii_whitespace() {
            whitespace += 1;
        } else if c.is_ascii() {
            if is_code_symbol(c) {
                code_symbols += 1;
            } else {
                ascii_chars += 1;
            }
        } else if is_cjk_char(c) {
            cjk_chars += 1;
        } else {
            other_unicode += 1;
        }
    }

    let ascii_tokens = (ascii_chars + whitespace).div_ceil(4);
    let symbol_tokens = code_symbols.div_ceil(2);
    let cjk_tokens = (cjk_chars * 2).div_ceil(3);
    let other_tokens = other_unicode.div_ceil(2);

    ascii_tokens + symbol_tokens + cjk_tokens + other_tokens
}

/// Calculate statistics for a single file
fn calculate_file_stats(path: &Path, relative_path: &str) -> Option<FileStats> {
    let content = fs::read_to_string(path).ok()?;

    let chars = content.chars().count();
    let chars_no_space = content.chars().filter(|c| !c.is_whitespace()).count();
    let lines = content.lines().count();

    // Count English words (sequences of ASCII alphanumeric)
    let words = content
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .count();

    // Count CJK characters
    let cjk_chars = content.chars().filter(|c| is_cjk_char(*c)).count();

    // Estimate tokens
    let tokens = estimate_tokens_smart(&content);

    // Count anchors
    let anchors = parse_file(path, relative_path);
    let anchor_count = anchors.len();

    Some(FileStats {
        path: relative_path.to_string(),
        chars,
        chars_no_space,
        words,
        cjk_chars,
        lines,
        tokens,
        anchors: anchor_count,
    })
}

/// Calculate project-wide statistics
pub fn calculate_project_stats(
    root: &Path,
    scope: Option<&Path>,
    extensions: Option<&[&str]>,
    top_n: usize,
) -> Result<ProjectStats> {
    let files = scan_files(root, scope, None, false, true, Some("file"))?;

    let mut stats = ProjectStats::default();
    let mut all_file_stats = Vec::new();
    let mut anchors_by_tag: HashMap<String, usize> = HashMap::new();

    // Default text extensions if not specified
    let default_exts = ["md", "txt", "rst", "adoc", "org", "tex", "html", "xml"];
    let exts: &[&str] = extensions.unwrap_or(&default_exts);

    for file_item in files.items {
        if let Some(path) = &file_item.path {
            // Check extension filter
            let has_valid_ext = exts.iter().any(|ext| path.ends_with(&format!(".{}", ext)));
            if !has_valid_ext {
                continue;
            }

            let full_path = root.join(path);
            if let Some(file_stats) = calculate_file_stats(&full_path, path) {
                stats.total_files += 1;
                stats.total_chars += file_stats.chars;
                stats.total_chars_no_space += file_stats.chars_no_space;
                stats.total_words += file_stats.words;
                stats.total_cjk_chars += file_stats.cjk_chars;
                stats.total_lines += file_stats.lines;
                stats.total_tokens += file_stats.tokens;
                stats.total_anchors += file_stats.anchors;

                // Collect anchor tags
                let anchors = parse_file(&full_path, path);
                for anchor in anchors {
                    for tag in anchor.tags {
                        *anchors_by_tag.entry(tag).or_insert(0) += 1;
                    }
                }

                all_file_stats.push(file_stats);
            }
        }
    }

    // Sort by chars descending and take top N
    all_file_stats.sort_by(|a, b| b.chars.cmp(&a.chars));
    stats.file_stats = all_file_stats.into_iter().take(top_n).collect();
    stats.anchors_by_tag = anchors_by_tag;

    Ok(stats)
}

/// Convert stats to ResultSet for rendering
fn stats_to_result_set(stats: &ProjectStats) -> ResultSet {
    let mut result_set = ResultSet::new();

    // Create a summary item
    let summary = format!(
        "📊 Project Statistics\n\
         ─────────────────────────────────────\n\
         Files:        {}\n\
         Lines:        {}\n\
         Characters:   {} ({}  excl. spaces)\n\
         Words:        {} (English)\n\
         CJK Chars:    {}\n\
         Est. Tokens:  {}\n\
         Anchors:      {}\n\
         ─────────────────────────────────────",
        stats.total_files,
        stats.total_lines,
        stats.total_chars,
        stats.total_chars_no_space,
        stats.total_words,
        stats.total_cjk_chars,
        stats.total_tokens,
        stats.total_anchors,
    );

    let mut summary_item = ResultItem::file("project_stats");
    summary_item.kind = Kind::Flow;
    summary_item.excerpt = Some(summary);
    summary_item.confidence = Confidence::High;
    summary_item.source_mode = SourceMode::Scan;
    result_set.push(summary_item);

    // Add anchor tag distribution if there are tags
    if !stats.anchors_by_tag.is_empty() {
        let mut tag_lines: Vec<String> = stats
            .anchors_by_tag
            .iter()
            .map(|(tag, count)| format!("  {}: {}", tag, count))
            .collect();
        tag_lines.sort();

        let tags_summary = format!("📌 Anchors by Tag\n{}", tag_lines.join("\n"));

        let mut tags_item = ResultItem::file("anchor_tags");
        tags_item.kind = Kind::Flow;
        tags_item.excerpt = Some(tags_summary);
        tags_item.confidence = Confidence::Medium;
        tags_item.source_mode = SourceMode::Anchor;
        result_set.push(tags_item);
    }

    // Add top files
    if !stats.file_stats.is_empty() {
        let files_summary: Vec<String> = stats
            .file_stats
            .iter()
            .map(|f| {
                format!(
                    "  {} - {} chars, {} words, {} CJK, ~{} tokens",
                    f.path, f.chars, f.words, f.cjk_chars, f.tokens
                )
            })
            .collect();

        let files_header = format!(
            "📄 Top {} Files by Size\n{}",
            stats.file_stats.len(),
            files_summary.join("\n")
        );

        let mut files_item = ResultItem::file("top_files");
        files_item.kind = Kind::Flow;
        files_item.excerpt = Some(files_header);
        files_item.confidence = Confidence::Medium;
        files_item.source_mode = SourceMode::Scan;
        result_set.push(files_item);
    }

    result_set
}

/// Stats output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsFormat {
    /// Standard ResultSet format (respects --format flag)
    #[default]
    Standard,
    /// JSON object with full statistics
    Json,
    /// Human-readable summary
    Summary,
    /// Markdown table format
    Table,
}

impl std::str::FromStr for StatsFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" | "default" => Ok(StatsFormat::Standard),
            "json" => Ok(StatsFormat::Json),
            "summary" => Ok(StatsFormat::Summary),
            "table" | "md" => Ok(StatsFormat::Table),
            _ => Err(format!("Unknown stats format: {}", s)),
        }
    }
}

/// Run the stats command
pub fn run_stats(
    root: &Path,
    scope: Option<&Path>,
    extensions: Option<Vec<String>>,
    stats_format: StatsFormat,
    top_n: usize,
    config: RenderConfig,
) -> Result<()> {
    let ext_refs: Option<Vec<&str>> = extensions
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let ext_slice: Option<&[&str]> = ext_refs.as_deref();

    let stats = calculate_project_stats(root, scope, ext_slice, top_n)?;

    match stats_format {
        StatsFormat::Json => {
            let json = serde_json::to_string_pretty(&stats)?;
            println!("{}", json);
        }
        StatsFormat::Summary => {
            println!("📊 Project Statistics");
            println!("═══════════════════════════════════════");
            println!("  Files:        {}", stats.total_files);
            println!("  Lines:        {}", stats.total_lines);
            println!("  Characters:   {}", stats.total_chars);
            println!("  Chars (excl): {}", stats.total_chars_no_space);
            println!("  Words (EN):   {}", stats.total_words);
            println!("  CJK Chars:    {}", stats.total_cjk_chars);
            println!("  Est. Tokens:  {}", stats.total_tokens);
            println!("  Anchors:      {}", stats.total_anchors);
            println!("═══════════════════════════════════════");

            if !stats.anchors_by_tag.is_empty() {
                println!("\n📌 Anchors by Tag:");
                let mut tags: Vec<_> = stats.anchors_by_tag.iter().collect();
                tags.sort_by(|a, b| b.1.cmp(a.1));
                for (tag, count) in tags {
                    println!("  {:20} {}", tag, count);
                }
            }

            if !stats.file_stats.is_empty() {
                println!("\n📄 Top {} Files:", stats.file_stats.len());
                for f in &stats.file_stats {
                    println!(
                        "  {:40} {:>8} chars  {:>6} words  {:>6} CJK  ~{:>6} tokens",
                        f.path, f.chars, f.words, f.cjk_chars, f.tokens
                    );
                }
            }
        }
        StatsFormat::Table => {
            println!("# Project Statistics\n");
            println!("| Metric | Value |");
            println!("|--------|-------|");
            println!("| Files | {} |", stats.total_files);
            println!("| Lines | {} |", stats.total_lines);
            println!("| Characters | {} |", stats.total_chars);
            println!("| Characters (no space) | {} |", stats.total_chars_no_space);
            println!("| Words (English) | {} |", stats.total_words);
            println!("| CJK Characters | {} |", stats.total_cjk_chars);
            println!("| Estimated Tokens | {} |", stats.total_tokens);
            println!("| Anchors | {} |", stats.total_anchors);

            if !stats.file_stats.is_empty() {
                println!("\n## Top Files\n");
                println!("| File | Chars | Words | CJK | Tokens |");
                println!("|------|-------|-------|-----|--------|");
                for f in &stats.file_stats {
                    println!(
                        "| {} | {} | {} | {} | {} |",
                        f.path, f.chars, f.words, f.cjk_chars, f.tokens
                    );
                }
            }
        }
        StatsFormat::Standard => {
            let result_set = stats_to_result_set(&stats);
            let renderer = Renderer::with_config(config);
            println!("{}", renderer.render(&result_set));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cjk_char() {
        assert!(is_cjk_char('中'));
        assert!(is_cjk_char('文'));
        assert!(is_cjk_char('あ')); // Hiragana
        assert!(is_cjk_char('ア')); // Katakana
        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char('1'));
    }

    #[test]
    fn test_estimate_tokens_smart() {
        // Pure ASCII
        let ascii = "Hello world, this is a test.";
        let tokens = estimate_tokens_smart(ascii);
        assert!(tokens > 0);

        // Pure CJK
        let cjk = "这是一个测试文档";
        let cjk_tokens = estimate_tokens_smart(cjk);
        assert!(cjk_tokens > 0);

        // CJK should have higher token density
        let ascii_per_char = tokens as f64 / ascii.chars().count() as f64;
        let cjk_per_char = cjk_tokens as f64 / cjk.chars().count() as f64;
        assert!(cjk_per_char > ascii_per_char);
    }

    #[test]
    fn test_stats_format_parse() {
        assert_eq!("json".parse::<StatsFormat>().unwrap(), StatsFormat::Json);
        assert_eq!(
            "summary".parse::<StatsFormat>().unwrap(),
            StatsFormat::Summary
        );
        assert_eq!("table".parse::<StatsFormat>().unwrap(), StatsFormat::Table);
    }

    #[test]
    fn test_stats_format_invalid() {
        assert!("invalid".parse::<StatsFormat>().is_err());
    }

    #[test]
    fn test_is_code_symbol() {
        assert!(is_code_symbol('('));
        assert!(is_code_symbol(')'));
        assert!(is_code_symbol('['));
        assert!(is_code_symbol('{'));
        assert!(is_code_symbol(';'));
        assert!(is_code_symbol('='));
        assert!(!is_code_symbol('a'));
        assert!(!is_code_symbol('中'));
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens_smart(""), 0);
    }

    #[test]
    fn test_estimate_tokens_whitespace() {
        let text = "   \n\t  ";
        let tokens = estimate_tokens_smart(text);
        // Whitespace may still produce some tokens depending on tokenization
        assert!(tokens < 10); // Should be very few tokens
    }

    #[test]
    fn test_estimate_tokens_code() {
        let code = "fn main() { println!(\"hello\"); }";
        let tokens = estimate_tokens_smart(code);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_mixed() {
        let mixed = "Hello 你好 World 世界";
        let tokens = estimate_tokens_smart(mixed);
        assert!(tokens > 0);
    }

    #[test]
    fn test_project_stats_default() {
        let stats = ProjectStats::default();
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_chars, 0);
        assert_eq!(stats.total_tokens, 0);
        assert!(stats.anchors_by_tag.is_empty());
    }

    #[test]
    fn test_file_stats_creation() {
        let file_stats = FileStats {
            path: "test.md".to_string(),
            lines: 10,
            chars: 100,
            chars_no_space: 80,
            words: 20,
            cjk_chars: 5,
            tokens: 30,
            anchors: 2,
        };
        assert_eq!(file_stats.path, "test.md");
        assert_eq!(file_stats.lines, 10);
        assert_eq!(file_stats.chars, 100);
        assert_eq!(file_stats.anchors, 2);
    }

    #[test]
    fn test_file_stats_default() {
        let stats = FileStats::default();
        assert_eq!(stats.path, "");
        assert_eq!(stats.chars, 0);
        assert_eq!(stats.words, 0);
        assert_eq!(stats.lines, 0);
    }

    #[test]
    fn test_project_stats_with_data() {
        let mut stats = ProjectStats {
            total_files: 5,
            total_chars: 1000,
            total_words: 200,
            total_tokens: 300,
            ..Default::default()
        };
        stats.anchors_by_tag.insert("chapter".to_string(), 3);

        assert_eq!(stats.total_files, 5);
        assert_eq!(stats.anchors_by_tag.get("chapter"), Some(&3));
    }

    #[test]
    fn test_calculate_file_stats() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "Hello world\nThis is a test.\n").unwrap();

        let stats = calculate_file_stats(&file_path, "test.md");
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.path, "test.md");
        assert_eq!(stats.lines, 2);
        assert!(stats.chars > 0);
        assert!(stats.words > 0);
    }

    #[test]
    fn test_calculate_file_stats_nonexistent() {
        let stats = calculate_file_stats(Path::new("/nonexistent/path.txt"), "path.txt");
        assert!(stats.is_none());
    }

    #[test]
    fn test_calculate_file_stats_with_cjk() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "你好世界 Hello World").unwrap();

        let stats = calculate_file_stats(&file_path, "test.md").unwrap();
        assert!(stats.cjk_chars >= 4);
        assert!(stats.words >= 2);
    }

    #[test]
    fn test_calculate_project_stats() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file1.md"), "Hello world").unwrap();
        std::fs::write(temp.path().join("file2.txt"), "Test content").unwrap();

        let stats = calculate_project_stats(temp.path(), None, None, 10).unwrap();
        assert!(stats.total_files >= 2);
        assert!(stats.total_chars > 0);
    }

    #[test]
    fn test_stats_format_default() {
        let format: StatsFormat = Default::default();
        assert_eq!(format, StatsFormat::Standard);
    }

    #[test]
    fn test_stats_to_result_set() {
        let stats = ProjectStats {
            total_files: 10,
            total_chars: 1000,
            total_words: 200,
            ..Default::default()
        };

        let result_set = stats_to_result_set(&stats);
        assert!(!result_set.items.is_empty());
    }

    #[test]
    fn test_estimate_tokens_other_unicode() {
        // Test with non-ASCII, non-CJK characters (like emoji or accented chars)
        let text = "Café résumé naïve";
        let tokens = estimate_tokens_smart(text);
        assert!(tokens > 0);
    }
}
