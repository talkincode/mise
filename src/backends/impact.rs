//! Change impact analysis
//!
//! Analyzes the impact of code changes by combining git diff with
//! the dependency graph to understand "what will be affected by this change".

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::anchors::parse::parse_file;
use crate::backends::deps::{analyze_deps, DepGraph};
use crate::backends::scan::scan_files;
use crate::core::model::{Confidence, Kind, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Source of diff information
#[derive(Debug, Clone, Default)]
pub enum DiffSource {
    /// Unstaged changes (git diff)
    #[default]
    Unstaged,
    /// Staged changes (git diff --staged)
    Staged,
    /// Specific commit (git diff commit^..commit)
    Commit(String),
    /// Branch comparison (git diff base..head)
    Diff(String, String),
}

impl DiffSource {
    /// Parse diff source from CLI arguments
    pub fn from_args(staged: bool, commit: Option<&str>, diff: Option<&str>) -> Self {
        if staged {
            DiffSource::Staged
        } else if let Some(c) = commit {
            DiffSource::Commit(c.to_string())
        } else if let Some(d) = diff {
            // Parse "base..head" format
            if let Some((base, head)) = d.split_once("..") {
                DiffSource::Diff(base.to_string(), head.to_string())
            } else {
                // Treat as commit
                DiffSource::Commit(d.to_string())
            }
        } else {
            DiffSource::Unstaged
        }
    }

    /// Get git diff arguments for this source (only for simple cases)
    fn git_args(&self) -> Vec<String> {
        match self {
            DiffSource::Unstaged => vec!["diff".to_string(), "--name-only".to_string()],
            DiffSource::Staged => vec![
                "diff".to_string(),
                "--staged".to_string(),
                "--name-only".to_string(),
            ],
            DiffSource::Commit(c) => vec![
                "diff".to_string(),
                "--name-only".to_string(),
                format!("{}^", c),
                c.clone(),
            ],
            DiffSource::Diff(base, head) => vec![
                "diff".to_string(),
                "--name-only".to_string(),
                format!("{}..{}", base, head),
            ],
        }
    }

    /// Description for display
    pub fn description(&self) -> String {
        match self {
            DiffSource::Unstaged => "unstaged changes".to_string(),
            DiffSource::Staged => "staged changes".to_string(),
            DiffSource::Commit(c) => format!("commit {}", c),
            DiffSource::Diff(base, head) => format!("{}..{}", base, head),
        }
    }
}

/// Impact analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    /// Files that were changed
    pub changed_files: Vec<String>,
    /// Files that directly depend on changed files
    pub direct_impacts: Vec<String>,
    /// Files transitively affected (up to max_depth)
    pub transitive_impacts: Vec<String>,
    /// Anchors that are affected by the changes
    pub anchors_affected: Vec<String>,
    /// Description of the diff source
    pub source: String,
}

impl ImpactAnalysis {
    pub fn new(source: &str) -> Self {
        Self {
            changed_files: Vec::new(),
            direct_impacts: Vec::new(),
            transitive_impacts: Vec::new(),
            anchors_affected: Vec::new(),
            source: source.to_string(),
        }
    }

    /// Total number of affected files
    pub fn total_affected(&self) -> usize {
        self.changed_files.len() + self.direct_impacts.len() + self.transitive_impacts.len()
    }
}

/// Get changed files from git diff
fn get_changed_files(root: &Path, source: &DiffSource) -> Result<Vec<String>> {
    let args = source.git_args();

    // Build command
    let output = Command::new("git").current_dir(root).args(&args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(anyhow::anyhow!("Not a git repository"));
        }
        // For commits, if the diff fails, try a different approach
        if let DiffSource::Commit(c) = source {
            // Try single commit show
            let output = Command::new("git")
                .current_dir(root)
                .arg("show")
                .arg("--name-only")
                .arg("--pretty=format:")
                .arg(c)
                .output()?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Ok(stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect());
            }
        }
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Compute direct impacts (files that depend on changed files)
fn compute_direct_impacts(changed: &[String], graph: &DepGraph) -> Vec<String> {
    let changed_set: HashSet<_> = changed.iter().collect();
    let mut impacts = HashSet::new();

    for file in changed {
        // Get files that depend on this changed file (reverse deps)
        let reverse_deps = graph.get_reverse_deps(file);
        for dep in reverse_deps {
            if !changed_set.contains(&dep) {
                impacts.insert(dep);
            }
        }
    }

    let mut result: Vec<_> = impacts.into_iter().collect();
    result.sort();
    result
}

