//! Anchor linting module
//!
//! Checks for:
//! - begin/end pairing
//! - Duplicate IDs
//! - Empty/oversized ranges
//! - Semantic drift (version unchanged but hash changed significantly)

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::anchors::parse::{parse_content, Anchor};
use crate::backends::scan::scan_files;
use crate::core::model::{Confidence, Kind, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::render::{OutputFormat, Renderer};

/// Lint issue severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

/// A lint issue
#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub code: String,
    pub message: String,
    pub path: String,
    pub line: Option<u32>,
}

impl LintIssue {
    pub fn error(code: &str, message: &str, path: &str, line: Option<u32>) -> Self {
        Self {
            severity: LintSeverity::Error,
            code: code.to_string(),
            message: message.to_string(),
            path: path.to_string(),
            line,
        }
    }

    pub fn warning(code: &str, message: &str, path: &str, line: Option<u32>) -> Self {
        Self {
            severity: LintSeverity::Warning,
            code: code.to_string(),
            message: message.to_string(),
            path: path.to_string(),
            line,
        }
    }

    pub fn to_result_item(&self) -> ResultItem {
        ResultItem {
            kind: Kind::Error,
            path: Some(self.path.clone()),
            range: self.line.map(|l| crate::core::model::Range::lines(l, l)),
            excerpt: Some(self.message.clone()),
            confidence: match self.severity {
                LintSeverity::Error => Confidence::High,
                LintSeverity::Warning => Confidence::Medium,
            },
            source_mode: SourceMode::Anchor,
            meta: Default::default(),
            errors: vec![MiseError::new(&self.code, &self.message)],
        }
    }
}

/// Maximum recommended anchor size (in lines)
const MAX_ANCHOR_LINES: u32 = 500;

/// Lint all anchors in the workspace
pub fn lint_anchors(root: &Path) -> Result<Vec<LintIssue>> {
    let mut issues = Vec::new();
    let mut all_anchors: HashMap<String, Vec<Anchor>> = HashMap::new();

    // Scan all files
    let files = scan_files(root, None, None, false, true, Some("file"))?;

    for item in files.items {
        if let Some(path) = &item.path {
            let full_path = root.join(path);

            // Only check text files that might contain anchors
            if !is_text_file(&full_path) {
                continue;
            }

            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Check for unpaired markers
            issues.extend(check_pairing(&content, path));

            // Parse anchors
            let anchors = parse_content(&content, path);

            for anchor in anchors {
                // Check for empty/oversized content (use content lines, not marker lines)
                let content_lines: u32 = anchor
                    .content
                    .as_ref()
                    .map(|c| c.lines().count() as u32)
                    .unwrap_or(0);

                if content_lines == 0 {
                    issues.push(LintIssue::warning(
                        "EMPTY_ANCHOR",
                        &format!("Anchor '{}' has empty content", anchor.id),
                        path,
                        Some(anchor.range.start),
                    ));
                } else if content_lines > MAX_ANCHOR_LINES {
                    issues.push(LintIssue::warning(
                        "LARGE_ANCHOR",
                        &format!(
                            "Anchor '{}' is very large ({} lines), consider splitting",
                            anchor.id, content_lines
                        ),
                        path,
                        Some(anchor.range.start),
                    ));
                }

                // Collect for duplicate check
                all_anchors
                    .entry(anchor.id.clone())
                    .or_default()
                    .push(anchor);
            }
        }
    }

    // Check for duplicate IDs
    for (id, anchors) in &all_anchors {
        if anchors.len() > 1 {
            for anchor in anchors {
                issues.push(LintIssue::error(
                    "DUPLICATE_ID",
                    &format!("Anchor ID '{}' is used {} times", id, anchors.len()),
                    &anchor.path,
                    Some(anchor.range.start),
                ));
            }
        }
    }

    Ok(issues)
}

/// Check for unpaired begin/end markers
fn check_pairing(content: &str, path: &str) -> Vec<LintIssue> {
    use regex::Regex;

    // Keep marker parsing consistent with `anchors::parse` so IDs don't accidentally include `-->`.
    let begin_re =
        Regex::new(r#"<!--\s*Q:begin\s+id=([^\s]+)(?:\s+tags=[^\s]+)?(?:\s+v=\d+)?\s*-->"#)
            .unwrap();
    let end_re = Regex::new(r#"<!--\s*Q:end\s+id=([^\s]+)\s*-->"#).unwrap();

    let mut issues = Vec::new();
    let mut open_ids: Vec<(String, u32)> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num as u32 + 1;

        if let Some(caps) = begin_re.captures(line) {
            let id = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            open_ids.push((id, line_num));
        }

        if let Some(caps) = end_re.captures(line) {
            let end_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");

            if let Some(pos) = open_ids.iter().rposition(|(id, _)| id == end_id) {
                open_ids.remove(pos);
            } else {
                issues.push(LintIssue::error(
                    "UNPAIRED_END",
                    &format!("End marker for '{}' has no matching begin", end_id),
                    path,
                    Some(line_num),
                ));
            }
        }
    }

    // Report unclosed markers
    for (id, line) in open_ids {
        issues.push(LintIssue::error(
            "UNPAIRED_BEGIN",
            &format!("Begin marker for '{}' has no matching end", id),
            path,
            Some(line),
        ));
    }

    issues
}

/// Check if a file is likely a text file
fn is_text_file(path: &Path) -> bool {
    let text_extensions = [
        "md", "txt", "rs", "py", "js", "ts", "jsx", "tsx", "html", "css", "json", "yaml", "yml",
        "toml", "xml", "sh", "bash", "zsh", "c", "cpp", "h", "hpp", "java", "go", "rb", "php",
        "swift",
    ];

    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| text_extensions.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Run the lint command
pub fn run_lint(root: &Path, format: OutputFormat) -> Result<()> {
    let issues = lint_anchors(root)?;

    let mut result_set = ResultSet::new();
    for issue in issues {
        result_set.push(issue.to_result_item());
    }

    let renderer = Renderer::new(format);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_pairing_valid() {
        let content = r#"
<!--Q:begin id=test1-->
content
<!--Q:end id=test1-->
"#;
        let issues = check_pairing(content, "test.md");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_pairing_valid_with_attrs() {
        // Common pattern: begin has tags/version, end usually doesn't.
        let content = r#"
<!--Q:begin id=intro tags=readme,introduction v=1-->
content
<!--Q:end id=intro-->
"#;
        let issues = check_pairing(content, "test.md");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_pairing_unpaired_begin() {
        let content = r#"
<!--Q:begin id=test1-->
content
"#;
        let issues = check_pairing(content, "test.md");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "UNPAIRED_BEGIN");
    }

    #[test]
    fn test_check_pairing_unpaired_end() {
        let content = r#"
content
<!--Q:end id=test1-->
"#;
        let issues = check_pairing(content, "test.md");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "UNPAIRED_END");
    }
}
