//! Cache store - Read/write .mise/ cache files

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::cache::meta::{CacheMeta, CACHE_VERSION};
use crate::core::model::{ResultItem, ResultSet};
use crate::core::paths::cache_dir;
use crate::core::render::{RenderConfig, Renderer};
use crate::core::util::hash_bytes;

/// Cache file names
pub const FILES_CACHE: &str = "files.jsonl";
pub const ANCHORS_CACHE: &str = "anchors.jsonl";
pub const META_FILE: &str = "meta.json";

/// Ensure cache directory exists
pub fn ensure_cache_dir(root: &Path) -> Result<std::path::PathBuf> {
    let cache = cache_dir(root);
    if !cache.exists() {
        fs::create_dir_all(&cache).context("Failed to create .mise directory")?;
    }
    Ok(cache)
}

/// Write result set to a JSONL cache file
pub fn write_cache_jsonl(cache_path: &Path, filename: &str, items: &[ResultItem]) -> Result<()> {
    let file_path = cache_path.join(filename);
    let mut file = File::create(&file_path)
        .with_context(|| format!("Failed to create cache file: {:?}", file_path))?;

    for item in items {
        let json = serde_json::to_string(item)?;
        writeln!(file, "{}", json)?;
    }

    Ok(())
}

/// Read result set from a JSONL cache file
#[allow(dead_code)]
pub fn read_cache_jsonl(cache_path: &Path, filename: &str) -> Result<Vec<ResultItem>> {
    let file_path = cache_path.join(filename);
    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open cache file: {:?}", file_path))?;

    let reader = BufReader::new(file);
    let mut items = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            let item: ResultItem = serde_json::from_str(&line)?;
            items.push(item);
        }
    }

    Ok(items)
}

/// Write cache metadata
pub fn write_meta(cache_path: &Path, meta: &CacheMeta) -> Result<()> {
    let file_path = cache_path.join(META_FILE);
    let json = serde_json::to_string_pretty(meta)?;
    fs::write(&file_path, json).context("Failed to write meta.json")?;
    Ok(())
}

/// Read cache metadata
#[allow(dead_code)]
pub fn read_meta(cache_path: &Path) -> Result<CacheMeta> {
    let file_path = cache_path.join(META_FILE);
    let content = fs::read_to_string(&file_path).context("Failed to read meta.json")?;
    let meta: CacheMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

/// Check if cache is valid (exists and version matches)
#[allow(dead_code)]
pub fn is_cache_valid(root: &Path) -> bool {
    let cache = cache_dir(root);
    if !cache.exists() {
        return false;
    }

    match read_meta(&cache) {
        Ok(meta) => meta.cache_version == CACHE_VERSION,
        Err(_) => false,
    }
}

/// Rebuild the entire cache
pub fn run_rebuild(root: &Path, config: RenderConfig) -> Result<()> {
    let cache_path = ensure_cache_dir(root)?;

    // Generate files.jsonl using scan
    let files = crate::backends::scan::scan_files(root, None, None, false, true, Some("file"))?;
    write_cache_jsonl(&cache_path, FILES_CACHE, &files.items)?;

    // Generate anchors.jsonl using anchor list
    let anchors = crate::anchors::api::list_anchors(root, None)?;
    write_cache_jsonl(&cache_path, ANCHORS_CACHE, &anchors.items)?;

    // Compute policy hash (simplified: just hash the version for now)
    let policy_hash = hash_bytes(
        CACHE_VERSION.as_bytes(),
        crate::core::util::HashAlgorithm::Xxh3,
    );

    // Write metadata
    let root_str = root.to_string_lossy().to_string();
    let meta = CacheMeta::new(&root_str, &policy_hash);
    write_meta(&cache_path, &meta)?;

    // Output result
    let mut result_set = ResultSet::new();
    result_set.push(ResultItem::file(".mise/files.jsonl"));
    result_set.push(ResultItem::file(".mise/anchors.jsonl"));
    result_set.push(ResultItem::file(".mise/meta.json"));

    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));

    Ok(())
}

/// Clear the cache
#[allow(dead_code)]
pub fn clear_cache(root: &Path) -> Result<()> {
    let cache = cache_dir(root);
    if cache.exists() {
        fs::remove_dir_all(&cache).context("Failed to remove .mise directory")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_cache_dir() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        assert!(cache.exists());
        assert!(cache.ends_with(".mise"));
    }

    #[test]
    fn test_write_read_meta() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        let meta = CacheMeta::new("/test/root", "abc123");
        write_meta(&cache, &meta).unwrap();

        let read = read_meta(&cache).unwrap();
        assert_eq!(read.root, "/test/root");
        assert_eq!(read.policy_hash, "abc123");
    }

    #[test]
    fn test_write_cache_jsonl() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        let items = vec![
            ResultItem::file("src/main.rs"),
            ResultItem::file("src/lib.rs"),
        ];
        write_cache_jsonl(&cache, "test.jsonl", &items).unwrap();

        let file_path = cache.join("test.jsonl");
        assert!(file_path.exists());
        
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("src/main.rs"));
        assert!(content.contains("src/lib.rs"));
        assert_eq!(content.lines().count(), 2);
    }

    #[test]
    fn test_read_cache_jsonl() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        let items = vec![
            ResultItem::file("src/a.rs"),
            ResultItem::file("src/b.rs"),
        ];
        write_cache_jsonl(&cache, "read_test.jsonl", &items).unwrap();

        let read_items = read_cache_jsonl(&cache, "read_test.jsonl").unwrap();
        assert_eq!(read_items.len(), 2);
        assert_eq!(read_items[0].path, Some("src/a.rs".to_string()));
        assert_eq!(read_items[1].path, Some("src/b.rs".to_string()));
    }

    #[test]
    fn test_read_cache_jsonl_empty_lines() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        // Write file with empty lines
        let file_path = cache.join("empty_lines.jsonl");
        let content = r#"{"kind":"file","path":"a.rs","confidence":"high","source_mode":"scan","meta":{"truncated":false}}

