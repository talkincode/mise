//! Anchors module - Manage anchor points in documents
//!
//! Anchors are markers in documents that allow referencing specific sections.
//! Format: <!--Q:begin id=xxx tags=a,b v=1--> ... <!--Q:end id=xxx-->

pub mod api;
pub mod lint;
pub mod parse;
