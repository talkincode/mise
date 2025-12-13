//! Token counting module - Unified token estimation for LLM context budgeting
//!
//! Provides accurate token counting using tiktoken (cl100k_base by default),
//! with support for multiple models and a fast heuristic fallback.
//!
//! Supported models:
//! - gpt-4, gpt-4o, gpt-4-turbo, gpt-3.5-turbo (cl100k_base)
//! - claude-3, claude-3.5 (uses cl100k_base as approximation)
//! - o200k_base (gpt-4o native encoding)
//!
//! Usage:
//! ```rust
//! use mise::core::tokenizer::{count_tokens, TokenModel};
//!
//! // Default model (cl100k_base, good for GPT-4/Claude)
//! let tokens = count_tokens("Hello world", TokenModel::default());
//!
//! // Specific model
//! let tokens = count_tokens("你好世界", TokenModel::Gpt4);
//!
//! // Fast heuristic (no external encoding)
//! let tokens = count_tokens("mixed 混合 content", TokenModel::Heuristic);
//! ```

use once_cell::sync::Lazy;
use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;
use tiktoken_rs::{cl100k_base, o200k_base, CoreBPE};

/// Supported token models/encodings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenModel {
    /// cl100k_base encoding (GPT-4, GPT-3.5-turbo, Claude 3)
    #[default]
    Cl100k,
    /// o200k_base encoding (GPT-4o native)
    O200k,
    /// GPT-4 / GPT-4-turbo (alias for Cl100k)
    Gpt4,
    /// GPT-4o (alias for O200k)
    Gpt4o,
    /// GPT-3.5-turbo (alias for Cl100k)
    Gpt35Turbo,
    /// Claude 3 / 3.5 (approximated with Cl100k)
    Claude3,
    /// Fast heuristic estimation (no BPE encoding)
    Heuristic,
}

impl TokenModel {
    /// Get the underlying BPE encoding for this model
    fn get_bpe(&self) -> Option<&'static CoreBPE> {
        match self {
            TokenModel::O200k | TokenModel::Gpt4o => O200K_BPE.as_ref().ok(),
            TokenModel::Cl100k
            | TokenModel::Gpt4
            | TokenModel::Gpt35Turbo
            | TokenModel::Claude3 => CL100K_BPE.as_ref().ok(),
            TokenModel::Heuristic => None,
        }
    }

    /// List all available models
    pub fn available_models() -> &'static [&'static str] {
        &[
            "cl100k",
            "o200k",
            "gpt4",
            "gpt4o",
            "gpt35",
            "claude3",
            "heuristic",
        ]
    }
}

impl fmt::Display for TokenModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TokenModel::Cl100k => "cl100k",
            TokenModel::O200k => "o200k",
            TokenModel::Gpt4 => "gpt4",
            TokenModel::Gpt4o => "gpt4o",
            TokenModel::Gpt35Turbo => "gpt35",
            TokenModel::Claude3 => "claude3",
            TokenModel::Heuristic => "heuristic",
        };
        write!(f, "{}", name)
    }
}

impl FromStr for TokenModel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cl100k" | "cl100k_base" | "default" => Ok(TokenModel::Cl100k),
            "o200k" | "o200k_base" => Ok(TokenModel::O200k),
            "gpt4" | "gpt-4" | "gpt-4-turbo" => Ok(TokenModel::Gpt4),
            "gpt4o" | "gpt-4o" => Ok(TokenModel::Gpt4o),
            "gpt35" | "gpt-3.5" | "gpt-3.5-turbo" => Ok(TokenModel::Gpt35Turbo),
            "claude" | "claude3" | "claude-3" | "claude-3.5" => Ok(TokenModel::Claude3),
            "heuristic" | "fast" | "estimate" => Ok(TokenModel::Heuristic),
            _ => Err(format!(
                "Unknown model: {}. Available: {}",
                s,
                TokenModel::available_models().join(", ")
            )),
        }
    }
}

// Lazy-initialized BPE encodings (loaded once on first use)
static CL100K_BPE: Lazy<Result<CoreBPE, String>> =
    Lazy::new(|| cl100k_base().map_err(|e| format!("Failed to load cl100k_base: {}", e)));

static O200K_BPE: Lazy<Result<CoreBPE, String>> =
    Lazy::new(|| o200k_base().map_err(|e| format!("Failed to load o200k_base: {}", e)));

// Cache for model availability status
static CL100K_STATUS: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static O200K_STATUS: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));

