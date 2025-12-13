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

    /// Structured data payload for commands like deps/impact
    /// Allows direct embedding of structured data without JSON-in-string escaping
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

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
            data: None,
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
            data: None,
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
            data: None,
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
            data: None,
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
            data: None,
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

    /// Set structured data payload
    #[allow(dead_code)]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
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

    #[test]
    fn test_result_item_match_result() {
        let item = ResultItem::match_result("test.rs", Range::lines(10, 20), "content");
        assert_eq!(item.kind, Kind::Match);
        assert_eq!(item.path, Some("test.rs".to_string()));
        assert!(matches!(item.range, Some(Range::Line(_))));
        assert_eq!(item.excerpt, Some("content".to_string()));
        assert_eq!(item.source_mode, SourceMode::Rg);
    }

    #[test]
    fn test_result_item_extract() {
        let item = ResultItem::extract("lib.rs", Range::lines(1, 5), "use std;");
        assert_eq!(item.kind, Kind::Extract);
        assert_eq!(item.source_mode, SourceMode::Scan);
    }

    #[test]
    fn test_result_item_anchor() {
        let item = ResultItem::anchor("doc.md", Range::lines(5, 10));
        assert_eq!(item.kind, Kind::Anchor);
        assert_eq!(item.source_mode, SourceMode::Anchor);
        assert!(item.excerpt.is_none());
    }

    #[test]
    fn test_result_item_error() {
        let item = ResultItem::error(MiseError::new("ERR001", "Something went wrong"));
        assert_eq!(item.kind, Kind::Error);
        assert_eq!(item.errors.len(), 1);
        assert_eq!(item.errors[0].code, "ERR001");
        assert_eq!(item.errors[0].message, "Something went wrong");
    }

    #[test]
    fn test_result_item_with_meta() {
        let meta = Meta {
            mtime_ms: Some(12345),
            size: Some(1024),
            hash: Some("abc123".to_string()),
            truncated: true,
        };
        let item = ResultItem::file("test.rs").with_meta(meta);
        assert_eq!(item.meta.mtime_ms, Some(12345));
        assert_eq!(item.meta.size, Some(1024));
        assert!(item.meta.truncated);
    }

    #[test]
    fn test_result_item_with_confidence() {
        let item = ResultItem::file("test.rs").with_confidence(Confidence::Low);
        assert_eq!(item.confidence, Confidence::Low);
    }

    #[test]
    fn test_result_item_with_source_mode() {
        let item = ResultItem::file("test.rs").with_source_mode(SourceMode::AstGrep);
        assert_eq!(item.source_mode, SourceMode::AstGrep);
    }

    #[test]
    fn test_result_item_with_error() {
        let item =
            ResultItem::file("test.rs").with_error(MiseError::new("WARN", "Warning message"));
        assert_eq!(item.errors.len(), 1);
    }

    #[test]
    fn test_result_item_with_data() {
        let data = serde_json::json!({
            "depends_on": ["a.rs", "b.rs"],
            "language": "rust"
        });
        let item = ResultItem::file("test.rs").with_data(data.clone());
        assert!(item.data.is_some());
        assert_eq!(item.data.unwrap(), data);
    }

    #[test]
    fn test_result_item_data_serialization() {
        let data = serde_json::json!({
            "deps": ["x.rs"],
            "count": 42
        });
        let item = ResultItem::file("test.rs").with_data(data);
        let json = serde_json::to_string(&item).unwrap();
        // data field should be embedded directly, not as escaped string
        assert!(json.contains("\"data\":{"));
        assert!(json.contains("\"deps\":[\"x.rs\"]"));
        assert!(json.contains("\"count\":42"));
    }

    #[test]
    fn test_range_lines() {
        let range = Range::lines(10, 20);
        if let Range::Line(r) = range {
            assert_eq!(r.start, 10);
            assert_eq!(r.end, 20);
        } else {
            panic!("Expected Line range");
        }
    }

    #[test]
    fn test_range_bytes() {
        let range = Range::bytes(100, 200);
        if let Range::Byte(r) = range {
            assert_eq!(r.start, 100);
            assert_eq!(r.end, 200);
        } else {
            panic!("Expected Byte range");
        }
    }

    #[test]
    fn test_result_set_new() {
        let set = ResultSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_result_set_push() {
        let mut set = ResultSet::new();
        set.push(ResultItem::file("a.rs"));
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
    }

    #[test]
    fn test_result_set_extend() {
        let mut set = ResultSet::new();
        set.extend(vec![ResultItem::file("a.rs"), ResultItem::file("b.rs")]);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_result_set_into_iter() {
        let mut set = ResultSet::new();
        set.push(ResultItem::file("a.rs"));
        set.push(ResultItem::file("b.rs"));

        let items: Vec<_> = set.into_iter().collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_result_set_from_iter() {
        let items = vec![ResultItem::file("a.rs"), ResultItem::file("b.rs")];
        let set: ResultSet = items.into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_result_set_sort_by_range() {
        let mut set = ResultSet::new();
        set.push(ResultItem::match_result(
            "test.rs",
            Range::lines(20, 30),
            "b",
        ));
        set.push(ResultItem::match_result(
            "test.rs",
            Range::lines(10, 15),
            "a",
        ));
        set.sort();

        if let Some(Range::Line(r)) = &set.items[0].range {
            assert_eq!(r.start, 10);
        }
        if let Some(Range::Line(r)) = &set.items[1].range {
            assert_eq!(r.start, 20);
        }
    }

    #[test]
    fn test_result_set_sort_with_none_paths() {
        let mut set = ResultSet::new();
        set.push(ResultItem::error(MiseError::new("ERR", "error"))); // path is None
        set.push(ResultItem::file("a.rs"));
        set.sort();

        // Items with path should come before items without
        assert!(set.items[0].path.is_some());
    }

    #[test]
    fn test_result_set_sort_byte_ranges() {
        let mut set = ResultSet::new();
        let mut item1 = ResultItem::file("test.rs");
        item1.range = Some(Range::bytes(200, 300));
        let mut item2 = ResultItem::file("test.rs");
        item2.range = Some(Range::bytes(100, 150));
        set.push(item1);
        set.push(item2);
        set.sort();

        if let Some(Range::Byte(r)) = &set.items[0].range {
            assert_eq!(r.start, 100);
        }
    }

    #[test]
    fn test_meta_default() {
        let meta = Meta::default();
        assert!(meta.mtime_ms.is_none());
        assert!(meta.size.is_none());
        assert!(meta.hash.is_none());
        assert!(!meta.truncated);
    }

    #[test]
    fn test_mise_error_new() {
        let err = MiseError::new("CODE", "message");
        assert_eq!(err.code, "CODE");
        assert_eq!(err.message, "message");
    }

    #[test]
    fn test_mise_error_with_string() {
        let err = MiseError::new(String::from("CODE"), String::from("message"));
        assert_eq!(err.code, "CODE");
    }

    #[test]
    fn test_kind_serialization() {
        let item = ResultItem::file("test.rs");
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"kind\":\"file\""));
    }

    #[test]
    fn test_confidence_serialization() {
        let item = ResultItem::file("test.rs").with_confidence(Confidence::Medium);
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"confidence\":\"medium\""));
    }

    #[test]
    fn test_source_mode_serialization() {
        let item = ResultItem::file("test.rs").with_source_mode(SourceMode::Mixed);
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"source_mode\":\"mixed\""));
    }

    #[test]
    fn test_result_item_deserialization() {
        let json = r#"{"kind":"file","path":"test.rs","confidence":"high","source_mode":"scan","meta":{"truncated":false}}"#;
        let item: ResultItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.kind, Kind::File);
        assert_eq!(item.path, Some("test.rs".to_string()));
    }

    #[test]
    fn test_result_set_default() {
        let set: ResultSet = Default::default();
        assert!(set.is_empty());
    }

    #[test]
    fn test_result_set_sort_byte_range() {
        let mut set = ResultSet::new();

        // Create items with byte ranges
        let mut item1 = ResultItem::file("file.rs");
        item1.range = Some(Range::bytes(200, 300));

        let mut item2 = ResultItem::file("file.rs");
        item2.range = Some(Range::bytes(100, 150));

        set.push(item1);
        set.push(item2);
        set.sort();

        // Should be sorted by byte range start
        if let Some(Range::Byte(r)) = &set.items[0].range {
            assert_eq!(r.start, 100);
        }
        if let Some(Range::Byte(r)) = &set.items[1].range {
            assert_eq!(r.start, 200);
        }
    }

    #[test]
    fn test_result_set_sort_with_some_none_ranges() {
        let mut set = ResultSet::new();

        // Item without range
        let item_none = ResultItem::file("file.rs");

        // Item with range
        let mut item_some = ResultItem::file("file.rs");
        item_some.range = Some(Range::lines(10, 20));

        set.push(item_none.clone());
        set.push(item_some.clone());
        set.sort();

        // Item with range should come before item without range
        assert!(set.items[0].range.is_some());
        assert!(set.items[1].range.is_none());
    }

    #[test]
    fn test_result_set_sort_none_some_ranges() {
        let mut set = ResultSet::new();

        // Item with range
        let mut item_some = ResultItem::file("file.rs");
        item_some.range = Some(Range::lines(10, 20));

        // Item without range
        let item_none = ResultItem::file("file.rs");

        // Add in reverse order
        set.push(item_some);
        set.push(item_none);
        set.sort();

        // Item with range should come first
        assert!(set.items[0].range.is_some());
    }

    #[test]
    fn test_result_set_sort_both_none_ranges() {
        let mut set = ResultSet::new();

        let item1 = ResultItem::file("a.rs");
        let item2 = ResultItem::file("a.rs");

        set.push(item1);
        set.push(item2);
        set.sort();

        // Both should remain (both are "equal" in sort order)
        assert_eq!(set.items.len(), 2);
    }

    #[test]
    fn test_result_set_sort_path_comparison() {
        let mut set = ResultSet::new();

        // Item without path
        let item_none = ResultItem {
            kind: Kind::File,
            path: None,
            range: None,
            excerpt: None,
            data: None,
            confidence: Confidence::High,
            source_mode: SourceMode::Scan,
            meta: Meta::default(),
            errors: vec![],
        };

        // Item with path
        let item_some = ResultItem::file("test.rs");

        set.push(item_none);
        set.push(item_some);
        set.sort();

        // Item with path should come before item without path
        assert!(set.items[0].path.is_some());
        assert!(set.items[1].path.is_none());
    }
}
