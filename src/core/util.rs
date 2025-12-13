//! Common utilities

use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::SystemTime;
use xxhash_rust::xxh3::xxh3_64;

/// Hash algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    #[default]
    Xxh3,
    #[allow(dead_code)]
    Sha1,
}

/// Compute hash of file content
#[allow(dead_code)]
pub fn hash_file(path: &Path, algorithm: HashAlgorithm) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;

    Ok(hash_bytes(&buffer, algorithm))
}

/// Compute hash of bytes
pub fn hash_bytes(data: &[u8], algorithm: HashAlgorithm) -> String {
    match algorithm {
        HashAlgorithm::Xxh3 => format!("{:016x}", xxh3_64(data)),
        HashAlgorithm::Sha1 => {
            let mut hasher = Sha1::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        }
    }
}

/// Get file modification time in milliseconds since epoch
pub fn get_mtime_ms(path: &Path) -> std::io::Result<i64> {
    let metadata = std::fs::metadata(path)?;
    let mtime = metadata.modified()?;
    let duration = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_millis() as i64)
}

/// Get file size in bytes
pub fn get_file_size(path: &Path) -> std::io::Result<u64> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len())
}

/// Truncate string to max bytes, returning (truncated_string, was_truncated)
pub fn truncate_string(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_string(), false);
    }

    // Find a valid UTF-8 boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    (s[..end].to_string(), true)
}

/// Check if a command is available in PATH
pub fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get current timestamp in milliseconds
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_bytes() {
        let data = b"hello world";
        let hash = hash_bytes(data, HashAlgorithm::Xxh3);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 16); // 64-bit hex

        let sha1_hash = hash_bytes(data, HashAlgorithm::Sha1);
        assert_eq!(sha1_hash.len(), 40); // 160-bit hex
    }

    #[test]
    fn test_truncate_string() {
        let s = "hello world";
        let (truncated, was_truncated) = truncate_string(s, 5);
        assert_eq!(truncated, "hello");
        assert!(was_truncated);

        let (not_truncated, was_truncated) = truncate_string(s, 100);
        assert_eq!(not_truncated, s);
        assert!(!was_truncated);
    }

    #[test]
    fn test_truncate_string_utf8() {
        let s = "你好世界";
        let (truncated, _) = truncate_string(s, 6);
        assert_eq!(truncated, "你好"); // Each Chinese char is 3 bytes
    }

    #[test]
    fn test_hash_bytes_deterministic() {
        let data = b"test data";
        let hash1 = hash_bytes(data, HashAlgorithm::Xxh3);
        let hash2 = hash_bytes(data, HashAlgorithm::Xxh3);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_bytes_different_data() {
        let data1 = b"hello";
        let data2 = b"world";
        let hash1 = hash_bytes(data1, HashAlgorithm::Xxh3);
        let hash2 = hash_bytes(data2, HashAlgorithm::Xxh3);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_file() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let hash = hash_file(&file_path, HashAlgorithm::Xxh3).unwrap();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_get_mtime_ms() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "test").unwrap();

        let mtime = get_mtime_ms(&file_path).unwrap();
        assert!(mtime > 0);
    }

    #[test]
    fn test_get_file_size() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let size = get_file_size(&file_path).unwrap();
        assert_eq!(size, 11);
    }

    #[test]
    fn test_command_exists() {
        // 'ls' should exist on Unix systems
        assert!(command_exists("ls"));
        // Some random command should not exist
        assert!(!command_exists("nonexistent_command_xyz_123"));
    }

    #[test]
    fn test_now_ms() {
        let now = now_ms();
        assert!(now > 0);
        // Sanity check: should be after year 2020
        assert!(now > 1577836800000); // 2020-01-01 00:00:00 UTC
    }

    #[test]
    fn test_truncate_string_exact_boundary() {
        let s = "hello";
        let (truncated, was_truncated) = truncate_string(s, 5);
        assert_eq!(truncated, "hello");
        assert!(!was_truncated);
    }

    #[test]
    fn test_truncate_string_empty() {
        let s = "";
        let (truncated, was_truncated) = truncate_string(s, 10);
        assert_eq!(truncated, "");
        assert!(!was_truncated);
    }

    #[test]
    fn test_hash_algorithm_default() {
        let algo: HashAlgorithm = Default::default();
        assert_eq!(algo, HashAlgorithm::Xxh3);
    }

    #[test]
    fn test_truncate_string_unicode_boundary() {
        // Each Chinese character is 3 bytes in UTF-8
        // "你好" = 6 bytes (你=3, 好=3)
        // Truncating at 4 bytes would land in the middle of "好"
        // Should find previous valid boundary at 3 bytes
        let s = "你好";
        let (truncated, was_truncated) = truncate_string(s, 4);
        assert_eq!(truncated, "你"); // Only first char fits
        assert!(was_truncated);
    }

    #[test]
    fn test_truncate_string_mid_char() {
        // 🎉 is 4 bytes in UTF-8
        // Truncating at byte 2 should go back to 0
        let s = "🎉test";
        let (truncated, was_truncated) = truncate_string(s, 2);
        assert_eq!(truncated, ""); // Can't fit even one char
        assert!(was_truncated);
    }

    #[test]
    fn test_truncate_string_at_char_boundary() {
        // "abc" are each 1 byte
        let s = "abc";
        let (truncated, was_truncated) = truncate_string(s, 2);
        assert_eq!(truncated, "ab");
        assert!(was_truncated);
    }
}
