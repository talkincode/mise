//! Run module - Concurrent command execution
//!
//! Execute multiple independent misec commands (or external commands) in parallel
//! with structured result collection. Optimized for running multiple misec analysis
//! tasks concurrently.
//!
//! # Use Cases
//!
//! - Parallel code scanning across different directories
//! - Concurrent pattern matching with different queries
//! - Batch anchor operations
//! - Mixed misec and external command workflows
//!
//! # Output Management
//!
//! Results are saved to temporary files in the workspace's `rundata/` directory by default.
//! Each task creates a `<task_id>.log` file containing stdout/stderr and metadata.
//! This is ideal for intermediate results that can be processed or discarded.
//!
//! # Example Task Definitions
//!
//! ```json
//! // Parallel misec analysis
//! [
//!   {"id": "scan-src", "cmd": "misec scan --scope src --type file"},
//!   {"id": "find-todo", "cmd": "misec match 'TODO|FIXME' src/"},
//!   {"id": "list-anchors", "cmd": "misec anchor list --brief"}
//! ]
//!
//! // With dependencies
//! [
//!   {"id": "rebuild", "cmd": "misec rebuild"},
//!   {"id": "lint", "cmd": "misec anchor lint", "depends_on": ["rebuild"]}
//! ]
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::model::{Confidence, Kind, Meta, MiseError, ResultItem, ResultSet, SourceMode};
use crate::core::render::{RenderConfig, Renderer};

/// Task definition for concurrent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier
    pub id: String,

    /// Shell command to execute
    pub cmd: String,

    /// Working directory (relative to root)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Environment variables to set
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Timeout in seconds (default: 300)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Dependencies (other task IDs that must complete first)
    /// Note: Tasks with dependencies will wait for those tasks to complete
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,

    /// Tags for filtering and grouping
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Description for documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_timeout() -> u64 {
    300
}

/// Task group for organizing related tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGroup {
    /// Group name
    pub name: String,

    /// Tasks in this group
    pub tasks: Vec<Task>,

    /// Whether to run tasks in parallel (default: true)
    #[serde(default = "default_parallel")]
    pub parallel: bool,

    /// Continue on error (default: false)
    #[serde(default)]
    pub continue_on_error: bool,
}

fn default_parallel() -> bool {
    true
}

/// Task set containing multiple groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSet {
    /// Name of the task set
    #[serde(default)]
    pub name: String,

    /// Task groups
    #[serde(default)]
    pub groups: Vec<TaskGroup>,

    /// Standalone tasks (not in any group)
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// Result of a single task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID
    pub id: String,

    /// Exit code (None if timed out or failed to start)
    pub exit_code: Option<i32>,

    /// Standard output
    pub stdout: String,

    /// Standard error
    pub stderr: String,

    /// Execution duration in milliseconds
    pub duration_ms: u64,

    /// Whether the task succeeded
    pub success: bool,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Output file path (if written)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
}

/// Run options
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Maximum concurrent tasks (0 = auto based on CPU count)
    pub max_parallel: usize,

    /// Output directory for results
    pub output_dir: Option<PathBuf>,

    /// Whether to save individual task outputs to files
    pub save_outputs: bool,

    /// Continue execution on task failure
    pub continue_on_error: bool,

    /// Global timeout override in seconds
    pub timeout: Option<u64>,

    /// Filter tasks by tag
    pub filter_tag: Option<String>,

    /// Dry run (show what would be executed without running)
    pub dry_run: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_parallel: 0,
            output_dir: None,
            save_outputs: true,
            continue_on_error: false,
            timeout: None,
            filter_tag: None,
            dry_run: false,
        }
    }
}

/// Execution summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// Total tasks executed
    pub total: usize,

    /// Successful tasks
    pub succeeded: usize,

    /// Failed tasks
    pub failed: usize,

    /// Skipped tasks (due to dependency failure)
    pub skipped: usize,

    /// Total duration in milliseconds
    pub total_duration_ms: u64,

    /// Output directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
}

