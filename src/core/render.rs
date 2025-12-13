//! Renderer module
//!
//! Renders ResultSet to different output formats: jsonl, json, md, raw

use crate::core::model::{Kind, Range, ResultItem, ResultSet};
use std::io::Write;

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Jsonl,
    Json,
    Markdown,
    Raw,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "jsonl" => Ok(OutputFormat::Jsonl),
            "json" => Ok(OutputFormat::Json),
            "md" | "markdown" => Ok(OutputFormat::Markdown),
            "raw" => Ok(OutputFormat::Raw),
            _ => Err(format!("Unknown format: {}", s)),
        }
    }
}

/// Render configuration combining format and options
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderConfig {
    pub format: OutputFormat,
    pub pretty: bool,
}

impl RenderConfig {
    /// Create a new render config with default options
    #[allow(dead_code)]
    pub fn new(format: OutputFormat) -> Self {
        Self {
            format,
            pretty: false,
        }
    }

    /// Create a new render config with pretty option
    pub fn with_pretty(format: OutputFormat, pretty: bool) -> Self {
        Self { format, pretty }
    }
}

/// Renderer for result sets
pub struct Renderer {
    config: RenderConfig,
}

impl Renderer {
    #[allow(dead_code)]
    pub fn new(format: OutputFormat) -> Self {
        Self {
            config: RenderConfig::new(format),
        }
    }

    /// Create a new renderer with render config
    pub fn with_config(config: RenderConfig) -> Self {
        Self { config }
    }

    /// Render a result set to a string
    pub fn render(&self, result_set: &ResultSet) -> String {
        match self.config.format {
            OutputFormat::Jsonl => self.render_jsonl(result_set),
            OutputFormat::Json => self.render_json(result_set),
            OutputFormat::Markdown => self.render_markdown(result_set),
            OutputFormat::Raw => self.render_raw(result_set),
        }
    }

    /// Render to a writer
    #[allow(dead_code)]
    pub fn render_to<W: Write>(
        &self,
        result_set: &ResultSet,
        mut writer: W,
    ) -> std::io::Result<()> {
        let output = self.render(result_set);
        writer.write_all(output.as_bytes())
    }

    /// Render as JSON Lines (one JSON object per line)
    fn render_jsonl(&self, result_set: &ResultSet) -> String {
        result_set
            .items
            .iter()
            .filter_map(|item| {
                if self.config.pretty {
                    serde_json::to_string_pretty(item).ok()
                } else {
                    serde_json::to_string(item).ok()
                }
            })
            .collect::<Vec<_>>()
            .join(if self.config.pretty { "\n\n" } else { "\n" })
    }

    /// Render as a single JSON array
    fn render_json(&self, result_set: &ResultSet) -> String {
        if self.config.pretty {
            serde_json::to_string_pretty(&result_set.items).unwrap_or_else(|_| "[]".to_string())
        } else {
            serde_json::to_string(&result_set.items).unwrap_or_else(|_| "[]".to_string())
        }
    }

    /// Render as Markdown
    fn render_markdown(&self, result_set: &ResultSet) -> String {
        let mut output = String::new();

        // Group by kind
        let mut files = Vec::new();
        let mut matches = Vec::new();
        let mut extracts = Vec::new();
        let mut anchors = Vec::new();
        let mut flows = Vec::new();
        let mut errors = Vec::new();

        for item in &result_set.items {
            match item.kind {
                Kind::File => files.push(item),
                Kind::Match => matches.push(item),
                Kind::Extract => extracts.push(item),
                Kind::Anchor => anchors.push(item),
                Kind::Flow => flows.push(item),
                Kind::Error => errors.push(item),
            }
        }

        // Render each section
        if !errors.is_empty() {
            output.push_str("## Errors\n\n");
            for item in errors {
                for error in &item.errors {
                    output.push_str(&format!("- **{}**: {}\n", error.code, error.message));
                }
            }
            output.push('\n');
        }

        if !files.is_empty() {
            output.push_str("## Files\n\n");
            for item in files {
                if let Some(path) = &item.path {
                    output.push_str(&format!("- `{}`", path));
                    if let Some(size) = item.meta.size {
                        output.push_str(&format!(" ({} bytes)", size));
                    }
                    output.push('\n');
                }
            }
            output.push('\n');
        }

        if !matches.is_empty() {
            output.push_str("## Matches\n\n");
            for item in matches {
                self.render_item_md(&mut output, item);
            }
            output.push('\n');
        }

        if !extracts.is_empty() {
            output.push_str("## Extracts\n\n");
            for item in extracts {
                self.render_item_md(&mut output, item);
            }
            output.push('\n');
        }

        if !anchors.is_empty() {
            output.push_str("## Anchors\n\n");
            for item in anchors {
                self.render_item_md(&mut output, item);
            }
            output.push('\n');
        }

        if !flows.is_empty() {
            output.push_str("## Flow Results\n\n");
            for item in flows {
                self.render_item_md(&mut output, item);
            }
            output.push('\n');
        }

        output
    }

