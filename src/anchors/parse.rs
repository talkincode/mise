//! Anchor parsing module
//!
//! Parses anchor markers from files:
//! <!--Q:begin id=xxx tags=a,b v=1-->
//! ...content...
//! <!--Q:end id=xxx-->

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::core::file_reader::{read_file_safe, FileReadResult};
use crate::core::model::{Meta, Range, RangeLine};
use crate::core::util::{hash_bytes, HashAlgorithm};

/// Static regex for parsing anchor begin markers
/// Format: <!--Q:begin id=xxx tags=a,b v=1-->
pub static BEGIN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"<!--\s*Q:begin\s+id=([^\s]+)(?:\s+tags=([^\s]+))?(?:\s+v=(\d+))?\s*-->"#)
        .expect("Invalid BEGIN_RE regex")
});

/// Static regex for parsing anchor end markers
/// Format: <!--Q:end id=xxx-->
pub static END_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<!--\s*Q:end\s+id=([^\s]+)\s*-->"#).expect("Invalid END_RE regex"));

/// Anchor definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    /// Unique identifier
    pub id: String,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Version number
    pub version: u32,

    /// File path (relative to root)
    pub path: String,

    /// Line range of anchor content
    pub range: RangeLine,

    /// Content hash
    pub hash: String,

    /// The content between begin and end markers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Parse result for anchor begin marker
#[derive(Debug)]
struct BeginMarker {
    id: String,
    tags: Vec<String>,
    version: u32,
    line: u32,
}

/// Result of parsing a file for anchors
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParseFileResult {
    /// Parsed anchors
    pub anchors: Vec<Anchor>,
    /// File read result with warnings
    pub read_result: FileReadResult,
}

/// Parse anchors from a file with full result info
#[allow(dead_code)]
pub fn parse_file_with_result(path: &Path, relative_path: &str) -> ParseFileResult {
    let read_result = read_file_safe(path);

    let anchors = match &read_result.content {
        Some(content) => parse_content(content, relative_path),
        None => Vec::new(),
    };

    ParseFileResult {
        anchors,
        read_result,
    }
}

/// Parse anchors from a file (simple API, ignores warnings)
pub fn parse_file(path: &Path, relative_path: &str) -> Vec<Anchor> {
    parse_file_with_result(path, relative_path).anchors
}

/// Parse anchors from content string
pub fn parse_content(content: &str, path: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    let mut open_markers: Vec<BeginMarker> = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line_num = line_num as u32 + 1; // 1-indexed

        // Check for begin marker
        if let Some(caps) = BEGIN_RE.captures(line) {
            let id = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let tags = caps
                .get(2)
                .map(|m| {
                    m.as_str()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let version = caps
                .get(3)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(1);

            open_markers.push(BeginMarker {
                id,
                tags,
                version,
                line: line_num,
            });
        }

        // Check for end marker
        if let Some(caps) = END_RE.captures(line) {
            let end_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");

            // Find matching begin marker
            if let Some(pos) = open_markers.iter().rposition(|m| m.id == end_id) {
                let begin = open_markers.remove(pos);

                // Extract content between markers
                let content_start = begin.line as usize; // Line after begin
                let content_end = line_num as usize - 1; // Line before end

                let anchor_content = if content_start < content_end && content_end <= lines.len() {
                    Some(lines[content_start..content_end].join("\n"))
                } else {
                    None
                };

                let hash = anchor_content
                    .as_ref()
                    .map(|c| hash_bytes(c.as_bytes(), HashAlgorithm::Xxh3))
                    .unwrap_or_default();

                anchors.push(Anchor {
                    id: begin.id,
                    tags: begin.tags,
                    version: begin.version,
                    path: path.to_string(),
                    range: RangeLine {
                        start: begin.line,
                        end: line_num,
                    },
                    hash,
                    content: anchor_content,
                });
            }
        }
    }

    anchors
}

/// Convert anchor to ResultItem
impl Anchor {
    pub fn to_result_item(&self) -> crate::core::model::ResultItem {
        let mut item =
            crate::core::model::ResultItem::anchor(self.path.clone(), Range::Line(self.range));

        item.excerpt = self.content.clone();
        item.meta = Meta {
            hash: Some(self.hash.clone()),
            ..Default::default()
        };

        item
    }

    /// Convert anchor to brief ResultItem (metadata only, no content)
    /// This saves tokens for initial exploration - use `anchor get <id>` for full content
    pub fn to_result_item_brief(&self) -> crate::core::model::ResultItem {
        use serde_json::json;

        let mut item =
            crate::core::model::ResultItem::anchor(self.path.clone(), Range::Line(self.range));

        // No excerpt in brief mode
        item.excerpt = None;

        // Store anchor metadata (id, tags, version) in data field
        item.data = Some(json!({
            "id": self.id,
            "tags": self.tags,
            "version": self.version,
        }));

        item.meta = Meta {
            hash: Some(self.hash.clone()),
            ..Default::default()
        };

        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_anchor() {
        let content = r#"
Some text before

<!--Q:begin id=test1 tags=chapter,intro v=1-->
This is the content
of the anchor
<!--Q:end id=test1-->

Some text after
"#;
        let anchors = parse_content(content, "test.md");
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].id, "test1");
        assert_eq!(anchors[0].tags, vec!["chapter", "intro"]);
        assert_eq!(anchors[0].version, 1);
    }

    #[test]
    fn test_parse_nested_anchors() {
        let content = r#"
<!--Q:begin id=outer tags=parent v=1-->
Outer start
<!--Q:begin id=inner tags=child v=1-->
Inner content
<!--Q:end id=inner-->
Outer end
<!--Q:end id=outer-->
"#;
        let anchors = parse_content(content, "test.md");
        assert_eq!(anchors.len(), 2);
    }

    #[test]
    fn test_parse_no_tags() {
        let content = r#"
<!--Q:begin id=notags-->
Content without tags
<!--Q:end id=notags-->
"#;
        let anchors = parse_content(content, "test.md");
        assert_eq!(anchors.len(), 1);
        assert!(anchors[0].tags.is_empty());
        assert_eq!(anchors[0].version, 1);
    }
}
