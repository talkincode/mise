//! ripgrep integration
//!
//! Calls rg with --json and parses the output to ResultItems

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::core::model::{MiseError, Range, ResultItem, ResultSet, SourceMode};
use crate::core::paths::make_relative;
use crate::core::render::{OutputFormat, Renderer};
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
    format: OutputFormat,
) -> Result<()> {
    let result_set = run_rg(root, pattern, scopes)?;

    let renderer = Renderer::new(format);
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
}
