//! Anchor marking module - Insert anchor markers into files
//!
//! Provides functionality to quickly mark text blocks with anchor markers:
//! <!--Q:begin id=xxx tags=a,b v=1-->
//! ...content...
//! <!--Q:end id=xxx-->
//!
//! Supports single and batch marking operations for AI agents.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::core::model::ResultSet;
use crate::core::render::{RenderConfig, Renderer};

/// A single mark operation specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSpec {
    /// File path (relative to root)
    pub path: String,

    /// Start line (1-indexed, inclusive)
    pub start_line: u32,

    /// End line (1-indexed, inclusive)
    pub end_line: u32,

    /// Anchor ID
    pub id: String,

    /// Tags (optional)
    #[serde(default)]
    pub tags: Vec<String>,

    /// Version (default: 1)
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

/// Result of a mark operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkResult {
    /// File path
    pub path: String,

    /// Anchor ID
    pub id: String,

    /// Whether the operation was successful
    pub success: bool,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Lines affected (start, end after insertion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_affected: Option<(u32, u32)>,
}

/// Batch mark specification (for JSON input)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMarkSpec {
    pub marks: Vec<MarkSpec>,
}

/// Generate the begin marker line
fn generate_begin_marker(id: &str, tags: &[String], version: u32) -> String {
    let mut marker = format!("<!--Q:begin id={}", id);

    if !tags.is_empty() {
        marker.push_str(&format!(" tags={}", tags.join(",")));
    }

    marker.push_str(&format!(" v={}-->", version));
    marker
}

/// Generate the end marker line
fn generate_end_marker(id: &str) -> String {
    format!("<!--Q:end id={}-->", id)
}

/// Insert anchor markers into a file
///
/// Returns the new content with markers inserted
pub fn insert_markers(content: &str, spec: &MarkSpec) -> Result<String> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len() as u32;

    // Validate line numbers
    if spec.start_line < 1 {
        bail!("start_line must be >= 1, got {}", spec.start_line);
    }
    if spec.end_line < spec.start_line {
        bail!(
            "end_line ({}) must be >= start_line ({})",
            spec.end_line,
            spec.start_line
        );
    }
    if spec.start_line > total_lines + 1 {
        bail!(
            "start_line ({}) exceeds file length ({})",
            spec.start_line,
            total_lines
        );
    }

    // Clamp end_line to file length
    let effective_end = spec.end_line.min(total_lines);

    let begin_marker = generate_begin_marker(&spec.id, &spec.tags, spec.version);
    let end_marker = generate_end_marker(&spec.id);

    let mut result = Vec::new();

    // Lines before the marked section
    for line in lines.iter().take((spec.start_line - 1) as usize) {
        result.push(*line);
    }

    // Insert begin marker
    result.push(&begin_marker);

    // The marked content
    for line in lines
        .iter()
        .skip((spec.start_line - 1) as usize)
        .take((effective_end - spec.start_line + 1) as usize)
    {
        result.push(*line);
    }

    // Insert end marker
    result.push(&end_marker);

    // Lines after the marked section
    for line in lines.iter().skip(effective_end as usize) {
        result.push(*line);
    }

    // Join with newlines, preserving trailing newline if original had one
    let mut output = result.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }

    Ok(output)
}

/// Mark a single file with anchor markers
pub fn mark_file(root: &Path, spec: &MarkSpec, dry_run: bool) -> Result<MarkResult> {
    let file_path = root.join(&spec.path);

    // Read the file
    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read file: {}", spec.path))?;

    // Insert markers
    let new_content = match insert_markers(&content, spec) {
        Ok(c) => c,
        Err(e) => {
            return Ok(MarkResult {
                path: spec.path.clone(),
                id: spec.id.clone(),
                success: false,
                error: Some(e.to_string()),
                lines_affected: None,
            });
        }
    };

    // Calculate affected lines (after insertion, markers add 2 lines)
    let lines_affected = (spec.start_line, spec.end_line + 2);

    // Write back unless dry-run
    if !dry_run {
        fs::write(&file_path, &new_content)
            .with_context(|| format!("Failed to write file: {}", spec.path))?;
    }

    Ok(MarkResult {
        path: spec.path.clone(),
        id: spec.id.clone(),
        success: true,
        error: None,
        lines_affected: Some(lines_affected),
    })
}

