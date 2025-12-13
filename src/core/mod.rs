//! Core module - Contains the fundamental data structures and utilities
//!
//! This module provides:
//! - Unified result model (ResultItem)
//! - Rendering functions for different output formats
//! - Path normalization utilities
//! - Common utilities
//! - File reading strategies
//! - Token counting for LLM context budgeting

pub mod file_reader;
pub mod model;
pub mod paths;
pub mod render;
pub mod tokenizer;
pub mod util;