/// Compute transitive impacts (up to max_depth levels)
fn compute_transitive_impacts(
    changed: &[String],
    direct: &[String],
    graph: &DepGraph,
    max_depth: usize,
) -> Vec<String> {
    let mut seen: HashSet<String> = changed.iter().cloned().collect();
    seen.extend(direct.iter().cloned());

    let mut current_level: HashSet<String> = direct.iter().cloned().collect();
    let mut all_transitive = HashSet::new();

    for _depth in 0..max_depth {
        let mut next_level = HashSet::new();

        for file in &current_level {
            let reverse_deps = graph.get_reverse_deps(file);
            for dep in reverse_deps {
                if !seen.contains(&dep) {
                    next_level.insert(dep.clone());
                    all_transitive.insert(dep);
                }
            }
        }

        if next_level.is_empty() {
            break;
        }

        seen.extend(next_level.iter().cloned());
        current_level = next_level;
    }

    let mut result: Vec<_> = all_transitive.into_iter().collect();
    result.sort();
    result
}

/// Find anchors that are affected by file changes
fn find_affected_anchors(
    root: &Path,
    changed: &[String],
    direct: &[String],
    transitive: &[String],
) -> Vec<String> {
    // Collect all affected files
    let affected_files: HashSet<_> = changed
        .iter()
        .chain(direct.iter())
        .chain(transitive.iter())
        .collect();

    // Scan all files and parse anchors
    let files = match scan_files(root, None, None, false, true, Some("file")) {
        Ok(result) => result,
        Err(_) => return Vec::new(),
    };

    let mut affected_anchors = Vec::new();

    for item in files.items {
        if let Some(path) = &item.path {
            // Check if this file is in the affected set
            if !affected_files.contains(path) {
                continue;
            }

            let full_path = root.join(path);

            // Only process text files that might have anchors
            if !is_anchor_candidate(&full_path) {
                continue;
            }

            // Parse anchors from this file
            let anchors = parse_file(&full_path, path);
            for anchor in anchors {
                affected_anchors.push(anchor.id);
            }
        }
    }

    affected_anchors.sort();
    affected_anchors.dedup();
    affected_anchors
}

/// Check if a file might contain anchors
fn is_anchor_candidate(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "md" | "txt"
            | "rs"
            | "py"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "html"
            | "css"
            | "scss"
            | "yaml"
            | "yml"
            | "toml"
            | "json"
    )
}

/// Analyze the impact of changes
pub fn analyze_impact(root: &Path, source: DiffSource, max_depth: usize) -> Result<ImpactAnalysis> {
    let mut analysis = ImpactAnalysis::new(&source.description());

    // Step 1: Get changed files from git
    analysis.changed_files = get_changed_files(root, &source)?;

    if analysis.changed_files.is_empty() {
        return Ok(analysis);
    }

    // Step 2: Build dependency graph
    let graph = analyze_deps(root, None)?;

    // Step 3: Compute direct impacts
    analysis.direct_impacts = compute_direct_impacts(&analysis.changed_files, &graph);

    // Step 4: Compute transitive impacts
    analysis.transitive_impacts = compute_transitive_impacts(
        &analysis.changed_files,
        &analysis.direct_impacts,
        &graph,
        max_depth,
    );

    // Step 5: Find affected anchors
    analysis.anchors_affected = find_affected_anchors(
        root,
        &analysis.changed_files,
        &analysis.direct_impacts,
        &analysis.transitive_impacts,
    );

    Ok(analysis)
}

