//! Watch backend - File watching using watchexec
//!
//! This module is only available with the "watch" feature enabled.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use crate::core::util::command_exists;

/// Check if watchexec is available
pub fn is_watchexec_available() -> bool {
    command_exists("watchexec")
}

/// Run file watching
pub fn run_watch(root: &Path, cmd: Option<&str>) -> Result<()> {
    if !is_watchexec_available() {
        bail!("watchexec is not installed. Please install it: cargo install watchexec-cli");
    }

    let default_cmd = "mise rebuild";
    let watch_cmd = cmd.unwrap_or(default_cmd);

    let mut command = Command::new("watchexec");
    command
        .current_dir(root)
        .arg("--exts")
        .arg("rs,md,txt,py,js,ts,json,yaml,yml,toml")
        .arg("--ignore")
        .arg(".mise/")
        .arg("--ignore")
        .arg("target/")
        .arg("--ignore")
        .arg("node_modules/")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(watch_cmd);

    let status = command.status()?;

    if !status.success() {
        bail!("watchexec exited with error");
    }

    Ok(())
}
