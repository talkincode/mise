//! Unified file reading strategies
//!
//! Provides consistent handling for:
//! - Non-UTF-8 files
//! - Oversized files
//! - Binary files

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;

use crate::core::model::{MiseError, ResultItem};

/// Default maximum file size in bytes (64 MB)
pub const DEFAULT_MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

/// Default truncation size in bytes (64 KB)
pub const DEFAULT_TRUNCATE_SIZE: usize = 64 * 1024;

/// Strategy for handling non-UTF-8 content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodingStrategy {
    /// Skip non-UTF-8 files entirely
    Skip,
    /// Use lossy conversion (replace invalid bytes with ?)
    #[default]
    Lossy,
    /// Treat as binary (skip content extraction)
    Binary,
}

/// Strategy for handling oversized files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeStrategy {
    /// Skip files exceeding size limit
    Skip,
    /// Truncate content and mark as truncated
    #[default]
    Truncate,
    /// Read entire file regardless of size
    Full,
}

/// Configuration for file reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadConfig {
    /// Maximum file size to process (bytes)
    pub max_file_size: u64,

    /// Size at which to truncate content (bytes)
    pub truncate_size: usize,

    /// How to handle non-UTF-8 content
    pub encoding_strategy: EncodingStrategy,

    /// How to handle oversized files
    pub size_strategy: SizeStrategy,
}

impl Default for FileReadConfig {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            truncate_size: DEFAULT_TRUNCATE_SIZE,
            encoding_strategy: EncodingStrategy::Lossy,
            size_strategy: SizeStrategy::Truncate,
        }
    }
}

/// Result of reading a file
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileReadResult {
    /// The file content (if successfully read)
    pub content: Option<String>,

    /// Whether the content was truncated
    pub truncated: bool,

    /// Whether lossy conversion was used
    pub lossy_conversion: bool,

    /// Warnings generated during reading
    pub warnings: Vec<FileWarning>,

    /// Whether the file was skipped
    pub skipped: bool,

    /// Reason for skipping (if skipped)
    pub skip_reason: Option<String>,
}

impl FileReadResult {
    /// Create a successful read result
    pub fn success(content: String) -> Self {
        Self {
            content: Some(content),
            truncated: false,
            lossy_conversion: false,
            warnings: Vec::new(),
            skipped: false,
            skip_reason: None,
        }
    }

    /// Create a skipped result
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            content: None,
            truncated: false,
            lossy_conversion: false,
            warnings: Vec::new(),
            skipped: true,
            skip_reason: Some(reason.into()),
        }
    }

    /// Mark as truncated
    pub fn with_truncated(mut self) -> Self {
        self.truncated = true;
        self
    }

    /// Mark as lossy conversion
    pub fn with_lossy(mut self) -> Self {
        self.lossy_conversion = true;
        self
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: FileWarning) -> Self {
        self.warnings.push(warning);
        self
    }
}

/// Warning codes for file operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningCode {
    /// File was truncated due to size
    FileTruncated,
    /// File was skipped due to size
    FileSkippedSize,
    /// File was skipped due to encoding
    FileSkippedEncoding,
    /// Lossy encoding conversion used
    LossyConversion,
    /// File appears to be binary
    BinaryFile,
    /// Circular dependency detected
    CircularDependency,
    /// Raw format warning
    RawFormatUnstable,
    /// General warning
    General,
}

impl WarningCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WarningCode::FileTruncated => "FILE_TRUNCATED",
            WarningCode::FileSkippedSize => "FILE_SKIPPED_SIZE",
            WarningCode::FileSkippedEncoding => "FILE_SKIPPED_ENCODING",
            WarningCode::LossyConversion => "LOSSY_CONVERSION",
            WarningCode::BinaryFile => "BINARY_FILE",
            WarningCode::CircularDependency => "CIRCULAR_DEPENDENCY",
            WarningCode::RawFormatUnstable => "RAW_FORMAT_UNSTABLE",
            WarningCode::General => "WARNING",
        }
    }
}

/// A structured warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWarning {
    /// Warning code
    pub code: WarningCode,

    /// Warning message
    pub message: String,

    /// Associated file path (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl FileWarning {
    /// Create a new warning
    pub fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            details: None,
        }
    }

    /// Set the path
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set additional details
    #[allow(dead_code)]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Convert to a MiseError for embedding in ResultItem
    #[allow(dead_code)]
    pub fn to_mise_error(&self) -> MiseError {
        MiseError::new(self.code.as_str(), &self.message)
    }

    /// Convert to a ResultItem (Kind::Error with warning info)
    #[allow(dead_code)]
    pub fn to_result_item(&self) -> ResultItem {
        let mut item = ResultItem::error(self.to_mise_error());
        item.path = self.path.clone();
        item.excerpt = self.details.clone();
        item
    }
}

