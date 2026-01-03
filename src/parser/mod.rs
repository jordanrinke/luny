//! @toon
//! purpose: This module serves as the central hub for all language parsers in luny.
//!     It defines the LanguageParser trait that all parsers must implement and provides
//!     the ParserFactory for creating and retrieving language-specific parsers based on
//!     file extensions.
//!
//! when-editing:
//!     - !When adding a new language parser, you must register it in ParserFactory::new()
//!     - !The LanguageParser trait defines the contract all parsers must follow
//!     - Each parser is stored as Arc<dyn LanguageParser> for thread-safe sharing
//!
//! invariants:
//!     - Every file extension maps to exactly one parser implementation
//!     - The ParserFactory must be Send + Sync safe for concurrent access
//!     - All parsers must implement the full LanguageParser trait
//!
//! do-not:
//!     - Never add duplicate extension mappings to different parsers
//!     - Never expose mutable access to the parsers HashMap
//!
//! gotchas:
//!     - The TypeScript parser handles both .ts/.tsx and .js/.jsx extensions
//!     - Extensions are stored without the leading dot (e.g., "ts" not ".ts")
//!
//! flows:
//!     - Get parser: Call get_parser() with a Path, it extracts extension and looks up parser
//!     - Check support: Call is_supported() to verify a file type is handled
//!     - List extensions: Call supported_extensions() to get all registered extensions

mod csharp;
mod go;
mod python;
mod ruby;
mod rust;
pub mod toon_comment;
mod typescript;

use crate::types::{ASTInfo, ExtractedComments};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

pub use csharp::CSharpParser;
pub use go::GoParser;
pub use python::PythonParser;
pub use ruby::RubyParser;
pub use rust::RustParser;
pub use typescript::TypeScriptParser;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse source: {0}")]
    ParseError(String),
    #[error("Unsupported language for extension: {0}")]
    UnsupportedLanguage(String),
}

/// Trait for language-specific parsers
pub trait LanguageParser: Send + Sync {
    /// Returns the language name (e.g., "typescript", "python")
    fn language_name(&self) -> &'static str;

    /// Returns file extensions this parser handles
    fn file_extensions(&self) -> &[&'static str];

    /// Extract AST information (exports, imports, calls, signatures)
    fn extract_ast_info(&self, source: &str, file_path: &Path) -> Result<ASTInfo, ParseError>;

    /// Extract @toon comments from source
    fn extract_toon_comments(&self, source: &str) -> Result<ExtractedComments, ParseError>;

    /// Strip @toon comments from source, returning cleaned source
    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError>;
}

/// Factory for creating language parsers
pub struct ParserFactory {
    parsers: HashMap<String, Arc<dyn LanguageParser>>,
}

impl ParserFactory {
    pub fn new() -> Self {
        let mut parsers: HashMap<String, Arc<dyn LanguageParser>> = HashMap::new();

        // TypeScript/JavaScript parser
        let ts_parser: Arc<dyn LanguageParser> = Arc::new(TypeScriptParser::new());
        for ext in ts_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&ts_parser));
        }

        // Python parser
        let py_parser: Arc<dyn LanguageParser> = Arc::new(PythonParser::new());
        for ext in py_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&py_parser));
        }

        // Ruby parser
        let rb_parser: Arc<dyn LanguageParser> = Arc::new(RubyParser::new());
        for ext in rb_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&rb_parser));
        }

        // C# parser
        let cs_parser: Arc<dyn LanguageParser> = Arc::new(CSharpParser::new());
        for ext in cs_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&cs_parser));
        }

        // Go parser
        let go_parser: Arc<dyn LanguageParser> = Arc::new(GoParser::new());
        for ext in go_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&go_parser));
        }

        // Rust parser
        let rust_parser: Arc<dyn LanguageParser> = Arc::new(RustParser::new());
        for ext in rust_parser.file_extensions() {
            parsers.insert(ext.to_string(), Arc::clone(&rust_parser));
        }

        Self { parsers }
    }

    /// Get parser for a file path based on extension
    pub fn get_parser(&self, file_path: &Path) -> Option<&dyn LanguageParser> {
        let ext = file_path.extension()?.to_str()?;
        self.parsers.get(ext).map(|p| p.as_ref())
    }

    /// Get parser by extension string
    pub fn get_parser_by_ext(&self, ext: &str) -> Option<&dyn LanguageParser> {
        self.parsers.get(ext).map(|p| p.as_ref())
    }

    /// Check if a file extension is supported
    pub fn is_supported(&self, file_path: &Path) -> bool {
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.parsers.contains_key(ext))
            .unwrap_or(false)
    }

    /// Get all supported extensions
    pub fn supported_extensions(&self) -> Vec<&str> {
        self.parsers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ParserFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_factory() {
        let factory = ParserFactory::new();

        // Verify all language parsers are registered with correct language names
        assert_eq!(
            factory
                .get_parser(Path::new("test.ts"))
                .unwrap()
                .language_name(),
            "typescript"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.tsx"))
                .unwrap()
                .language_name(),
            "typescript"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.js"))
                .unwrap()
                .language_name(),
            "typescript"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.jsx"))
                .unwrap()
                .language_name(),
            "typescript"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.py"))
                .unwrap()
                .language_name(),
            "python"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.rb"))
                .unwrap()
                .language_name(),
            "ruby"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.cs"))
                .unwrap()
                .language_name(),
            "csharp"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.go"))
                .unwrap()
                .language_name(),
            "go"
        );
        assert_eq!(
            factory
                .get_parser(Path::new("test.rs"))
                .unwrap()
                .language_name(),
            "rust"
        );

        // Verify unsupported extensions return None
        assert!(factory.get_parser(Path::new("test.json")).is_none());
        assert!(factory.get_parser(Path::new("Makefile")).is_none());
    }

    #[test]
    fn test_supported_extensions() {
        let factory = ParserFactory::new();
        let mut exts = factory.supported_extensions();
        exts.sort();
        assert_eq!(
            exts,
            vec!["cs", "go", "js", "jsx", "py", "rb", "rs", "ts", "tsx"]
        );
    }
}
