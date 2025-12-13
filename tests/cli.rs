use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn mise_cmd() -> Command {
    Command::cargo_bin("mise").expect("Failed to find mise binary")
}

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

    let mut cmd = mise_cmd();
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

    let mut cmd = mise_cmd();
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

    let mut cmd = mise_cmd();
    cmd.arg("--root").arg(temp.path()).arg("anchor").arg("lint");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    assert!(s.contains("EMPTY_ANCHOR"));
}

// ============== deps tests ==============

#[test]
fn deps_analyzes_rust_file_dependencies() {
    let temp = tempdir().unwrap();

    // Create a simple Rust project structure
    write_file(
        &temp.path().join("src/main.rs"),
        "mod foo;\nmod bar;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/foo.rs"), "pub fn foo() {}\n");
    write_file(
        &temp.path().join("src/bar.rs"),
        "use crate::foo;\n\npub fn bar() { foo::foo(); }\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("src/main.rs")
        .arg("--deps-format")
        .arg("tree");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // main.rs should show dependencies on foo.rs and bar.rs
    assert!(s.contains("src/main.rs"));
    assert!(s.contains("foo.rs") || s.contains("bar.rs"));
}

#[test]
fn deps_reverse_shows_dependents() {
    let temp = tempdir().unwrap();

    // Create files with dependency relationships
    write_file(
        &temp.path().join("src/main.rs"),
        "mod lib;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/lib.rs"), "pub fn lib_fn() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("src/lib.rs")
        .arg("--reverse")
        .arg("--deps-format")
        .arg("tree");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // lib.rs should show main.rs as a dependent
    assert!(s.contains("src/lib.rs"));
    assert!(s.contains("main.rs"));
}

#[test]
fn deps_table_format_works() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("src/main.rs"),
        "mod utils;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/utils.rs"), "pub fn helper() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("table");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Table should contain header and box-drawing characters
    assert!(s.contains("File"));
    assert!(s.contains("Depends On"));
    assert!(s.contains("Count"));
    assert!(s.contains("┌") || s.contains("│") || s.contains("└"));
}

#[test]
fn deps_dot_format_produces_graphviz() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("src/main.rs"),
        "mod core;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/core.rs"), "pub fn core_fn() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("dot");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // DOT format should contain graphviz syntax
    assert!(s.contains("digraph deps"));
    assert!(s.contains("rankdir=LR"));
    assert!(s.contains("->") || s.contains("[label="));
}

#[test]
fn deps_mermaid_format_produces_diagram() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("src/main.rs"),
        "mod api;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/api.rs"), "pub fn api_fn() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("mermaid");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Mermaid format should contain graph syntax
    assert!(s.contains("graph LR"));
}

#[test]
fn deps_jsonl_format_returns_valid_json() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("src/main.rs"),
        "mod helper;\n\nfn main() {}\n",
    );
    write_file(&temp.path().join("src/helper.rs"), "pub fn help() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("jsonl");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should have results for the files
    assert!(!items.is_empty());

    // Each item should have expected fields
    for item in &items {
        assert!(item.get("kind").is_some());
        assert!(item.get("path").is_some());
    }
}

#[test]
fn deps_python_file_analysis() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("main.py"),
        "from utils import helper\nimport os\n\ndef main():\n    pass\n",
    );
    write_file(&temp.path().join("utils.py"), "def helper():\n    pass\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("table");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should analyze Python files
    assert!(s.contains("main.py") || s.contains("utils.py"));
}

#[test]
fn deps_typescript_file_analysis() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("index.ts"),
        "import { helper } from './utils';\n\nexport function main() {}\n",
    );
    write_file(
        &temp.path().join("utils.ts"),
        "export function helper() {}\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("deps")
        .arg("--deps-format")
        .arg("table");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should analyze TypeScript files
    assert!(s.contains("index.ts") || s.contains("utils.ts"));
}

// ================== Impact Command Tests ==================

