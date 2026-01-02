//! @toon
//! purpose: This module parses Python source files to extract top-level functions, classes,
//!     and import statements. It uses tree-sitter for robust parsing and follows Python
//!     conventions for determining which items are public exports.
//!
//! when-editing:
//!     - !Functions and classes starting with underscore are considered private
//!     - !Decorated functions and classes need special handling via decorated_definition nodes
//!     - Only top-level definitions (depth <= 1) are considered exports
//!
//! invariants:
//!     - Private items (names starting with _) are never included in exports
//!     - All imports from the same module are grouped into a single ImportInfo
//!     - The parser returns empty vectors for missing information rather than erroring
//!
//! do-not:
//!     - Never treat class methods as top-level exports
//!     - Never include __init__ or __main__ as exports
//!
//! gotchas:
//!     - Python has no explicit export syntax; all public top-level items are exports
//!     - The @toon block can be in either triple-quoted docstrings or hash comments
//!     - Relative imports use dots and need special handling in import_prefix nodes
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set Python language, parse source
//!     - Extract exports: Walk AST at depth 0-1, collect function_definition and class_definition
//!     - Extract imports: Walk AST collecting import_statement and import_from_statement nodes

use crate::parser::{LanguageParser, ParseError};
use crate::types::{
    ASTInfo, ExportInfo, ExtractedComments, ImportInfo, ToonCommentBlock, WhenEditingItem,
};
use regex::Regex;
use std::path::Path;
use tree_sitter::Parser;

/// Parser for Python files
#[derive(Clone)]
pub struct PythonParser;

impl PythonParser {
    pub fn new() -> Self {
        Self
    }

