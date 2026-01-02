//! @toon
//! purpose: This module provides text compression utilities that reduce verbose prose to
//!     compact, token-efficient text for TOON files. It applies pattern-based substitutions
//!     to remove filler words and redundant phrases.
//!
//! when-editing:
//!     - !Patterns are applied in order - earlier patterns may affect later matches
//!     - !The COMPRESSION_PATTERNS static is lazily initialized for performance
//!     - All patterns are case-insensitive
//!
//! invariants:
//!     - Compression never changes the semantic meaning of text
//!     - compress_item always returns a string of at most MAX_ITEM_LENGTH characters
//!     - Trailing and leading whitespace is always trimmed
//!
//! do-not:
//!     - Never add patterns that could change meaning (e.g., removing "not")
//!     - Never remove domain-specific terminology
//!
//! gotchas:
//!     - The whitespace normalization pattern (\s+) should stay last to clean up gaps
//!     - Long items are truncated with "..." suffix, not intelligently summarized
//!     - Pattern order matters - more specific patterns should come before general ones

use once_cell::sync::Lazy;
use regex::Regex;

/// Compression patterns for reducing prose verbosity
static COMPRESSION_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (Regex::new(r"(?i)\bmust always\b").unwrap(), "must"),
        (Regex::new(r"(?i)\bin order to\b").unwrap(), "to"),
        (Regex::new(r"(?i)\bat a time\b").unwrap(), ""),
        (Regex::new(r"(?i)\bmake sure to\b").unwrap(), ""),
        (Regex::new(r"(?i)\bensure that\b").unwrap(), ""),
        (Regex::new(r"(?i)\bit is important to\b").unwrap(), ""),
        (Regex::new(r"(?i)\bwhen you\b").unwrap(), "when"),
        (Regex::new(r"(?i)\byou should\b").unwrap(), ""),
        (Regex::new(r"(?i)\byou must\b").unwrap(), "must"),
        (Regex::new(r"(?i)\byou need to\b").unwrap(), "must"),
        (Regex::new(r"(?i)\bthis is\b").unwrap(), ""),
        (Regex::new(r"(?i)\bthere is\b").unwrap(), ""),
        (Regex::new(r"(?i)\bthere are\b").unwrap(), ""),
        (Regex::new(r"(?i)\bthe following\b").unwrap(), ""),
        (Regex::new(r"\s+").unwrap(), " "),
    ]
});

/// Maximum length for list items
pub const MAX_ITEM_LENGTH: usize = 120;

/// Compress prose by removing filler words
pub fn compress(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in COMPRESSION_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result.trim().to_string()
}

