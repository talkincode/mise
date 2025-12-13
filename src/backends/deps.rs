//! Dependency graph analysis
//!
//! Analyzes code dependencies using ast-grep patterns to understand
//! "what does this file depend on" and "what depends on this file".

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::backends::ast_grep::get_ast_grep_command;
use crate::backends::scan::{scan_files, ScanOptions};
use crate::core::model::{Confidence, Kind, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::paths::{make_relative, normalize_path};
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Supported languages for dependency analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Unknown,
}

impl Language {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Language::Rust,
            Some("ts") | Some("tsx") => Language::TypeScript,
            Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Language::JavaScript,
            Some("py") => Language::Python,
            _ => Language::Unknown,
        }
    }

    /// Get ast-grep language name
    #[allow(dead_code)]
    pub fn sg_lang(&self) -> Option<&'static str> {
        match self {
            Language::Rust => Some("rust"),
            Language::TypeScript => Some("typescript"),
            Language::JavaScript => Some("javascript"),
            Language::Python => Some("python"),
            Language::Unknown => None,
        }
    }

    /// Get file extensions for this language
    #[allow(dead_code)]
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::TypeScript => &["ts", "tsx"],
            Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Language::Python => &["py"],
            Language::Unknown => &[],
        }
    }
}

/// Import patterns for each language
struct ImportPatterns {
    patterns: Vec<&'static str>,
    lang: &'static str,
}

impl ImportPatterns {
    fn for_language(lang: Language) -> Option<Self> {
        match lang {
            Language::Rust => Some(ImportPatterns {
                patterns: vec![
                    "use $MOD",
                    "use $MOD::$_",
                    "use $MOD::{$$$_}",
                    "mod $MOD",
                    "pub mod $MOD",
                    "pub use $MOD",
                    "pub use $MOD::$_",
                ],
                lang: "rust",
            }),
            Language::TypeScript | Language::JavaScript => Some(ImportPatterns {
                patterns: vec![
                    "import $_ from '$PATH'",
                    "import $_ from \"$PATH\"",
                    "import '$PATH'",
                    "import \"$PATH\"",
                    "import { $$$_ } from '$PATH'",
                    "import { $$$_ } from \"$PATH\"",
                    "import * as $_ from '$PATH'",
                    "import * as $_ from \"$PATH\"",
                    "require('$PATH')",
                    "require(\"$PATH\")",
                    "export $_ from '$PATH'",
                    "export $_ from \"$PATH\"",
                    "export { $$$_ } from '$PATH'",
                    "export { $$$_ } from \"$PATH\"",
                ],
                lang: if lang == Language::TypeScript {
                    "typescript"
                } else {
                    "javascript"
                },
            }),
            Language::Python => Some(ImportPatterns {
                patterns: vec![
                    "import $MOD",
                    "import $MOD as $_",
                    "from $MOD import $_",
                    "from $MOD import $_ as $_",
                    "from $MOD import ($$$_)",
                ],
                lang: "python",
            }),
            Language::Unknown => None,
        }
    }
}

/// A single dependency relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// The import statement text
    pub import_text: String,
    /// The module/path being imported
    pub module: String,
    /// Resolved file path (if found)
    pub resolved_path: Option<String>,
    /// Line number in source file
    pub line: u32,
}

/// Dependency analysis result for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeps {
    /// The source file path
    pub path: String,
    /// Language detected
    pub language: Language,
    /// Dependencies (what this file imports)
    pub depends_on: Vec<Dependency>,
    /// Reverse dependencies (what imports this file) - populated in graph
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub depended_by: Vec<String>,
}