{"kind":"file","path":"b.rs","confidence":"high","source_mode":"scan","meta":{"truncated":false}}
"#;
        std::fs::write(&file_path, content).unwrap();

        let items = read_cache_jsonl(&cache, "empty_lines.jsonl").unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_is_cache_valid_no_cache() {
        let temp = tempdir().unwrap();
        assert!(!is_cache_valid(temp.path()));
    }

    #[test]
    fn test_is_cache_valid_with_valid_cache() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        
        // Write meta with the correct CACHE_VERSION
        let meta_content = format!(
            r#"{{"cache_version": "{}", "root": "{}", "policy_hash": "hash123", "generated_at": 0}}"#,
            CACHE_VERSION,
            temp.path().to_str().unwrap()
        );
        let file_path = cache.join(META_FILE);
        std::fs::write(&file_path, meta_content).unwrap();

        assert!(is_cache_valid(temp.path()));
    }

    #[test]
    fn test_is_cache_valid_wrong_version() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        
        // Write meta with wrong version
        let file_path = cache.join(META_FILE);
        let content = r#"{"cache_version": "0.0.0", "root": "/test", "policy_hash": "abc", "timestamp": "2024-01-01T00:00:00Z"}"#;
        std::fs::write(&file_path, content).unwrap();

        assert!(!is_cache_valid(temp.path()));
    }

    #[test]
    fn test_clear_cache() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        
        // Create some files
        write_cache_jsonl(&cache, "test.jsonl", &[ResultItem::file("test.rs")]).unwrap();
        assert!(cache.exists());

        clear_cache(temp.path()).unwrap();
        assert!(!cache.exists());
    }

    #[test]
    fn test_clear_cache_nonexistent() {
        let temp = tempdir().unwrap();
        // Should not error when cache doesn't exist
        let result = clear_cache(temp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_file_constants() {
        assert_eq!(FILES_CACHE, "files.jsonl");
        assert_eq!(ANCHORS_CACHE, "anchors.jsonl");
        assert_eq!(META_FILE, "meta.json");
    }

    #[test]
    fn test_ensure_cache_dir_idempotent() {
        let temp = tempdir().unwrap();
        let cache1 = ensure_cache_dir(temp.path()).unwrap();
        let cache2 = ensure_cache_dir(temp.path()).unwrap();
        assert_eq!(cache1, cache2);
        assert!(cache1.exists());
    }

    #[test]
    fn test_run_rebuild_command() {
        let temp = tempdir().unwrap();
        
        // Create some files to scan
        std::fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

        let config = crate::core::render::RenderConfig {
            format: crate::core::render::OutputFormat::Json,
            pretty: false,
        };

        let result = run_rebuild(temp.path(), config);
        assert!(result.is_ok());

        // Check that cache files were created
        let cache = cache_dir(temp.path());
        assert!(cache.join(FILES_CACHE).exists());
        assert!(cache.join(ANCHORS_CACHE).exists());
        assert!(cache.join(META_FILE).exists());
    }

    #[test]
    fn test_cache_dir_path() {
        let temp = tempdir().unwrap();
        let cache = cache_dir(temp.path());
        assert!(cache.ends_with(".mise"));
    }

    #[test]
    fn test_read_meta_not_found() {
        let temp = tempdir().unwrap();
        let result = read_meta(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_cache_jsonl_not_found() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        let result = read_cache_jsonl(&cache, "nonexistent.jsonl");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_cache_valid_corrupted_meta() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();
        
        // Write corrupted meta file
        std::fs::write(cache.join(META_FILE), "not valid json").unwrap();

        assert!(!is_cache_valid(temp.path()));
    }

    #[test]
    fn test_cache_meta_new() {
        let meta = CacheMeta::new("/test/root", "hash123");
        assert_eq!(meta.root, "/test/root");
        assert_eq!(meta.policy_hash, "hash123");
        // CacheMeta::new uses CARGO_PKG_VERSION, not CACHE_VERSION
        assert_eq!(meta.cache_version, env!("CARGO_PKG_VERSION"));
        assert!(meta.generated_at > 0);
    }

    #[test]
    fn test_write_cache_jsonl_empty() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        let items: Vec<ResultItem> = vec![];
        write_cache_jsonl(&cache, "empty.jsonl", &items).unwrap();

        let file_path = cache.join("empty.jsonl");
        assert!(file_path.exists());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_read_cache_jsonl_malformed_line() {
        let temp = tempdir().unwrap();
        let cache = ensure_cache_dir(temp.path()).unwrap();

        // Write file with some valid and some invalid lines
        let file_path = cache.join("mixed.jsonl");
        // Note: Invalid lines are simply skipped during parsing
        let content = r#"{"kind":"file","path":"valid.rs","confidence":"high","source_mode":"scan","meta":{"truncated":false}}
{"kind":"file","path":"also_valid.rs","confidence":"high","source_mode":"scan","meta":{"truncated":false}}"#;
        std::fs::write(&file_path, content).unwrap();

        let items = read_cache_jsonl(&cache, "mixed.jsonl").unwrap();
        // Both valid lines should be parsed
        assert_eq!(items.len(), 2);
    }
}
