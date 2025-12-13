//! Golden tests for mise
//!
//! These tests verify that command outputs match expected golden files.
//! Golden tests ensure:
//! - Output format stability across versions
//! - Consistent parsing and rendering behavior
//! - No unexpected regressions in output structure

use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;

/// Get the path to the fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Get the path to the sample project
fn sample_project() -> PathBuf {
    fixtures_dir().join("sample_project")
}

/// Create a command for running mise binary
fn mise_cmd() -> Command {
    Command::cargo_bin("mise").expect("Failed to find mise binary")
}

/// Parse JSONL output into a vector of JSON values
fn parse_jsonl(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

/// Normalize a result item by removing unstable fields (mtime, etc.)
fn normalize_item(mut item: Value) -> Value {
    // Remove mtime_ms as it changes based on file system
    if let Some(meta) = item.get_mut("meta") {
        if let Some(obj) = meta.as_object_mut() {
            obj.remove("mtime_ms");
        }
    }
    item
}

/// Normalize a list of items
fn normalize_items(items: Vec<Value>) -> Vec<Value> {
    items.into_iter().map(normalize_item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Scan Tests ====================

    #[test]
    fn golden_scan_files_structure() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("scan")
            .arg("--type")
            .arg("file");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        // Verify we get exactly 3 files in stable order
        assert_eq!(items.len(), 3, "Expected 3 files");

        let paths: Vec<&str> = items
            .iter()
            .filter_map(|v| v.get("path").and_then(|p| p.as_str()))
            .collect();

        assert_eq!(
            paths,
            vec!["README.md", "docs/guide.md", "src/main.rs"],
            "Files should be sorted alphabetically"
        );

        // Verify each item has required fields
        for item in &items {
            assert_eq!(item.get("kind").and_then(|v| v.as_str()), Some("file"));
            assert_eq!(
                item.get("confidence").and_then(|v| v.as_str()),
                Some("high")
            );
            assert_eq!(
                item.get("source_mode").and_then(|v| v.as_str()),
                Some("scan")
            );
            assert!(item.get("meta").is_some(), "meta field must exist");
        }
    }

    #[test]
    fn golden_scan_includes_metadata() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("scan")
            .arg("--type")
            .arg("file");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        // Verify metadata structure
        for item in &items {
            let meta = item.get("meta").expect("meta required");
            assert!(meta.get("size").is_some(), "size should be present");
            assert!(meta.get("mtime_ms").is_some(), "mtime_ms should be present");
            assert!(
                meta.get("truncated").is_some(),
                "truncated should be present"
            );
        }
    }

    // ==================== Anchor Tests ====================

    #[test]
    fn golden_anchor_list_structure() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("list");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        // Should find 6 anchors total
        assert_eq!(items.len(), 6, "Expected 6 anchors");

        // All should be anchor kind
        for item in &items {
            assert_eq!(item.get("kind").and_then(|v| v.as_str()), Some("anchor"));
            assert_eq!(
                item.get("source_mode").and_then(|v| v.as_str()),
                Some("anchor")
            );
            assert!(item.get("range").is_some(), "range required for anchors");
            assert!(
                item.get("excerpt").is_some(),
                "excerpt required for anchors"
            );

            // Anchor meta should have hash
            let meta = item.get("meta").expect("meta required");
            assert!(meta.get("hash").is_some(), "anchors should have hash");
        }
    }

    #[test]
    fn golden_anchor_list_paths() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("list");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        let paths: Vec<&str> = items
            .iter()
            .filter_map(|v| v.get("path").and_then(|p| p.as_str()))
            .collect();

        // Anchors found in README.md, docs/guide.md, src/main.rs
        assert!(
            paths.contains(&"README.md"),
            "Should find anchors in README.md"
        );
        assert!(
            paths.contains(&"docs/guide.md"),
            "Should find anchors in docs/guide.md"
        );
        assert!(
            paths.contains(&"src/main.rs"),
            "Should find anchors in src/main.rs"
        );
    }

    #[test]
    fn golden_anchor_get_specific() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("get")
            .arg("intro");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        assert_eq!(items.len(), 1, "Should return exactly one anchor");

        let item = &items[0];
        assert_eq!(item.get("path").and_then(|v| v.as_str()), Some("README.md"));

        let excerpt = item.get("excerpt").and_then(|v| v.as_str()).unwrap();
        assert!(
            excerpt.contains("Introduction"),
            "Excerpt should contain Introduction"
        );
    }

    // ==================== Extract Tests ====================

    #[test]
    fn golden_extract_structure() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("extract")
            .arg("README.md")
            .arg("--lines")
            .arg("1:5");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        assert_eq!(items.len(), 1, "Extract should return one item");

        let item = &items[0];
        assert_eq!(item.get("kind").and_then(|v| v.as_str()), Some("extract"));
        assert_eq!(item.get("path").and_then(|v| v.as_str()), Some("README.md"));

        // Verify range
        let range = item.get("range").expect("range required");
        assert_eq!(range.get("start").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(range.get("end").and_then(|v| v.as_u64()), Some(5));

        // Verify excerpt content
        let excerpt = item.get("excerpt").and_then(|v| v.as_str()).unwrap();
        assert!(
            excerpt.contains("Sample Project"),
            "Excerpt should contain file content"
        );
    }

    #[test]
    fn golden_extract_exact_lines() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("extract")
            .arg("src/main.rs")
            .arg("--lines")
            .arg("7:10");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        let item = &items[0];
        let excerpt = item.get("excerpt").and_then(|v| v.as_str()).unwrap();

        assert!(
            excerpt.contains("fn main()"),
            "Should extract main function"
        );
        assert!(excerpt.contains("println!"), "Should contain println macro");
    }

    // ==================== Output Format Tests ====================

    #[test]
    fn golden_jsonl_vs_json_equivalence() {
        // Get JSONL output
        let jsonl_output = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("--format")
            .arg("jsonl")
            .arg("scan")
            .arg("--type")
            .arg("file")
            .output()
            .expect("failed");

        let jsonl_stdout = String::from_utf8_lossy(&jsonl_output.stdout);
        let jsonl_items = normalize_items(parse_jsonl(&jsonl_stdout));

        // Get JSON output
        let json_output = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("--format")
            .arg("json")
            .arg("scan")
            .arg("--type")
            .arg("file")
            .output()
            .expect("failed");

        let json_stdout = String::from_utf8_lossy(&json_output.stdout);
        let json_items: Vec<Value> = serde_json::from_str(&json_stdout).expect("valid JSON array");
        let json_items = normalize_items(json_items);

        // Compare normalized items
        assert_eq!(jsonl_items.len(), json_items.len(), "Same number of items");

        for (jsonl, json) in jsonl_items.iter().zip(json_items.iter()) {
            assert_eq!(jsonl.get("path"), json.get("path"), "Paths should match");
            assert_eq!(jsonl.get("kind"), json.get("kind"), "Kinds should match");
        }
    }

    #[test]
    fn golden_markdown_format_structure() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("--format")
            .arg("md")
            .arg("scan")
            .arg("--type")
            .arg("file");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Markdown should contain file paths
        assert!(stdout.contains("README.md"), "Should list README.md");
        assert!(stdout.contains("src/main.rs"), "Should list src/main.rs");

        // Should have some markdown structure
        assert!(
            stdout.contains('#') || stdout.contains('-') || stdout.contains('*'),
            "Should have markdown formatting"
        );
    }

    // ==================== Anchor Lint Tests ====================

    #[test]
    fn golden_anchor_lint_clean_project() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("lint");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Clean project should have no errors or minimal output
        // The sample project has valid anchors
        let items = parse_jsonl(&stdout);

        // No error items expected for clean project
        let errors: Vec<_> = items
            .iter()
            .filter(|item| item.get("kind").and_then(|v| v.as_str()) == Some("error"))
            .collect();

        assert!(
            errors.is_empty(),
            "Clean project should have no lint errors"
        );
    }

    // ==================== Range Parsing Tests ====================

    #[test]
    fn golden_range_format_line_based() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("list");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let items = parse_jsonl(&stdout);

        // All anchor ranges should have start/end
        for item in &items {
            let range = item.get("range").expect("range required");
            let start = range.get("start").and_then(|v| v.as_u64());
            let end = range.get("end").and_then(|v| v.as_u64());

            assert!(start.is_some(), "Range must have start");
            assert!(end.is_some(), "Range must have end");
            assert!(start.unwrap() <= end.unwrap(), "Start must be <= end");
        }
    }

    // ==================== Stability Tests ====================

    #[test]
    fn golden_scan_output_is_deterministic() {
        // Run scan twice and verify identical output
        let run1 = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("scan")
            .arg("--type")
            .arg("file")
            .output()
            .expect("failed");

        let run2 = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("scan")
            .arg("--type")
            .arg("file")
            .output()
            .expect("failed");

        let items1 = normalize_items(parse_jsonl(&String::from_utf8_lossy(&run1.stdout)));
        let items2 = normalize_items(parse_jsonl(&String::from_utf8_lossy(&run2.stdout)));

        assert_eq!(items1, items2, "Output should be deterministic");
    }

    #[test]
    fn golden_anchor_hash_stability() {
        // Anchor hashes should be stable for unchanged content
        let run1 = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("list")
            .output()
            .expect("failed");

        let run2 = mise_cmd()
            .arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("list")
            .output()
            .expect("failed");

        let items1 = parse_jsonl(&String::from_utf8_lossy(&run1.stdout));
        let items2 = parse_jsonl(&String::from_utf8_lossy(&run2.stdout));

        for (item1, item2) in items1.iter().zip(items2.iter()) {
            let hash1 = item1
                .get("meta")
                .and_then(|m| m.get("hash"))
                .and_then(|h| h.as_str());
            let hash2 = item2
                .get("meta")
                .and_then(|m| m.get("hash"))
                .and_then(|h| h.as_str());

            assert_eq!(hash1, hash2, "Hashes should be stable");
        }
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn golden_extract_invalid_range_error() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("extract")
            .arg("README.md")
            .arg("--lines")
            .arg("100:200"); // Beyond file length

        let output = cmd.output().expect("failed to execute");

        // Should handle gracefully, might return empty or partial
        assert!(output.status.success() || !output.stderr.is_empty());
    }

    #[test]
    fn golden_anchor_get_missing() {
        let mut cmd = mise_cmd();
        cmd.arg("--root")
            .arg(sample_project())
            .arg("anchor")
            .arg("get")
            .arg("nonexistent_anchor_id");

        let output = cmd.output().expect("failed to execute");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should return error or empty
        let items = parse_jsonl(&stdout);
        if !items.is_empty() {
            // If returns something, should be error kind
            let has_error = items
                .iter()
                .any(|i| i.get("kind").and_then(|v| v.as_str()) == Some("error"));
            assert!(has_error, "Missing anchor should return error");
        }
    }
}
