//! Outline generation - Generate document outline from anchors
//!
//! Creates a hierarchical outline of the project based on anchor structure.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::anchors::parse::{parse_file, Anchor};
use crate::backends::scan::scan_files;
use crate::core::model::{Confidence, Kind, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};

/// Outline item representing an anchor with its content stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineItem {
    /// Anchor ID
    pub id: String,
    /// File path
    pub path: String,
    /// Tags
    pub tags: Vec<String>,
    /// Line range start
    pub start_line: u32,
    /// Line range end
    pub end_line: u32,
    /// Character count
    pub chars: usize,
    /// Word count (English)
    pub words: usize,
    /// CJK character count
    pub cjk_chars: usize,
    /// Estimated tokens
    pub tokens: usize,
    /// Content preview (first line or title)
    pub preview: Option<String>,
    /// Nested level (based on tag hierarchy or nesting)
    pub level: usize,
}

/// Project outline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectOutline {
    /// All outline items
    pub items: Vec<OutlineItem>,
    /// Total character count
    pub total_chars: usize,
    /// Total word count
    pub total_words: usize,
    /// Total CJK characters
    pub total_cjk_chars: usize,
    /// Total estimated tokens
    pub total_tokens: usize,
    /// Anchors grouped by tag
    pub by_tag: HashMap<String, Vec<String>>,
}

/// Check if a character is CJK
#[inline]
fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xFF00..=0xFFEF).contains(&cp)
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

/// Estimate tokens for text
fn estimate_tokens(text: &str) -> usize {
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

/// Count words in text
fn count_words(text: &str) -> usize {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .count()
}

/// Count CJK characters in text
fn count_cjk_chars(text: &str) -> usize {
    text.chars().filter(|c| is_cjk_char(*c)).count()
}

/// Extract preview from content (first non-empty line or title)
fn extract_preview(content: &str, max_len: usize) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("<!--") {
            // Use char count for proper Unicode handling
            let char_count = trimmed.chars().count();
            let preview = if char_count > max_len {
                let truncated: String = trimmed.chars().take(max_len).collect();
                format!("{}...", truncated)
            } else {
                trimmed.to_string()
            };
            return Some(preview);
        }
    }
    None
}

/// Determine nesting level based on anchor ID and tags
fn determine_level(anchor: &Anchor, all_anchors: &[Anchor]) -> usize {
    // Check if this anchor is nested inside another anchor in the same file
    let mut level = 0;

    for other in all_anchors {
        if other.id != anchor.id
            && other.path == anchor.path
            && other.range.start < anchor.range.start
            && other.range.end > anchor.range.end
        {
            level += 1;
        }
    }

    // Also consider ID structure (e.g., ch01.scene1 has level 1)
    let dot_count = anchor.id.matches('.').count();
    level.max(dot_count)
}

/// Build outline from anchor
fn anchor_to_outline_item(anchor: &Anchor, all_anchors: &[Anchor]) -> OutlineItem {
    let content = anchor.content.as_deref().unwrap_or("");
    let chars = content.chars().count();
    let words = count_words(content);
    let cjk_chars = count_cjk_chars(content);
    let tokens = estimate_tokens(content);
    let preview = extract_preview(content, 60);
    let level = determine_level(anchor, all_anchors);

    OutlineItem {
        id: anchor.id.clone(),
        path: anchor.path.clone(),
        tags: anchor.tags.clone(),
        start_line: anchor.range.start,
        end_line: anchor.range.end,
        chars,
        words,
        cjk_chars,
        tokens,
        preview,
        level,
    }
}