/// Convert impact analysis to ResultSet
#[allow(dead_code)]
fn impact_to_result_set(analysis: &ImpactAnalysis) -> ResultSet {
    let mut result_set = ResultSet::new();

    // Add changed files with high confidence
    for file in &analysis.changed_files {
        let mut item = ResultItem::file(file);
        item.kind = Kind::Flow;
        item.confidence = Confidence::High;
        item.source_mode = SourceMode::Scan;
        item.excerpt = Some("changed".to_string());
        result_set.push(item);
    }

    // Add direct impacts with medium confidence
    for file in &analysis.direct_impacts {
        let mut item = ResultItem::file(file);
        item.kind = Kind::Flow;
        item.confidence = Confidence::Medium;
        item.source_mode = SourceMode::Mixed;
        item.excerpt = Some("direct_impact".to_string());
        result_set.push(item);
    }

    // Add transitive impacts with low confidence
    for file in &analysis.transitive_impacts {
        let mut item = ResultItem::file(file);
        item.kind = Kind::Flow;
        item.confidence = Confidence::Low;
        item.source_mode = SourceMode::Mixed;
        item.excerpt = Some("transitive_impact".to_string());
        result_set.push(item);
    }

    result_set
}

/// Output format for impact command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImpactFormat {
    #[default]
    Jsonl,
    Json,
    Summary,
    Table,
}

impl std::str::FromStr for ImpactFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "jsonl" => Ok(ImpactFormat::Jsonl),
            "json" => Ok(ImpactFormat::Json),
            "summary" => Ok(ImpactFormat::Summary),
            "table" => Ok(ImpactFormat::Table),
            _ => Err(format!("Unknown impact format: {}", s)),
        }
    }
}

/// Format impact analysis as summary
fn format_summary(analysis: &ImpactAnalysis) -> String {
    let mut output = String::new();

    output.push_str(&format!("📊 Impact Analysis: {}\n", analysis.source));
    output.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n");

    if analysis.changed_files.is_empty() {
        output.push_str("No changes detected.\n");
        return output;
    }

    // Changed files
    output.push_str(&format!(
        "🔴 Changed files ({})\n",
        analysis.changed_files.len()
    ));
    for file in &analysis.changed_files {
        output.push_str(&format!("   {}\n", file));
    }
    output.push('\n');

    // Direct impacts
    if !analysis.direct_impacts.is_empty() {
        output.push_str(&format!(
            "🟠 Direct impacts ({})\n",
            analysis.direct_impacts.len()
        ));
        for file in &analysis.direct_impacts {
            output.push_str(&format!("   {}\n", file));
        }
        output.push('\n');
    }

    // Transitive impacts
    if !analysis.transitive_impacts.is_empty() {
        output.push_str(&format!(
            "🟡 Transitive impacts ({})\n",
            analysis.transitive_impacts.len()
        ));
        for file in &analysis.transitive_impacts {
            output.push_str(&format!("   {}\n", file));
        }
        output.push('\n');
    }

    // Affected anchors
    if !analysis.anchors_affected.is_empty() {
        output.push_str(&format!(
            "📌 Affected anchors ({})\n",
            analysis.anchors_affected.len()
        ));
        for anchor in &analysis.anchors_affected {
            output.push_str(&format!("   {}\n", anchor));
        }
        output.push('\n');
    }

    // Summary
    output.push_str(&format!(
        "Total affected: {} files\n",
        analysis.total_affected()
    ));

    output
}

