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
}
