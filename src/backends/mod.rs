//! Backends module - External tool integrations and file operations
//!
//! Provides:
//! - scan: File scanning with walkdir
//! - extract: Ranged file reading
//! - rg: ripgrep integration
//! - ast_grep: ast-grep integration
//! - doctor: Dependency checking
//! - watch: File watching (optional)

pub mod ast_grep;
pub mod doctor;
pub mod extract;
pub mod rg;
pub mod scan;

#[cfg(feature = "watch")]
pub mod watch;
