//! Backends module - External tool integrations and file operations
//!
//! Provides:
//! - scan: File scanning with walkdir
//! - extract: Ranged file reading
//! - rg: ripgrep integration
//! - ast_grep: ast-grep integration
//! - deps: Dependency graph analysis
//! - doctor: Dependency checking
//! - run: Concurrent command execution
//! - watch: File watching (optional)

pub mod ast_grep;
pub mod deps;
pub mod doctor;
pub mod extract;
pub mod impact;
pub mod rg;
pub mod run;
pub mod scan;

#[cfg(feature = "watch")]
pub mod watch;