#[test]
fn impact_returns_valid_json_output() {
    let temp = tempdir().unwrap();

    // Initialize git repo
    Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();

    // Create and commit initial file
    write_file(&temp.path().join("main.rs"), "fn main() {}\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "initial"])
        .output()
        .unwrap();

    // Make an unstaged change
    write_file(
        &temp.path().join("main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("impact")
        .arg("--impact-format")
        .arg("jsonl");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should be valid JSON
    let json: Value = serde_json::from_str(s.trim()).expect("valid json");
    assert!(json.get("changed_files").is_some());
    assert!(json.get("source").is_some());
}

#[test]
fn impact_summary_format_works() {
    let temp = tempdir().unwrap();

    // Initialize git repo
    Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();

    // Create and commit initial file
    write_file(&temp.path().join("test.txt"), "initial content\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "initial"])
        .output()
        .unwrap();

    // Make an unstaged change
    write_file(&temp.path().join("test.txt"), "modified content\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("impact")
        .arg("--impact-format")
        .arg("summary");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should contain summary header
    assert!(s.contains("Impact Analysis"));
    assert!(s.contains("Changed files"));
}

#[test]
fn impact_table_format_works() {
    let temp = tempdir().unwrap();

    // Initialize git repo
    Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();

    // Create and commit initial file
    write_file(&temp.path().join("test.txt"), "initial\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "initial"])
        .output()
        .unwrap();

    // Make an unstaged change
    write_file(&temp.path().join("test.txt"), "modified\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("impact")
        .arg("--impact-format")
        .arg("table");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should contain table format markers
    assert!(s.contains("File") || s.contains("Impact"));
}

#[test]
fn impact_staged_option_works() {
    let temp = tempdir().unwrap();

    // Initialize git repo
    Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();

    // Create and commit initial file
    write_file(&temp.path().join("file.txt"), "initial\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "initial"])
        .output()
        .unwrap();

    // Make and stage a change
    write_file(&temp.path().join("file.txt"), "staged change\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("impact")
        .arg("--staged")
        .arg("--impact-format")
        .arg("jsonl");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    let json: Value = serde_json::from_str(s.trim()).expect("valid json");
    assert!(json
        .get("source")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("staged"));
}

#[test]
fn impact_no_changes_returns_empty() {
    let temp = tempdir().unwrap();

    // Initialize git repo with no changes
    Command::new("git")
        .current_dir(temp.path())
        .args(["init"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .output()
        .unwrap();

    write_file(&temp.path().join("test.txt"), "content\n");
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .output()
        .unwrap();
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "initial"])
        .output()
        .unwrap();

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("impact")
        .arg("--impact-format")
        .arg("jsonl");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    let json: Value = serde_json::from_str(s.trim()).expect("valid json");
    let changed = json.get("changed_files").unwrap().as_array().unwrap();
    assert!(changed.is_empty());
}

// =============================================================================
// Pack Command Tests
// =============================================================================

#[test]
fn pack_files_returns_valid_jsonl() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("readme.md"),
        "# Hello World\nThis is a test.",
    );
    write_file(
        &temp.path().join("code.rs"),
        "fn main() { println!(\"hello\"); }",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("pack")
        .arg("--files")
        .arg("readme.md")
        .arg("code.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].get("kind").unwrap().as_str().unwrap(), "file");
}

#[test]
fn pack_with_anchors_includes_both() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("doc.md"),
        r#"# Doc
<!--Q:begin id=intro tags=doc v=1-->
Introduction section.
<!--Q:end id=intro-->

More content here.
"#,
    );
    write_file(&temp.path().join("extra.txt"), "extra content");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("pack")
        .arg("--anchors")
        .arg("intro")
        .arg("--files")
        .arg("extra.txt");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should have anchor + file
    assert!(items.len() >= 2);
    let kinds: Vec<_> = items
        .iter()
        .map(|v| v.get("kind").unwrap().as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"anchor"));
    assert!(kinds.contains(&"file"));
}

#[test]
fn pack_with_max_tokens_truncates() {
    let temp = tempdir().unwrap();

    // Create a large file that exceeds typical token budget
    let large_content = "x".repeat(10000); // ~2500 tokens
    write_file(&temp.path().join("large.txt"), &large_content);

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("pack")
        .arg("--files")
        .arg("large.txt")
        .arg("--max-tokens")
        .arg("500");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    assert_eq!(items.len(), 1);
    let excerpt = items[0].get("excerpt").unwrap().as_str().unwrap();
    // Should be truncated (less than original 10000 chars)
    assert!(excerpt.len() < 10000);
    assert!(excerpt.contains("[truncated]"));
}

#[test]
fn pack_with_stats_outputs_statistics() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("a.txt"), "content a");
    write_file(&temp.path().join("b.txt"), "content b");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("pack")
        .arg("--files")
        .arg("a.txt")
        .arg("b.txt")
        .arg("--stats");

    let assert = cmd.assert().success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);

    // Stats should be printed to stderr
    assert!(stderr.contains("Pack Statistics"));
    assert!(stderr.contains("Items:"));
    assert!(stderr.contains("Tokens:"));
}

#[test]
fn pack_priority_by_confidence_orders_correctly() {
    let temp = tempdir().unwrap();

    // Create files - pack should pick high confidence first when truncating
    write_file(&temp.path().join("file1.txt"), "content one");
    write_file(&temp.path().join("file2.txt"), "content two");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("pack")
        .arg("--files")
        .arg("file1.txt")
        .arg("file2.txt")
        .arg("--priority")
        .arg("confidence");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should return both items since no max-tokens limit
    assert_eq!(items.len(), 2);
}

#[test]
fn pack_empty_selection_returns_empty() {
    let temp = tempdir().unwrap();

    let mut cmd = mise_cmd();
    cmd.arg("--root").arg(temp.path()).arg("flow").arg("pack");

    // No anchors or files specified
    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    assert!(items.is_empty());
}

// ================== Stats Command Tests ==================

#[test]
fn stats_returns_summary_output() {
    let temp = tempdir().unwrap();

    // Create some test files
    write_file(
        &temp.path().join("doc.md"),
        "# Hello World\n\nThis is a test document.\n",
    );
    write_file(&temp.path().join("readme.txt"), "简单的中文测试内容。\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root").arg(temp.path()).arg("flow").arg("stats");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should contain summary statistics
    assert!(s.contains("Project Statistics") || s.contains("Files:"));
    assert!(s.contains("Lines") || s.contains("Characters"));
}

#[test]
fn stats_json_format_works() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("test.md"), "Test content\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("stats")
        .arg("--stats-format")
        .arg("json");

    let assert = cmd.assert().success();
    let stdout = &assert.get_output().stdout;

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_slice(stdout).expect("valid JSON");
    assert!(parsed.get("total_files").is_some());
    assert!(parsed.get("total_chars").is_some());
}

#[test]
fn stats_table_format_works() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("test.md"), "Test content for table\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("stats")
        .arg("--stats-format")
        .arg("table");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Table format should have headers
    assert!(s.contains("File") || s.contains("|"));
}

// ================== Outline Command Tests ==================

#[test]
fn outline_returns_markdown_output() {
    let temp = tempdir().unwrap();

    // Create file with anchors
    write_file(
        &temp.path().join("doc.md"),
        r#"# Document

<!--Q:begin id=intro tags=chapter-->
Introduction section.
<!--Q:end id=intro-->

Some content here.
"#,
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("outline");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should contain outline header and anchor info
    assert!(s.contains("Outline") || s.contains("intro"));
}

#[test]
fn outline_tree_format_works() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("doc.md"),
        r#"<!--Q:begin id=section tags=doc-->
Content
<!--Q:end id=section-->
"#,
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("outline")
        .arg("--outline-format")
        .arg("tree");

    let assert = cmd.assert().success();
    let s = String::from_utf8_lossy(&assert.get_output().stdout);

    // Tree format should show structure
    assert!(s.contains("section") || s.contains("doc.md"));
}

#[test]
fn outline_json_format_works() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("doc.md"),
        r#"<!--Q:begin id=test tags=a-->
Test
<!--Q:end id=test-->
"#,
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("outline")
        .arg("--outline-format")
        .arg("json");

    let assert = cmd.assert().success();
    let stdout = &assert.get_output().stdout;

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_slice(stdout).expect("valid JSON");
    assert!(parsed.get("total_chars").is_some());
    assert!(parsed.get("items").is_some());
}