    fn render_item_md(&self, output: &mut String, item: &ResultItem) {
        if let Some(path) = &item.path {
            output.push_str(&format!("### `{}`", path));
            if let Some(range) = &item.range {
                match range {
                    Range::Line(r) => output.push_str(&format!(" (lines {}-{})", r.start, r.end)),
                    Range::Byte(r) => output.push_str(&format!(" (bytes {}-{})", r.start, r.end)),
                }
            }
            output.push('\n');
        }

        if let Some(excerpt) = &item.excerpt {
            output.push_str("\n```\n");
            output.push_str(excerpt);
            if !excerpt.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n");
        }

        if item.meta.truncated {
            output.push_str("\n> ⚠️ Content was truncated\n");
        }

        output.push('\n');
    }

    /// Render as raw output (for debugging)
    fn render_raw(&self, result_set: &ResultSet) -> String {
        // Raw mode: just output excerpts directly
        result_set
            .items
            .iter()
            .filter_map(|item| item.excerpt.clone())
            .collect::<Vec<_>>()
            .join("\n---\n")
    }
}

/// Write raw mode warning to stderr
#[allow(dead_code)]
pub fn write_raw_warning() {
    eprintln!("# WARNING: Raw mode output - not parseable, unstable format");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{MiseError, Range, ResultItem};

    #[test]
    fn test_render_jsonl() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("src/main.rs"));
        result_set.push(ResultItem::file("src/lib.rs"));

        let renderer = Renderer::new(OutputFormat::Jsonl);
        let output = renderer.render(&result_set);

        assert!(output.contains("src/main.rs"));
        assert!(output.contains("src/lib.rs"));
        assert_eq!(output.lines().count(), 2);
    }

    #[test]
    fn test_render_json() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("src/main.rs"));

        let renderer = Renderer::new(OutputFormat::Json);
        let output = renderer.render(&result_set);

        assert!(output.starts_with('['));
        assert!(output.ends_with(']'));
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!(
            "jsonl".parse::<OutputFormat>().unwrap(),
            OutputFormat::Jsonl
        );
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "md".parse::<OutputFormat>().unwrap(),
            OutputFormat::Markdown
        );
        assert_eq!("raw".parse::<OutputFormat>().unwrap(), OutputFormat::Raw);
    }

    #[test]
    fn test_output_format_parse_case_insensitive() {
        assert_eq!("JSONL".parse::<OutputFormat>().unwrap(), OutputFormat::Jsonl);
        assert_eq!("JSON".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("MD".parse::<OutputFormat>().unwrap(), OutputFormat::Markdown);
        assert_eq!("MARKDOWN".parse::<OutputFormat>().unwrap(), OutputFormat::Markdown);
        assert_eq!("RAW".parse::<OutputFormat>().unwrap(), OutputFormat::Raw);
    }

    #[test]
    fn test_output_format_parse_invalid() {
        let result = "invalid".parse::<OutputFormat>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown format"));
    }

    #[test]
    fn test_render_config_new() {
        let config = RenderConfig::new(OutputFormat::Json);
        assert_eq!(config.format, OutputFormat::Json);
        assert!(!config.pretty);
    }

    #[test]
    fn test_render_config_with_pretty() {
        let config = RenderConfig::with_pretty(OutputFormat::Jsonl, true);
        assert_eq!(config.format, OutputFormat::Jsonl);
        assert!(config.pretty);
    }

    #[test]
    fn test_render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(config.format, OutputFormat::Jsonl);
        assert!(!config.pretty);
    }

    #[test]
    fn test_render_jsonl_pretty() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("src/main.rs"));
        
        let config = RenderConfig::with_pretty(OutputFormat::Jsonl, true);
        let renderer = Renderer::with_config(config);
        let output = renderer.render(&result_set);
        
        // Pretty output should have indentation
        assert!(output.contains('\n'));
    }

    #[test]
    fn test_render_json_pretty() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("src/main.rs"));
        
        let config = RenderConfig::with_pretty(OutputFormat::Json, true);
        let renderer = Renderer::with_config(config);
        let output = renderer.render(&result_set);
        
        // Pretty JSON should have indentation
        assert!(output.contains("  "));
    }

    #[test]
    fn test_render_markdown_empty() {
        let result_set = ResultSet::new();
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        assert!(output.is_empty());
    }

    #[test]
    fn test_render_markdown_files() {
        let mut result_set = ResultSet::new();
        let mut item = ResultItem::file("src/main.rs");
        item.meta.size = Some(1024);
        result_set.push(item);
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Files"));
        assert!(output.contains("`src/main.rs`"));
        assert!(output.contains("1024 bytes"));
    }

    #[test]
    fn test_render_markdown_matches() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::match_result(
            "src/main.rs",
            Range::lines(10, 15),
            "fn main() {}",
        ));
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Matches"));
        assert!(output.contains("lines 10-15"));
        assert!(output.contains("fn main()"));
    }

    #[test]
    fn test_render_markdown_extracts() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::extract(
            "src/lib.rs",
            Range::lines(1, 10),
            "use std::io;",
        ));
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Extracts"));
    }

    #[test]
    fn test_render_markdown_anchors() {
        let mut result_set = ResultSet::new();
        let mut item = ResultItem::anchor("doc.md", Range::lines(5, 20));
        item.excerpt = Some("Anchor content".to_string());
        result_set.push(item);
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Anchors"));
    }

    #[test]
    fn test_render_markdown_errors() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::error(MiseError::new("TEST_ERROR", "Test error message")));
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Errors"));
        assert!(output.contains("TEST_ERROR"));
        assert!(output.contains("Test error message"));
    }

    #[test]
    fn test_render_markdown_truncated() {
        let mut result_set = ResultSet::new();
        let mut item = ResultItem::extract("test.rs", Range::lines(1, 100), "content");
        item.meta.truncated = true;
        result_set.push(item);
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("truncated"));
    }

    #[test]
    fn test_render_markdown_byte_range() {
        let mut result_set = ResultSet::new();
        let mut item = ResultItem::file("test.rs");
        item.range = Some(Range::bytes(100, 200));
        item.kind = Kind::Extract;
        item.excerpt = Some("content".to_string());
        result_set.push(item);
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("bytes 100-200"));
    }

    #[test]
    fn test_render_raw() {
        let mut result_set = ResultSet::new();
        let mut item1 = ResultItem::file("a.rs");
        item1.excerpt = Some("content 1".to_string());
        let mut item2 = ResultItem::file("b.rs");
        item2.excerpt = Some("content 2".to_string());
        result_set.push(item1);
        result_set.push(item2);
        
        let renderer = Renderer::new(OutputFormat::Raw);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("content 1"));
        assert!(output.contains("content 2"));
        assert!(output.contains("---"));
    }

    #[test]
    fn test_render_raw_no_excerpt() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("test.rs")); // No excerpt
        
        let renderer = Renderer::new(OutputFormat::Raw);
        let output = renderer.render(&result_set);
        
        assert!(output.is_empty());
    }

    #[test]
    fn test_render_to_writer() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::file("test.rs"));
        
        let renderer = Renderer::new(OutputFormat::Json);
        let mut buffer = Vec::new();
        renderer.render_to(&result_set, &mut buffer).unwrap();
        
        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("test.rs"));
    }

    #[test]
    fn test_render_markdown_flow() {
        let mut result_set = ResultSet::new();
        let mut item = ResultItem::file("test.rs");
        item.kind = Kind::Flow;
        item.excerpt = Some("flow result".to_string());
        result_set.push(item);
        
        let renderer = Renderer::new(OutputFormat::Markdown);
        let output = renderer.render(&result_set);
        
        assert!(output.contains("## Flow Results"));
    }

    #[test]
    fn test_output_format_default() {
        let format: OutputFormat = Default::default();
        assert_eq!(format, OutputFormat::Jsonl);
    }
}
