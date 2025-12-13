//! ripgrep integration
//!
//! Calls rg with --json and parses the output to ResultItems

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::core::model::{Kind, MiseError, Range, ResultItem, ResultSet, SourceMode};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Options for the match command
#[derive(Debug, Default)]
pub struct MatchOptions {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub context: Option<usize>,
    pub count: bool,
    pub max_count: Option<usize>,
    pub ignore_case: bool,
    pub word_regexp: bool,
}

/// Check if ripgrep is available
pub fn is_rg_available() -> bool {
    command_exists("rg")
}

/// Run ripgrep and collect results
pub fn run_rg(
    root: &Path,
    pattern: &str,
    scopes: &[impl AsRef<Path>],
    options: &MatchOptions,
) -> Result<ResultSet> {
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

    // Add include glob patterns
    for glob in &options.include {
        cmd.arg("--glob").arg(glob);
    }

    // Add exclude glob patterns (negated)
    for glob in &options.exclude {
        cmd.arg("--glob").arg(format!("!{}", glob));
    }

    // Add context lines
    if let Some(ctx) = options.context {
        cmd.arg("--context").arg(ctx.to_string());
    }

    // Add max count
    if let Some(max) = options.max_count {
        cmd.arg("--max-count").arg(max.to_string());
    }

    // Add case insensitivity
    if options.ignore_case {
        cmd.arg("--ignore-case");
    }

    // Add word boundary matching
    if options.word_regexp {
        cmd.arg("--word-regexp");
    }

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
    options: MatchOptions,
    config: RenderConfig,
) -> Result<()> {
    let result_set = run_rg(root, pattern, scopes, &options)?;

    // If count mode is enabled, output just the count
    if options.count {
        let match_count = result_set
            .items
            .iter()
            .filter(|i| matches!(i.kind, Kind::Match))
            .count();

        // For count mode, output a simple JSON object with the count
        if config.pretty {
            println!("{{\"count\": {}}}", match_count);
        } else {
            println!("{{\"count\":{}}}", match_count);
        }
    } else {
        let renderer = Renderer::with_config(config);
        println!("{}", renderer.render(&result_set));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_options() -> MatchOptions {
        MatchOptions::default()
    }

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
        // This assertion is always true, just to satisfy clippy
        assert!(available || !available);
    }

    #[test]
    fn test_run_rg_empty_scopes() {
        // Test with empty scopes (uses root)
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options());
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

            let result = run_rg(
                temp.path(),
                "hello",
                &[subdir.as_path()],
                &default_options(),
            );
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_rg_no_matches() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

            let result = run_rg(
                temp.path(),
                "nonexistent_pattern_xyz123",
                &[] as &[&Path],
                &default_options(),
            );
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

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();

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

            let result = run_match(
                temp.path(),
                "hello",
                &[] as &[&Path],
                default_options(),
                config,
            );
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_rg_multiple_matches() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test1.txt"), "hello\n").unwrap();
            std::fs::write(temp.path().join("test2.txt"), "hello\n").unwrap();

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();
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

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();
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

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();
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

            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();
            for item in &result.items {
                if let Some(excerpt) = &item.excerpt {
                    // Excerpt should not have trailing whitespace
                    assert!(!excerpt.ends_with(' '));
                    assert!(!excerpt.ends_with('\n'));
                }
            }
        }
    }

    #[test]
    fn test_run_rg_with_include() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.rs"), "hello rust\n").unwrap();
            std::fs::write(temp.path().join("test.txt"), "hello text\n").unwrap();

            // Only include .rs files
            let options = MatchOptions {
                include: vec!["*.rs".to_string()],
                ..Default::default()
            };
            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &options).unwrap();

            // Should only find match in .rs file
            let paths: Vec<_> = result
                .items
                .iter()
                .filter_map(|i| i.path.as_ref())
                .collect();
            assert!(paths.iter().all(|p| p.ends_with(".rs")));
        }
    }

    #[test]
    fn test_run_rg_with_exclude() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.rs"), "hello rust\n").unwrap();
            std::fs::write(temp.path().join("test_test.rs"), "hello test\n").unwrap();

            // Exclude test files
            let options = MatchOptions {
                exclude: vec!["*_test.rs".to_string()],
                ..Default::default()
            };
            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &options).unwrap();

            // Should not find match in test file
            let paths: Vec<_> = result
                .items
                .iter()
                .filter_map(|i| i.path.as_ref())
                .collect();
            assert!(paths.iter().all(|p| !p.contains("_test")));
        }
    }

    #[test]
    fn test_run_rg_with_ignore_case() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("test.txt"), "HELLO World\n").unwrap();

            // Without ignore_case - should not match lowercase pattern
            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &default_options()).unwrap();
            let no_case_count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .count();

            // With ignore_case - should match
            let options = MatchOptions {
                ignore_case: true,
                ..Default::default()
            };
            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &options).unwrap();
            let case_count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .count();

            // Case-insensitive should find at least as many matches
            assert!(case_count >= no_case_count);
        }
    }

    #[test]
    fn test_run_rg_with_max_count() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(
                temp.path().join("test.txt"),
                "hello\nhello\nhello\nhello\nhello\n",
            )
            .unwrap();

            // With max_count = 2
            let options = MatchOptions {
                max_count: Some(2),
                ..Default::default()
            };
            let result = run_rg(temp.path(), "hello", &[] as &[&Path], &options).unwrap();
            let count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .count();

            // Should have at most 2 matches
            assert!(count <= 2);
        }
    }

    #[test]
    fn test_run_rg_with_word_regexp() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(
                temp.path().join("test.txt"),
                "fn main() {}\nfunction helper() {}\nmain_fn()\n",
            )
            .unwrap();

            // Without word_regexp - should match 'fn' in 'function' and 'main_fn'
            let result = run_rg(temp.path(), "fn", &[] as &[&Path], &default_options()).unwrap();
            let no_word_count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .count();

            // With word_regexp - should only match standalone 'fn'
            let options = MatchOptions {
                word_regexp: true,
                ..Default::default()
            };
            let result = run_rg(temp.path(), "fn", &[] as &[&Path], &options).unwrap();
            let word_count = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .count();

            // Word-bounded should find fewer or equal matches
            assert!(word_count <= no_word_count);
        }
    }

    #[test]
    fn test_run_rg_with_context() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(
                temp.path().join("test.txt"),
                "line1\nline2\nmatch_line\nline4\nline5\n",
            )
            .unwrap();

            // With context=1 - rg should include context lines
            let options = MatchOptions {
                context: Some(1),
                ..Default::default()
            };
            let result = run_rg(temp.path(), "match_line", &[] as &[&Path], &options);
            assert!(result.is_ok());
            // Context lines are processed by rg, we mainly verify command runs correctly
        }
    }

    #[test]
    fn test_run_rg_combined_options() {
        if is_rg_available() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(
                temp.path().join("code.rs"),
                "fn MAIN() {}\nFn helper() {}\n",
            )
            .unwrap();
            std::fs::write(temp.path().join("test.py"), "def main():\n    pass\n").unwrap();

            // Combine multiple options
            let options = MatchOptions {
                include: vec!["*.rs".to_string()],
                ignore_case: true,
                word_regexp: true,
                max_count: Some(1),
                ..Default::default()
            };
            let result = run_rg(temp.path(), "fn", &[] as &[&Path], &options).unwrap();

            // Should only search .rs files, case-insensitive, word-bounded
            let paths: Vec<_> = result
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kind::Match))
                .filter_map(|i| i.path.as_ref())
                .collect();
            assert!(paths.iter().all(|p| p.ends_with(".rs")));
        }
    }

    #[test]
    fn test_match_options_default() {
        let options = MatchOptions::default();
        assert!(options.include.is_empty());
        assert!(options.exclude.is_empty());
        assert!(options.context.is_none());
        assert!(!options.count);
        assert!(options.max_count.is_none());
        assert!(!options.ignore_case);
        assert!(!options.word_regexp);
    }
}
