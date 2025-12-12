//! Cache store - Read/write .mise/ cache files

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use crate::cache::meta::{CacheMeta, CACHE_VERSION};
use crate::core::model::{ResultItem, ResultSet};
use crate::core::paths::cache_dir;
use crate::core::render::{OutputFormat, Renderer};
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
pub fn run_rebuild(root: &Path, format: OutputFormat) -> Result<()> {
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

    let renderer = Renderer::new(format);
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
}
