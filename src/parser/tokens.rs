//! @dose
//! purpose: Token counting utility using tiktoken for accurate LLM token estimation.
//!
//! when-editing:
//!     - !The tokenizer is lazily initialized and cached for performance
//!     - Uses cl100k_base encoding (GPT-4/ChatGPT) as a reasonable approximation for Claude
//!
//! invariants:
//!     - count_tokens always returns a valid count (falls back to heuristic on error)
//!     - The tokenizer singleton is thread-safe
//!
//! gotchas:
//!     - Different LLMs use different tokenizers; cl100k_base is an approximation
//!     - First call has initialization overhead; subsequent calls are fast

use once_cell::sync::Lazy;
use tiktoken_rs::CoreBPE;

/// Cached tokenizer instance for performance
static TOKENIZER: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::cl100k_base().ok());

/// Count tokens in the given text using tiktoken's cl100k_base encoding.
///
/// This provides accurate token counts compatible with GPT-4/ChatGPT models,
/// which is a reasonable approximation for Claude and other modern LLMs.
///
/// Falls back to a simple heuristic (len/4) if tokenizer initialization fails.
pub fn count_tokens(text: &str) -> usize {
    match TOKENIZER.as_ref() {
        Some(tokenizer) => tokenizer.encode_ordinary(text).len(),
        None => {
            // Fallback to simple heuristic if tokenizer fails to initialize
            text.len() / 4
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_basic() {
        // Simple text should have reasonable token count
        let tokens = count_tokens("Hello, world!");
        assert!(tokens > 0);
        assert!(tokens < 10); // Should be ~4 tokens
    }

    #[test]
    fn test_count_tokens_code() {
        let code = r#"
function hello() {
    console.log("Hello, world!");
}
"#;
        let tokens = count_tokens(code);
        assert!(tokens > 5);
        assert!(tokens < 50); // Code has more tokens per character
    }

    #[test]
    fn test_count_tokens_empty() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn test_count_tokens_whitespace() {
        // Whitespace typically tokenizes efficiently
        let tokens = count_tokens("   \n\t   ");
        assert!(tokens < 5);
    }

    #[test]
    fn test_count_tokens_unicode() {
        // Unicode characters may use multiple tokens
        let tokens = count_tokens("Hello 世界 🌍");
        assert!(tokens > 0);
    }

    #[test]
    fn test_tokenizer_initialized() {
        // Verify the tokenizer actually works (not falling back)
        assert!(
            TOKENIZER.is_some(),
            "Tokenizer should initialize successfully"
        );
    }
}