/// Parse task set from JSON or YAML string
pub fn parse_tasks(input: &str) -> Result<TaskSet> {
    // Try as single task first (most specific)
    if let Ok(task) = serde_json::from_str::<Task>(input) {
        // Check if it has required fields (id and cmd)
        if !task.id.is_empty() && !task.cmd.is_empty() {
            return Ok(TaskSet {
                name: "single".to_string(),
                groups: Vec::new(),
                tasks: vec![task],
            });
        }
    }

    // Try as array of tasks
    if let Ok(tasks) = serde_json::from_str::<Vec<Task>>(input) {
        if !tasks.is_empty() {
            return Ok(TaskSet {
                name: "tasks".to_string(),
                groups: Vec::new(),
                tasks,
            });
        }
    }

    // Try full TaskSet
    if let Ok(task_set) = serde_json::from_str::<TaskSet>(input) {
        if !task_set.tasks.is_empty() || !task_set.groups.is_empty() {
            return Ok(task_set);
        }
    }

    anyhow::bail!("Failed to parse task definition. Expected JSON object with 'tasks' or 'groups' field, an array of tasks, or a single task object.")
}

/// Parse task set from file
pub fn parse_tasks_from_file(path: &Path) -> Result<TaskSet> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read task file: {}", path.display()))?;
    parse_tasks(&content)
}