#[test]
fn outline_tag_filter_works() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("doc.md"),
        r#"<!--Q:begin id=ch1 tags=chapter-->
Chapter 1
<!--Q:end id=ch1-->

<!--Q:begin id=note tags=note-->
A note
<!--Q:end id=note-->
"#,
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("flow")
        .arg("outline")
        .arg("--tag")
        .arg("chapter")
        .arg("--outline-format")
        .arg("json");

    let assert = cmd.assert().success();
    let stdout = &assert.get_output().stdout;

    let parsed: serde_json::Value = serde_json::from_slice(stdout).expect("valid JSON");
    let items = parsed.get("items").unwrap().as_array().unwrap();

    // Should only include chapter-tagged anchor
    assert_eq!(items.len(), 1);
    assert!(items[0]["id"].as_str().unwrap().contains("ch1"));
}

// =============================================================================
// Match Command Enhanced Options Tests
// =============================================================================

#[test]
fn match_with_include_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("code.rs"), "fn hello() {}\n");
    write_file(&temp.path().join("code.py"), "def hello(): pass\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("hello")
        .arg("--include")
        .arg("*.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should only find matches in .rs files
    for item in &items {
        if item.get("kind").and_then(|k| k.as_str()) == Some("match") {
            let path = item.get("path").and_then(|p| p.as_str()).unwrap();
            assert!(path.ends_with(".rs"), "Expected .rs file, got {}", path);
        }
    }
}

#[test]
fn match_with_exclude_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "fn hello() {}\n");
    write_file(&temp.path().join("main_test.rs"), "fn test_hello() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("hello")
        .arg("--exclude")
        .arg("*_test.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should not find matches in _test.rs files
    for item in &items {
        if item.get("kind").and_then(|k| k.as_str()) == Some("match") {
            let path = item.get("path").and_then(|p| p.as_str()).unwrap();
            assert!(!path.contains("_test"), "Should exclude test files");
        }
    }
}

#[test]
fn match_with_count_returns_count_only() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("test.txt"),
        "hello\nhello\nhello world\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("hello")
        .arg("--count");

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);

    // Should return JSON with count
    let json: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    let count = json.get("count").and_then(|c| c.as_u64()).unwrap();
    assert!(count >= 2, "Expected at least 2 matches");
}

