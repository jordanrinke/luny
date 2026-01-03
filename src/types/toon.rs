//! @toon
//! purpose: This module defines all core data types used throughout luny, including export
//!     information, import tracking, call relationships, and the main ToonData structure
//!     that represents the complete DOSE for a source file.
//!
//! when-editing:
//!     - !ToonData is the central type - all fields map directly to TOON file format
//!     - !ExportInfo.kind must match the values used by each language parser
//!     - All Option fields use skip_serializing_if to avoid empty fields in output
//!
//! invariants:
//!     - ToonData.purpose is the only user-required field; tokens/exports are auto-generated
//!     - All semantic fields (invariants, gotchas, etc.) are optional
//!     - ValidationResult tracks both errors (fatal) and warnings (non-fatal)
//!
//! do-not:
//!     - Never change ExportInfo or ImportInfo without updating all parsers
//!     - Never add required fields to ToonData without a migration strategy
//!
//! gotchas:
//!     - ToonCommentBlock is used for parsing, ToonData is used for generating
//!     - FunctionAnnotation supports per-function metadata but is rarely populated
//!     - WhenEditingItem.important=true maps to the ! prefix in TOON format
//!
//! flows:
//!     - Parser extracts ASTInfo from source code
//!     - Parser extracts ExtractedComments from @toon blocks
//!     - generate command merges both into ToonData
//!     - formatter converts ToonData to TOON file text

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Export information extracted from AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportInfo {
    /// Export name (e.g., "AuthProvider", "useAuth")
    pub name: String,
    /// Kind of export (fn, type, const, class, component, hook, schema)
    pub kind: String,
}

/// Import information extracted from AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportInfo {
    /// Source module (e.g., "react", "./shared")
    pub from: String,
    /// Imported items (e.g., ["useState", "useEffect"])
    pub items: Vec<String>,
}

/// Call information extracted from AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInfo {
    /// Target module (e.g., "./api-client", "./storage")
    pub target: String,
    /// Method or function name (e.g., "refresh", "readTokens")
    pub method: String,
}

/// Reverse call information (what calls this file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalledByInfo {
    /// File path that calls this
    pub from: String,
    /// Function name in the calling file
    pub function: String,
}

/// Full signature information for AI reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    /// Export name
    pub name: String,
    /// Kind (fn, type, interface, class, const, hook, component, schema, enum)
    pub kind: String,
    /// Full type signature (e.g., "(props: Props) => JSX.Element")
    pub signature: String,
}

/// When-editing item with optional importance flag
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhenEditingItem {
    /// The guidance text
    pub text: String,
    /// Whether this is important (marked with ! prefix)
    pub important: bool,
}

/// Function-level annotations (subset of fields applicable to functions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnnotation {
    /// Function name
    pub name: String,
    /// Invariants specific to this function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariants: Option<Vec<String>>,
    /// Gotchas specific to this function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotchas: Option<Vec<String>>,
    /// Do-not rules for this function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub do_not: Option<Vec<String>>,
    /// Error handling for this function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<Vec<String>>,
    /// Constraints for this function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Vec<String>>,
}

/// Structural information extracted from source AST
#[derive(Debug, Clone, Default)]
pub struct ASTInfo {
    /// Approximate token count (source.length / 4)
    pub tokens: usize,
    /// Exports found in the file
    pub exports: Vec<ExportInfo>,
    /// Imports in the file
    pub imports: Vec<ImportInfo>,
    /// External calls made by this file
    pub calls: Vec<CallInfo>,
    /// Full signatures for all exports
    pub signatures: Vec<SignatureInfo>,
}

/// Result of extracting @toon comments from source
#[derive(Debug, Clone, Default)]
pub struct ExtractedComments {
    /// File-level @toon block content (parsed fields)
    pub file_block: Option<ToonCommentBlock>,
    /// Function-level annotations: function_name -> annotations
    pub function_annotations: HashMap<String, FunctionAnnotation>,
}

/// Parsed content from a @toon block comment
#[derive(Debug, Clone, Default)]
pub struct ToonCommentBlock {
    pub purpose: Option<String>,
    pub when_editing: Option<Vec<WhenEditingItem>>,
    pub do_not: Option<Vec<String>>,
    pub invariants: Option<Vec<String>>,
    pub error_handling: Option<Vec<String>>,
    pub constraints: Option<Vec<String>>,
    pub gotchas: Option<Vec<String>>,
    pub flows: Option<Vec<String>>,
    pub testing: Option<Vec<String>>,
    pub common_mistakes: Option<Vec<String>>,
    pub change_impacts: Option<Vec<String>>,
    pub related: Option<Vec<String>>,
}

/// Combined data for generating TOON DOSE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToonData {
    // Required fields
    pub purpose: String,
    pub tokens: usize,
    pub exports: Vec<ExportInfo>,

    // When-editing guidance (from docs) - ! prefix marks important items
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_editing: Option<Vec<WhenEditingItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub do_not: Option<Vec<String>>,

    // Structural (from AST + dependency graph)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imports: Option<Vec<ImportInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<CallInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_by: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub called_by: Option<Vec<CalledByInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<SignatureInfo>>,

    // Semantic (from docs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariants: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotchas: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flows: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub testing: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_mistakes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_impacts: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<Vec<String>>,

    // Function-level annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_annotations: Option<Vec<FunctionAnnotation>>,

    // Validation control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
}

impl ToonData {
    pub fn new(purpose: String, tokens: usize, exports: Vec<ExportInfo>) -> Self {
        Self {
            purpose,
            tokens,
            exports,
            when_editing: None,
            do_not: None,
            imports: None,
            calls: None,
            imported_by: None,
            called_by: None,
            signatures: None,
            invariants: None,
            error_handling: None,
            constraints: None,
            gotchas: None,
            flows: None,
            testing: None,
            common_mistakes: None,
            change_impacts: None,
            related: None,
            function_annotations: None,
            ignore: None,
        }
    }
}

/// Validation result for a single file
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub source_path: String,
    pub toon_path: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new(source_path: String, toon_path: String) -> Self {
        Self {
            source_path,
            toon_path,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    pub fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}