/// The complete dependency graph
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DepGraph {
    /// File path -> dependencies
    pub files: HashMap<String, FileDeps>,
    /// Module name -> file paths (for resolution)
    pub module_map: HashMap<String, String>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build reverse dependency map
    pub fn build_reverse_deps(&mut self) {
        // Collect all reverse dependencies
        let mut reverse_map: HashMap<String, Vec<String>> = HashMap::new();

        for (source_path, file_deps) in &self.files {
            for dep in &file_deps.depends_on {
                if let Some(resolved) = &dep.resolved_path {
                    reverse_map
                        .entry(resolved.clone())
                        .or_default()
                        .push(source_path.clone());
                }
            }
        }

        // Update files with reverse deps
        for (path, deps) in reverse_map {
            if let Some(file_deps) = self.files.get_mut(&path) {
                file_deps.depended_by = deps;
            }
        }
    }

    /// Get files that depend on the given file
    pub fn get_reverse_deps(&self, path: &str) -> Vec<String> {
        self.files
            .get(path)
            .map(|f| f.depended_by.clone())
            .unwrap_or_default()
    }

    /// Get files that the given file depends on
    pub fn get_forward_deps(&self, path: &str) -> Vec<String> {
        self.files
            .get(path)
            .map(|f| {
                let mut deps: Vec<String> = f
                    .depends_on
                    .iter()
                    .filter_map(|d| d.resolved_path.clone())
                    .collect();
                deps.sort();
                deps.dedup();
                deps
            })
            .unwrap_or_default()
    }

    /// Detect circular dependencies
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for file in self.files.keys() {
            if !visited.contains(file) {
                self.dfs_cycle(file, &mut visited, &mut rec_stack, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        for dep in self.get_forward_deps(node) {
            if !visited.contains(&dep) {
                self.dfs_cycle(&dep, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(&dep) {
                // Found a cycle
                let cycle_start = path.iter().position(|p| p == &dep).unwrap_or(0);
                let cycle: Vec<String> = path[cycle_start..].to_vec();
                if !cycle.is_empty() {
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }
}

/// Output format for deps command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepsFormat {
    #[default]
    Jsonl,
    Json,
    Dot,
    Tree,
    Table,
    Mermaid,
}

impl std::str::FromStr for DepsFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "jsonl" => Ok(DepsFormat::Jsonl),
            "json" => Ok(DepsFormat::Json),
            "dot" | "graphviz" => Ok(DepsFormat::Dot),
            "tree" => Ok(DepsFormat::Tree),
            "table" => Ok(DepsFormat::Table),
            "mermaid" | "mmd" => Ok(DepsFormat::Mermaid),
            _ => Err(format!("Unknown deps format: {}", s)),
        }
    }
}

/// Parse import statements from a file using ast-grep
fn parse_imports_with_sg(root: &Path, file_path: &Path, lang: Language) -> Result<Vec<Dependency>> {
    let patterns = match ImportPatterns::for_language(lang) {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let sg_cmd = match get_ast_grep_command() {
        Some(cmd) => cmd,
        None => return Ok(Vec::new()),
    };

    // Ensure we use absolute path for ast-grep
    let abs_file_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        root.join(file_path)
    };

    let mut all_deps = Vec::new();

    for pattern in patterns.patterns {
        let mut cmd = Command::new(sg_cmd);
        cmd.arg("run")
            .arg("--pattern")
            .arg(pattern)
            .arg("--lang")
            .arg(patterns.lang)
            .arg("--json")
            .arg(&abs_file_path);

        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse ast-grep JSON output
        if let Ok(matches) = serde_json::from_str::<Vec<SgMatch>>(&stdout) {
            for m in matches {
                let module = extract_module_from_match(&m.text, lang);
                let resolved = resolve_module(root, file_path, &module, lang);

                all_deps.push(Dependency {
                    import_text: m.text.trim().to_string(),
                    module,
                    resolved_path: resolved,
                    line: m.range.start.line + 1,
                });
            }
        }
    }

    // Deduplicate by line number
    all_deps.sort_by_key(|d| d.line);
    all_deps.dedup_by_key(|d| d.line);

    Ok(all_deps)
}

/// Parse imports using regex as fallback
fn parse_imports_with_regex(
    root: &Path,
    file_path: &Path,
    lang: Language,
) -> Result<Vec<Dependency>> {
    let content = std::fs::read_to_string(file_path)?;
    let mut deps = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        let module = match lang {
            Language::Rust => {
                if line.starts_with("use ") || line.starts_with("pub use ") {
                    // Extract module path: use crate::foo::bar -> foo
                    line.strip_prefix("pub use ")
                        .or_else(|| line.strip_prefix("use "))
                        .and_then(|s| s.strip_suffix(';'))
                        .map(|s| {
                            // Handle crate::, self::, super::
                            let s = s.trim();
                            if s.starts_with("crate::") {
                                s.strip_prefix("crate::").unwrap_or(s)
                            } else if s.starts_with("self::") {
                                s.strip_prefix("self::").unwrap_or(s)
                            } else if s.starts_with("super::") {
                                s.strip_prefix("super::").unwrap_or(s)
                            } else {
                                s
                            }
                        })
                        .map(|s| s.split("::").next().unwrap_or(s).to_string())
                } else if line.starts_with("mod ") || line.starts_with("pub mod ") {
                    line.strip_prefix("pub mod ")
                        .or_else(|| line.strip_prefix("mod "))
                        .and_then(|s| s.strip_suffix(';'))
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            }
            Language::TypeScript | Language::JavaScript => {
                if line.contains("import ") || line.contains("require(") {
                    extract_js_import_path(line)
                } else {
                    None
                }
            }
            Language::Python => {
                if line.starts_with("import ") {
                    line.strip_prefix("import ")
                        .map(|s| s.split_whitespace().next().unwrap_or(s))
                        .map(|s| s.split('.').next().unwrap_or(s).to_string())
                } else if line.starts_with("from ") {
                    line.strip_prefix("from ")
                        .and_then(|s| s.split_whitespace().next())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            }
            Language::Unknown => None,
        };

        if let Some(module) = module {
            if !module.is_empty() {
                let resolved = resolve_module(root, file_path, &module, lang);
                deps.push(Dependency {
                    import_text: line.to_string(),
                    module,
                    resolved_path: resolved,
                    line: (line_num + 1) as u32,
                });
            }
        }
    }

    Ok(deps)
}

/// Extract JavaScript/TypeScript import path from line
fn extract_js_import_path(line: &str) -> Option<String> {
    // Match patterns like: from 'path' or from "path" or require('path')
    let patterns = [
        (r#"from '"#, "'"),
        (r#"from ""#, "\""),
        ("require('", "')"),
        ("require(\"", "\")"),
    ];

    for (start, end) in patterns {
        if let Some(idx) = line.find(start) {
            let rest = &line[idx + start.len()..];
            if let Some(end_idx) = rest.find(end) {
                return Some(rest[..end_idx].to_string());
            }
        }
    }
    None
}

/// Resolve a module name to a file path
fn resolve_module(root: &Path, source_file: &Path, module: &str, lang: Language) -> Option<String> {
    let source_dir = source_file.parent()?;

    match lang {
        Language::Rust => resolve_rust_module(root, source_file, module),
        Language::TypeScript | Language::JavaScript => {
            resolve_js_module(root, source_dir, module, lang)
        }
        Language::Python => resolve_python_module(root, source_dir, module),
        Language::Unknown => None,
    }
}

/// Resolve Rust module
fn resolve_rust_module(root: &Path, source_file: &Path, module: &str) -> Option<String> {
    // Skip std library - but be careful not to skip project modules with same names
    // We only skip if there's no local module with that name
    let skip_modules = [
        "std",
        "alloc",
        "anyhow",
        "clap",
        "serde",
        "serde_json",
        "ignore",
        "walkdir",
        "sha1",
        "xxhash_rust",
        "tempfile",
        "tokio",
        "async_std",
        "futures",
        "log",
        "env_logger",
    ];

    let source_dir = source_file.parent()?;

    // For 'mod foo' or 'use foo', look for:
    // 1. src/foo.rs
    // 2. src/foo/mod.rs
    // 3. sibling foo.rs
    // 4. sibling foo/mod.rs
    let candidates = vec![
        root.join("src").join(format!("{}.rs", module)),
        root.join("src").join(module).join("mod.rs"),
        source_dir.join(format!("{}.rs", module)),
        source_dir.join(module).join("mod.rs"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return make_relative(&candidate, root);
        }
    }

    // Only skip external modules if no local file was found
    if skip_modules.contains(&module) {
        return None;
    }

    None
}

/// Resolve JavaScript/TypeScript module
fn resolve_js_module(
    root: &Path,
    source_dir: &Path,
    module: &str,
    lang: Language,
) -> Option<String> {
    // Skip node_modules imports
    if !module.starts_with('.') && !module.starts_with('/') {
        return None;
    }

    let base_path = if module.starts_with('.') {
        source_dir.join(module)
    } else {
        root.join(module.trim_start_matches('/'))
    };

    let extensions: &[&str] = match lang {
        Language::TypeScript => &[".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.tsx"],
        Language::JavaScript => &[".js", ".jsx", ".mjs", ".cjs", "/index.js", "/index.jsx"],
        _ => return None,
    };

    // Try exact path first
    if base_path.exists() && base_path.is_file() {
        return make_relative(&base_path, root);
    }

    // Try with extensions
    for ext in extensions {
        let candidate = if let Some(stripped) = ext.strip_prefix('/') {
            base_path.join(stripped)
        } else {
            PathBuf::from(format!("{}{}", base_path.display(), ext))
        };

        if candidate.exists() && candidate.is_file() {
            return make_relative(&candidate, root);
        }
    }

    None
}

/// Resolve Python module
fn resolve_python_module(root: &Path, source_dir: &Path, module: &str) -> Option<String> {
    // Handle relative imports
    let (base_dir, module_parts) = if module.starts_with('.') {
        let dots = module.chars().take_while(|c| *c == '.').count();
        let mut dir = source_dir.to_path_buf();
        for _ in 1..dots {
            dir = dir.parent()?.to_path_buf();
        }
        let rest = &module[dots..];
        (dir, rest)
    } else {
        (root.to_path_buf(), module)
    };

    let module_path = module_parts.replace('.', "/");

    let candidates = vec![
        base_dir.join(format!("{}.py", module_path)),
        base_dir.join(&module_path).join("__init__.py"),
        root.join(format!("{}.py", module_path)),
        root.join(&module_path).join("__init__.py"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return make_relative(&candidate, root);
        }
    }

    None
}

/// ast-grep match structure
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SgMatch {
    #[allow(dead_code)]
    file: String,
    range: SgRange,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SgRange {
    start: SgPosition,
    #[allow(dead_code)]
    end: SgPosition,
    #[allow(dead_code)]
    byte_offset: Option<SgByteOffset>,
}

#[derive(Debug, Deserialize)]
struct SgByteOffset {
    #[allow(dead_code)]
    start: u64,
    #[allow(dead_code)]
    end: u64,
}

#[derive(Debug, Deserialize)]
struct SgPosition {
    line: u32,
    #[allow(dead_code)]
    column: u32,
}

/// Extract module name from matched import text
fn extract_module_from_match(text: &str, lang: Language) -> String {
    let text = text.trim();

    match lang {
        Language::Rust => {
            // "use foo::bar" -> "foo"
            // "use crate::foo::bar" -> "foo"
            // "mod foo" -> "foo"
            let stripped = text
                .strip_prefix("pub ")
                .unwrap_or(text)
                .strip_prefix("use ")
                .or_else(|| text.strip_prefix("mod "))
                .unwrap_or(text)
                .trim();

            // Handle crate::, self::, super:: prefixes
            let stripped = stripped
                .strip_prefix("crate::")
                .or_else(|| stripped.strip_prefix("self::"))
                .or_else(|| stripped.strip_prefix("super::"))
                .unwrap_or(stripped);

            stripped
                .split("::")
                .next()
                .unwrap_or(stripped)
                .trim_end_matches(';')
                .trim_end_matches('{')
                .trim()
                .to_string()
        }
        Language::TypeScript | Language::JavaScript => {
            extract_js_import_path(text).unwrap_or_else(|| text.to_string())
        }
        Language::Python => {
            // "import foo" -> "foo"
            // "from foo import bar" -> "foo"
            text.strip_prefix("import ")
                .map(|s| s.split_whitespace().next().unwrap_or(s))
                .or_else(|| {
                    text.strip_prefix("from ")
                        .and_then(|s| s.split_whitespace().next())
                })
                .unwrap_or(text)
                .split('.')
                .next()
                .unwrap_or(text)
                .to_string()
        }
        Language::Unknown => text.to_string(),
    }
}

/// Analyze dependencies for a single file
pub fn analyze_file(root: &Path, file_path: &Path) -> Result<FileDeps> {
    let lang = Language::from_path(file_path);
    let relative_path = make_relative(file_path, root).unwrap_or_else(|| normalize_path(file_path));

    // Try ast-grep first, fall back to regex
    let deps = if get_ast_grep_command().is_some() {
        parse_imports_with_sg(root, file_path, lang).unwrap_or_default()
    } else {
        Vec::new()
    };

    // If ast-grep found nothing, try regex
    let deps = if deps.is_empty() {
        parse_imports_with_regex(root, file_path, lang).unwrap_or_default()
    } else {
        deps
    };

    Ok(FileDeps {
        path: relative_path,
        language: lang,
        depends_on: deps,
        depended_by: Vec::new(),
    })
}

/// Analyze dependencies for all files in scope
pub fn analyze_deps(root: &Path, scope: Option<&Path>) -> Result<DepGraph> {
    let scan_root = scope.unwrap_or(root);
    let options = ScanOptions {
        scope: if scope.is_some() {
            Some(scan_root.to_path_buf())
        } else {
            None
        },
        file_type: Some("file".to_string()),
        ignore: true,
        ..Default::default()
    };
    let file_results = scan_files(root, &options)?;

    let mut graph = DepGraph::new();

    for file_result in file_results.items {
        if let Some(path_str) = &file_result.path {
            let file_path = root.join(path_str);
            let lang = Language::from_path(&file_path);

            // Skip non-supported languages
            if lang == Language::Unknown {
                continue;
            }

            // Skip files that don't exist
            if !file_path.exists() {
                continue;
            }

            if let Ok(file_deps) = analyze_file(root, &file_path) {
                graph.files.insert(file_deps.path.clone(), file_deps);
            }
        }
    }

    // Build reverse dependency map
    graph.build_reverse_deps();

    Ok(graph)
}

/// Format dependency graph as DOT (Graphviz)
fn format_dot(graph: &DepGraph, file: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("digraph deps {\n");
    output.push_str("    rankdir=LR;\n");
    output.push_str("    node [shape=box, style=rounded];\n\n");

    let files_to_show: HashSet<String> = if let Some(f) = file {
        // Show only deps related to this file
        let mut set = HashSet::new();
        set.insert(f.to_string());

        if let Some(file_deps) = graph.files.get(f) {
            for dep in &file_deps.depends_on {
                if let Some(resolved) = &dep.resolved_path {
                    set.insert(resolved.clone());
                }
            }
            for dep_by in &file_deps.depended_by {
                set.insert(dep_by.clone());
            }
        }
        set
    } else {
        graph.files.keys().cloned().collect()
    };

    // Add nodes
    for path in &files_to_show {
        let label = path.rsplit('/').next().unwrap_or(path);
        output.push_str(&format!("    \"{}\" [label=\"{}\"];\n", path, label));
    }

    output.push('\n');

    // Add edges
    for (path, file_deps) in &graph.files {
        if !files_to_show.contains(path) {
            continue;
        }

        for dep in &file_deps.depends_on {
            if let Some(resolved) = &dep.resolved_path {
                if files_to_show.contains(resolved) {
                    output.push_str(&format!("    \"{}\" -> \"{}\";\n", path, resolved));
                }
            }
        }
    }

    output.push_str("}\n");
    output
}

/// Format dependency graph as Mermaid
fn format_mermaid(graph: &DepGraph, file: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("graph LR\n");

    let files_to_show: HashSet<String> = if let Some(f) = file {
        let mut set = HashSet::new();
        set.insert(f.to_string());

        if let Some(file_deps) = graph.files.get(f) {
            for dep in &file_deps.depends_on {
                if let Some(resolved) = &dep.resolved_path {
                    set.insert(resolved.clone());
                }
            }
            for dep_by in &file_deps.depended_by {
                set.insert(dep_by.clone());
            }
        }
        set
    } else {
        graph.files.keys().cloned().collect()
    };

    // Mermaid node IDs can't have special chars, create mapping
    let mut node_ids: HashMap<String, String> = HashMap::new();
    for (idx, path) in files_to_show.iter().enumerate() {
        let id = format!("N{}", idx);
        let label = path.rsplit('/').next().unwrap_or(path);
        output.push_str(&format!("    {}[{}]\n", id, label));
        node_ids.insert(path.clone(), id);
    }

    // Add edges
    for (path, file_deps) in &graph.files {
        if !files_to_show.contains(path) {
            continue;
        }

        let from_id = match node_ids.get(path) {
            Some(id) => id,
            None => continue,
        };

        for dep in &file_deps.depends_on {
            if let Some(resolved) = &dep.resolved_path {
                if let Some(to_id) = node_ids.get(resolved) {
                    output.push_str(&format!("    {} --> {}\n", from_id, to_id));
                }
            }
        }
    }

    output
}

/// Format as tree
fn format_tree(graph: &DepGraph, file: &str, reverse: bool) -> String {
    let mut output = String::new();
    output.push_str(&format!("{}\n", file));

    let deps = if reverse {
        graph.get_reverse_deps(file)
    } else {
        graph.get_forward_deps(file)
    };

    for (idx, dep) in deps.iter().enumerate() {
        let is_last = idx == deps.len() - 1;
        let prefix = if is_last { "└── " } else { "├── " };
        output.push_str(&format!("{}{}\n", prefix, dep));
    }

    output
}

/// Format as table
fn format_table(graph: &DepGraph) -> String {
    let mut output = String::new();

    // Calculate column widths
    let max_path_len = graph
        .files
        .keys()
        .map(|p| p.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let max_deps_len = 40;

    // Header
    output.push_str(&format!(
        "┌─{:─<width$}─┬─{:─<deps_width$}─┬───────┐\n",
        "",
        "",
        width = max_path_len,
        deps_width = max_deps_len
    ));
    output.push_str(&format!(
        "│ {:width$} │ {:deps_width$} │ Count │\n",
        "File",
        "Depends On",
        width = max_path_len,
        deps_width = max_deps_len
    ));
    output.push_str(&format!(
        "├─{:─<width$}─┼─{:─<deps_width$}─┼───────┤\n",
        "",
        "",
        width = max_path_len,
        deps_width = max_deps_len
    ));

    // Rows
    let mut files: Vec<_> = graph.files.iter().collect();
    files.sort_by_key(|(k, _)| k.as_str());

    for (path, file_deps) in files {
        let deps: Vec<_> = file_deps
            .depends_on
            .iter()
            .filter_map(|d| d.resolved_path.as_ref())
            .map(|p| p.rsplit('/').next().unwrap_or(p))
            .collect();

        let deps_str = if deps.is_empty() {
            "-".to_string()
        } else {
            let joined = deps.join(", ");
            if joined.len() > max_deps_len {
                format!("{}...", &joined[..max_deps_len - 3])
            } else {
                joined
            }
        };

        output.push_str(&format!(
            "│ {:width$} │ {:deps_width$} │ {:>5} │\n",
            path,
            deps_str,
            file_deps.depends_on.len(),
            width = max_path_len,
            deps_width = max_deps_len
        ));
    }

    output.push_str(&format!(
        "└─{:─<width$}─┴─{:─<deps_width$}─┴───────┘\n",
        "",
        "",
        width = max_path_len,
        deps_width = max_deps_len
    ));

    output
}

/// Convert dependency analysis to ResultSet
fn deps_to_result_set(
    graph: &DepGraph,
    file: Option<&str>,
    reverse: bool,
    cycles: &[Vec<String>],
) -> ResultSet {
    let mut result_set = ResultSet::new();

    // Add circular dependency warnings to result set
    for cycle in cycles {
        let cycle_str = cycle.join(" -> ");
        let mut warning_item = ResultItem::error(MiseError::new(
            "CIRCULAR_DEPENDENCY",
            format!("Circular dependency detected: {}", cycle_str),
        ));
        warning_item.confidence = Confidence::High;
        warning_item.source_mode = SourceMode::AstGrep;
        // Set path to first file in cycle for reference
        if let Some(first) = cycle.first() {
            warning_item.path = Some(first.clone());
        }
        warning_item.data = Some(serde_json::json!({
            "cycle": cycle,
            "cycle_length": cycle.len(),
        }));
        result_set.push(warning_item);
    }

    if let Some(file_path) = file {
        // Single file mode
        if let Some(file_deps) = graph.files.get(file_path) {
            let deps: Vec<String> = if reverse {
                file_deps.depended_by.clone()
            } else {
                // Collect unique resolved paths
                let mut unique_deps: Vec<String> = file_deps
                    .depends_on
                    .iter()
                    .filter_map(|d| d.resolved_path.clone())
                    .collect();
                unique_deps.sort();
                unique_deps.dedup();
                unique_deps
            };

            let kind_str = if reverse { "depended_by" } else { "depends_on" };

            // Create a custom result with dep info
            let mut item = ResultItem::file(file_path);
            item.kind = Kind::Flow; // Use Flow kind for deps
            item.source_mode = SourceMode::AstGrep;
            item.data = Some(serde_json::json!({
                kind_str: deps,
                "language": file_deps.language,
            }));

            result_set.push(item);
        }
    } else {
        // Full graph mode
        for (path, file_deps) in &graph.files {
            let mut item = ResultItem::file(path);
            item.kind = Kind::Flow;
            item.source_mode = SourceMode::AstGrep;
            item.confidence = Confidence::High;

            let forward_deps: Vec<_> = file_deps
                .depends_on
                .iter()
                .filter_map(|d| d.resolved_path.clone())
                .collect();

            item.data = Some(serde_json::json!({
                "depends_on": forward_deps,
                "depended_by": file_deps.depended_by,
                "language": file_deps.language,
            }));

            result_set.push(item);
        }
    }

    result_set.sort();
    result_set
}

/// Run the deps command
pub fn run_deps(
    root: &Path,
    file: Option<&Path>,
    reverse: bool,
    format: DepsFormat,
    config: RenderConfig,
) -> Result<()> {
    // Check if ast-grep is available
    if get_ast_grep_command().is_none() && !command_exists("rg") {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::error(MiseError::new(
            "DEPS_TOOL_NOT_FOUND",
            "Neither ast-grep (sg) nor ripgrep (rg) is installed. Please install at least one.",
        )));
        let renderer = Renderer::with_config(config);
        println!("{}", renderer.render(&result_set));
        return Ok(());
    }

    // Analyze dependencies
    let graph = analyze_deps(root, None)?;

    // Convert file path to relative string
    let file_str = file.map(|f| {
        // If file is already relative, use it directly
        // Otherwise, make it relative to root
        if f.is_absolute() {
            make_relative(f, root).unwrap_or_else(|| normalize_path(f))
        } else {
            normalize_path(f)
        }
    });

    // Check for circular dependencies
    let cycles = graph.find_cycles();

    // Output based on format
    let output = match format {
        DepsFormat::Dot => format_dot(&graph, file_str.as_deref()),
        DepsFormat::Mermaid => format_mermaid(&graph, file_str.as_deref()),
        DepsFormat::Tree => {
            if let Some(f) = &file_str {
                format_tree(&graph, f, reverse)
            } else {
                // Tree format requires a file - return as structured error
                let mut result_set = ResultSet::new();
                result_set.push(ResultItem::error(MiseError::new(
                    "TREE_REQUIRES_FILE",
                    "Tree format requires a specific file. Use: mise deps <file> --format tree",
                )));
                let renderer = Renderer::with_config(config);
                println!("{}", renderer.render(&result_set));
                return Ok(());
            }
        }
        DepsFormat::Table => format_table(&graph),
        DepsFormat::Jsonl | DepsFormat::Json => {
            let result_set = deps_to_result_set(&graph, file_str.as_deref(), reverse, &cycles);
            let renderer = Renderer::with_config(config);
            renderer.render(&result_set)
        }
    };

    println!("{}", output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_path(Path::new("foo.rs")), Language::Rust);
        assert_eq!(
            Language::from_path(Path::new("foo.ts")),
            Language::TypeScript
        );
        assert_eq!(
            Language::from_path(Path::new("foo.tsx")),
            Language::TypeScript
        );
        assert_eq!(
            Language::from_path(Path::new("foo.js")),
            Language::JavaScript
        );
        assert_eq!(Language::from_path(Path::new("foo.py")), Language::Python);
        assert_eq!(Language::from_path(Path::new("foo.txt")), Language::Unknown);
    }

    #[test]
    fn test_extract_js_import_path() {
        assert_eq!(
            extract_js_import_path("import foo from './bar'"),
            Some("./bar".to_string())
        );
        assert_eq!(
            extract_js_import_path("import foo from \"./bar\""),
            Some("./bar".to_string())
        );
        assert_eq!(
            extract_js_import_path("const x = require('./bar')"),
            Some("./bar".to_string())
        );
    }

    #[test]
    fn test_deps_format_parse() {
        assert_eq!("jsonl".parse::<DepsFormat>().unwrap(), DepsFormat::Jsonl);
        assert_eq!("dot".parse::<DepsFormat>().unwrap(), DepsFormat::Dot);
        assert_eq!(
            "mermaid".parse::<DepsFormat>().unwrap(),
            DepsFormat::Mermaid
        );
        assert_eq!("tree".parse::<DepsFormat>().unwrap(), DepsFormat::Tree);
        assert_eq!("table".parse::<DepsFormat>().unwrap(), DepsFormat::Table);
    }

    #[test]
    fn test_language_from_path_js_variants() {
        assert_eq!(
            Language::from_path(Path::new("file.jsx")),
            Language::JavaScript
        );
        assert_eq!(
            Language::from_path(Path::new("file.mjs")),
            Language::JavaScript
        );
        assert_eq!(
            Language::from_path(Path::new("file.cjs")),
            Language::JavaScript
        );
    }

    #[test]
    fn test_language_sg_lang() {
        assert_eq!(Language::Rust.sg_lang(), Some("rust"));
        assert_eq!(Language::TypeScript.sg_lang(), Some("typescript"));
        assert_eq!(Language::JavaScript.sg_lang(), Some("javascript"));
        assert_eq!(Language::Python.sg_lang(), Some("python"));
        assert_eq!(Language::Unknown.sg_lang(), None);
    }

    #[test]
    fn test_language_extensions() {
        assert_eq!(Language::Rust.extensions(), &["rs"]);
        assert_eq!(Language::TypeScript.extensions(), &["ts", "tsx"]);
        assert_eq!(
            Language::JavaScript.extensions(),
            &["js", "jsx", "mjs", "cjs"]
        );
        assert_eq!(Language::Python.extensions(), &["py"]);
        assert_eq!(Language::Unknown.extensions(), &[] as &[&str]);
    }

    #[test]
    fn test_dep_graph_new() {
        let graph = DepGraph::new();
        assert!(graph.files.is_empty());
        assert!(graph.module_map.is_empty());
    }

    #[test]
    fn test_dep_graph_default() {
        let graph = DepGraph::default();
        assert!(graph.files.is_empty());
        assert!(graph.module_map.is_empty());
    }

    #[test]
    fn test_dep_graph_get_forward_deps_empty() {
        let graph = DepGraph::new();
        let deps = graph.get_forward_deps("nonexistent.rs");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_dep_graph_get_reverse_deps_empty() {
        let graph = DepGraph::new();
        let deps = graph.get_reverse_deps("nonexistent.rs");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_dep_graph_with_files() {
        let mut graph = DepGraph::new();

        graph.files.insert(
            "main.rs".to_string(),
            FileDeps {
                path: "main.rs".to_string(),
                language: Language::Rust,
                depends_on: vec![Dependency {
                    import_text: "use lib".to_string(),
                    module: "lib".to_string(),
                    resolved_path: Some("lib.rs".to_string()),
                    line: 1,
                }],
                depended_by: vec![],
            },
        );

        graph.files.insert(
            "lib.rs".to_string(),
            FileDeps {
                path: "lib.rs".to_string(),
                language: Language::Rust,
                depends_on: vec![],
                depended_by: vec![],
            },
        );

        // Build reverse deps
        graph.build_reverse_deps();

        // Check forward deps
        let forward = graph.get_forward_deps("main.rs");
        assert_eq!(forward, vec!["lib.rs".to_string()]);

        // Check reverse deps
        let reverse = graph.get_reverse_deps("lib.rs");
        assert_eq!(reverse, vec!["main.rs".to_string()]);
    }

    #[test]
    fn test_dep_graph_find_cycles_empty() {
        let graph = DepGraph::new();
        let cycles = graph.find_cycles();
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_dependency_creation() {
        let dep = Dependency {
            import_text: "import foo".to_string(),
            module: "foo".to_string(),
            resolved_path: Some("foo.ts".to_string()),
            line: 5,
        };
        assert_eq!(dep.module, "foo");
        assert_eq!(dep.line, 5);
    }

    #[test]
    fn test_file_deps_creation() {
        let file_deps = FileDeps {
            path: "test.rs".to_string(),
            language: Language::Rust,
            depends_on: vec![],
            depended_by: vec!["other.rs".to_string()],
        };
        assert_eq!(file_deps.path, "test.rs");
        assert_eq!(file_deps.language, Language::Rust);
    }

    #[test]
    fn test_extract_js_import_path_require() {
        assert_eq!(
            extract_js_import_path("const x = require('lodash')"),
            Some("lodash".to_string())
        );
        assert_eq!(
            extract_js_import_path("const x = require(\"lodash\")"),
            Some("lodash".to_string())
        );
    }

    #[test]
    fn test_extract_js_import_path_no_match() {
        assert_eq!(extract_js_import_path("const x = 5"), None);
        assert_eq!(extract_js_import_path("// comment"), None);
        assert_eq!(extract_js_import_path(""), None);
    }

    #[test]
    fn test_deps_format_default() {
        let format = DepsFormat::default();
        assert_eq!(format, DepsFormat::Jsonl);
    }

    #[test]
    fn test_deps_format_parse_invalid() {
        assert!("invalid".parse::<DepsFormat>().is_err());
    }

    #[test]
    fn test_deps_format_parse_aliases() {
        assert_eq!("graphviz".parse::<DepsFormat>().unwrap(), DepsFormat::Dot);
        assert_eq!("mmd".parse::<DepsFormat>().unwrap(), DepsFormat::Mermaid);
        assert_eq!("json".parse::<DepsFormat>().unwrap(), DepsFormat::Json);
    }

    #[test]
    fn test_import_patterns_for_rust() {
        let patterns = ImportPatterns::for_language(Language::Rust);
        assert!(patterns.is_some());
        let p = patterns.unwrap();
        assert_eq!(p.lang, "rust");
        assert!(!p.patterns.is_empty());
    }

    #[test]
    fn test_import_patterns_for_typescript() {
        let patterns = ImportPatterns::for_language(Language::TypeScript);
        assert!(patterns.is_some());
        let p = patterns.unwrap();
        assert_eq!(p.lang, "typescript");
    }

    #[test]
    fn test_import_patterns_for_javascript() {
        let patterns = ImportPatterns::for_language(Language::JavaScript);
        assert!(patterns.is_some());
        let p = patterns.unwrap();
        assert_eq!(p.lang, "javascript");
    }

    #[test]
    fn test_import_patterns_for_python() {
        let patterns = ImportPatterns::for_language(Language::Python);
        assert!(patterns.is_some());
        let p = patterns.unwrap();
        assert_eq!(p.lang, "python");
    }

    #[test]
    fn test_import_patterns_for_unknown() {
        let patterns = ImportPatterns::for_language(Language::Unknown);
        assert!(patterns.is_none());
    }

    #[test]
    fn test_dep_graph_multiple_deps() {
        let mut graph = DepGraph::new();

        graph.files.insert(
            "main.rs".to_string(),
            FileDeps {
                path: "main.rs".to_string(),
                language: Language::Rust,
                depends_on: vec![
                    Dependency {
                        import_text: "use a".to_string(),
                        module: "a".to_string(),
                        resolved_path: Some("a.rs".to_string()),
                        line: 1,
                    },
                    Dependency {
                        import_text: "use b".to_string(),
                        module: "b".to_string(),
                        resolved_path: Some("b.rs".to_string()),
                        line: 2,
                    },
                    Dependency {
                        import_text: "use a".to_string(),
                        module: "a".to_string(),
                        resolved_path: Some("a.rs".to_string()), // duplicate
                        line: 3,
                    },
                ],
                depended_by: vec![],
            },
        );

        let deps = graph.get_forward_deps("main.rs");
        // Should be deduplicated and sorted
        assert_eq!(deps, vec!["a.rs".to_string(), "b.rs".to_string()]);
    }
}