/// Read a file with the given configuration
pub fn read_file_with_config(path: &Path, config: &FileReadConfig) -> FileReadResult {
    // Check file size first
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return FileReadResult::skipped(format!("Cannot read metadata: {}", e));
        }
    };

    let file_size = metadata.len();

    // Check size limit
    match config.size_strategy {
        SizeStrategy::Skip if file_size > config.max_file_size => {
            let warning = FileWarning::new(
                WarningCode::FileSkippedSize,
                format!(
                    "File exceeds size limit ({} > {} bytes)",
                    file_size, config.max_file_size
                ),
            )
            .with_path(path.display().to_string());
            return FileReadResult::skipped(format!(
                "File size {} exceeds limit {}",
                file_size, config.max_file_size
            ))
            .with_warning(warning);
        }
        _ => {}
    }

    // Read file content
    let bytes = match read_file_bytes(path, config) {
        Ok(b) => b,
        Err(e) => {
            return FileReadResult::skipped(format!("Cannot read file: {}", e));
        }
    };

    // Check if binary (contains null bytes in first 8KB)
    let check_len = std::cmp::min(8192, bytes.len());
    if bytes[..check_len].contains(&0) {
        let warning = FileWarning::new(
            WarningCode::BinaryFile,
            "File appears to be binary (contains null bytes)",
        )
        .with_path(path.display().to_string());

        match config.encoding_strategy {
            EncodingStrategy::Skip | EncodingStrategy::Binary => {
                return FileReadResult::skipped("Binary file").with_warning(warning);
            }
            EncodingStrategy::Lossy => {
                // Continue with lossy conversion
            }
        }
    }

    // Try to convert to UTF-8
    match String::from_utf8(bytes.clone()) {
        Ok(content) => {
            let truncated = content.len() > config.truncate_size
                && config.size_strategy == SizeStrategy::Truncate;

            if truncated {
                let truncated_content = truncate_at_char_boundary(&content, config.truncate_size);
                let warning = FileWarning::new(
                    WarningCode::FileTruncated,
                    format!(
                        "Content truncated from {} to {} bytes",
                        content.len(),
                        truncated_content.len()
                    ),
                )
                .with_path(path.display().to_string());

                FileReadResult::success(truncated_content)
                    .with_truncated()
                    .with_warning(warning)
            } else {
                FileReadResult::success(content)
            }
        }
        Err(_) => {
            // Handle non-UTF-8 based on strategy
            match config.encoding_strategy {
                EncodingStrategy::Skip => {
                    let warning = FileWarning::new(
                        WarningCode::FileSkippedEncoding,
                        "File contains invalid UTF-8 sequences",
                    )
                    .with_path(path.display().to_string());
                    FileReadResult::skipped("Invalid UTF-8").with_warning(warning)
                }
                EncodingStrategy::Lossy | EncodingStrategy::Binary => {
                    let content = String::from_utf8_lossy(&bytes).into_owned();
                    let warning = FileWarning::new(
                        WarningCode::LossyConversion,
                        "Lossy UTF-8 conversion applied (some characters replaced)",
                    )
                    .with_path(path.display().to_string());

                    let truncated = content.len() > config.truncate_size
                        && config.size_strategy == SizeStrategy::Truncate;

                    if truncated {
                        let truncated_content =
                            truncate_at_char_boundary(&content, config.truncate_size);
                        FileReadResult::success(truncated_content)
                            .with_truncated()
                            .with_lossy()
                            .with_warning(warning)
                    } else {
                        FileReadResult::success(content)
                            .with_lossy()
                            .with_warning(warning)
                    }
                }
            }
        }
    }
}

/// Read file bytes with size handling
fn read_file_bytes(path: &Path, config: &FileReadConfig) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len() as usize;

    let read_size = match config.size_strategy {
        SizeStrategy::Truncate => std::cmp::min(file_size, config.truncate_size + 1024),
        SizeStrategy::Skip => file_size,
        SizeStrategy::Full => file_size,
    };

    let mut reader = std::io::BufReader::new(file);
    let mut buffer = Vec::with_capacity(read_size);

    if read_size < file_size {
        reader.take(read_size as u64).read_to_end(&mut buffer)?;
    } else {
        reader.read_to_end(&mut buffer)?;
    }

    Ok(buffer)
}

/// Truncate string at a valid UTF-8 character boundary
fn truncate_at_char_boundary(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    // Find the last valid character boundary before max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    s[..end].to_string()
}

/// Convenience function with default config
pub fn read_file_safe(path: &Path) -> FileReadResult {
    read_file_with_config(path, &FileReadConfig::default())
}

/// Read file as string, returning None if skipped/failed
#[allow(dead_code)]
pub fn read_file_string(path: &Path) -> Option<String> {
    let result = read_file_safe(path);
    result.content
}

