//! Flows module - Complex operations combining multiple sources
//!
//! Provides:
//! - writing: Gather evidence for writing tasks from anchors and search
//! - pack: Bundle anchors and files into a context package
//! - stats: Project statistics (word count, token estimates, etc.)
//! - outline: Generate document outline from anchors

pub mod outline;
pub mod pack;
pub mod stats;
pub mod writing;