/// Check if a tiktoken model is available (downloaded/cached)
///
/// Returns (available, error_message)
pub fn check_tiktoken_model(model: TokenModel) -> (bool, Option<String>) {
    match model {
        TokenModel::Heuristic => (true, None),
        TokenModel::O200k | TokenModel::Gpt4o => {
            // Check cached status first
            let mut status = O200K_STATUS.lock().expect("O200K_STATUS mutex poisoned");
            if let Some(available) = *status {
                return (
                    available,
                    if available {
                        None
                    } else {
                        Some("o200k_base not loaded".to_string())
                    },
                );
            }
            // Try to load
            match &*O200K_BPE {
                Ok(_) => {
                    *status = Some(true);
                    (true, None)
                }
                Err(e) => {
                    *status = Some(false);
                    (false, Some(e.clone()))
                }
            }
        }
        _ => {
            // cl100k variants
            let mut status = CL100K_STATUS.lock().expect("CL100K_STATUS mutex poisoned");
            if let Some(available) = *status {
                return (
                    available,
                    if available {
                        None
                    } else {
                        Some("cl100k_base not loaded".to_string())
                    },
                );
            }
            match &*CL100K_BPE {
                Ok(_) => {
                    *status = Some(true);
                    (true, None)
                }
                Err(e) => {
                    *status = Some(false);
                    (false, Some(e.clone()))
                }
            }
        }
    }
}

/// Get status of all tiktoken models
pub fn check_all_tiktoken_models() -> Vec<(String, bool, Option<String>)> {
    vec![
        {
            let (ok, err) = check_tiktoken_model(TokenModel::Cl100k);
            ("cl100k_base (GPT-4/Claude)".to_string(), ok, err)
        },
        {
            let (ok, err) = check_tiktoken_model(TokenModel::O200k);
            ("o200k_base (GPT-4o)".to_string(), ok, err)
        },
    ]
}

/// Count tokens in text using the specified model
///
/// # Arguments
/// * `text` - The text to tokenize
/// * `model` - The token model/encoding to use
///
/// # Returns
/// The number of tokens in the text
pub fn count_tokens(text: &str, model: TokenModel) -> usize {
    if text.is_empty() {
        return 0;
    }

    match model.get_bpe() {
        Some(bpe) => bpe.encode_with_special_tokens(text).len(),
        None => estimate_tokens_heuristic(text),
    }
}

/// Estimate tokens using a fast heuristic (no BPE encoding)
///
/// This is useful when:
/// - Performance is critical and approximate counts are acceptable
/// - The BPE encoding is not available
///
/// The heuristic accounts for:
/// - ASCII text: ~4 characters per token
/// - Code symbols: ~2 characters per token
/// - CJK characters: ~1.5 characters per token
/// - Other Unicode: ~2 characters per token
pub fn estimate_tokens_heuristic(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let mut ascii_chars = 0usize;
    let mut cjk_chars = 0usize;
    let mut other_unicode = 0usize;
    let mut whitespace = 0usize;
    let mut code_symbols = 0usize;

    for c in text.chars() {
        if c.is_ascii_whitespace() {
            whitespace += 1;
        } else if c.is_ascii() {
            if is_code_symbol(c) {
                code_symbols += 1;
            } else {
                ascii_chars += 1;
            }
        } else if is_cjk_char(c) {
            cjk_chars += 1;
        } else {
            other_unicode += 1;
        }
    }

    // Token estimation rules (based on GPT/Claude tokenizer behavior):
    // - ASCII words: ~4 chars/token (including spaces between words)
    // - Code symbols: ~1-2 chars/token (operators often split)
    // - CJK characters: ~1.5-2 chars/token (often 1-2 chars per token)
    // - Other unicode: ~2-3 chars/token

    let ascii_tokens = (ascii_chars + whitespace).div_ceil(4);
    let symbol_tokens = code_symbols.div_ceil(2);
    let cjk_tokens = (cjk_chars * 2).div_ceil(3); // ~1.5 chars per token
    let other_tokens = other_unicode.div_ceil(2);

    ascii_tokens + symbol_tokens + cjk_tokens + other_tokens
}

/// Check if a character is a common code symbol/operator
#[inline]
fn is_code_symbol(c: char) -> bool {
    matches!(
        c,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '&'
            | '|'
            | '^'
            | '!'
            | '~'
            | '?'
            | ':'
            | ';'
            | ','
            | '.'
            | '@'
            | '#'
            | '$'
            | '\\'
            | '"'
            | '\''
            | '`'
    )
}

