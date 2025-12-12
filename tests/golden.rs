//! Golden tests for mise
//!
//! These tests verify that command outputs match expected golden files.

use std::path::PathBuf;

/// Get the path to the fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_exist() {
        let dir = fixtures_dir();
        // This test will pass even if fixtures don't exist yet
        // It's here to help verify the test setup
        if dir.exists() {
            assert!(dir.is_dir());
        }
    }
}
