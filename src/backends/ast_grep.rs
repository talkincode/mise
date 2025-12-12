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
}
