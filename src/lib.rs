//! @toon
//! purpose: This is the library crate root for luny, exposing the public API for use as both
//!     a CLI tool and a library. It re-exports key types and functions from all modules
//!     for convenient access by consumers.
//!
//! when-editing:
//!     - !All public modules must be declared here with pub mod
//!     - !Re-exports should include commonly used types and functions
//!     - Keep the re-export list organized by module
//!
//! invariants:
//!     - The public API surface is stable - all re-exported items are public contract
//!     - All language parsers are accessible through ParserFactory
//!
//! do-not:
//!     - Never remove a re-export without major version bump (breaking change)
//!     - Never expose internal implementation details
//!
//! gotchas:
//!     - The lib.rs is separate from main.rs - library consumers get lib, CLI gets main
//!     - Some types like ToonCommentBlock are only used internally but exported for testing

pub mod cli;
pub mod commands;
pub mod formatter;
pub mod parser;
pub mod types;

// Re-export main types for convenience
pub use cli::{Cli, Commands, GenerateArgs, StripArgs, ValidateArgs};
pub use formatter::{format_toon, parse_toon};
pub use parser::{LanguageParser, ParseError, ParserFactory};
pub use types::{
    ASTInfo, CallInfo, CalledByInfo, ExportInfo, ExtractedComments, FunctionAnnotation, ImportInfo,
    SignatureInfo, ToonCommentBlock, ToonData, ValidationResult, WhenEditingItem,
};
