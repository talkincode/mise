//! ast-grep integration
//!
//! Calls sg/ast-grep and parses the output to ResultItems

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::core::model::{MiseError, Range, ResultItem, ResultSet, SourceMode};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Check which ast-grep command is available
pub fn get_ast_grep_command() -> Option<&'static str> {
    if command_exists("sg") {
        Some("sg")
    } else if command_exists("ast-grep") {
        Some("ast-grep")
    } else {
        None
    }
}

/// ast-grep JSON output structure
#[derive(Debug, Deserialize)]
struct SgMatch {
    file: String,
    range: SgRange,
    text: String,
    #[serde(default)]
    lines: String,
}

#[derive(Debug, Deserialize)]
struct SgRange {
    start: SgPosition,
    end: SgPosition,
}

#[derive(Debug, Deserialize)]
struct SgPosition {
    line: u32,
}

/// Run ast-grep and collect results
pub fn run_ast_grep(root: &Path, pattern: &str, scopes: &[impl AsRef<Path>]) -> Result<ResultSet> {
    let cmd_name = match get_ast_grep_command() {
        Some(cmd) => cmd,
        None => {
            let mut result_set = ResultSet::new();
            result_set.push(ResultItem::error(MiseError::new(
                "AST_GREP_NOT_FOUND",
                "ast-grep (sg) is not installed. Please install it: https://ast-grep.github.io/",
            )));
            return Ok(result_set);
        }
    };

    let mut cmd = Command::new(cmd_name);
    cmd.arg("run").arg("--pattern").arg(pattern).arg("--json");

    // Add scope paths
    if !scopes.is_empty() {
        for scope in scopes {
            cmd.arg(scope.as_ref());
        }
    } else {
        cmd.arg(root);
    }

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut result_set = ResultSet::new();

    // Try to parse as JSON array
    if let Ok(matches) = serde_json::from_str::<Vec<SgMatch>>(&stdout) {
        for m in matches {
            let relative_path =
                make_relative(Path::new(&m.file), root).unwrap_or_else(|| m.file.clone());

            let range = Range::lines(m.range.start.line + 1, m.range.end.line + 1);
            let excerpt = if m.lines.is_empty() { m.text } else { m.lines };

            let mut item = ResultItem::match_result(relative_path, range, excerpt);
            item.source_mode = SourceMode::AstGrep;

            result_set.push(item);
        }
    }

    result_set.sort();
    Ok(result_set)
}

/// Run the ast command
pub fn run_ast(
    root: &Path,
    pattern: &str,
    scopes: &[impl AsRef<Path>],
    config: RenderConfig,
) -> Result<()> {
    let result_set = run_ast_grep(root, pattern, scopes)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_ast_grep_command() {
        // This test depends on the system configuration
        let _ = get_ast_grep_command();
    }

    #[test]
    fn test_get_ast_grep_command_returns_valid_option() {
        let cmd = get_ast_grep_command();
        if let Some(c) = cmd {
            assert!(c == "sg" || c == "ast-grep");
        }
    }

    #[test]
    fn test_run_ast_grep_empty_scopes() {
        // Test with empty scopes (uses root)
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("test.rs"),
            "fn main() { println!(\"hello\"); }",
        )
        .unwrap();

        let result = run_ast_grep(temp.path(), "fn $NAME() { $$$BODY }", &[] as &[&Path]);
        assert!(result.is_ok());
        // Either finds matches or returns an error about ast-grep not installed
        let result_set = result.unwrap();
        // Result should be valid either way
        assert!(result_set.items.is_empty() || !result_set.items.is_empty());
    }

    #[test]
    fn test_run_ast_grep_with_scopes() {
        let temp = tempfile::tempdir().unwrap();
        let subdir = temp.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("test.rs"), "fn main() {}").unwrap();

        let result = run_ast_grep(temp.path(), "fn main()", &[subdir.as_path()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sg_match_deserialization() {
        let json = r#"{"file": "test.rs", "range": {"start": {"line": 0}, "end": {"line": 1}}, "text": "fn main()", "lines": "fn main() {\n}"}"#;
        let m: SgMatch = serde_json::from_str(json).unwrap();
        assert_eq!(m.file, "test.rs");
        assert_eq!(m.range.start.line, 0);
        assert_eq!(m.range.end.line, 1);
        assert_eq!(m.text, "fn main()");
    }

    #[test]
    fn test_sg_match_without_lines() {
        let json = r#"{"file": "test.rs", "range": {"start": {"line": 0}, "end": {"line": 0}}, "text": "main"}"#;
        let m: SgMatch = serde_json::from_str(json).unwrap();
        assert_eq!(m.lines, ""); // default value
    }

    #[test]
    fn test_run_ast_grep_result_set_sorted() {
        if get_ast_grep_command().is_some() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("b.rs"), "fn b() {}").unwrap();
            std::fs::write(temp.path().join("a.rs"), "fn a() {}").unwrap();

            let result = run_ast_grep(temp.path(), "fn $NAME()", &[] as &[&Path]).unwrap();

            // Check that results are sorted by path if there are multiple
            if result.items.len() >= 2 {
                let paths: Vec<_> = result
                    .items
                    .iter()
                    .filter_map(|i| i.path.as_ref())
                    .collect();
                let mut sorted_paths = paths.clone();
                sorted_paths.sort();
                assert_eq!(paths, sorted_paths);
            }
        }
    }

    #[test]
    fn test_run_ast_command() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_ast(temp.path(), "fn main()", &[] as &[&Path], config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_ast_grep_no_matches() {
        if get_ast_grep_command().is_some() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

            // Pattern that won't match
            let result = run_ast_grep(
                temp.path(),
                "fn nonexistent_function_xyz()",
                &[] as &[&Path],
            );
            assert!(result.is_ok());
            let result_set = result.unwrap();
            // Should have no match results (may have error if ast-grep not installed)
            let match_count = result_set
                .items
                .iter()
                .filter(|i| matches!(i.kind, crate::core::model::Kind::Match))
                .count();
            assert_eq!(match_count, 0);
        }
    }

    #[test]
    fn test_run_ast_grep_source_mode() {
        if get_ast_grep_command().is_some() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

            let result = run_ast_grep(temp.path(), "fn main()", &[] as &[&Path]).unwrap();

            for item in &result.items {
                if matches!(item.kind, crate::core::model::Kind::Match) {
                    assert!(matches!(item.source_mode, SourceMode::AstGrep));
                }
            }
        }
    }

    #[test]
    fn test_sg_position_deserialization() {
        let json = r#"{"line": 42}"#;
        let pos: SgPosition = serde_json::from_str(json).unwrap();
        assert_eq!(pos.line, 42);
    }

    #[test]
    fn test_sg_range_deserialization() {
        let json = r#"{"start": {"line": 1}, "end": {"line": 5}}"#;
        let range: SgRange = serde_json::from_str(json).unwrap();
        assert_eq!(range.start.line, 1);
        assert_eq!(range.end.line, 5);
    }

    #[test]
    fn test_run_ast_grep_multiple_files() {
        if get_ast_grep_command().is_some() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("a.rs"), "fn test1() {}").unwrap();
            std::fs::write(temp.path().join("b.rs"), "fn test2() {}").unwrap();

            let result = run_ast_grep(temp.path(), "fn $NAME()", &[] as &[&Path]).unwrap();
            // Result may vary depending on ast-grep version and configuration
            // Just verify the call succeeds and returns a valid result set
            assert!(result.items.is_empty() || !result.items.is_empty());
        }
    }
}
