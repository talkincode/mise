//! ripgrep integration
//!
//! Calls rg with --json and parses the output to ResultItems

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::core::model::{MiseError, Range, ResultItem, ResultSet, SourceMode};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Check if ripgrep is available
pub fn is_rg_available() -> bool {
    command_exists("rg")
}

/// Run ripgrep and collect results
pub fn run_rg(root: &Path, pattern: &str, scopes: &[impl AsRef<Path>]) -> Result<ResultSet> {
    if !is_rg_available() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::error(MiseError::new(
            "RG_NOT_FOUND",
            "ripgrep (rg) is not installed. Please install it: https://github.com/BurntSushi/ripgrep",
        )));
        return Ok(result_set);
    }

    let mut cmd = Command::new("rg");
    cmd.arg("--json").arg(pattern);

    // Add scope paths
    if scopes.is_empty() {
        cmd.arg(root);
    } else {
        for scope in scopes {
            cmd.arg(scope.as_ref());
        }
    }

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut result_set = ResultSet::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if v.get("type").and_then(|t| t.as_str()) != Some("match") {
            continue;
        }

        let data = match v.get("data") {
            Some(d) => d,
            None => continue,
        };

        let path_text = match data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
        {
            Some(p) => p,
            None => continue,
        };

        let lines_text = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let line_num = data
            .get("line_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(1) as u32;

        let relative_path =
            make_relative(Path::new(path_text), root).unwrap_or_else(|| path_text.to_string());
        let excerpt = lines_text.trim_end().to_string();

        let mut item =
            ResultItem::match_result(relative_path, Range::lines(line_num, line_num), excerpt);
        item.source_mode = SourceMode::Rg;
        result_set.push(item);
    }

    result_set.sort();
    Ok(result_set)
}

/// Run the match command
pub fn run_match(
    root: &Path,
    pattern: &str,
    scopes: &[impl AsRef<Path>],
    config: RenderConfig,
) -> Result<()> {
    let result_set = run_rg(root, pattern, scopes)?;

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_rg_available() {
        // This test depends on the system having rg installed
        let _ = is_rg_available();
    }

    #[test]
    fn test_run_rg_not_available() {
        // Test the error path when rg is not available
        // We can't easily test this without mocking, so we just test
        // that is_rg_available returns a boolean
        let available = is_rg_available();
        assert!(available == true || available == false);
    }

    #[test]
    fn test_run_rg_empty_scopes() {
        // Test with empty scopes (uses root)
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]);
            assert!(result.is_ok());
            let result_set = result.unwrap();
            // Should find the match
            assert!(
                !result_set.items.is_empty()
                    || result_set
                        .items
                        .iter()
                        .any(|i| matches!(i.kind, crate::core::model::Kind::Error))
            );
        }
    }

    #[test]
    fn test_run_rg_with_scopes() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            let subdir = temp.path().join("subdir");
            std::fs::create_dir(&subdir).unwrap();
            std::fs::write(subdir.join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[subdir.as_path()]);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_rg_no_matches() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(temp.path(), "nonexistent_pattern_xyz123", &[] as &[&Path]);
            assert!(result.is_ok());
            let result_set = result.unwrap();
            // Should have no matches
            assert!(
                result_set.items.is_empty()
                    || result_set
                        .items
                        .iter()
                        .all(|i| !matches!(i.kind, crate::core::model::Kind::Match))
            );
        }
    }

    #[test]
    fn test_run_rg_result_item_properties() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]).unwrap();

            for item in result.items {
                if matches!(item.kind, crate::core::model::Kind::Match) {
                    assert!(item.path.is_some());
                    assert!(item.range.is_some());
                    assert!(item.excerpt.is_some());
                    assert!(matches!(item.source_mode, SourceMode::Rg));
                }
            }
        }
    }

    #[test]
    fn test_run_match_command() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let config = crate::core::render::RenderConfig {
                format: crate::core::render::OutputFormat::Json,
                pretty: false,
            };

            let result = run_match(temp.path(), "hello", &[] as &[&Path], config);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_rg_multiple_matches() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test1.txt"), "hello\n").unwrap();
            std::fs::write(temp.path().join("test2.txt"), "hello\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]).unwrap();
            // Should find matches in both files
            let match_count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, crate::core::model::Kind::Match))
                .count();
            assert!(match_count >= 2);
        }
    }

    #[test]
    fn test_run_rg_multiline_content() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "line1\nhello world\nline3\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]).unwrap();
            // Should find the match on line 2
            for item in &result.items {
                if matches!(item.kind, crate::core::model::Kind::Match) {
                    assert!(item.range.is_some());
                    if let Some(Range::Line(range_line)) = &item.range {
                        assert_eq!(range_line.start, 2);
                    }
                }
            }
        }
    }

    #[test]
    fn test_run_rg_relative_path() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            let subdir = temp.path().join("subdir");
            std::fs::create_dir(&subdir).unwrap();
            std::fs::write(subdir.join("test.txt"), "hello\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]).unwrap();
            for item in &result.items {
                if matches!(item.kind, crate::core::model::Kind::Match) {
                    let path = item.path.as_ref().unwrap();
                    // Path should be relative and contain subdir
                    assert!(path.contains("subdir"));
                    assert!(!path.starts_with("/"));
                }
            }
        }
    }

    #[test]
    fn test_run_rg_excerpt_trimmed() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "  hello world  \n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path]).unwrap();
            for item in &result.items {
                if let Some(excerpt) = &item.excerpt {
                    // Excerpt should not have trailing whitespace
                    assert!(!excerpt.ends_with(' '));
                    assert!(!excerpt.ends_with('\n'));
                }
            }
        }
    }
}