/// Format impact analysis as table
fn format_table(analysis: &ImpactAnalysis) -> String {
    let mut output = String::new();

    // Calculate max widths
    let all_files: Vec<_> = analysis
        .changed_files
        .iter()
        .chain(analysis.direct_impacts.iter())
        .chain(analysis.transitive_impacts.iter())
        .collect();

    if all_files.is_empty() {
        return "No changes detected.\n".to_string();
    }

    let max_path_len = all_files.iter().map(|p| p.len()).max().unwrap_or(4).max(4);
    let type_width = 18;

    // Header
    output.push_str(&format!(
        "┌─{:─<width$}─┬─{:─<type_width$}─┐\n",
        "",
        "",
        width = max_path_len,
        type_width = type_width
    ));
    output.push_str(&format!(
        "│ {:width$} │ {:type_width$} │\n",
        "File",
        "Impact Type",
        width = max_path_len,
        type_width = type_width
    ));
    output.push_str(&format!(
        "├─{:─<width$}─┼─{:─<type_width$}─┤\n",
        "",
        "",
        width = max_path_len,
        type_width = type_width
    ));

    // Changed files
    for file in &analysis.changed_files {
        output.push_str(&format!(
            "│ {:width$} │ {:type_width$} │\n",
            file,
            "🔴 changed",
            width = max_path_len,
            type_width = type_width
        ));
    }

    // Direct impacts
    for file in &analysis.direct_impacts {
        output.push_str(&format!(
            "│ {:width$} │ {:type_width$} │\n",
            file,
            "🟠 direct impact",
            width = max_path_len,
            type_width = type_width
        ));
    }

    // Transitive impacts
    for file in &analysis.transitive_impacts {
        output.push_str(&format!(
            "│ {:width$} │ {:type_width$} │\n",
            file,
            "🟡 transitive",
            width = max_path_len,
            type_width = type_width
        ));
    }

    output.push_str(&format!(
        "└─{:─<width$}─┴─{:─<type_width$}─┘\n",
        "",
        "",
        width = max_path_len,
        type_width = type_width
    ));

    // Anchors section
    if !analysis.anchors_affected.is_empty() {
        output.push_str(&format!(
            "\n📌 Affected anchors: {}\n",
            analysis.anchors_affected.join(", ")
        ));
    }

    output
}

/// Run the impact command
pub fn run_impact(
    root: &Path,
    staged: bool,
    commit: Option<&str>,
    diff: Option<&str>,
    max_depth: usize,
    format: ImpactFormat,
    config: RenderConfig,
) -> Result<()> {
    // Check if git is available
    if !command_exists("git") {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::error(MiseError::new(
            "GIT_NOT_FOUND",
            "git is not installed. Please install git to use impact analysis.",
        )));
        let renderer = Renderer::with_config(config);
        println!("{}", renderer.render(&result_set));
        return Ok(());
    }

    // Determine diff source
    let source = DiffSource::from_args(staged, commit, diff);

    // Analyze impact
    let analysis = analyze_impact(root, source, max_depth)?;

    // Output based on format
    let output = match format {
        ImpactFormat::Summary => format_summary(&analysis),
        ImpactFormat::Table => format_table(&analysis),
        ImpactFormat::Jsonl | ImpactFormat::Json => {
            // For JSON formats, output the analysis directly
            if format == ImpactFormat::Json {
                serde_json::to_string_pretty(&analysis)?
            } else {
                serde_json::to_string(&analysis)?
            }
        }
    };

    println!("{}", output);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_source_from_args() {
        assert!(matches!(
            DiffSource::from_args(false, None, None),
            DiffSource::Unstaged
        ));
        assert!(matches!(
            DiffSource::from_args(true, None, None),
            DiffSource::Staged
        ));
        assert!(matches!(
            DiffSource::from_args(false, Some("abc123"), None),
            DiffSource::Commit(_)
        ));
        assert!(matches!(
            DiffSource::from_args(false, None, Some("main..feature")),
            DiffSource::Diff(_, _)
        ));
    }

    #[test]
    fn test_impact_format_parse() {
        assert_eq!(
            "jsonl".parse::<ImpactFormat>().unwrap(),
            ImpactFormat::Jsonl
        );
        assert_eq!("json".parse::<ImpactFormat>().unwrap(), ImpactFormat::Json);
        assert_eq!(
            "summary".parse::<ImpactFormat>().unwrap(),
            ImpactFormat::Summary
        );
        assert_eq!(
            "table".parse::<ImpactFormat>().unwrap(),
            ImpactFormat::Table
        );
    }

    #[test]
    fn test_impact_analysis_new() {
        let analysis = ImpactAnalysis::new("test");
        assert_eq!(analysis.source, "test");
        assert!(analysis.changed_files.is_empty());
        assert_eq!(analysis.total_affected(), 0);
    }
}
