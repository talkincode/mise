//! mise - A unified CLI tool for file scanning, anchor management, and code search
//!
//! mise provides:
//! - File scanning with configurable ignore patterns
//! - Anchor-based content management
//! - Integration with ripgrep and ast-grep
//! - Unified output format (jsonl/json/md/raw)

use anyhow::Result;
use clap::Parser;

mod anchors;
mod backends;
mod cache;
mod cli;
mod core;
mod flows;

fn main() -> Result<()> {
    // Check for unsupported platforms
    #[cfg(windows)]
    {
        eprintln!("Error: Windows is not supported. Please use WSL (not guaranteed to work).");
        std::process::exit(1);
    }

    let cli = cli::Cli::parse();
    cli::run(cli)
}