/// Generate project outline
pub fn generate_outline(
    root: &Path,
    scope: Option<&Path>,
    tag_filter: Option<&str>,
    extensions: Option<&[&str]>,
) -> Result<ProjectOutline> {
    use crate::cache::reader::get_files_cached;

    let files = if scope.is_some() {
        // If scope is specified, do a direct scan (scope is specific)
        scan_files(root, scope, None, false, true, Some("file"))?
    } else {
        // Use cached files when no scope
        get_files_cached(root)?
    };

    let default_exts = ["md", "txt", "rst", "adoc", "org", "tex", "html", "xml"];
    let exts: &[&str] = extensions.unwrap_or(&default_exts);

    let mut all_anchors: Vec<Anchor> = Vec::new();

    // Collect all anchors
    for file_item in &files.items {
        if let Some(path) = &file_item.path {
            let has_valid_ext = exts.iter().any(|ext| path.ends_with(&format!(".{}", ext)));
            if !has_valid_ext {
                continue;
            }

            let full_path = root.join(path);
            let anchors = parse_file(&full_path, path);
            all_anchors.extend(anchors);
        }
    }

    // Filter by tag if specified
    if let Some(tag) = tag_filter {
        all_anchors.retain(|a| a.tags.contains(&tag.to_string()));
    }

    // Build outline items
    let mut items: Vec<OutlineItem> = all_anchors
        .iter()
        .map(|a| anchor_to_outline_item(a, &all_anchors))
        .collect();

    // Sort by path, then by start line
    items.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.start_line.cmp(&b.start_line))
    });

    // Calculate totals
    let total_chars: usize = items.iter().map(|i| i.chars).sum();
    let total_words: usize = items.iter().map(|i| i.words).sum();
    let total_cjk_chars: usize = items.iter().map(|i| i.cjk_chars).sum();
    let total_tokens: usize = items.iter().map(|i| i.tokens).sum();

    // Group by tag
    let mut by_tag: HashMap<String, Vec<String>> = HashMap::new();
    for item in &items {
        for tag in &item.tags {
            by_tag.entry(tag.clone()).or_default().push(item.id.clone());
        }
    }

    Ok(ProjectOutline {
        items,
        total_chars,
        total_words,
        total_cjk_chars,
        total_tokens,
        by_tag,
    })
}

/// Outline output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutlineFormat {
    /// Markdown outline
    #[default]
    Markdown,
    /// JSON output
    Json,
    /// Tree view
    Tree,
    /// Standard ResultSet
    Standard,
}

impl std::str::FromStr for OutlineFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(OutlineFormat::Markdown),
            "json" => Ok(OutlineFormat::Json),
            "tree" => Ok(OutlineFormat::Tree),
            "standard" | "default" => Ok(OutlineFormat::Standard),
            _ => Err(format!("Unknown outline format: {}", s)),
        }
    }
}

/// Render outline as Markdown
fn render_markdown(outline: &ProjectOutline) -> String {
    let mut output = String::new();

    output.push_str("# 📑 Document Outline\n\n");

    // Summary
    output.push_str(&format!(
        "**Total:** {} anchors | {} chars | {} words | {} CJK | ~{} tokens\n\n",
        outline.items.len(),
        outline.total_chars,
        outline.total_words,
        outline.total_cjk_chars,
        outline.total_tokens
    ));

    output.push_str("---\n\n");

    // Group by file
    let mut current_file = String::new();
    for item in &outline.items {
        if item.path != current_file {
            current_file = item.path.clone();
            output.push_str(&format!("## 📄 {}\n\n", current_file));
        }

        // Indent based on level
        let indent = "  ".repeat(item.level);
        let tags_str = if item.tags.is_empty() {
            String::new()
        } else {
            format!(" `{}`", item.tags.join("` `"))
        };

        output.push_str(&format!(
            "{}- **[{}]** (L{}-{}) {} chars, {} words{}\n",
            indent, item.id, item.start_line, item.end_line, item.chars, item.words, tags_str
        ));

        if let Some(preview) = &item.preview {
            output.push_str(&format!("{}  > {}\n", indent, preview));
        }
    }

    // Tag index
    if !outline.by_tag.is_empty() {
        output.push_str("\n---\n\n## 🏷️ By Tag\n\n");
        let mut tags: Vec<_> = outline.by_tag.iter().collect();
        tags.sort_by_key(|(k, _)| *k);

        for (tag, ids) in tags {
            output.push_str(&format!("- **{}**: {}\n", tag, ids.join(", ")));
        }
    }

    output
}