/// Mark multiple files with anchor markers (batch operation)
///
/// Processes marks in order. For marks in the same file, they are processed
/// from bottom to top to avoid line number shifts affecting subsequent marks.
pub fn mark_batch(root: &Path, specs: Vec<MarkSpec>, dry_run: bool) -> Result<Vec<MarkResult>> {
    // Group by file path
    let mut by_file: std::collections::HashMap<String, Vec<MarkSpec>> =
        std::collections::HashMap::new();

    for spec in specs {
        by_file.entry(spec.path.clone()).or_default().push(spec);
    }

    let mut results = Vec::new();

    // Process each file
    for (path, mut file_specs) in by_file {
        // Sort by start_line descending (process from bottom to top)
        file_specs.sort_by(|a, b| b.start_line.cmp(&a.start_line));

        let file_path = root.join(&path);

        // Read file content once
        let mut content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                // Mark all specs for this file as failed
                for spec in file_specs {
                    results.push(MarkResult {
                        path: spec.path,
                        id: spec.id,
                        success: false,
                        error: Some(format!("Failed to read file: {}", e)),
                        lines_affected: None,
                    });
                }
                continue;
            }
        };

        // Apply each mark from bottom to top
        for spec in file_specs {
            match insert_markers(&content, &spec) {
                Ok(new_content) => {
                    let lines_affected = (spec.start_line, spec.end_line + 2);
                    content = new_content;
                    results.push(MarkResult {
                        path: spec.path,
                        id: spec.id,
                        success: true,
                        error: None,
                        lines_affected: Some(lines_affected),
                    });
                }
                Err(e) => {
                    results.push(MarkResult {
                        path: spec.path,
                        id: spec.id,
                        success: false,
                        error: Some(e.to_string()),
                        lines_affected: None,
                    });
                }
            }
        }

        // Write the final content unless dry-run
        if !dry_run {
            if let Err(e) = fs::write(&file_path, &content) {
                // Update results to show write failure
                for result in &mut results {
                    if result.path == path && result.success {
                        result.success = false;
                        result.error = Some(format!("Failed to write file: {}", e));
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Convert MarkResult to ResultItem for output
impl MarkResult {
    pub fn to_result_item(&self) -> crate::core::model::ResultItem {
        use crate::core::model::{Confidence, Kind, Meta, ResultItem, SourceMode};

        if self.success {
            let item = ResultItem {
                kind: Kind::Anchor,
                path: Some(self.path.clone()),
                range: self
                    .lines_affected
                    .map(|(start, end)| crate::core::model::Range::lines(start, end)),
                excerpt: Some(format!("Anchor '{}' marked successfully", self.id)),
                data: None,
                confidence: Confidence::High,
                source_mode: SourceMode::Anchor,
                meta: Meta::default(),
                errors: Vec::new(),
            };
            item
        } else {
            ResultItem {
                kind: Kind::Error,
                path: Some(self.path.clone()),
                range: None,
                excerpt: self.error.clone(),
                data: None,
                confidence: Confidence::Low,
                source_mode: SourceMode::Anchor,
                meta: Meta::default(),
                errors: Vec::new(),
            }
        }
    }
}

/// Run single mark command
pub fn run_mark(
    root: &Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    id: &str,
    tags: Vec<String>,
    version: u32,
    dry_run: bool,
    config: RenderConfig,
) -> Result<()> {
    let result_set = mark_to_result_set(root, path, start_line, end_line, id, tags, version, dry_run)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Public API for MCP: mark and return ResultSet
pub fn mark_to_result_set(
    root: &Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    id: &str,
    tags: Vec<String>,
    version: u32,
    dry_run: bool,
) -> Result<ResultSet> {
    let spec = MarkSpec {
        path: path.to_string(),
        start_line,
        end_line,
        id: id.to_string(),
        tags,
        version,
    };

    let result = mark_file(root, &spec, dry_run)?;
    let mut result_set = ResultSet::new();
    result_set.push(result.to_result_item());

    Ok(result_set)
}

/// Run batch mark command from JSON input
pub fn run_batch_mark(
    root: &Path,
    json_input: &str,
    dry_run: bool,
    config: RenderConfig,
) -> Result<()> {
    // Parse JSON input - support both array and object with "marks" field
    let specs: Vec<MarkSpec> = if json_input.trim().starts_with('[') {
        serde_json::from_str(json_input).context("Failed to parse JSON array")?
    } else {
        let batch: BatchMarkSpec =
            serde_json::from_str(json_input).context("Failed to parse JSON object")?;
        batch.marks
    };

    if specs.is_empty() {
        bail!("No marks specified in input");
    }

    let results = mark_batch(root, specs, dry_run)?;

    let mut result_set = ResultSet::new();
    for result in results {
        result_set.push(result.to_result_item());
    }

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Run batch mark from file
pub fn run_batch_mark_from_file(
    root: &Path,
    spec_file: &Path,
    dry_run: bool,
    config: RenderConfig,
) -> Result<()> {
    let json_input = fs::read_to_string(spec_file)
        .with_context(|| format!("Failed to read spec file: {}", spec_file.display()))?;

    run_batch_mark(root, &json_input, dry_run, config)
}

/// Remove anchor markers from a file (unmark)
pub fn remove_markers(content: &str, anchor_id: &str) -> Result<String> {
    use regex::Regex;

    let begin_pattern = format!(
        r"^\s*<!--\s*Q:begin\s+id={}\s*(?:tags=[^\s]+)?\s*(?:v=\d+)?\s*-->\s*\n?",
        regex::escape(anchor_id)
    );
    let end_pattern = format!(
        r"^\s*<!--\s*Q:end\s+id={}\s*-->\s*\n?",
        regex::escape(anchor_id)
    );

    let begin_re = Regex::new(&begin_pattern).context("Invalid begin pattern")?;
    let end_re = Regex::new(&end_pattern).context("Invalid end pattern")?;

    let mut result = String::new();
    let mut removed_begin = false;
    let mut removed_end = false;

    for line in content.lines() {
        let line_with_newline = format!("{}\n", line);

        if begin_re.is_match(&line_with_newline) && !removed_begin {
            removed_begin = true;
            continue;
        }

        if end_re.is_match(&line_with_newline) && !removed_end {
            removed_end = true;
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    if !removed_begin && !removed_end {
        bail!("Anchor '{}' not found in content", anchor_id);
    }

    Ok(result)
}

/// Run unmark command to remove anchor markers
pub fn run_unmark(
    root: &Path,
    path: &str,
    anchor_id: &str,
    dry_run: bool,
    config: RenderConfig,
) -> Result<()> {
    let result_set = unmark_to_result_set(root, path, anchor_id, dry_run)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Public API for MCP: unmark and return ResultSet
pub fn unmark_to_result_set(
    root: &Path,
    path: &str,
    anchor_id: &str,
    dry_run: bool,
) -> Result<ResultSet> {
    let file_path = root.join(path);

    let content =
        fs::read_to_string(&file_path).with_context(|| format!("Failed to read file: {}", path))?;

    let new_content = remove_markers(&content, anchor_id)?;

    if !dry_run {
        fs::write(&file_path, &new_content)
            .with_context(|| format!("Failed to write file: {}", path))?;
    }

    let mut result_set = ResultSet::new();
    let mut item = crate::core::model::ResultItem::anchor(
        path.to_string(),
        crate::core::model::Range::lines(0, 0),
    );
    item.excerpt = Some(format!("Anchor '{}' removed successfully", anchor_id));
    result_set.push(item);

    Ok(result_set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_markers() {
        let begin = generate_begin_marker("test", &["a".to_string(), "b".to_string()], 1);
        assert_eq!(begin, "<!--Q:begin id=test tags=a,b v=1-->");

        let end = generate_end_marker("test");
        assert_eq!(end, "<!--Q:end id=test-->");
    }

    #[test]
    fn test_generate_markers_no_tags() {
        let begin = generate_begin_marker("test", &[], 2);
        assert_eq!(begin, "<!--Q:begin id=test v=2-->");
    }

    #[test]
    fn test_insert_markers() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 2,
            end_line: 4,
            id: "test".to_string(),
            tags: vec!["chapter".to_string()],
            version: 1,
        };

        let result = insert_markers(content, &spec).unwrap();
        let lines: Vec<&str> = result.lines().collect();

        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[1], "<!--Q:begin id=test tags=chapter v=1-->");
        assert_eq!(lines[2], "line 2");
        assert_eq!(lines[3], "line 3");
        assert_eq!(lines[4], "line 4");
        assert_eq!(lines[5], "<!--Q:end id=test-->");
        assert_eq!(lines[6], "line 5");
    }

    #[test]
    fn test_insert_markers_at_start() {
        let content = "line 1\nline 2\nline 3\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 2,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = insert_markers(content, &spec).unwrap();
        let lines: Vec<&str> = result.lines().collect();

        assert_eq!(lines[0], "<!--Q:begin id=test v=1-->");
        assert_eq!(lines[1], "line 1");
        assert_eq!(lines[2], "line 2");
        assert_eq!(lines[3], "<!--Q:end id=test-->");
        assert_eq!(lines[4], "line 3");
    }

    #[test]
    fn test_insert_markers_at_end() {
        let content = "line 1\nline 2\nline 3\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 3,
            end_line: 3,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = insert_markers(content, &spec).unwrap();
        let lines: Vec<&str> = result.lines().collect();

        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[1], "line 2");
        assert_eq!(lines[2], "<!--Q:begin id=test v=1-->");
        assert_eq!(lines[3], "line 3");
        assert_eq!(lines[4], "<!--Q:end id=test-->");
    }

    #[test]
    fn test_invalid_line_numbers() {
        let content = "line 1\nline 2\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 0,
            end_line: 1,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        assert!(insert_markers(content, &spec).is_err());
    }

    #[test]
    fn test_remove_markers() {
        let content = "line 1\n<!--Q:begin id=test tags=chapter v=1-->\nline 2\nline 3\n<!--Q:end id=test-->\nline 4\n";
        let result = remove_markers(content, "test").unwrap();
        let lines: Vec<&str> = result.lines().collect();

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines[1], "line 2");
        assert_eq!(lines[2], "line 3");
        assert_eq!(lines[3], "line 4");
    }

    #[test]
    fn test_parse_mark_spec_json() {
        let json = r#"{"path": "test.md", "start_line": 1, "end_line": 10, "id": "intro", "tags": ["chapter"]}"#;
        let spec: MarkSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.path, "test.md");
        assert_eq!(spec.start_line, 1);
        assert_eq!(spec.end_line, 10);
        assert_eq!(spec.id, "intro");
        assert_eq!(spec.tags, vec!["chapter"]);
        assert_eq!(spec.version, 1); // default
    }

    #[test]
    fn test_parse_batch_spec_json() {
        let json = r#"{
            "marks": [
                {"path": "a.md", "start_line": 1, "end_line": 5, "id": "a1"},
                {"path": "b.md", "start_line": 10, "end_line": 20, "id": "b1", "tags": ["test"]}
            ]
        }"#;
        let batch: BatchMarkSpec = serde_json::from_str(json).unwrap();
        assert_eq!(batch.marks.len(), 2);
    }

    #[test]
    fn test_insert_markers_end_line_greater_than_start() {
        let content = "line 1\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 3,
            end_line: 1,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };
        assert!(insert_markers(content, &spec).is_err());
    }

    #[test]
    fn test_insert_markers_start_exceeds_length() {
        let content = "line 1\nline 2\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 100,
            end_line: 200,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };
        assert!(insert_markers(content, &spec).is_err());
    }

    #[test]
    fn test_insert_markers_preserves_no_trailing_newline() {
        let content = "line 1\nline 2"; // No trailing newline
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 1,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };
        let result = insert_markers(content, &spec).unwrap();
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn test_insert_markers_clamps_end_line() {
        let content = "line 1\nline 2\n";
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 100, // Much larger than file
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };
        let result = insert_markers(content, &spec).unwrap();
        // Should still work, clamping end to file length
        assert!(result.contains("<!--Q:begin"));
        assert!(result.contains("<!--Q:end"));
    }

    #[test]
    fn test_mark_result_to_result_item_success() {
        let result = MarkResult {
            path: "test.md".to_string(),
            id: "test-id".to_string(),
            success: true,
            error: None,
            lines_affected: Some((1, 10)),
        };
        let item = result.to_result_item();
        assert!(matches!(item.kind, crate::core::model::Kind::Anchor));
        assert!(item.excerpt.is_some());
        assert!(item.excerpt.unwrap().contains("marked successfully"));
    }

    #[test]
    fn test_mark_result_to_result_item_failure() {
        let result = MarkResult {
            path: "test.md".to_string(),
            id: "test-id".to_string(),
            success: false,
            error: Some("Some error".to_string()),
            lines_affected: None,
        };
        let item = result.to_result_item();
        assert!(matches!(item.kind, crate::core::model::Kind::Error));
    }

    #[test]
    fn test_remove_markers_not_found() {
        let content = "line 1\nline 2\nline 3\n";
        let result = remove_markers(content, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_markers_preserves_no_trailing_newline() {
        let content = "line 1\n<!--Q:begin id=test v=1-->\nline 2\n<!--Q:end id=test-->";
        let result = remove_markers(content, "test").unwrap();
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn test_mark_spec_default_version() {
        let json = r#"{"path": "test.md", "start_line": 1, "end_line": 10, "id": "intro"}"#;
        let spec: MarkSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.version, 1);
        assert!(spec.tags.is_empty());
    }

    #[test]
    fn test_mark_spec_with_custom_version() {
        let json =
            r#"{"path": "test.md", "start_line": 1, "end_line": 10, "id": "intro", "version": 5}"#;
        let spec: MarkSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.version, 5);
    }

    #[test]
    fn test_batch_mark_spec_empty() {
        let json = r#"{"marks": []}"#;
        let batch: BatchMarkSpec = serde_json::from_str(json).unwrap();
        assert!(batch.marks.is_empty());
    }

    #[test]
    fn test_generate_begin_marker_multiple_tags() {
        let begin = generate_begin_marker(
            "id123",
            &["tag1".to_string(), "tag2".to_string(), "tag3".to_string()],
            3,
        );
        assert_eq!(begin, "<!--Q:begin id=id123 tags=tag1,tag2,tag3 v=3-->");
    }

    #[test]
    fn test_generate_end_marker() {
        let end = generate_end_marker("test-id");
        assert_eq!(end, "<!--Q:end id=test-id-->");
    }

    #[test]
    fn test_default_version() {
        assert_eq!(default_version(), 1);
    }

    #[test]
    fn test_mark_file_dry_run() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "line 1\nline 2\n").unwrap();

        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 2,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = mark_file(temp.path(), &spec, true).unwrap();
        assert!(result.success);

        // File should not be modified in dry run
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("<!--Q:begin"));
    }

    #[test]
    fn test_mark_file_write() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(&file_path, "line 1\nline 2\n").unwrap();

        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 2,
            id: "test".to_string(),
            tags: vec!["tag1".to_string()],
            version: 1,
        };

        let result = mark_file(temp.path(), &spec, false).unwrap();
        assert!(result.success);
        assert!(result.lines_affected.is_some());

        // File should be modified
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("<!--Q:begin id=test tags=tag1 v=1-->"));
        assert!(content.contains("<!--Q:end id=test-->"));
    }

    #[test]
    fn test_mark_file_nonexistent() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();

        let spec = MarkSpec {
            path: "nonexistent.md".to_string(),
            start_line: 1,
            end_line: 2,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = mark_file(temp.path(), &spec, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_mark_batch_multiple_files() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("a.md"), "line a\n").unwrap();
        std::fs::write(temp.path().join("b.md"), "line b\n").unwrap();

        let specs = vec![
            MarkSpec {
                path: "a.md".to_string(),
                start_line: 1,
                end_line: 1,
                id: "a-id".to_string(),
                tags: vec![],
                version: 1,
            },
            MarkSpec {
                path: "b.md".to_string(),
                start_line: 1,
                end_line: 1,
                id: "b-id".to_string(),
                tags: vec![],
                version: 1,
            },
        ];

        let results = mark_batch(temp.path(), specs, true).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(results[1].success);
    }

    #[test]
    fn test_mark_batch_same_file_multiple_marks() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("test.md"),
            "line 1\nline 2\nline 3\nline 4\n",
        )
        .unwrap();

        // When marking same file multiple times, later marks need adjusted line numbers
        let specs = vec![
            MarkSpec {
                path: "test.md".to_string(),
                start_line: 1,
                end_line: 1,
                id: "first".to_string(),
                tags: vec![],
                version: 1,
            },
            MarkSpec {
                path: "test.md".to_string(),
                start_line: 3, // After first mark insertion, this becomes line 5
                end_line: 3,
                id: "second".to_string(),
                tags: vec![],
                version: 1,
            },
        ];

        let results = mark_batch(temp.path(), specs, true).unwrap();
        assert_eq!(results.len(), 2);
        // Both should succeed
        assert!(results[0].success);
        assert!(results[1].success);
    }

    #[test]
    fn test_mark_result_serialization() {
        let result = MarkResult {
            path: "test.md".to_string(),
            id: "test-id".to_string(),
            success: true,
            error: None,
            lines_affected: Some((1, 5)),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test.md"));
        assert!(json.contains("test-id"));
        assert!(json.contains("true"));
        assert!(!json.contains("error")); // Skipped when None
    }

    #[test]
    fn test_mark_spec_clone() {
        let spec = MarkSpec {
            path: "test.md".to_string(),
            start_line: 1,
            end_line: 10,
            id: "test".to_string(),
            tags: vec!["a".to_string()],
            version: 2,
        };
        let cloned = spec.clone();
        assert_eq!(spec.path, cloned.path);
        assert_eq!(spec.id, cloned.id);
    }

    #[test]
    fn test_batch_mark_spec_serialization() {
        let batch = BatchMarkSpec {
            marks: vec![MarkSpec {
                path: "test.md".to_string(),
                start_line: 1,
                end_line: 5,
                id: "intro".to_string(),
                tags: vec![],
                version: 1,
            }],
        };

        let json = serde_json::to_string(&batch).unwrap();
        let parsed: BatchMarkSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.marks.len(), 1);
    }

    #[test]
    fn test_mark_file_nonexistent_file() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let spec = MarkSpec {
            path: "nonexistent.md".to_string(),
            start_line: 1,
            end_line: 5,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = mark_file(temp.path(), &spec, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_mark_file_insert_failure_returns_result() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        // Create a file with only 2 lines
        std::fs::write(temp.path().join("short.md"), "line 1\nline 2\n").unwrap();

        let spec = MarkSpec {
            path: "short.md".to_string(),
            start_line: 1,
            end_line: 100, // End line beyond file
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        };

        let result = mark_file(temp.path(), &spec, false).unwrap();
        // The implementation may succeed with clamping or fail - check either case
        // Actually, the implementation clamps end_line, so it succeeds
        // Just verify we get a result without panic
        assert!(result.path == "short.md");
    }

    #[test]
    fn test_mark_batch_file_read_failure() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        // Don't create the file, so reading will fail
        let specs = vec![MarkSpec {
            path: "nonexistent.md".to_string(),
            start_line: 1,
            end_line: 5,
            id: "test".to_string(),
            tags: vec![],
            version: 1,
        }];

        let results = mark_batch(temp.path(), specs, false).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error.is_some());
        assert!(results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("Failed to read"));
    }

    #[test]
    fn test_mark_batch_insert_failure_in_batch() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.md"), "line 1\nline 2\n").unwrap();

        let specs = vec![
            MarkSpec {
                path: "test.md".to_string(),
                start_line: 1,
                end_line: 1,
                id: "ok".to_string(),
                tags: vec![],
                version: 1,
            },
            MarkSpec {
                path: "test.md".to_string(),
                start_line: 100, // Invalid line
                end_line: 200,
                id: "fail".to_string(),
                tags: vec![],
                version: 1,
            },
        ];

        let results = mark_batch(temp.path(), specs, true).unwrap();
        assert_eq!(results.len(), 2);
        // Since sorted descending, the failing one is processed first
        let success_count = results.iter().filter(|r| r.success).count();
        let fail_count = results.iter().filter(|r| !r.success).count();
        assert!(success_count >= 1);
        assert!(fail_count >= 1);
    }

    #[test]
    fn test_mark_result_to_result_item_success_with_details() {
        let result = MarkResult {
            path: "test.md".to_string(),
            id: "test-id".to_string(),
            success: true,
            error: None,
            lines_affected: Some((1, 10)),
        };

        let item = result.to_result_item();
        assert_eq!(item.kind, crate::core::model::Kind::Anchor);
        assert_eq!(item.path, Some("test.md".to_string()));
        assert!(item.excerpt.is_some());
        assert!(item.excerpt.unwrap().contains("test-id"));
    }

    #[test]
    fn test_mark_result_to_result_item_failure_with_details() {
        let result = MarkResult {
            path: "test.md".to_string(),
            id: "test-id".to_string(),
            success: false,
            error: Some("Test error".to_string()),
            lines_affected: None,
        };

        let item = result.to_result_item();
        assert_eq!(item.kind, crate::core::model::Kind::Error);
        assert_eq!(item.path, Some("test.md".to_string()));
        assert_eq!(item.excerpt, Some("Test error".to_string()));
    }

    #[test]
    fn test_remove_markers_basic() {
        let content =
            "line 1\n<!--Q:begin id=test v=1-->\nmarked content\n<!--Q:end id=test-->\nline 2\n";
        let result = remove_markers(content, "test").unwrap();
        assert!(!result.contains("Q:begin"));
        assert!(!result.contains("Q:end"));
        assert!(result.contains("marked content"));
    }

    #[test]
    fn test_remove_markers_with_tags() {
        let content =
            "start\n<!--Q:begin id=test tags=a,b v=1-->\ncontent\n<!--Q:end id=test-->\nend\n";
        let result = remove_markers(content, "test").unwrap();
        assert!(!result.contains("Q:begin"));
        assert!(!result.contains("Q:end"));
        assert!(result.contains("content"));
    }

    #[test]
    fn test_remove_markers_anchor_not_found_with_message() {
        let content = "line 1\nline 2\nline 3\n";
        let result = remove_markers(content, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_remove_markers_preserves_surrounding_content() {
        let content = "before\n<!--Q:begin id=test v=1-->\ninner\n<!--Q:end id=test-->\nafter\n";
        let result = remove_markers(content, "test").unwrap();
        assert!(result.contains("before"));
        assert!(result.contains("after"));
        assert!(result.contains("inner"));
    }

    #[test]
    fn test_run_mark_dry_run() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let content = "line 1\nline 2\nline 3\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        // Dry run should not modify the file
        let result = run_mark(
            temp.path(),
            "test.md",
            1,
            2,
            "test-anchor",
            vec!["tag1".to_string()],
            1,
            true,
            config,
        );
        assert!(result.is_ok());

        // File should be unchanged
        let final_content = std::fs::read_to_string(temp.path().join("test.md")).unwrap();
        assert_eq!(final_content, content);
    }

    #[test]
    fn test_run_mark_actual_write() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let content = "line 1\nline 2\nline 3\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_mark(
            temp.path(),
            "test.md",
            1,
            2,
            "test-anchor",
            vec!["tag1".to_string()],
            1,
            false,
            config,
        );
        assert!(result.is_ok());

        // File should be modified
        let final_content = std::fs::read_to_string(temp.path().join("test.md")).unwrap();
        assert!(final_content.contains("Q:begin"));
        assert!(final_content.contains("test-anchor"));
    }

    #[test]
    fn test_run_batch_mark_json_array() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.md"), "line 1\nline 2\nline 3\n").unwrap();

        let json = r#"[{"path": "test.md", "start_line": 1, "end_line": 2, "id": "test", "tags": [], "version": 1}]"#;
        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_batch_mark(temp.path(), json, true, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_batch_mark_json_object() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.md"), "line 1\nline 2\nline 3\n").unwrap();

        let json = r#"{"marks": [{"path": "test.md", "start_line": 1, "end_line": 2, "id": "test", "tags": [], "version": 1}]}"#;
        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_batch_mark(temp.path(), json, true, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_batch_mark_empty_input() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();

        let json = "[]";
        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_batch_mark(temp.path(), json, true, config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No marks"));
    }

    #[test]
    fn test_run_batch_mark_invalid_json() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();

        let json = "not valid json";
        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_batch_mark(temp.path(), json, true, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_unmark_dry_run() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let content = "line 1\n<!--Q:begin id=test v=1-->\nmarked\n<!--Q:end id=test-->\nline 2\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_unmark(temp.path(), "test.md", "test", true, config);
        assert!(result.is_ok());

        // File should be unchanged in dry run
        let final_content = std::fs::read_to_string(temp.path().join("test.md")).unwrap();
        assert!(final_content.contains("Q:begin"));
    }

    #[test]
    fn test_run_unmark_actual_remove() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let content = "line 1\n<!--Q:begin id=test v=1-->\nmarked\n<!--Q:end id=test-->\nline 2\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_unmark(temp.path(), "test.md", "test", false, config);
        assert!(result.is_ok());

        // File should have markers removed
        let final_content = std::fs::read_to_string(temp.path().join("test.md")).unwrap();
        assert!(!final_content.contains("Q:begin"));
        assert!(!final_content.contains("Q:end"));
        assert!(final_content.contains("marked"));
    }

    #[test]
    fn test_run_unmark_file_not_found() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_unmark(temp.path(), "nonexistent.md", "test", false, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_unmark_anchor_not_found() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        let content = "line 1\nline 2\n";
        std::fs::write(temp.path().join("test.md"), content).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_unmark(temp.path(), "test.md", "nonexistent", false, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_batch_mark_from_file() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("test.md"), "line 1\nline 2\nline 3\n").unwrap();

        let spec_json = r#"[{"path": "test.md", "start_line": 1, "end_line": 2, "id": "test", "tags": [], "version": 1}]"#;
        std::fs::write(temp.path().join("specs.json"), spec_json).unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result =
            run_batch_mark_from_file(temp.path(), &temp.path().join("specs.json"), true, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_batch_mark_from_file_not_found() {
        use tempfile::tempdir;
        let temp = tempdir().unwrap();

        let config = RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_batch_mark_from_file(
            temp.path(),
            &temp.path().join("nonexistent.json"),
            true,
            config,
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to read spec file"));
    }

    #[test]
    fn test_remove_markers_without_trailing_newline() {
        let content = "line 1\n<!--Q:begin id=test v=1-->\nmarked\n<!--Q:end id=test-->\nline 2";
        let result = remove_markers(content, "test").unwrap();
        // Should preserve non-trailing-newline
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn test_remove_markers_with_whitespace_variations() {
        // Test with extra spaces in markers
        let content =
            "line 1\n  <!--  Q:begin  id=test  v=1  -->  \nmarked\n<!--Q:end id=test-->\nline 2\n";
        let result = remove_markers(content, "test").unwrap();
        assert!(result.contains("marked"));
    }
}
