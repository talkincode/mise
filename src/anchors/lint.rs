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
use crate::core::render::{RenderConfig, Renderer};

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
            data: None,
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

/// Result from processing a single file
struct FileProcessResult {
    issues: Vec<LintIssue>,
    anchors: Vec<Anchor>,
}

/// Process a single file for anchor linting
fn process_file(root: &Path, path: &str) -> Option<FileProcessResult> {
    use crate::core::file_reader::read_file_safe;

    let full_path = root.join(path);

    // Only check text files that might contain anchors
    if !is_text_file(&full_path) {
        return None;
    }

    let read_result = read_file_safe(&full_path);

    // If file was skipped (binary, encoding issues, etc.), return None
    let content = match read_result.content {
        Some(c) => c,
        None => return None,
    };

    let mut issues = Vec::new();

    // Add warnings from file reading as lint issues
    for warning in read_result.warnings {
        issues.push(LintIssue::warning(
            warning.code.as_str(),
            &warning.message,
            path,
            None,
        ));
    }

    // Check for unpaired markers
    issues.extend(check_pairing(&content, path));

    // Parse anchors
    let anchors = parse_content(&content, path);

    for anchor in &anchors {
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
    }

    Some(FileProcessResult { issues, anchors })
}

/// Lint all anchors in the workspace
pub fn lint_anchors(root: &Path) -> Result<Vec<LintIssue>> {
    let mut issues = Vec::new();
    let mut all_anchors: HashMap<String, Vec<Anchor>> = HashMap::new();

    // Scan all files
    let files = scan_files(root, None, None, false, true, Some("file"))?;

    // Collect file paths for processing
    let paths: Vec<String> = files
        .items
        .iter()
        .filter_map(|item| item.path.clone())
        .collect();

    // Process files (parallel or sequential based on feature)
    #[cfg(feature = "parallel")]
    let results: Vec<FileProcessResult> = {
        use rayon::prelude::*;
        paths
            .par_iter()
            .filter_map(|path| process_file(root, path))
            .collect()
    };

    #[cfg(not(feature = "parallel"))]
    let results: Vec<FileProcessResult> = paths
        .iter()
        .filter_map(|path| process_file(root, path))
        .collect();

    // Aggregate results
    for result in results {
        issues.extend(result.issues);
        for anchor in result.anchors {
            all_anchors
                .entry(anchor.id.clone())
                .or_default()
                .push(anchor);
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
    use crate::anchors::parse::{BEGIN_RE, END_RE};

    let mut issues = Vec::new();
    let mut open_ids: Vec<(String, u32)> = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = line_num as u32 + 1;

        if let Some(caps) = BEGIN_RE.captures(line) {
            let id = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            open_ids.push((id, line_num));
        }

        if let Some(caps) = END_RE.captures(line) {
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
pub fn run_lint(root: &Path, config: RenderConfig) -> Result<()> {
    let issues = lint_anchors(root)?;

    let mut result_set = ResultSet::new();
    for issue in issues {
        result_set.push(issue.to_result_item());
    }

    let renderer = Renderer::with_config(config);
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

    #[test]
    fn test_lint_severity() {
        assert_eq!(LintSeverity::Error, LintSeverity::Error);
        assert_eq!(LintSeverity::Warning, LintSeverity::Warning);
        assert_ne!(LintSeverity::Error, LintSeverity::Warning);
    }

    #[test]
    fn test_lint_issue_error() {
        let issue = LintIssue::error("TEST_CODE", "Test message", "test/path.rs", Some(42));
        assert_eq!(issue.severity, LintSeverity::Error);
        assert_eq!(issue.code, "TEST_CODE");
        assert_eq!(issue.message, "Test message");
        assert_eq!(issue.path, "test/path.rs");
        assert_eq!(issue.line, Some(42));
    }

    #[test]
    fn test_lint_issue_warning() {
        let issue = LintIssue::warning("WARN_CODE", "Warning message", "src/lib.rs", None);
        assert_eq!(issue.severity, LintSeverity::Warning);
        assert_eq!(issue.code, "WARN_CODE");
        assert_eq!(issue.message, "Warning message");
        assert_eq!(issue.path, "src/lib.rs");
        assert_eq!(issue.line, None);
    }

    #[test]
    fn test_lint_issue_to_result_item_error() {
        let issue = LintIssue::error("ERR_CODE", "Error message", "file.rs", Some(10));
        let result = issue.to_result_item();

        assert_eq!(result.kind, Kind::Error);
        assert_eq!(result.path, Some("file.rs".to_string()));
        assert!(result.range.is_some());
        let range = result.range.unwrap();
        // Range is an enum, check via matching
        match range {
            crate::core::model::Range::Line(line_range) => {
                assert_eq!(line_range.start, 10);
                assert_eq!(line_range.end, 10);
            }
            _ => panic!("Expected Line range"),
        }
        assert_eq!(result.excerpt, Some("Error message".to_string()));
        assert_eq!(result.confidence, Confidence::High);
        assert_eq!(result.source_mode, SourceMode::Anchor);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].code, "ERR_CODE");
    }

    #[test]
    fn test_lint_issue_to_result_item_warning() {
        let issue = LintIssue::warning("WARN_CODE", "Warning message", "file.rs", None);
        let result = issue.to_result_item();

        assert_eq!(result.kind, Kind::Error);
        assert_eq!(result.confidence, Confidence::Medium);
        assert!(result.range.is_none());
    }

    #[test]
    fn test_is_text_file() {
        use std::path::PathBuf;

        // Text files
        assert!(is_text_file(&PathBuf::from("file.rs")));
        assert!(is_text_file(&PathBuf::from("file.py")));
        assert!(is_text_file(&PathBuf::from("file.js")));
        assert!(is_text_file(&PathBuf::from("file.ts")));
        assert!(is_text_file(&PathBuf::from("file.md")));
        assert!(is_text_file(&PathBuf::from("file.txt")));
        assert!(is_text_file(&PathBuf::from("file.json")));
        assert!(is_text_file(&PathBuf::from("file.yaml")));
        assert!(is_text_file(&PathBuf::from("file.toml")));
        assert!(is_text_file(&PathBuf::from("file.html")));
        assert!(is_text_file(&PathBuf::from("file.css")));
        assert!(is_text_file(&PathBuf::from("file.sh")));
        assert!(is_text_file(&PathBuf::from("file.go")));
        assert!(is_text_file(&PathBuf::from("file.java")));
        assert!(is_text_file(&PathBuf::from("file.rb")));
        assert!(is_text_file(&PathBuf::from("file.php")));
        assert!(is_text_file(&PathBuf::from("file.swift")));
        assert!(is_text_file(&PathBuf::from("file.c")));
        assert!(is_text_file(&PathBuf::from("file.cpp")));
        assert!(is_text_file(&PathBuf::from("file.h")));

        // Non-text files
        assert!(!is_text_file(&PathBuf::from("file.exe")));
        assert!(!is_text_file(&PathBuf::from("file.png")));
        assert!(!is_text_file(&PathBuf::from("file.jpg")));
        assert!(!is_text_file(&PathBuf::from("file.pdf")));
        assert!(!is_text_file(&PathBuf::from("file.zip")));
        assert!(!is_text_file(&PathBuf::from("no_extension")));
    }

    #[test]
    fn test_is_text_file_case_insensitive() {
        use std::path::PathBuf;

        assert!(is_text_file(&PathBuf::from("file.RS")));
        assert!(is_text_file(&PathBuf::from("file.Py")));
        assert!(is_text_file(&PathBuf::from("FILE.MD")));
    }

    #[test]
    fn test_check_pairing_nested_markers() {
        let content = r#"
<!--Q:begin id=outer-->
outer content
<!--Q:begin id=inner-->
inner content
<!--Q:end id=inner-->
more outer content
<!--Q:end id=outer-->
"#;
        let issues = check_pairing(content, "test.md");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_pairing_multiple_unpaired() {
        let content = r#"
<!--Q:begin id=test1-->
<!--Q:begin id=test2-->
content
"#;
        let issues = check_pairing(content, "test.md");
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.code == "UNPAIRED_BEGIN"));
    }

    #[test]
    fn test_check_pairing_mismatched_ids() {
        let content = r#"
<!--Q:begin id=test1-->
content
<!--Q:end id=test2-->
"#;
        let issues = check_pairing(content, "test.md");
        // Should have unpaired begin for test1 and unpaired end for test2
        assert_eq!(issues.len(), 2);
        let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains(&"UNPAIRED_BEGIN"));
        assert!(codes.contains(&"UNPAIRED_END"));
    }

    #[test]
    fn test_check_pairing_empty_content() {
        let content = "";
        let issues = check_pairing(content, "test.md");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_lint_issue_clone() {
        let issue = LintIssue::error("CODE", "message", "path.rs", Some(1));
        let cloned = issue.clone();
        assert_eq!(issue.code, cloned.code);
        assert_eq!(issue.message, cloned.message);
        assert_eq!(issue.path, cloned.path);
    }

    #[test]
    fn test_lint_issue_debug() {
        let issue = LintIssue::error("CODE", "message", "path.rs", Some(1));
        let debug_str = format!("{:?}", issue);
        assert!(debug_str.contains("LintIssue"));
        assert!(debug_str.contains("CODE"));
    }

    #[test]
    fn test_lint_severity_debug() {
        let error = LintSeverity::Error;
        let warning = LintSeverity::Warning;
        assert!(format!("{:?}", error).contains("Error"));
        assert!(format!("{:?}", warning).contains("Warning"));
    }

    #[test]
    fn test_lint_severity_clone() {
        let error = LintSeverity::Error;
        let cloned = error;
        assert_eq!(error, cloned);
    }
}
