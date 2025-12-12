//! Cache metadata management

use serde::{Deserialize, Serialize};

/// Cache metadata stored in .mise/meta.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    /// Cache format version
    pub cache_version: String,

    /// Root directory (absolute path)
    pub root: String,

    /// Hash of the caching policy/configuration
    pub policy_hash: String,

    /// Timestamp when cache was generated (ms since epoch)
    pub generated_at: i64,
}

impl CacheMeta {
    pub fn new(root: &str, policy_hash: &str) -> Self {
        Self {
            cache_version: env!("CARGO_PKG_VERSION").to_string(),
            root: root.to_string(),
            policy_hash: policy_hash.to_string(),
            generated_at: crate::core::util::now_ms(),
        }
    }
}

/// Default cache version
pub const CACHE_VERSION: &str = "0.1";
