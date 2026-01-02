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

mod typescript;
mod python;
mod ruby;
mod csharp;
mod go;
mod rust;

use crate::types::{ASTInfo, ExtractedComments};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

pub use typescript::TypeScriptParser;
pub use python::PythonParser;
pub use ruby::RubyParser;
pub use csharp::CSharpParser;
pub use go::GoParser;
pub use rust::RustParser;

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
    fn test_parser_factory_new() {
        let factory = ParserFactory::new();
        assert!(!factory.parsers.is_empty());
    }

    #[test]
    fn test_get_parser_typescript() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("test.ts"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "typescript");
    }

    #[test]
    fn test_get_parser_tsx() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("component.tsx"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "typescript");
    }

    #[test]
    fn test_get_parser_javascript() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("script.js"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "typescript");
    }

    #[test]
    fn test_get_parser_jsx() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("component.jsx"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "typescript");
    }

    #[test]
    fn test_get_parser_python() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("script.py"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "python");
    }

    #[test]
    fn test_get_parser_ruby() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("script.rb"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "ruby");
    }

    #[test]
    fn test_get_parser_csharp() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("Program.cs"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "csharp");
    }

    #[test]
    fn test_get_parser_go() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("main.go"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "go");
    }

    #[test]
    fn test_get_parser_rust() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("lib.rs"));
        assert!(parser.is_some());
        assert_eq!(parser.unwrap().language_name(), "rust");
    }

    #[test]
    fn test_get_parser_unsupported() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("data.json"));
        assert!(parser.is_none());
    }

    #[test]
    fn test_get_parser_no_extension() {
        let factory = ParserFactory::new();
        let parser = factory.get_parser(Path::new("Makefile"));
        assert!(parser.is_none());
    }

    #[test]
    fn test_get_parser_by_ext() {
        let factory = ParserFactory::new();
        assert!(factory.get_parser_by_ext("ts").is_some());
        assert!(factory.get_parser_by_ext("py").is_some());
        assert!(factory.get_parser_by_ext("rb").is_some());
        assert!(factory.get_parser_by_ext("cs").is_some());
        assert!(factory.get_parser_by_ext("go").is_some());
        assert!(factory.get_parser_by_ext("rs").is_some());
        assert!(factory.get_parser_by_ext("xyz").is_none());
    }

    #[test]
    fn test_is_supported() {
        let factory = ParserFactory::new();
        assert!(factory.is_supported(Path::new("test.ts")));
        assert!(factory.is_supported(Path::new("test.tsx")));
        assert!(factory.is_supported(Path::new("test.js")));
        assert!(factory.is_supported(Path::new("test.jsx")));
        assert!(factory.is_supported(Path::new("test.py")));
        assert!(factory.is_supported(Path::new("test.rb")));
        assert!(factory.is_supported(Path::new("test.cs")));
        assert!(factory.is_supported(Path::new("test.go")));
        assert!(factory.is_supported(Path::new("test.rs")));
        assert!(!factory.is_supported(Path::new("test.txt")));
        assert!(!factory.is_supported(Path::new("Makefile")));
    }

    #[test]
    fn test_supported_extensions() {
        let factory = ParserFactory::new();
        let exts = factory.supported_extensions();
        assert!(exts.contains(&"ts"));
        assert!(exts.contains(&"tsx"));
        assert!(exts.contains(&"js"));
        assert!(exts.contains(&"jsx"));
        assert!(exts.contains(&"py"));
        assert!(exts.contains(&"rb"));
        assert!(exts.contains(&"cs"));
        assert!(exts.contains(&"go"));
        assert!(exts.contains(&"rs"));
    }

    #[test]
    fn test_parser_factory_default() {
        let factory = ParserFactory::default();
        assert!(!factory.parsers.is_empty());
    }

    #[test]
    fn test_parser_file_extensions() {
        let factory = ParserFactory::new();
        let ts_parser = factory.get_parser_by_ext("ts").unwrap();
        let extensions = ts_parser.file_extensions();
        assert!(extensions.contains(&"ts"));
        assert!(extensions.contains(&"tsx"));
        assert!(extensions.contains(&"js"));
        assert!(extensions.contains(&"jsx"));
    }
}