/// Execute a single task
fn execute_task(
    root: &Path,
    task: &Task,
    timeout_override: Option<u64>,
    output_dir: Option<&Path>,
    save_output: bool,
) -> TaskResult {
    let start = Instant::now();
    let timeout_secs = timeout_override.unwrap_or(task.timeout);

    // Determine working directory
    let work_dir = if let Some(cwd) = &task.cwd {
        root.join(cwd)
    } else {
        root.to_path_buf()
    };

    // Build command
    let shell = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut cmd = Command::new(shell.0);
    cmd.arg(shell.1)
        .arg(&task.cmd)
        .current_dir(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set environment variables
    for (key, value) in &task.env {
        cmd.env(key, value);
    }

    // Execute with timeout
    let result = match cmd.spawn() {
        Ok(mut child) => {
            // Wait with timeout
            let timeout = Duration::from_secs(timeout_secs);
            let start_wait = Instant::now();

            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Process completed
                        let stdout = child
                            .stdout
                            .take()
                            .map(|mut o| {
                                let mut s = String::new();
                                std::io::Read::read_to_string(&mut o, &mut s).ok();
                                s
                            })
                            .unwrap_or_default();

                        let stderr = child
                            .stderr
                            .take()
                            .map(|mut e| {
                                let mut s = String::new();
                                std::io::Read::read_to_string(&mut e, &mut s).ok();
                                s
                            })
                            .unwrap_or_default();

                        let exit_code = status.code();
                        let success = status.success();

                        break TaskResult {
                            id: task.id.clone(),
                            exit_code,
                            stdout,
                            stderr,
                            duration_ms: start.elapsed().as_millis() as u64,
                            success,
                            error: if success {
                                None
                            } else {
                                Some(format!("Exit code: {:?}", exit_code))
                            },
                            output_file: None,
                        };
                    }
                    Ok(None) => {
                        // Still running, check timeout
                        if start_wait.elapsed() > timeout {
                            let _ = child.kill();
                            break TaskResult {
                                id: task.id.clone(),
                                exit_code: None,
                                stdout: String::new(),
                                stderr: String::new(),
                                duration_ms: start.elapsed().as_millis() as u64,
                                success: false,
                                error: Some(format!("Timeout after {} seconds", timeout_secs)),
                                output_file: None,
                            };
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        break TaskResult {
                            id: task.id.clone(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: String::new(),
                            duration_ms: start.elapsed().as_millis() as u64,
                            success: false,
                            error: Some(format!("Failed to wait for process: {}", e)),
                            output_file: None,
                        };
                    }
                }
            }
        }
        Err(e) => TaskResult {
            id: task.id.clone(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(format!("Failed to start command: {}", e)),
            output_file: None,
        },
    };

    // Save output to file if requested
    let mut final_result = result;
    if save_output {
        if let Some(out_dir) = output_dir {
            let output_file = out_dir.join(format!("{}.log", sanitize_filename(&task.id)));
            if let Ok(mut file) = fs::File::create(&output_file) {
                // Format output - if stdout looks like JSON/JSONL, preserve it cleanly
                let is_json_output = final_result.stdout.trim_start().starts_with('{')
                    || final_result.stdout.trim_start().starts_with('[');

                let content = if is_json_output && final_result.stderr.is_empty() {
                    // Clean JSON output (typical misec output)
                    format!(
                        "# Task: {} | Exit: {:?} | Duration: {}ms\n# Command: {}\n\n{}\n",
                        task.id,
                        final_result.exit_code.unwrap_or(-1),
                        final_result.duration_ms,
                        task.cmd,
                        final_result.stdout
                    )
                } else {
                    // Full format with sections
                    format!(
                        "# Task: {}\n# Command: {}\n# Exit Code: {:?}\n# Duration: {}ms\n# Success: {}\n\n## STDOUT:\n{}\n{}",
                        task.id,
                        task.cmd,
                        final_result.exit_code,
                        final_result.duration_ms,
                        final_result.success,
                        final_result.stdout,
                        if !final_result.stderr.is_empty() {
                            format!("\n## STDERR:\n{}\n", final_result.stderr)
                        } else {
                            String::new()
                        }
                    )
                };
                let _ = file.write_all(content.as_bytes());
                final_result.output_file = Some(output_file.to_string_lossy().to_string());
            }
        }
    }

    final_result
}

/// Sanitize filename for task output
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Execute tasks concurrently
pub fn execute_tasks(
    root: &Path,
    task_set: &TaskSet,
    options: &RunOptions,
) -> Result<(Vec<TaskResult>, ExecutionSummary)> {
    let start = Instant::now();

    // Collect all tasks (from groups and standalone)
    let mut all_tasks: Vec<Task> = Vec::new();

    for group in &task_set.groups {
        all_tasks.extend(group.tasks.clone());
    }
    all_tasks.extend(task_set.tasks.clone());

    // Filter by tag if specified
    if let Some(tag) = &options.filter_tag {
        all_tasks.retain(|t| t.tags.contains(tag));
    }

    if all_tasks.is_empty() {
        return Ok((
            Vec::new(),
            ExecutionSummary {
                total: 0,
                succeeded: 0,
                failed: 0,
                skipped: 0,
                total_duration_ms: start.elapsed().as_millis() as u64,
                output_dir: options
                    .output_dir
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
            },
        ));
    }

    // Create output directory
    let output_dir = if options.save_outputs {
        let dir = options
            .output_dir
            .clone()
            .unwrap_or_else(|| root.join("rundata"));
        fs::create_dir_all(&dir)?;
        Some(dir)
    } else {
        None
    };

    // Dry run: just show what would be executed
    if options.dry_run {
        let results: Vec<TaskResult> = all_tasks
            .iter()
            .map(|t| TaskResult {
                id: t.id.clone(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
                success: true,
                error: None,
                output_file: None,
            })
            .collect();

        return Ok((
            results,
            ExecutionSummary {
                total: all_tasks.len(),
                succeeded: all_tasks.len(),
                failed: 0,
                skipped: 0,
                total_duration_ms: 0,
                output_dir: output_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
            },
        ));
    }

    // Determine parallelism
    let max_parallel = if options.max_parallel == 0 {
        num_cpus()
    } else {
        options.max_parallel
    };

    // Build dependency graph
    let mut task_map: HashMap<String, Task> = HashMap::new();
    for task in &all_tasks {
        task_map.insert(task.id.clone(), task.clone());
    }

    // Separate tasks by dependencies
    let (independent_tasks, dependent_tasks): (Vec<_>, Vec<_>) =
        all_tasks.iter().partition(|t| t.depends_on.is_empty());

    let results: Arc<Mutex<Vec<TaskResult>>> = Arc::new(Mutex::new(Vec::new()));
    let completed: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));

    // Execute independent tasks in parallel
    execute_parallel(
        root,
        &independent_tasks.into_iter().cloned().collect::<Vec<_>>(),
        max_parallel,
        options.timeout,
        output_dir.as_deref(),
        options.save_outputs,
        options.continue_on_error,
        &results,
        &completed,
    );

    // Execute dependent tasks (respecting dependencies)
    for task in dependent_tasks {
        // Check if all dependencies succeeded
        let deps_ok = {
            let completed_guard = completed.lock().unwrap();
            task.depends_on
                .iter()
                .all(|dep| completed_guard.get(dep).copied().unwrap_or(false))
        };

        if deps_ok {
            let result = execute_task(
                root,
                task,
                options.timeout,
                output_dir.as_deref(),
                options.save_outputs,
            );

            {
                let mut completed_guard = completed.lock().unwrap();
                completed_guard.insert(task.id.clone(), result.success);
            }
            {
                let mut results_guard = results.lock().unwrap();
                results_guard.push(result);
            }
        } else {
            // Skip task due to dependency failure
            let result = TaskResult {
                id: task.id.clone(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
                success: false,
                error: Some("Skipped: dependency failed".to_string()),
                output_file: None,
            };
            {
                let mut completed_guard = completed.lock().unwrap();
                completed_guard.insert(task.id.clone(), false);
            }
            {
                let mut results_guard = results.lock().unwrap();
                results_guard.push(result);
            }
        }
    }

    let final_results = match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    };

    let succeeded = final_results.iter().filter(|r| r.success).count();
    let failed = final_results
        .iter()
        .filter(|r| !r.success && r.error.as_deref() != Some("Skipped: dependency failed"))
        .count();
    let skipped = final_results
        .iter()
        .filter(|r| r.error.as_deref() == Some("Skipped: dependency failed"))
        .count();

    let summary = ExecutionSummary {
        total: final_results.len(),
        succeeded,
        failed,
        skipped,
        total_duration_ms: start.elapsed().as_millis() as u64,
        output_dir: output_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
    };

    Ok((final_results, summary))
}

