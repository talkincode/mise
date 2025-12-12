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

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
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

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
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
}
