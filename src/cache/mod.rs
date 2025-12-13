//! Cache module - Manages .mise/ cache directory
//!
//! Provides:
//! - Cache storage (files.jsonl, anchors.jsonl, meta.json)
//! - Cache metadata management
//! - Rebuild functionality
//! - Smart cache reader with fallback

pub mod meta;
pub mod reader;
pub mod store;