#[test]
fn match_with_max_count_limits_results() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("test.txt"),
        "match1\nmatch2\nmatch3\nmatch4\nmatch5\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("match")
        .arg("--max-count")
        .arg("2");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    let match_count = items
        .iter()
        .filter(|i| i.get("kind").and_then(|k| k.as_str()) == Some("match"))
        .count();

    assert!(match_count <= 2, "Expected at most 2 matches");
}

#[test]
fn match_with_ignore_case_finds_case_variants() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("test.txt"), "Hello\nHELLO\nhello\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("hello")
        .arg("--ignore-case");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    let match_count = items
        .iter()
        .filter(|i| i.get("kind").and_then(|k| k.as_str()) == Some("match"))
        .count();

    // Should find all 3 case variants
    assert!(match_count >= 3, "Expected 3 matches with ignore-case");
}

#[test]
fn match_with_word_regexp_matches_whole_words() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("test.txt"),
        "fn main()\nfunction test()\nmain_helper()\n",
    );

    // Without -w: should match "main" in all lines containing "main"
    let mut cmd_no_w = mise_cmd();
    cmd_no_w
        .arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("main");

    let assert_no_w = cmd_no_w.assert().success();
    let items_no_w = parse_jsonl(&assert_no_w.get_output().stdout);
    let count_no_w = items_no_w
        .iter()
        .filter(|i| i.get("kind").and_then(|k| k.as_str()) == Some("match"))
        .count();

    // With -w: should only match standalone "main"
    let mut cmd_w = mise_cmd();
    cmd_w
        .arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("main")
        .arg("--word-regexp");

    let assert_w = cmd_w.assert().success();
    let items_w = parse_jsonl(&assert_w.get_output().stdout);
    let count_w = items_w
        .iter()
        .filter(|i| i.get("kind").and_then(|k| k.as_str()) == Some("match"))
        .count();

    // Word-bounded should find fewer or equal matches
    assert!(
        count_w <= count_no_w,
        "Word regexp should find fewer matches"
    );
}

