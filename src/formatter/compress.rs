//! @dose
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

    /// Test all compression patterns in one comprehensive test
    #[test]
    fn test_compress_all_patterns() {
        // All 14 compression patterns + whitespace normalization
        assert_eq!(compress("must always validate"), "must validate");
        assert_eq!(compress("in order to work"), "to work");
        assert_eq!(compress("at a time process"), "process");
        assert_eq!(compress("make sure to check"), "check");
        assert_eq!(compress("ensure that values are valid"), "values are valid");
        assert_eq!(compress("it is important to check"), "check");
        assert_eq!(compress("when you edit the file"), "when edit the file");
        assert_eq!(compress("you should validate input"), "validate input");
        assert_eq!(compress("you must check this"), "must check this");
        assert_eq!(compress("you need to verify"), "must verify");
        assert_eq!(compress("this is a test"), "a test");
        assert_eq!(compress("there is an issue"), "an issue");
        assert_eq!(compress("there are many cases"), "many cases");
        assert_eq!(compress("the following rules apply"), "rules apply");
        // Whitespace normalization
        assert_eq!(compress("multiple   spaces   here"), "multiple spaces here");
        // Case insensitive
        assert_eq!(compress("MUST ALWAYS do"), "must do");
        assert_eq!(compress("In Order To work"), "to work");
        // Multiple patterns combined
        let input = "you must always ensure that in order to make sure to do this";
        let result = compress(input);
        assert!(!result.contains("must always"));
        assert!(!result.contains("ensure that"));
        // Preserves meaningful content
        assert_eq!(compress("validate input"), "validate input");
    }

    /// Test compress_item truncation and integration with compress
    #[test]
    fn test_compress_item() {
        // Short text - no truncation
        assert_eq!(compress_item("short text"), "short text");
        // Exact length - no truncation
        let exact = "a".repeat(MAX_ITEM_LENGTH);
        assert_eq!(compress_item(&exact).len(), MAX_ITEM_LENGTH);
        assert!(!compress_item(&exact).ends_with("..."));
        // Over length - truncated
        let long = "a".repeat(150);
        assert!(compress_item(&long).len() <= MAX_ITEM_LENGTH);
        assert!(compress_item(&long).ends_with("..."));
        // Applies compression patterns
        let verbose = "you must always check in order to ensure that the values are valid";
        assert!(!compress_item(verbose).contains("must always"));
    }

    /// Test edge cases
    #[test]
    fn test_compress_edge_cases() {
        assert_eq!(compress(""), "");
        assert_eq!(compress("  text with spaces  "), "text with spaces");
        assert_eq!(compress("line one\nline two"), "line one line two");
        assert_eq!(compress("tab\there"), "tab here");
        assert_eq!(compress_item(""), "");
        assert_eq!(compress_item("   "), "");
        assert_eq!(MAX_ITEM_LENGTH, 120);
    }
}
