//! Cache module - Manages .mise/ cache directory
//!
//! Provides:
//! - Cache storage (files.jsonl, anchors.jsonl, meta.json)
//! - Cache metadata management
//! - Rebuild functionality

pub mod meta;
pub mod store;