/// Render outline as tree
fn render_tree(outline: &ProjectOutline) -> String {
    let mut output = String::new();

    output.push_str("📑 Document Outline\n");
    output.push_str(&format!(
        "   {} anchors | {} chars | {} words | ~{} tokens\n\n",
        outline.items.len(),
        outline.total_chars,
        outline.total_words,
        outline.total_tokens
    ));

    let mut current_file = String::new();
    let total_items = outline.items.len();

    for (idx, item) in outline.items.iter().enumerate() {
        let is_last_in_file = idx + 1 >= total_items
            || outline.items.get(idx + 1).map(|i| &i.path) != Some(&item.path);

        if item.path != current_file {
            current_file = item.path.clone();
            output.push_str(&format!("📄 {}\n", current_file));
        }

        let prefix = if is_last_in_file {
            "└── "
        } else {
            "├── "
        };
        let level_indent = "│   ".repeat(item.level);

        output.push_str(&format!(
            "{}{}[{}] {} chars ({} words)\n",
            level_indent, prefix, item.id, item.chars, item.words
        ));
    }

    output
}

/// Convert outline to ResultSet
fn outline_to_result_set(outline: &ProjectOutline) -> ResultSet {
    let mut result_set = ResultSet::new();

    // Summary item
    let summary = format!(
        "📑 Document Outline\n\
         ─────────────────────────────────────\n\
         Anchors:      {}\n\
         Characters:   {}\n\
         Words:        {}\n\
         CJK Chars:    {}\n\
         Est. Tokens:  {}\n\
         ─────────────────────────────────────",
        outline.items.len(),
        outline.total_chars,
        outline.total_words,
        outline.total_cjk_chars,
        outline.total_tokens
    );

    let mut summary_item = ResultItem::file("outline_summary");
    summary_item.kind = Kind::Flow;
    summary_item.excerpt = Some(summary);
    summary_item.confidence = Confidence::High;
    summary_item.source_mode = SourceMode::Anchor;
    result_set.push(summary_item);

    // Each anchor as an item
    for item in &outline.items {
        let excerpt = format!(
            "[{}] {} chars, {} words, {} CJK\n{}",
            item.id,
            item.chars,
            item.words,
            item.cjk_chars,
            item.preview.as_deref().unwrap_or("")
        );

        let mut result_item = ResultItem::anchor(
            item.path.clone(),
            crate::core::model::Range::lines(item.start_line, item.end_line),
        );
        result_item.excerpt = Some(excerpt);
        result_item.confidence = Confidence::High;
        result_set.push(result_item);
    }

    result_set
}