/// Execute tasks in parallel using thread pool
fn execute_parallel(
    root: &Path,
    tasks: &[Task],
    max_parallel: usize,
    timeout_override: Option<u64>,
    output_dir: Option<&Path>,
    save_output: bool,
    continue_on_error: bool,
    results: &Arc<Mutex<Vec<TaskResult>>>,
    completed: &Arc<Mutex<HashMap<String, bool>>>,
) {
    let root = root.to_path_buf();
    let output_dir = output_dir.map(|p| p.to_path_buf());

    // Use scoped threads for parallel execution
    let chunk_size = (tasks.len() + max_parallel - 1) / max_parallel;
    let chunks: Vec<_> = tasks.chunks(chunk_size.max(1)).collect();

    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let root = root.clone();
            let output_dir = output_dir.clone();
            let chunk = chunk.to_vec();
            let results = Arc::clone(results);
            let completed = Arc::clone(completed);

            thread::spawn(move || {
                for task in chunk {
                    let result = execute_task(
                        &root,
                        &task,
                        timeout_override,
                        output_dir.as_deref(),
                        save_output,
                    );

                    let success = result.success;
                    {
                        let mut completed_guard = completed.lock().unwrap();
                        completed_guard.insert(task.id.clone(), success);
                    }
                    {
                        let mut results_guard = results.lock().unwrap();
                        results_guard.push(result);
                    }

                    // Stop if error and not continuing
                    if !success && !continue_on_error {
                        break;
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }
}

/// Get number of CPUs
fn num_cpus() -> usize {
    thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

/// Convert task results to ResultSet
fn results_to_result_set(results: &[TaskResult], summary: &ExecutionSummary) -> ResultSet {
    let mut result_set = ResultSet::new();

    for task_result in results {
        let item = ResultItem {
            kind: Kind::Flow,
            path: task_result.output_file.clone(),
            range: None,
            excerpt: if task_result.stdout.is_empty() && task_result.stderr.is_empty() {
                None
            } else {
                // Combine stdout and stderr, truncate if too long
                let combined = format!(
                    "{}{}",
                    task_result.stdout,
                    if !task_result.stderr.is_empty() {
                        format!("\n[STDERR]\n{}", task_result.stderr)
                    } else {
                        String::new()
                    }
                );
                let truncated = if combined.len() > 4096 {
                    format!("{}...[truncated]", &combined[..4096])
                } else {
                    combined
                };
                Some(truncated)
            },
            data: Some(serde_json::json!({
                "task_id": task_result.id,
                "exit_code": task_result.exit_code,
                "duration_ms": task_result.duration_ms,
                "success": task_result.success,
                "error": task_result.error,
            })),
            confidence: if task_result.success {
                Confidence::High
            } else {
                Confidence::Low
            },
            source_mode: SourceMode::Mixed,
            meta: Meta {
                truncated: task_result.stdout.len() > 4096 || task_result.stderr.len() > 4096,
                ..Default::default()
            },
            errors: if let Some(err) = &task_result.error {
                vec![MiseError::new("task_error", err)]
            } else {
                Vec::new()
            },
        };

        result_set.push(item);
    }

    // Add summary item
    let summary_item = ResultItem {
        kind: Kind::Flow,
        path: None,
        range: None,
        excerpt: None,
        data: Some(serde_json::json!({
            "summary": {
                "total": summary.total,
                "succeeded": summary.succeeded,
                "failed": summary.failed,
                "skipped": summary.skipped,
                "total_duration_ms": summary.total_duration_ms,
                "output_dir": summary.output_dir,
            }
        })),
        confidence: Confidence::High,
        source_mode: SourceMode::Mixed,
        meta: Meta::default(),
        errors: Vec::new(),
    };
    result_set.push(summary_item);

    result_set
}

/// Run the 'run' command
pub fn run_run(
    root: &Path,
    json_input: Option<&str>,
    file_input: Option<&Path>,
    options: RunOptions,
    render_config: RenderConfig,
) -> Result<()> {
    // Parse task set
    let task_set = if let Some(json) = json_input {
        parse_tasks(json)?
    } else if let Some(file) = file_input {
        parse_tasks_from_file(file)?
    } else {
        anyhow::bail!("Either --json or --file must be provided")
    };

    let total_tasks =
        task_set.tasks.len() + task_set.groups.iter().map(|g| g.tasks.len()).sum::<usize>();

    // Dry run output
    if options.dry_run {
        eprintln!("╭─── DRY RUN ───────────────────────────────────────────╮");
        eprintln!(
            "│ Would execute {} task(s)                              ",
            total_tasks
        );
        eprintln!("├────────────────────────────────────────────────────────┤");

        for task in &task_set.tasks {
            let deps = if task.depends_on.is_empty() {
                String::new()
            } else {
                format!(" (after: {})", task.depends_on.join(", "))
            };
            eprintln!("│ [{}]{}", task.id, deps);
            eprintln!("│   └─ {}", task.cmd);
        }

        for group in &task_set.groups {
            eprintln!(
                "│ Group '{}' ({} tasks, parallel={})",
                group.name,
                group.tasks.len(),
                group.parallel
            );
            for task in &group.tasks {
                eprintln!("│   [{}] {}", task.id, task.cmd);
            }
        }

        let output_dir = options
            .output_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{}/rundata", root.display()));
        eprintln!("├────────────────────────────────────────────────────────┤");
        eprintln!("│ Output: {}", output_dir);
        eprintln!("╰────────────────────────────────────────────────────────╯");
    }

    // Execute tasks
    let (results, summary) = execute_tasks(root, &task_set, &options)?;

    // Convert to ResultSet and render
    let result_set = results_to_result_set(&results, &summary);
    let renderer = Renderer::with_config(render_config);
    println!("{}", renderer.render(&result_set));

    // Print summary to stderr
    if !options.dry_run {
        eprintln!();
        eprintln!("╭─── Execution Summary ─────────────────────────────────╮");
        eprintln!(
            "│ Total: {} │ ✓ Succeeded: {} │ ✗ Failed: {} │ ⊘ Skipped: {}",
            summary.total, summary.succeeded, summary.failed, summary.skipped
        );
        eprintln!("│ Duration: {}ms", summary.total_duration_ms);
        if let Some(dir) = &summary.output_dir {
            eprintln!("│ Output: {}", dir);
        }
        eprintln!("╰────────────────────────────────────────────────────────╯");
    }

    // Exit with error if any task failed and not continuing
    if summary.failed > 0 && !options.continue_on_error {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_task() {
        let json = r#"{"id": "test", "cmd": "echo hello"}"#;
        let task_set = parse_tasks(json).unwrap();
        assert_eq!(task_set.tasks.len(), 1);
        assert_eq!(task_set.tasks[0].id, "test");
        assert_eq!(task_set.tasks[0].cmd, "echo hello");
    }

    #[test]
    fn test_parse_task_array() {
        let json = r#"[
            {"id": "t1", "cmd": "echo 1"},
            {"id": "t2", "cmd": "echo 2"}
        ]"#;
        let task_set = parse_tasks(json).unwrap();
        assert_eq!(task_set.tasks.len(), 2);
    }

    #[test]
    fn test_parse_task_set() {
        let json = r#"{
            "name": "my-tasks",
            "tasks": [
                {"id": "build", "cmd": "cargo build"},
                {"id": "test", "cmd": "cargo test", "depends_on": ["build"]}
            ],
            "groups": [
                {
                    "name": "lint",
                    "parallel": true,
                    "tasks": [
                        {"id": "clippy", "cmd": "cargo clippy"},
                        {"id": "fmt", "cmd": "cargo fmt --check"}
                    ]
                }
            ]
        }"#;
        let task_set = parse_tasks(json).unwrap();
        assert_eq!(task_set.name, "my-tasks");
        assert_eq!(task_set.tasks.len(), 2);
        assert_eq!(task_set.groups.len(), 1);
        assert_eq!(task_set.groups[0].tasks.len(), 2);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("my-task_1"), "my-task_1");
        assert_eq!(sanitize_filename("task/name:invalid"), "task_name_invalid");
        assert_eq!(sanitize_filename("hello world!"), "hello_world_");
    }
}
