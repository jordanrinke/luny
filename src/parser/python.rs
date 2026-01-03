//! @dose
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
//!     - The @dose block can be in either triple-quoted docstrings or hash comments
//!     - Relative imports use dots and need special handling in import_prefix nodes
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set Python language, parse source
//!     - Extract exports: Walk AST at depth 0-1, collect function_definition and class_definition
//!     - Extract imports: Walk AST collecting import_statement and import_from_statement nodes

use crate::parser::{toon_comment, LanguageParser, ParseError};
use crate::types::{ASTInfo, ExportInfo, ExtractedComments, ImportInfo};
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

        let tokens = super::tokens::count_tokens(source);

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

        // Find @dose in docstrings (triple-quoted strings)
        let docstring_pattern = Regex::new(r#""""[\s\S]*?@dose[\s\S]*?""""#).unwrap();

        if let Some(mat) = docstring_pattern.find(source) {
            let comment = mat.as_str();
            let content = comment
                .trim_start_matches("\"\"\"")
                .trim_end_matches("\"\"\"");

            // Remove @dose marker
            let content = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim();
                    if trimmed == "@dose" {
                        ""
                    } else {
                        trimmed
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            result.file_block = Some(toon_comment::parse_toon_block(&content));
        }

        // Also check for # @dose comments
        let comment_pattern = Regex::new(r"#\s*@dose\s*\n((?:#[^\n]*\n)*)").unwrap();
        if result.file_block.is_none() {
            if let Some(caps) = comment_pattern.captures(source) {
                if let Some(block) = caps.get(1) {
                    let content = block
                        .as_str()
                        .lines()
                        .map(|line| line.trim_start_matches('#').trim())
                        .collect::<Vec<_>>()
                        .join("\n");
                    result.file_block = Some(toon_comment::parse_toon_block(&content));
                }
            }
        }

        Ok(result)
    }

    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError> {
        let mut result = source.to_string();

        // Replace docstring @dose comments
        let docstring_pattern = Regex::new(r#""""[\s\S]*?@dose[\s\S]*?""""#).unwrap();
        result = docstring_pattern
            .replace_all(&result, &format!("# @dose -> {}", toon_path))
            .to_string();

        // Remove # @dose comment blocks
        let comment_pattern = Regex::new(r"#\s*@dose[^\n]*\n(?:#[^\n]*\n)*").unwrap();
        result = comment_pattern
            .replace_all(&result, &format!("# @dose -> {}\n", toon_path))
            .to_string();

        Ok(result)
    }

    fn get_string_ranges(&self, source: &str) -> Result<Vec<(usize, usize)>, ParseError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse source".to_string()))?;

        let mut ranges = Vec::new();
        collect_string_ranges_py(&mut tree.walk(), &mut ranges);
        Ok(ranges)
    }
}

fn collect_string_ranges_py(
    cursor: &mut tree_sitter::TreeCursor,
    ranges: &mut Vec<(usize, usize)>,
) {
    loop {
        let node = cursor.node();
        if node.kind() == "string" {
            ranges.push((node.start_byte(), node.end_byte()));
        }
        if cursor.goto_first_child() {
            collect_string_ranges_py(cursor, ranges);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY_FIXTURE: &str = include_str!("../../test_fixtures/sample.py");

    #[test]
    fn test_extract_ast_info() {
        let parser = PythonParser::new();
        let info = parser
            .extract_ast_info(PY_FIXTURE, Path::new("sample.py"))
            .unwrap();

        // Exact export assertions
        let mut exports: Vec<_> = info
            .exports
            .iter()
            .map(|e| (&e.name[..], &e.kind[..]))
            .collect();
        exports.sort();
        assert_eq!(
            exports,
            vec![
                ("BaseService", "class"),
                ("Cache", "class"),
                ("UserConfig", "class"),
                ("UserService", "class"),
                ("create_user", "fn"),
                ("fetch_user_async", "fn"),
                ("log_calls", "fn"),
                ("validate_email", "fn"),
            ]
        );

        // Exact import assertions (includes nested import uuid in create_user)
        let mut imports: Vec<_> = info.imports.iter().map(|i| &i.from[..]).collect();
        imports.sort();
        assert_eq!(
            imports,
            vec![
                "abc",
                "asyncio",
                "collections",
                "dataclasses",
                "functools",
                "json",
                "os",
                "typing",
                "uuid"
            ]
        );
    }

    #[test]
    fn test_toon_comments() {
        let parser = PythonParser::new();
        let comments = parser.extract_toon_comments(PY_FIXTURE).unwrap();
        let block = comments.file_block.unwrap();

        assert_eq!(block.purpose.unwrap(), "Sample Python fixture for testing the luny Python parser. This file contains various Python constructs including classes, functions, decorators, and type hints to verify extraction works correctly.");

        let stripped = parser
            .strip_toon_comments(PY_FIXTURE, "sample.py.toon")
            .unwrap();
        assert!(stripped.starts_with("# @dose -> sample.py.toon"));
    }
}