    fn create_parser(&self) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;
        Ok(parser)
    }

    fn extract_exports(&self, root: tree_sitter::Node, source: &str) -> Vec<ExportInfo> {
        let mut exports = Vec::new();
        let mut cursor = root.walk();

        self.visit_exports(&mut cursor, source, &mut exports, 0);
        exports
    }

    fn visit_exports(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        exports: &mut Vec<ExportInfo>,
        depth: usize,
    ) {
        // Only look at top-level definitions
        if depth > 1 {
            return;
        }

        loop {
            let node = cursor.node();

            match node.kind() {
                "function_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        // Skip private functions (starting with _)
                        if !name.starts_with('_') {
                            exports.push(ExportInfo {
                                name,
                                kind: "fn".to_string(),
                            });
                        }
                    }
                }
                "class_definition" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        if !name.starts_with('_') {
                            exports.push(ExportInfo {
                                name,
                                kind: "class".to_string(),
                            });
                        }
                    }
                }
                "decorated_definition" => {
                    // Handle @decorator decorated functions/classes
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "function_definition" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = self.node_text(name_node, source);
                                    if !name.starts_with('_') {
                                        exports.push(ExportInfo {
                                            name,
                                            kind: "fn".to_string(),
                                        });
                                    }
                                }
                            } else if child.kind() == "class_definition" {
                                if let Some(name_node) = child.child_by_field_name("name") {
                                    let name = self.node_text(name_node, source);
                                    if !name.starts_with('_') {
                                        exports.push(ExportInfo {
                                            name,
                                            kind: "class".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.visit_exports(cursor, source, exports, depth + 1);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_imports(&self, root: tree_sitter::Node, source: &str) -> Vec<ImportInfo> {
        let mut imports = Vec::new();
        let mut cursor = root.walk();

        self.visit_imports(&mut cursor, source, &mut imports);
        imports
    }

    fn visit_imports(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        imports: &mut Vec<ImportInfo>,
    ) {
        loop {
            let node = cursor.node();

            match node.kind() {
                "import_statement" => {
                    // import foo, bar
                    let mut items = Vec::new();
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "dotted_name" {
                                items.push(self.node_text(child, source));
                            }
                        }
                    }
                    if !items.is_empty() {
                        imports.push(ImportInfo {
                            from: items[0].clone(),
                            items,
                        });
                    }
                }
                "import_from_statement" => {
                    // from foo import bar, baz
                    let mut from = String::new();
                    let mut items = Vec::new();

                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            match child.kind() {
                                "dotted_name" | "relative_import" => {
                                    if from.is_empty() {
                                        from = self.node_text(child, source);
                                    }
                                }
                                "import_prefix" => {
                                    from = self.node_text(child, source);
                                }
                                "aliased_import" => {
                                    if let Some(name) = child.child_by_field_name("name") {
                                        items.push(self.node_text(name, source));
                                    }
                                }
                                "identifier" => {
                                    items.push(self.node_text(child, source));
                                }
                                _ => {}
                            }
                        }
                    }

                    if !from.is_empty() {
                        imports.push(ImportInfo { from, items });
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.visit_imports(cursor, source, imports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn node_text(&self, node: tree_sitter::Node, source: &str) -> String {
        source[node.start_byte()..node.end_byte()].to_string()
    }

    fn parse_toon_block(&self, content: &str) -> ToonCommentBlock {
        let mut block = ToonCommentBlock::default();

        let lines: Vec<&str> = content.lines().collect();
        let mut current_section: Option<&str> = None;
        let mut current_items: Vec<String> = Vec::new();

        for line in lines {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            if let Some(header) = self.parse_section_header(trimmed) {
                self.save_section(&mut block, current_section, &current_items);
                current_section = Some(header);
                current_items.clear();
            } else if current_section.is_none() && block.purpose.is_none() {
                block.purpose = Some(trimmed.to_string());
            } else if trimmed.starts_with('-') || trimmed.starts_with('•') {
                let item = trimmed.trim_start_matches('-').trim_start_matches('•').trim();
                if !item.is_empty() {
                    current_items.push(item.to_string());
                }
            } else if current_section.is_some() {
                current_items.push(trimmed.to_string());
            }
        }

        self.save_section(&mut block, current_section, &current_items);
        block
    }

    fn parse_section_header<'a>(&self, line: &'a str) -> Option<&'a str> {
        let headers = [
            "When-Editing:",
            "When Editing:",
            "DO-NOT:",
            "Do-Not:",
            "Invariants:",
            "Error Handling:",
            "Error-Handling:",
            "Constraints:",
            "Gotchas:",
            "Flows:",
            "Testing:",
            "Common Mistakes:",
            "Common-Mistakes:",
            "Change Impacts:",
            "Change-Impacts:",
            "Related:",
        ];

        for header in headers {
            if line.eq_ignore_ascii_case(header) || line.starts_with(header) {
                return Some(header.trim_end_matches(':'));
            }
        }
        None
    }

    fn save_section(&self, block: &mut ToonCommentBlock, section: Option<&str>, items: &[String]) {
        if items.is_empty() {
            return;
        }

        match section {
            Some(s)
                if s.eq_ignore_ascii_case("When-Editing")
                    || s.eq_ignore_ascii_case("When Editing") =>
            {
                block.when_editing = Some(
                    items
                        .iter()
                        .map(|item| {
                            let important = item.starts_with('!');
                            let text = if important { &item[1..] } else { item };
                            WhenEditingItem {
                                text: text.trim().to_string(),
                                important,
                            }
                        })
                        .collect(),
                );
            }
            Some(s) if s.eq_ignore_ascii_case("DO-NOT") || s.eq_ignore_ascii_case("Do-Not") => {
                block.do_not = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Invariants") || s.eq_ignore_ascii_case("Invariant") => {
                block.invariants = Some(items.to_vec());
            }
            Some(s)
                if s.eq_ignore_ascii_case("Error Handling")
                    || s.eq_ignore_ascii_case("Error-Handling") =>
            {
                block.error_handling = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Constraints") || s.eq_ignore_ascii_case("Constraint") => {
                block.constraints = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Gotchas") || s.eq_ignore_ascii_case("Gotcha") => {
                block.gotchas = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Flows") || s.eq_ignore_ascii_case("Flow") => {
                block.flows = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Testing") => {
                block.testing = Some(items.to_vec());
            }
            Some(s)
                if s.eq_ignore_ascii_case("Common Mistakes")
                    || s.eq_ignore_ascii_case("Common-Mistakes") =>
            {
                block.common_mistakes = Some(items.to_vec());
            }
            Some(s)
                if s.eq_ignore_ascii_case("Change Impacts")
                    || s.eq_ignore_ascii_case("Change-Impacts") =>
            {
                block.change_impacts = Some(items.to_vec());
            }
            Some(s) if s.eq_ignore_ascii_case("Related") => {
                block.related = Some(items.to_vec());
            }
            _ => {}
        }
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for PythonParser {
    fn language_name(&self) -> &'static str {
        "python"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["py"]
    }

    fn extract_ast_info(&self, source: &str, _file_path: &Path) -> Result<ASTInfo, ParseError> {
        let mut parser = self.create_parser()?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse source".to_string()))?;

        let root = tree.root_node();

        let exports = self.extract_exports(root, source);
        let imports = self.extract_imports(root, source);

        let tokens = source.len() / 4;

        Ok(ASTInfo {
            tokens,
            exports,
            imports,
            calls: Vec::new(),
            signatures: Vec::new(),
        })
    }

    fn extract_toon_comments(&self, source: &str) -> Result<ExtractedComments, ParseError> {
        let mut result = ExtractedComments::default();

        // Find @toon in docstrings (triple-quoted strings)
        let docstring_pattern = Regex::new(r#""""[\s\S]*?@toon[\s\S]*?""""#).unwrap();

        if let Some(mat) = docstring_pattern.find(source) {
            let comment = mat.as_str();
            let content = comment.trim_start_matches("\"\"\"").trim_end_matches("\"\"\"");

            // Remove @toon marker
            let content = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim();
                    if trimmed == "@toon" {
                        ""
                    } else {
                        trimmed
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            result.file_block = Some(self.parse_toon_block(&content));
        }

        // Also check for # @toon comments
        let comment_pattern = Regex::new(r"#\s*@toon\s*\n((?:#[^\n]*\n)*)").unwrap();
        if result.file_block.is_none() {
            if let Some(caps) = comment_pattern.captures(source) {
                if let Some(block) = caps.get(1) {
                    let content = block
                        .as_str()
                        .lines()
                        .map(|line| line.trim_start_matches('#').trim())
                        .collect::<Vec<_>>()
                        .join("\n");
                    result.file_block = Some(self.parse_toon_block(&content));
                }
            }
        }

        Ok(result)
    }

    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError> {
        let mut result = source.to_string();

        // Replace docstring @toon comments
        let docstring_pattern = Regex::new(r#""""[\s\S]*?@toon[\s\S]*?""""#).unwrap();
        result = docstring_pattern
            .replace_all(&result, &format!("# @toon -> {}", toon_path))
            .to_string();

        // Remove # @toon comment blocks
        let comment_pattern = Regex::new(r"#\s*@toon[^\n]*\n(?:#[^\n]*\n)*").unwrap();
        result = comment_pattern
            .replace_all(&result, &format!("# @toon -> {}\n", toon_path))
            .to_string();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY_FIXTURE: &str = include_str!("../../test_fixtures/sample.py");

    #[test]
    fn test_language_name() {
        let parser = PythonParser::new();
        assert_eq!(parser.language_name(), "python");
    }

    #[test]
    fn test_file_extensions() {
        let parser = PythonParser::new();
        let exts = parser.file_extensions();
        assert!(exts.contains(&"py"));
        assert_eq!(exts.len(), 1);
    }

    #[test]
    fn test_extract_exports() {
        let parser = PythonParser::new();
        let info = parser.extract_ast_info(PY_FIXTURE, Path::new("sample.py")).unwrap();

        assert!(!info.exports.is_empty());

        // Check for class exports
        let class_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "class")
            .collect();
        assert!(class_exports.iter().any(|e| e.name == "UserConfig"));
        assert!(class_exports.iter().any(|e| e.name == "BaseService"));
        assert!(class_exports.iter().any(|e| e.name == "UserService"));
        assert!(class_exports.iter().any(|e| e.name == "Cache"));

        // Check for function exports
        let fn_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "fn")
            .collect();
        assert!(fn_exports.iter().any(|e| e.name == "validate_email"));
        assert!(fn_exports.iter().any(|e| e.name == "fetch_user_async"));
        assert!(fn_exports.iter().any(|e| e.name == "create_user"));

        // Check that private function is NOT exported
        assert!(!info.exports.iter().any(|e| e.name == "_internal_helper"));
    }

    #[test]
    fn test_extract_decorated_function() {
        let parser = PythonParser::new();
        let info = parser.extract_ast_info(PY_FIXTURE, Path::new("sample.py")).unwrap();

        // validate_email has @log_calls decorator
        let decorated_fn = info.exports.iter()
            .find(|e| e.name == "validate_email");
        assert!(decorated_fn.is_some());
    }

    #[test]
    fn test_extract_imports() {
        let parser = PythonParser::new();
        let info = parser.extract_ast_info(PY_FIXTURE, Path::new("sample.py")).unwrap();

        assert!(!info.imports.is_empty());

        // Check typing import (from typing import ...)
        let typing_import = info.imports.iter()
            .find(|i| i.from == "typing");
        assert!(typing_import.is_some(), "typing import not found");

        // Check dataclasses import
        let dc_import = info.imports.iter()
            .find(|i| i.from == "dataclasses");
        assert!(dc_import.is_some());

        // Check simple imports (import asyncio, import json, etc.)
        assert!(info.imports.iter().any(|i| i.from == "asyncio"));
        assert!(info.imports.iter().any(|i| i.from == "json"));
    }

    #[test]
    fn test_private_functions_excluded() {
        let parser = PythonParser::new();
        let source = r#"
def public_function():
    pass

def _private_function():
    pass

class PublicClass:
    pass

class _PrivateClass:
    pass
"#;
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "public_function"));
        assert!(!info.exports.iter().any(|e| e.name == "_private_function"));
        assert!(info.exports.iter().any(|e| e.name == "PublicClass"));
        assert!(!info.exports.iter().any(|e| e.name == "_PrivateClass"));
    }

    #[test]
    fn test_extract_toon_comments() {
        let parser = PythonParser::new();
        let comments = parser.extract_toon_comments(PY_FIXTURE).unwrap();

        assert!(comments.file_block.is_some());
        let block = comments.file_block.unwrap();
        assert!(block.purpose.is_some());
        assert!(block.purpose.unwrap().contains("Sample Python"));
    }

    #[test]
    fn test_strip_toon_comments() {
        let parser = PythonParser::new();
        let stripped = parser.strip_toon_comments(PY_FIXTURE, "sample.py.toon").unwrap();

        // Should have replaced @toon block with stub
        assert!(stripped.contains("# @toon ->"));
        assert!(stripped.contains("sample.py.toon"));
    }

    #[test]
    fn test_empty_source() {
        let parser = PythonParser::new();
        let info = parser.extract_ast_info("", Path::new("empty.py")).unwrap();

        assert!(info.exports.is_empty());
        assert!(info.imports.is_empty());
    }

    #[test]
    fn test_token_estimation() {
        let parser = PythonParser::new();
        let source = "def foo(): pass";
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        assert!(info.tokens > 0);
        assert_eq!(info.tokens, source.len() / 4);
    }

    #[test]
    fn test_default_impl() {
        let parser = PythonParser::new();
        assert_eq!(parser.language_name(), "python");
    }

    #[test]
    fn test_import_from_statement() {
        let parser = PythonParser::new();
        let source = "from os import path, getcwd\nfrom sys import argv";
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        // Check that the modules are found
        let os_import = info.imports.iter().find(|i| i.from == "os");
        assert!(os_import.is_some());

        let sys_import = info.imports.iter().find(|i| i.from == "sys");
        assert!(sys_import.is_some());
    }

    #[test]
    fn test_simple_import() {
        let parser = PythonParser::new();
        let source = "import json\nimport os.path";
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        assert!(info.imports.iter().any(|i| i.from == "json"));
    }

    #[test]
    fn test_nested_class_not_exported() {
        let parser = PythonParser::new();
        let source = r#"
class Outer:
    class Inner:
        pass
"#;
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "Outer"));
        // Inner should not be exported as it's nested
    }

    #[test]
    fn test_parse_toon_block_sections() {
        let parser = PythonParser::new();
        let content = r#"
purpose: Test purpose

when-editing:
    - !Important rule
    - Normal rule

do-not:
    - Avoid this
"#;
        let block = parser.parse_toon_block(content);

        assert!(block.purpose.is_some());
        assert!(block.when_editing.is_some());
        assert!(block.do_not.is_some());
    }

    #[test]
    fn test_dataclass() {
        let parser = PythonParser::new();
        let source = r#"
from dataclasses import dataclass

@dataclass
class Config:
    name: str
    value: int
"#;
        let info = parser.extract_ast_info(source, Path::new("test.py")).unwrap();

        let config_export = info.exports.iter().find(|e| e.name == "Config");
        assert!(config_export.is_some());
        assert_eq!(config_export.unwrap().kind, "class");
    }
}
