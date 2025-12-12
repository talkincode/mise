//! Extract backend - Ranged file reading

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::core::model::{Meta, Range, ResultItem, ResultSet};
use crate::core::paths::make_relative;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::truncate_string;

/// Parse line range string (format: "start:end")
fn parse_line_range(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        bail!(
            "Invalid line range format. Expected 'start:end', got '{}'",
            s
        );
    }

    let start: u32 = parts[0]
        .parse()
        .with_context(|| format!("Invalid start line: {}", parts[0]))?;
    let end: u32 = parts[1]
        .parse()
        .with_context(|| format!("Invalid end line: {}", parts[1]))?;

    if start > end {
        bail!("Start line ({}) must be <= end line ({})", start, end);
    }

    if start == 0 {
        bail!("Line numbers are 1-indexed, start cannot be 0");
    }

    Ok((start, end))
}

/// Extract lines from a file
pub fn extract_lines(
    root: &Path,
    path: &Path,
    start_line: u32,
    end_line: u32,
    max_bytes: usize,
) -> Result<ResultItem> {
    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };

    let relative_path =
        make_relative(&full_path, root).unwrap_or_else(|| path.display().to_string());

    let file =
        File::open(&full_path).with_context(|| format!("Failed to open file: {:?}", full_path))?;
    let reader = BufReader::new(file);

    let mut content = String::new();
    let mut current_line = 0u32;
    let mut actual_end = start_line;

    for line in reader.lines() {
        current_line += 1;

        if current_line < start_line {
            continue;
        }

        if current_line > end_line {
            break;
        }

        let line = line?;

        // Check if adding this line would exceed max_bytes
        if content.len() + line.len() + 1 > max_bytes {
            let (truncated, _) = truncate_string(&line, max_bytes - content.len());
            content.push_str(&truncated);

            return Ok(ResultItem::extract(
                relative_path,
                Range::lines(start_line, current_line),
                content,
            )
            .with_meta(Meta {
                truncated: true,
                ..Default::default()
            }));
        }

        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&line);
        actual_end = current_line;
    }

    if content.is_empty() && start_line > current_line {
        bail!(
            "Start line {} is beyond end of file ({} lines)",
            start_line,
            current_line
        );
    }

    Ok(ResultItem::extract(
        relative_path,
        Range::lines(start_line, actual_end),
        content,
    ))
}

/// Run the extract command
pub fn run_extract(
    root: &Path,
    path: &Path,
    lines: &str,
    max_bytes: usize,
    config: RenderConfig,
) -> Result<()> {
    let (start, end) = parse_line_range(lines)?;
    let item = extract_lines(root, path, start, end, max_bytes)?;

    let mut result_set = ResultSet::new();
    result_set.push(item);

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_line_range() {
        assert_eq!(parse_line_range("1:10").unwrap(), (1, 10));
        assert_eq!(parse_line_range("5:5").unwrap(), (5, 5));
        assert!(parse_line_range("10:5").is_err());
        assert!(parse_line_range("0:10").is_err());
        assert!(parse_line_range("invalid").is_err());
    }

    #[test]
    fn test_extract_lines() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        writeln!(file, "line 3").unwrap();
        writeln!(file, "line 4").unwrap();
        writeln!(file, "line 5").unwrap();

        let result = extract_lines(temp.path(), &file_path, 2, 4, 65536).unwrap();
        assert_eq!(result.excerpt, Some("line 2\nline 3\nline 4".to_string()));
    }

    #[test]
    fn test_extract_with_truncation() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "this is a very long line that should be truncated").unwrap();
        writeln!(file, "another line").unwrap();

        let result = extract_lines(temp.path(), &file_path, 1, 2, 20).unwrap();
        assert!(result.meta.truncated);
        assert!(result.excerpt.unwrap().len() <= 20);
    }
}
