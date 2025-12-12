use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn parse_jsonl(stdout: &[u8]) -> Vec<Value> {
    let s = String::from_utf8_lossy(stdout);
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("valid jsonl line"))
        .collect()
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn scan_lists_files_in_stable_order() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("b.txt"), "b");
    write_file(&temp.path().join("a.txt"), "a");
    write_file(&temp.path().join("sub/zz.md"), "z");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("mise"));
    cmd.arg("--root")
        .arg(temp.path())
        .arg("scan")
        .arg("--type")
        .arg("file");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    let paths: Vec<_> = items
        .iter()
        .map(|v| v.get("path").and_then(|p| p.as_str()).unwrap().to_string())
        .collect();

    assert_eq!(paths, vec!["a.txt", "b.txt", "sub/zz.md"]);
}

#[test]
fn extract_returns_expected_excerpt() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("note.txt"),
        "line 1\nline 2\nline 3\nline 4\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("mise"));
    cmd.arg("--root")
        .arg(temp.path())
        .arg("extract")
        .arg("note.txt")
        .arg("--lines")
        .arg("2:3");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);
    assert_eq!(items.len(), 1);

    let excerpt = items[0]
        .get("excerpt")
        .and_then(|e| e.as_str())
        .expect("excerpt present");

    assert_eq!(excerpt, "line 2\nline 3");
}

#[test]
fn anchor_lint_flags_empty_anchor() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("doc.md"),
        "<!--Q:begin id=empty tags=t v=1-->\n<!--Q:end id=empty-->\n",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("mise"));
    cmd.arg("--root").arg(temp.path()).arg("anchor").arg("lint");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(s.contains("EMPTY_ANCHOR"));
}