/// Read file as string with lossy conversion
#[allow(dead_code)]
pub fn read_file_lossy(path: &Path) -> Option<String> {
    let config = FileReadConfig {
        encoding_strategy: EncodingStrategy::Lossy,
        size_strategy: SizeStrategy::Full,
        ..Default::default()
    };
    let result = read_file_with_config(path, &config);
    result.content
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_file_read_config_default() {
        let config = FileReadConfig::default();
        assert_eq!(config.max_file_size, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(config.truncate_size, DEFAULT_TRUNCATE_SIZE);
        assert_eq!(config.encoding_strategy, EncodingStrategy::Lossy);
        assert_eq!(config.size_strategy, SizeStrategy::Truncate);
    }

    #[test]
    fn test_read_file_success() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let result = read_file_safe(&file_path);
        assert!(!result.skipped);
        assert_eq!(result.content, Some("Hello, World!".to_string()));
        assert!(!result.truncated);
        assert!(!result.lossy_conversion);
    }

    #[test]
    fn test_read_file_truncated() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("large.txt");

        // Create content larger than default truncate size
        let content = "x".repeat(DEFAULT_TRUNCATE_SIZE + 1000);
        fs::write(&file_path, &content).unwrap();

        let result = read_file_safe(&file_path);
        assert!(!result.skipped);
        assert!(result.truncated);
        assert!(result.content.as_ref().unwrap().len() <= DEFAULT_TRUNCATE_SIZE);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, WarningCode::FileTruncated);
    }

    #[test]
    fn test_read_file_skip_size() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello").unwrap();

        let config = FileReadConfig {
            max_file_size: 1, // Very small limit
            size_strategy: SizeStrategy::Skip,
            ..Default::default()
        };

        let result = read_file_with_config(&file_path, &config);
        assert!(result.skipped);
        assert!(result.skip_reason.is_some());
    }

    #[test]
    fn test_read_file_binary() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("binary.bin");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(&[0x00, 0x01, 0x02, 0x00, 0x03]).unwrap();

        let config = FileReadConfig {
            encoding_strategy: EncodingStrategy::Skip,
            ..Default::default()
        };

        let result = read_file_with_config(&file_path, &config);
        assert!(result.skipped);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, WarningCode::BinaryFile);
    }

    #[test]
    fn test_read_file_lossy_conversion() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("invalid_utf8.txt");

        // Write invalid UTF-8 sequence
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(&[0xFF, 0xFE, 0x48, 0x65, 0x6C, 0x6C, 0x6F])
            .unwrap();

        let config = FileReadConfig {
            encoding_strategy: EncodingStrategy::Lossy,
            ..Default::default()
        };

        let result = read_file_with_config(&file_path, &config);
        assert!(!result.skipped);
        assert!(result.lossy_conversion);
        assert!(result.content.is_some());
    }

    #[test]
    fn test_read_file_skip_encoding() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("invalid_utf8.txt");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(&[0xFF, 0xFE, 0x48, 0x65, 0x6C, 0x6C, 0x6F])
            .unwrap();

        let config = FileReadConfig {
            encoding_strategy: EncodingStrategy::Skip,
            ..Default::default()
        };

        let result = read_file_with_config(&file_path, &config);
        assert!(result.skipped);
    }

    #[test]
    fn test_truncate_at_char_boundary() {
        // ASCII string
        assert_eq!(truncate_at_char_boundary("Hello", 3), "Hel");

        // UTF-8 string with multi-byte chars
        let s = "你好世界"; // Each Chinese char is 3 bytes
        assert_eq!(truncate_at_char_boundary(s, 3), "你");
        assert_eq!(truncate_at_char_boundary(s, 6), "你好");

        // Truncate in middle of multi-byte char should back up
        assert_eq!(truncate_at_char_boundary(s, 4), "你");
        assert_eq!(truncate_at_char_boundary(s, 5), "你");
    }

    #[test]
    fn test_warning_code_as_str() {
        assert_eq!(WarningCode::FileTruncated.as_str(), "FILE_TRUNCATED");
        assert_eq!(
            WarningCode::CircularDependency.as_str(),
            "CIRCULAR_DEPENDENCY"
        );
    }

    #[test]
    fn test_file_warning_to_result_item() {
        let warning = FileWarning::new(WarningCode::FileTruncated, "Test message")
            .with_path("test/file.rs")
            .with_details("Additional info");

        let item = warning.to_result_item();
        assert_eq!(item.path, Some("test/file.rs".to_string()));
        assert_eq!(item.excerpt, Some("Additional info".to_string()));
        assert!(!item.errors.is_empty());
        assert_eq!(item.errors[0].code, "FILE_TRUNCATED");
    }

    #[test]
    fn test_file_read_result_builders() {
        let result = FileReadResult::success("content".to_string())
            .with_truncated()
            .with_lossy()
            .with_warning(FileWarning::new(WarningCode::General, "test"));

        assert!(result.truncated);
        assert!(result.lossy_conversion);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_encoding_strategy_default() {
        assert_eq!(EncodingStrategy::default(), EncodingStrategy::Lossy);
    }

    #[test]
    fn test_size_strategy_default() {
        assert_eq!(SizeStrategy::default(), SizeStrategy::Truncate);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = read_file_safe(Path::new("/nonexistent/file.txt"));
        assert!(result.skipped);
        assert!(result.skip_reason.is_some());
    }
}
