//! Unified Result Model
//!
//! All commands (internal or external tools) must map to this unified Result Model
//! before rendering output.

use serde::{Deserialize, Serialize};

/// The kind of result item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Match,
    Extract,
    Anchor,
    Flow,
    Error,
}

/// Confidence level of a result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Source mode indicating how the result was obtained
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceMode {
    Scan,
    Rg,
    AstGrep,
    Anchor,
    Mixed,
}

/// Line-based range
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeLine {
    pub start: u32,
    pub end: u32,
}

/// Byte-based range
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeByte {
    pub start: u64,
    pub end: u64,
}

/// Range can be either line-based or byte-based
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Range {
    Line(RangeLine),
    Byte(RangeByte),
}

impl Range {
    /// Create a new line range
    pub fn lines(start: u32, end: u32) -> Self {
        Range::Line(RangeLine { start, end })
    }

    /// Create a new byte range
    #[allow(dead_code)]
    pub fn bytes(start: u64, end: u64) -> Self {
        Range::Byte(RangeByte { start, end })
    }
}

/// Metadata for a result item
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    /// Modification time in milliseconds since epoch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<i64>,

    /// File size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Content hash (SHA1 or XXH3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,

    /// Whether the content was truncated
    #[serde(default)]
    pub truncated: bool,
}

/// Error information for a result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiseError {
    pub code: String,
    pub message: String,
}

impl MiseError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The unified result item that all commands must produce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultItem {
    /// The kind of this result
    pub kind: Kind,

    /// Path relative to root, using '/' as separator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Range within the file (line or byte based)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,

    /// Excerpt of the content (may be truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,

    /// Confidence level
    pub confidence: Confidence,

    /// How this result was obtained
    pub source_mode: SourceMode,

    /// Metadata
    pub meta: Meta,

    /// Errors (if any)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<MiseError>,
}

impl ResultItem {
    /// Create a new file result
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            kind: Kind::File,
            path: Some(path.into()),
            range: None,
            excerpt: None,
            confidence: Confidence::High,
            source_mode: SourceMode::Scan,
            meta: Meta::default(),
            errors: Vec::new(),
        }
    }

    /// Create a new match result
    pub fn match_result(path: impl Into<String>, range: Range, excerpt: impl Into<String>) -> Self {
        Self {
            kind: Kind::Match,
            path: Some(path.into()),
            range: Some(range),
            excerpt: Some(excerpt.into()),
            confidence: Confidence::High,
            source_mode: SourceMode::Rg,
            meta: Meta::default(),
            errors: Vec::new(),
        }
    }

    /// Create a new extract result
    pub fn extract(path: impl Into<String>, range: Range, excerpt: impl Into<String>) -> Self {
        Self {
            kind: Kind::Extract,
            path: Some(path.into()),
            range: Some(range),
            excerpt: Some(excerpt.into()),
            confidence: Confidence::High,
            source_mode: SourceMode::Scan,
            meta: Meta::default(),
            errors: Vec::new(),
        }
    }

    /// Create a new anchor result
    pub fn anchor(path: impl Into<String>, range: Range) -> Self {
        Self {
            kind: Kind::Anchor,
            path: Some(path.into()),
            range: Some(range),
            excerpt: None,
            confidence: Confidence::High,
            source_mode: SourceMode::Anchor,
            meta: Meta::default(),
            errors: Vec::new(),
        }
    }

    /// Create a new error result
    pub fn error(error: MiseError) -> Self {
        Self {
            kind: Kind::Error,
            path: None,
            range: None,
            excerpt: None,
            confidence: Confidence::High,
            source_mode: SourceMode::Scan,
            meta: Meta::default(),
            errors: vec![error],
        }
    }

    /// Set metadata
    pub fn with_meta(mut self, meta: Meta) -> Self {
        self.meta = meta;
        self
    }

    /// Set confidence level
    #[allow(dead_code)]
    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = confidence;
        self
    }

    /// Set source mode
    #[allow(dead_code)]
    pub fn with_source_mode(mut self, source_mode: SourceMode) -> Self {
        self.source_mode = source_mode;
        self
    }

    /// Add an error
    #[allow(dead_code)]
    pub fn with_error(mut self, error: MiseError) -> Self {
        self.errors.push(error);
        self
    }
}

/// Result set containing multiple result items
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResultSet {
    pub items: Vec<ResultItem>,
}

impl ResultSet {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn push(&mut self, item: ResultItem) {
        self.items.push(item);
    }

    #[allow(dead_code)]
    pub fn extend(&mut self, items: impl IntoIterator<Item = ResultItem>) {
        self.items.extend(items);
    }

    /// Sort items by path and range start for stable output
    pub fn sort(&mut self) {
        self.items.sort_by(|a, b| {
            match (&a.path, &b.path) {
                (Some(pa), Some(pb)) => {
                    let path_cmp = pa.cmp(pb);
                    if path_cmp != std::cmp::Ordering::Equal {
                        return path_cmp;
                    }
                    // Compare by range start if paths are equal
                    match (&a.range, &b.range) {
                        (Some(Range::Line(ra)), Some(Range::Line(rb))) => ra.start.cmp(&rb.start),
                        (Some(Range::Byte(ra)), Some(Range::Byte(rb))) => ra.start.cmp(&rb.start),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl IntoIterator for ResultSet {
    type Item = ResultItem;
    type IntoIter = std::vec::IntoIter<ResultItem>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl FromIterator<ResultItem> for ResultSet {
    fn from_iter<T: IntoIterator<Item = ResultItem>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_item_file() {
        let item = ResultItem::file("src/main.rs");
        assert_eq!(item.kind, Kind::File);
        assert_eq!(item.path, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_result_set_sort() {
        let mut set = ResultSet::new();
        set.push(ResultItem::file("src/b.rs"));
        set.push(ResultItem::file("src/a.rs"));
        set.sort();
        assert_eq!(set.items[0].path, Some("src/a.rs".to_string()));
        assert_eq!(set.items[1].path, Some("src/b.rs".to_string()));
    }
}