/// Compress and truncate a list item
pub fn compress_item(text: &str) -> String {
    let compressed = compress(text);
    if compressed.len() > MAX_ITEM_LENGTH {
        format!("{}...", &compressed[..MAX_ITEM_LENGTH - 3])
    } else {
        compressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== compress Tests ====================

    #[test]
    fn test_compress_filler_words() {
        assert_eq!(compress("you must always do this"), "must do this");
        assert_eq!(compress("in order to achieve this"), "to achieve this");
        assert_eq!(compress("make sure to check"), "check");
    }

    #[test]
    fn test_compress_whitespace() {
        assert_eq!(compress("multiple   spaces   here"), "multiple spaces here");
    }

    #[test]
    fn test_compress_must_always() {
        assert_eq!(compress("must always validate"), "must validate");
    }

    #[test]
    fn test_compress_in_order_to() {
        assert_eq!(compress("in order to work"), "to work");
    }

    #[test]
    fn test_compress_ensure_that() {
        assert_eq!(compress("ensure that values are valid"), "values are valid");
    }

    #[test]
    fn test_compress_it_is_important() {
        assert_eq!(compress("it is important to check"), "check");
    }

    #[test]
    fn test_compress_you_should() {
        assert_eq!(compress("you should validate input"), "validate input");
    }

    #[test]
    fn test_compress_you_must() {
        assert_eq!(compress("you must check this"), "must check this");
    }

    #[test]
    fn test_compress_you_need_to() {
        assert_eq!(compress("you need to verify"), "must verify");
    }

    #[test]
    fn test_compress_this_is() {
        assert_eq!(compress("this is a test"), "a test");
    }

    #[test]
    fn test_compress_there_is() {
        assert_eq!(compress("there is an issue"), "an issue");
    }

    #[test]
    fn test_compress_there_are() {
        assert_eq!(compress("there are many cases"), "many cases");
    }

    #[test]
    fn test_compress_the_following() {
        assert_eq!(compress("the following rules apply"), "rules apply");
    }

    #[test]
    fn test_compress_when_you() {
        assert_eq!(compress("when you edit the file"), "when edit the file");
    }

    #[test]
    fn test_compress_case_insensitive() {
        assert_eq!(compress("MUST ALWAYS do"), "must do");
        assert_eq!(compress("In Order To work"), "to work");
        assert_eq!(compress("ENSURE THAT it works"), "it works");
    }

    #[test]
    fn test_compress_multiple_patterns() {
        let input = "you must always ensure that in order to make sure to do this";
        let result = compress(input);
        assert!(!result.contains("must always"));
        assert!(!result.contains("ensure that"));
        assert!(!result.contains("in order to"));
    }

    #[test]
    fn test_compress_preserves_content() {
        // Should not remove meaningful words
        assert_eq!(compress("validate input"), "validate input");
        assert_eq!(compress("check for null"), "check for null");
    }

    #[test]
    fn test_compress_trims() {
        assert_eq!(compress("  text with spaces  "), "text with spaces");
    }

    #[test]
    fn test_compress_empty_string() {
        assert_eq!(compress(""), "");
    }

    #[test]
    fn test_compress_newlines() {
        assert_eq!(compress("line one\nline two"), "line one line two");
    }

    #[test]
    fn test_compress_tabs() {
        assert_eq!(compress("tab\there"), "tab here");
    }

    // ==================== compress_item Tests ====================

    #[test]
    fn test_compress_item_truncation() {
        let long_text = "a".repeat(150);
        let compressed = compress_item(&long_text);
        assert!(compressed.len() <= MAX_ITEM_LENGTH);
        assert!(compressed.ends_with("..."));
    }

    #[test]
    fn test_compress_item_short_text() {
        let short_text = "short text";
        let compressed = compress_item(short_text);
        assert_eq!(compressed, "short text");
        assert!(!compressed.ends_with("..."));
    }

    #[test]
    fn test_compress_item_exact_length() {
        let exact_text = "a".repeat(MAX_ITEM_LENGTH);
        let compressed = compress_item(&exact_text);
        assert_eq!(compressed.len(), MAX_ITEM_LENGTH);
        assert!(!compressed.ends_with("..."));
    }

    #[test]
    fn test_compress_item_one_over() {
        let one_over = "a".repeat(MAX_ITEM_LENGTH + 1);
        let compressed = compress_item(&one_over);
        assert!(compressed.len() <= MAX_ITEM_LENGTH);
        assert!(compressed.ends_with("..."));
    }

    #[test]
    fn test_compress_item_applies_compression() {
        let verbose = "you must always check in order to ensure that the values are valid";
        let compressed = compress_item(verbose);
        assert!(!compressed.contains("must always"));
        assert!(!compressed.contains("in order to"));
    }

    #[test]
    fn test_compress_item_empty() {
        assert_eq!(compress_item(""), "");
    }

    #[test]
    fn test_compress_item_whitespace_only() {
        assert_eq!(compress_item("   "), "");
    }

    // ==================== Constant Tests ====================

    #[test]
    fn test_max_item_length() {
        assert_eq!(MAX_ITEM_LENGTH, 120);
    }
}