/// Check if a character is CJK (Chinese/Japanese/Korean)
#[inline]
fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    // CJK Unified Ideographs and common ranges
    (0x4E00..=0x9FFF).contains(&cp)      // CJK Unified Ideographs
        || (0x3400..=0x4DBF).contains(&cp)  // CJK Extension A
        || (0x3000..=0x303F).contains(&cp)  // CJK Symbols and Punctuation
        || (0x3040..=0x309F).contains(&cp)  // Hiragana
        || (0x30A0..=0x30FF).contains(&cp)  // Katakana
        || (0xAC00..=0xD7AF).contains(&cp)  // Hangul Syllables
        || (0xFF00..=0xFFEF).contains(&cp) // Fullwidth Forms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_empty() {
        assert_eq!(count_tokens("", TokenModel::default()), 0);
        assert_eq!(count_tokens("", TokenModel::Heuristic), 0);
    }

    #[test]
    fn test_count_tokens_ascii() {
        let text = "Hello, world!";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        // tiktoken should give accurate count
        assert!(tokens > 0 && tokens < 10);
    }

    #[test]
    fn test_count_tokens_cjk() {
        let text = "你好世界";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        // CJK typically has more tokens per character
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_tokens_mixed() {
        let text = "Hello 你好 World 世界";
        let tokens = count_tokens(text, TokenModel::Cl100k);
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_tokens_code() {
        let text = r#"fn main() { println!("Hello"); }"#;
        let tokens = count_tokens(text, TokenModel::Cl100k);
        assert!(tokens > 0);
    }

    #[test]
    fn test_heuristic_ascii() {
        let text = "Hello world, this is a test.";
        let tokens = estimate_tokens_heuristic(text);
        // ~28 chars / 4 ≈ 7 tokens
        assert!((5..=12).contains(&tokens));
    }

    #[test]
    fn test_heuristic_cjk() {
        let text = "这是一个测试文档";
        let tokens = estimate_tokens_heuristic(text);
        // 8 CJK chars * 2 / 3 ≈ 5-6 tokens
        assert!((4..=8).contains(&tokens));
    }

    #[test]
    fn test_heuristic_code() {
        let text = "fn main() { println!(); }";
        let tokens = estimate_tokens_heuristic(text);
        assert!(tokens > 5);
    }

    #[test]
    fn test_model_from_str() {
        assert_eq!("cl100k".parse::<TokenModel>().unwrap(), TokenModel::Cl100k);
        assert_eq!("gpt4".parse::<TokenModel>().unwrap(), TokenModel::Gpt4);
        assert_eq!("gpt-4o".parse::<TokenModel>().unwrap(), TokenModel::Gpt4o);
        assert_eq!(
            "claude3".parse::<TokenModel>().unwrap(),
            TokenModel::Claude3
        );
        assert_eq!(
            "heuristic".parse::<TokenModel>().unwrap(),
            TokenModel::Heuristic
        );
        assert!("unknown".parse::<TokenModel>().is_err());
    }

    #[test]
    fn test_model_display() {
        assert_eq!(TokenModel::Cl100k.to_string(), "cl100k");
        assert_eq!(TokenModel::Gpt4o.to_string(), "gpt4o");
    }

    #[test]
    fn test_different_models_produce_results() {
        let text = "Hello world, 你好世界!";

        let cl100k = count_tokens(text, TokenModel::Cl100k);
        let o200k = count_tokens(text, TokenModel::O200k);
        let heuristic = count_tokens(text, TokenModel::Heuristic);

        // All should produce non-zero results
        assert!(cl100k > 0);
        assert!(o200k > 0);
        assert!(heuristic > 0);

        // cl100k and o200k may differ slightly
        // heuristic is an approximation, so it can be different
    }

    #[test]
    fn test_is_cjk_char() {
        assert!(is_cjk_char('中'));
        assert!(is_cjk_char('日'));
        assert!(is_cjk_char('あ')); // Hiragana
        assert!(is_cjk_char('ア')); // Katakana
        assert!(is_cjk_char('한')); // Hangul
        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char('1'));
    }

    #[test]
    fn test_is_code_symbol() {
        assert!(is_code_symbol('('));
        assert!(is_code_symbol(')'));
        assert!(is_code_symbol('{'));
        assert!(is_code_symbol('='));
        assert!(!is_code_symbol('a'));
        assert!(!is_code_symbol('中'));
    }

    #[test]
    fn test_heuristic_vs_tiktoken_approximation() {
        // The heuristic should be within a reasonable range of tiktoken
        let texts = [
            "Hello, world!",
            "This is a longer piece of English text for testing.",
            "fn main() { println!(\"test\"); }",
            "这是中文测试",
            "Mixed 混合 content テスト",
        ];

        for text in texts {
            let tiktoken_count = count_tokens(text, TokenModel::Cl100k);
            let heuristic_count = estimate_tokens_heuristic(text);

            // Heuristic should be within 50% of tiktoken for most cases
            let ratio = if tiktoken_count > 0 {
                heuristic_count as f64 / tiktoken_count as f64
            } else {
                1.0
            };
            assert!(
                (0.5..=2.0).contains(&ratio),
                "Heuristic too far from tiktoken for '{}': {} vs {}",
                text,
                heuristic_count,
                tiktoken_count
            );
        }
    }
}
