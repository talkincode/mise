//! Flows module - Complex operations combining multiple sources
//!
//! Provides:
//! - writing: Gather evidence for writing tasks from anchors and search
//! - pack: Bundle anchors and files into a context package
//! - stats: Project statistics (word count, token estimates, etc.)

pub mod pack;
pub mod stats;
pub mod writing;