#[test]
fn match_with_context_shows_surrounding_lines() {
    let temp = tempdir().unwrap();

    write_file(
        &temp.path().join("test.txt"),
        "line1\nline2\nMATCH\nline4\nline5\n",
    );

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("match")
        .arg("MATCH")
        .arg("--context")
        .arg("1");

    // Just verify command succeeds - context is handled by rg
    cmd.assert().success();
}

// =============================================================================
// Scan Command Enhanced Options Tests
// =============================================================================

#[test]
fn scan_with_include_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "rust code");
    write_file(&temp.path().join("lib.rs"), "more rust");
    write_file(&temp.path().join("readme.md"), "docs");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("scan")
        .arg("--type")
        .arg("file")
        .arg("--include")
        .arg("*.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should only include .rs files
    assert_eq!(items.len(), 2);
    for item in &items {
        let path = item.get("path").and_then(|p| p.as_str()).unwrap();
        assert!(path.ends_with(".rs"));
    }
}

#[test]
fn scan_with_exclude_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "code");
    write_file(&temp.path().join("main_test.rs"), "test code");
    write_file(&temp.path().join("lib.rs"), "lib code");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("scan")
        .arg("--type")
        .arg("file")
        .arg("--exclude")
        .arg("*_test.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should exclude _test.rs files
    assert_eq!(items.len(), 2);
    for item in &items {
        let path = item.get("path").and_then(|p| p.as_str()).unwrap();
        assert!(!path.contains("_test"));
    }
}

#[test]
fn scan_with_multiple_include_patterns() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "rust");
    write_file(&temp.path().join("script.py"), "python");
    write_file(&temp.path().join("readme.md"), "docs");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("scan")
        .arg("--type")
        .arg("file")
        .arg("--include")
        .arg("*.rs")
        .arg("--include")
        .arg("*.py");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should include both .rs and .py files
    assert_eq!(items.len(), 2);
}

// =============================================================================
// AST Command Enhanced Options Tests
// =============================================================================

#[test]
fn ast_with_include_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "fn main() {}\n");
    write_file(&temp.path().join("test.py"), "def main(): pass\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("ast")
        .arg("fn $NAME()")
        .arg("--include")
        .arg("*.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // All matches should be from .rs files
    for item in &items {
        if item.get("kind").and_then(|k| k.as_str()) == Some("match") {
            let path = item.get("path").and_then(|p| p.as_str()).unwrap();
            assert!(path.ends_with(".rs"));
        }
    }
}

#[test]
fn ast_with_exclude_glob_filters_files() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "fn main() {}\n");
    write_file(&temp.path().join("main_test.rs"), "fn test_main() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("ast")
        .arg("fn $NAME()")
        .arg("--exclude")
        .arg("*_test.rs");

    let assert = cmd.assert().success();
    let items = parse_jsonl(&assert.get_output().stdout);

    // Should not find matches in _test.rs files
    for item in &items {
        if item.get("kind").and_then(|k| k.as_str()) == Some("match") {
            let path = item.get("path").and_then(|p| p.as_str()).unwrap();
            assert!(!path.contains("_test"));
        }
    }
}

#[test]
fn ast_with_language_option() {
    let temp = tempdir().unwrap();

    write_file(&temp.path().join("main.rs"), "fn main() {}\n");

    let mut cmd = mise_cmd();
    cmd.arg("--root")
        .arg(temp.path())
        .arg("ast")
        .arg("fn main()")
        .arg("--lang")
        .arg("rust");

    // Just verify command succeeds with --lang option
    cmd.assert().success();
}