/// Run the outline command
pub fn run_outline(
    root: &Path,
    scope: Option<&Path>,
    tag_filter: Option<&str>,
    extensions: Option<Vec<String>>,
    outline_format: OutlineFormat,
    config: RenderConfig,
) -> Result<()> {
    let ext_refs: Option<Vec<&str>> = extensions
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());
    let ext_slice: Option<&[&str]> = ext_refs.as_deref();

    let outline = generate_outline(root, scope, tag_filter, ext_slice)?;

    match outline_format {
        OutlineFormat::Json => {
            let json = serde_json::to_string_pretty(&outline)?;
            println!("{}", json);
        }
        OutlineFormat::Markdown => {
            println!("{}", render_markdown(&outline));
        }
        OutlineFormat::Tree => {
            println!("{}", render_tree(&outline));
        }
        OutlineFormat::Standard => {
            let result_set = outline_to_result_set(&outline);
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
    fn test_outline_format_parse() {
        assert_eq!(
            "markdown".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Markdown
        );
        assert_eq!(
            "json".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Json
        );
        assert_eq!(
            "tree".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Tree
        );
    }

    #[test]
    fn test_count_words() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("This is a test sentence."), 4);
        assert_eq!(count_words("a b c"), 0); // too short
    }

    #[test]
    fn test_count_cjk_chars() {
        assert_eq!(count_cjk_chars("hello"), 0);
        assert_eq!(count_cjk_chars("你好世界"), 4);
        assert_eq!(count_cjk_chars("hello 你好"), 2);
    }

    #[test]
    fn test_extract_preview() {
        let content = "First line\nSecond line";
        assert_eq!(extract_preview(content, 20), Some("First line".to_string()));

        let empty = "";
        assert_eq!(extract_preview(empty, 20), None);

        let long = "This is a very long line that should be truncated";
        let preview = extract_preview(long, 20).unwrap();
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_estimate_tokens() {
        let ascii = "Hello world";
        assert!(estimate_tokens(ascii) > 0);

        let cjk = "你好世界";
        assert!(estimate_tokens(cjk) > 0);
    }

    #[test]
    fn test_outline_format_invalid() {
        assert!("invalid".parse::<OutlineFormat>().is_err());
    }

    #[test]
    fn test_extract_preview_skips_comments() {
        let content = "<!--Q:begin-->\nActual content";
        let preview = extract_preview(content, 20).unwrap();
        assert_eq!(preview, "Actual content");
    }

    #[test]
    fn test_extract_preview_cjk() {
        let content = "这是一个很长的中文句子需要被截断显示";
        let preview = extract_preview(content, 10).unwrap();
        assert!(preview.ends_with("..."));
        assert!(preview.chars().count() <= 14); // 10 + "..."
    }

    #[test]
    fn test_count_words_empty() {
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn test_count_words_cjk_only() {
        // CJK doesn't count as words (no ASCII alphanumeric)
        assert_eq!(count_words("你好世界"), 0);
    }

    #[test]
    fn test_count_cjk_chars_mixed() {
        assert_eq!(count_cjk_chars("hello世界test"), 2);
    }

    #[test]
    fn test_is_cjk_char_fullwidth() {
        // Fullwidth forms
        assert!(is_cjk_char('Ａ')); // U+FF21 fullwidth A
        assert!(is_cjk_char('１')); // U+FF11 fullwidth 1
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_estimate_tokens_code_symbols() {
        let code = "fn() { }";
        let tokens = estimate_tokens(code);
        assert!(tokens > 0);
    }

    #[test]
    fn test_outline_item_creation() {
        let item = OutlineItem {
            id: "test.id".to_string(),
            path: "test.md".to_string(),
            tags: vec!["tag1".to_string()],
            start_line: 1,
            end_line: 10,
            chars: 100,
            words: 20,
            cjk_chars: 5,
            tokens: 30,
            preview: Some("Preview".to_string()),
            level: 0,
        };
        assert_eq!(item.id, "test.id");
        assert_eq!(item.level, 0);
    }

    #[test]
    fn test_project_outline_creation() {
        let outline = ProjectOutline {
            total_chars: 500,
            total_words: 100,
            total_cjk_chars: 50,
            total_tokens: 150,
            items: vec![],
            by_tag: std::collections::HashMap::new(),
        };
        assert_eq!(outline.total_chars, 500);
        assert!(outline.items.is_empty());
    }

    #[test]
    fn test_is_code_symbol() {
        // Brackets and parentheses
        assert!(is_code_symbol('('));
        assert!(is_code_symbol(')'));
        assert!(is_code_symbol('['));
        assert!(is_code_symbol(']'));
        assert!(is_code_symbol('{'));
        assert!(is_code_symbol('}'));
        assert!(is_code_symbol('<'));
        assert!(is_code_symbol('>'));

        // Operators
        assert!(is_code_symbol('='));
        assert!(is_code_symbol('+'));
        assert!(is_code_symbol('-'));
        assert!(is_code_symbol('*'));
        assert!(is_code_symbol('/'));
        assert!(is_code_symbol('%'));
        assert!(is_code_symbol('&'));
        assert!(is_code_symbol('|'));
        assert!(is_code_symbol('^'));
        assert!(is_code_symbol('!'));
        assert!(is_code_symbol('~'));
        assert!(is_code_symbol('?'));

        // Punctuation
        assert!(is_code_symbol(':'));
        assert!(is_code_symbol(';'));
        assert!(is_code_symbol(','));
        assert!(is_code_symbol('.'));

        // Special chars
        assert!(is_code_symbol('@'));
        assert!(is_code_symbol('#'));
        assert!(is_code_symbol('$'));
        assert!(is_code_symbol('\\'));
        assert!(is_code_symbol('"'));
        assert!(is_code_symbol('\''));
        assert!(is_code_symbol('`'));

        // Non-code symbols
        assert!(!is_code_symbol('a'));
        assert!(!is_code_symbol('A'));
        assert!(!is_code_symbol('0'));
        assert!(!is_code_symbol(' '));
    }

    #[test]
    fn test_estimate_tokens_whitespace() {
        let text = "   \t\n   ";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_mixed_content() {
        let text = "fn test() { let x = 你好; }";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_other_unicode() {
        let text = "Hello мир świat"; // Russian and Polish
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_words_with_numbers() {
        assert_eq!(count_words("test123 hello456"), 2);
    }

    #[test]
    fn test_extract_preview_all_comments() {
        let content = "<!--comment1-->\n<!--comment2-->";
        assert_eq!(extract_preview(content, 20), None);
    }

    #[test]
    fn test_extract_preview_whitespace_only() {
        let content = "   \n\t\n   ";
        assert_eq!(extract_preview(content, 20), None);
    }

    #[test]
    fn test_outline_format_default() {
        let format = OutlineFormat::default();
        assert_eq!(format, OutlineFormat::Markdown);
    }

    #[test]
    fn test_outline_format_all_variants() {
        assert_eq!(
            "md".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Markdown
        );
        assert_eq!(
            "standard".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Standard
        );
        assert_eq!(
            "default".parse::<OutlineFormat>().unwrap(),
            OutlineFormat::Standard
        );
    }

    #[test]
    fn test_render_markdown_empty() {
        let outline = ProjectOutline {
            items: vec![],
            total_chars: 0,
            total_words: 0,
            total_cjk_chars: 0,
            total_tokens: 0,
            by_tag: HashMap::new(),
        };
        let md = render_markdown(&outline);
        assert!(md.contains("Document Outline"));
        assert!(md.contains("0 anchors"));
    }

    #[test]
    fn test_render_markdown_with_items() {
        let mut by_tag = HashMap::new();
        by_tag.insert("test".to_string(), vec!["item1".to_string()]);

        let outline = ProjectOutline {
            items: vec![OutlineItem {
                id: "item1".to_string(),
                path: "test.md".to_string(),
                tags: vec!["test".to_string()],
                start_line: 1,
                end_line: 10,
                chars: 100,
                words: 20,
                cjk_chars: 0,
                tokens: 25,
                preview: Some("Preview text".to_string()),
                level: 0,
            }],
            total_chars: 100,
            total_words: 20,
            total_cjk_chars: 0,
            total_tokens: 25,
            by_tag,
        };
        let md = render_markdown(&outline);
        assert!(md.contains("📄 test.md"));
        assert!(md.contains("[item1]"));
        assert!(md.contains("Preview text"));
        assert!(md.contains("By Tag"));
    }

    #[test]
    fn test_render_tree_empty() {
        let outline = ProjectOutline {
            items: vec![],
            total_chars: 0,
            total_words: 0,
            total_cjk_chars: 0,
            total_tokens: 0,
            by_tag: HashMap::new(),
        };
        let tree = render_tree(&outline);
        assert!(tree.contains("Document Outline"));
        assert!(tree.contains("0 anchors"));
    }

    #[test]
    fn test_render_tree_with_items() {
        let outline = ProjectOutline {
            items: vec![
                OutlineItem {
                    id: "item1".to_string(),
                    path: "test.md".to_string(),
                    tags: vec![],
                    start_line: 1,
                    end_line: 10,
                    chars: 100,
                    words: 20,
                    cjk_chars: 0,
                    tokens: 25,
                    preview: None,
                    level: 0,
                },
                OutlineItem {
                    id: "item2".to_string(),
                    path: "test.md".to_string(),
                    tags: vec![],
                    start_line: 15,
                    end_line: 25,
                    chars: 50,
                    words: 10,
                    cjk_chars: 0,
                    tokens: 12,
                    preview: None,
                    level: 0,
                },
            ],
            total_chars: 150,
            total_words: 30,
            total_cjk_chars: 0,
            total_tokens: 37,
            by_tag: HashMap::new(),
        };
        let tree = render_tree(&outline);
        assert!(tree.contains("📄 test.md"));
        assert!(tree.contains("[item1]"));
        assert!(tree.contains("[item2]"));
        assert!(tree.contains("├──") || tree.contains("└──"));
    }

    #[test]
    fn test_outline_to_result_set() {
        let outline = ProjectOutline {
            items: vec![OutlineItem {
                id: "test.anchor".to_string(),
                path: "test.md".to_string(),
                tags: vec!["tag1".to_string()],
                start_line: 5,
                end_line: 15,
                chars: 200,
                words: 40,
                cjk_chars: 10,
                tokens: 50,
                preview: Some("Test preview".to_string()),
                level: 1,
            }],
            total_chars: 200,
            total_words: 40,
            total_cjk_chars: 10,
            total_tokens: 50,
            by_tag: HashMap::new(),
        };

        let result_set = outline_to_result_set(&outline);
        // Should have summary item + 1 anchor item
        assert_eq!(result_set.items.len(), 2);

        // Check summary item
        let summary = &result_set.items[0];
        assert_eq!(summary.kind, Kind::Flow);
        assert!(summary
            .excerpt
            .as_ref()
            .unwrap()
            .contains("Document Outline"));

        // Check anchor item
        let anchor = &result_set.items[1];
        assert_eq!(anchor.path, Some("test.md".to_string()));
        assert!(anchor.excerpt.as_ref().unwrap().contains("test.anchor"));
    }

    #[test]
    fn test_outline_item_with_nested_level() {
        let item = OutlineItem {
            id: "parent.child.grandchild".to_string(),
            path: "test.md".to_string(),
            tags: vec![],
            start_line: 1,
            end_line: 10,
            chars: 100,
            words: 20,
            cjk_chars: 0,
            tokens: 25,
            preview: None,
            level: 2, // Nested 2 levels
        };
        assert_eq!(item.level, 2);
    }

    #[test]
    fn test_is_cjk_char_extended() {
        // CJK Extension A
        assert!(is_cjk_char('\u{3400}')); // U+3400
        assert!(is_cjk_char('\u{4DB5}')); // U+4DB5

        // CJK Symbols
        assert!(is_cjk_char('〇')); // CJK zero

        // Hiragana
        assert!(is_cjk_char('あ'));
        assert!(is_cjk_char('ん'));

        // Katakana
        assert!(is_cjk_char('ア'));
        assert!(is_cjk_char('ン'));

        // Hangul
        assert!(is_cjk_char('가'));
        assert!(is_cjk_char('힣'));
    }

    #[test]
    fn test_render_markdown_nested_items() {
        let outline = ProjectOutline {
            items: vec![
                OutlineItem {
                    id: "parent".to_string(),
                    path: "test.md".to_string(),
                    tags: vec![],
                    start_line: 1,
                    end_line: 20,
                    chars: 200,
                    words: 40,
                    cjk_chars: 0,
                    tokens: 50,
                    preview: None,
                    level: 0,
                },
                OutlineItem {
                    id: "child".to_string(),
                    path: "test.md".to_string(),
                    tags: vec![],
                    start_line: 5,
                    end_line: 15,
                    chars: 100,
                    words: 20,
                    cjk_chars: 0,
                    tokens: 25,
                    preview: None,
                    level: 1,
                },
            ],
            total_chars: 300,
            total_words: 60,
            total_cjk_chars: 0,
            total_tokens: 75,
            by_tag: HashMap::new(),
        };
        let md = render_markdown(&outline);
        assert!(md.contains("[parent]"));
        assert!(md.contains("[child]"));
    }
}
