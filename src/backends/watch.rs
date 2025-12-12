//! Watch backend - File watching using watchexec
//!
//! Provides file system watching capabilities to trigger rebuilds or custom commands
//! when files change. Uses watchexec as the backend.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use crate::core::model::{MiseError, ResultItem, ResultSet};
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::command_exists;

/// Check if watchexec is available
pub fn is_watchexec_available() -> bool {
    command_exists("watchexec")
}

/// Watch options
#[derive(Debug, Clone, Default)]
pub struct WatchOptions {
    /// Command to run on file changes
    pub cmd: Option<String>,
    /// File extensions to watch (comma-separated)
    pub extensions: Option<String>,
    /// Paths to ignore (can be specified multiple times)
    pub ignore: Vec<String>,
    /// Debounce delay in milliseconds
    pub debounce: Option<u64>,
    /// Clear screen before each run
    pub clear: bool,
    /// Restart command if it's still running
    pub restart: bool,
    /// Postpone: wait for first change before running (don't run at startup)
    pub postpone: bool,
    /// Verbose output
    pub verbose: bool,
}

/// Default file extensions to watch
const DEFAULT_EXTENSIONS: &str = "rs,md,txt,py,js,ts,jsx,tsx,json,yaml,yml,toml,html,css,scss";

/// Default paths to ignore
const DEFAULT_IGNORES: &[&str] = &[
    ".mise/",
    ".git/",
    "target/",
    "node_modules/",
    "__pycache__/",
    ".venv/",
    "dist/",
    "build/",
    "*.log",
    "*.tmp",
];

/// Run file watching with options
pub fn run_watch(root: &Path, opts: WatchOptions, config: RenderConfig) -> Result<()> {
    // Check if watchexec is available
    if !is_watchexec_available() {
        let mut result_set = ResultSet::new();
        result_set.push(ResultItem::error(MiseError::new(
            "WATCHEXEC_NOT_FOUND",
            "watchexec is not installed. Install: cargo install watchexec-cli / brew install watchexec",
        )));
        let renderer = Renderer::with_config(config);
        println!("{}", renderer.render(&result_set));
        bail!("watchexec is not installed");
    }

    // Default command is mise rebuild
    let watch_cmd = opts.cmd.as_deref().unwrap_or("mise rebuild");

    // Build watchexec command
    let mut command = Command::new("watchexec");
    command.current_dir(root);

    // Extensions to watch
    let extensions = opts.extensions.as_deref().unwrap_or(DEFAULT_EXTENSIONS);
    command.arg("--exts").arg(extensions);

    // Add default ignores
    for ignore in DEFAULT_IGNORES {
        command.arg("--ignore").arg(ignore);
    }

    // Add user-specified ignores
    for ignore in &opts.ignore {
        command.arg("--ignore").arg(ignore);
    }

    // Debounce delay
    if let Some(debounce) = opts.debounce {
        command.arg("--debounce").arg(format!("{}ms", debounce));
    }

    // Clear screen before each run
    if opts.clear {
        command.arg("--clear");
    }

    // Restart if still running
    if opts.restart {
        command.arg("--restart");
    }

    // Postpone: don't run immediately at startup, wait for first change
    if opts.postpone {
        command.arg("--postpone");
    }

    // Verbose output
    if opts.verbose {
        command.arg("--verbose");
    }

    // The command to run
    command.arg("--").arg("sh").arg("-c").arg(watch_cmd);

    // Print startup message
    eprintln!("👁️  Watching for changes in: {}", root.display());
    eprintln!("📝 Extensions: {}", extensions);
    eprintln!("🚀 Command: {}", watch_cmd);
    if opts.postpone {
        eprintln!("⏳ Waiting for first change...");
    }
    eprintln!("⏹️  Press Ctrl+C to stop\n");

    // Run watchexec
    let status = command.status()?;

    if !status.success() {
        bail!("watchexec exited with error");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_watchexec_available() {
        // Just check that the function runs without panic
        let _ = is_watchexec_available();
    }

    #[test]
    fn test_watch_options_default() {
        let opts = WatchOptions::default();
        assert!(opts.cmd.is_none());
        assert!(opts.extensions.is_none());
        assert!(opts.ignore.is_empty());
        assert!(!opts.clear);
        assert!(!opts.restart);
        assert!(!opts.postpone);
    }

    #[test]
    fn test_default_extensions() {
        assert!(DEFAULT_EXTENSIONS.contains("rs"));
        assert!(DEFAULT_EXTENSIONS.contains("md"));
        assert!(DEFAULT_EXTENSIONS.contains("py"));
        assert!(DEFAULT_EXTENSIONS.contains("js"));
    }

    #[test]
    fn test_default_ignores() {
        assert!(DEFAULT_IGNORES.contains(&".git/"));
        assert!(DEFAULT_IGNORES.contains(&"target/"));
        assert!(DEFAULT_IGNORES.contains(&"node_modules/"));
    }
}
