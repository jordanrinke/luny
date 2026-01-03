//! @toon
//! purpose: This module parses C# source files to extract public classes, interfaces,
//!     structs, enums, records, and methods. It uses tree-sitter for robust parsing
//!     and respects C# visibility modifiers.
//!
//! when-editing:
//!     - !Only items with the "public" modifier are considered exports
//!     - !Each declaration type (class, interface, struct, enum, record) needs its own handling
//!     - The is_public() helper checks for modifier nodes containing "public"
//!
//! invariants:
//!     - Private, protected, and internal members are never exported
//!     - Using directives are captured as imports
//!     - The parser handles all C# 9+ features including records
//!
//! do-not:
//!     - Never export private or internal members
//!     - Never assume default visibility is public (it's internal for classes)
//!
//! gotchas:
//!     - C# uses explicit visibility modifiers; default is not public
//!     - Records with primary constructors have special AST structure
//!     - The @toon block can be in block comments or triple-slash XML comments
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set C# language, parse source
//!     - Extract exports: Walk AST finding declarations with public modifier
//!     - Extract imports: Walk AST collecting using_directive nodes

use crate::parser::{toon_comment, LanguageParser, ParseError};
use crate::types::{ASTInfo, ExportInfo, ExtractedComments, ImportInfo};
use regex::Regex;
use std::path::Path;
use tree_sitter::Parser;

/// Parser for C# files
#[derive(Clone)]
pub struct CSharpParser;

impl CSharpParser {
    pub fn new() -> Self {
        Self
    }

    fn create_parser(&self) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;
        Ok(parser)
    }

    fn extract_exports(&self, root: tree_sitter::Node, source: &str) -> Vec<ExportInfo> {
        let mut exports = Vec::new();
        let mut cursor = root.walk();

        self.visit_exports(&mut cursor, source, &mut exports);
        exports
    }

    fn visit_exports(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        exports: &mut Vec<ExportInfo>,
    ) {
        loop {
            let node = cursor.node();

            match node.kind() {
                "class_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "class".to_string(),
                            });
                        }
                    }
                }
                "interface_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "interface".to_string(),
                            });
                        }
                    }
                }
                "struct_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "struct".to_string(),
                            });
                        }
                    }
                }
                "enum_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "enum".to_string(),
                            });
                        }
                    }
                }
                "method_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "method".to_string(),
                            });
                        }
                    }
                }
                "record_declaration" => {
                    if self.is_public(&node, source) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "record".to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.visit_exports(cursor, source, exports);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn is_public(&self, node: &tree_sitter::Node, source: &str) -> bool {
        // Check for public modifier
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "modifier" {
                    let text = self.node_text(child, source);
                    if text == "public" {
                        return true;
                    }
                }
            }
        }
        false
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

            if node.kind() == "using_directive" {
                // using System.Collections.Generic;
                let text = self.node_text(node, source);
                let namespace = text
                    .trim_start_matches("using")
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                if !namespace.is_empty() {
                    imports.push(ImportInfo {
                        from: namespace.clone(),
                        items: vec![namespace],
                    });
                }
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

impl Default for CSharpParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for CSharpParser {
    fn language_name(&self) -> &'static str {
        "csharp"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["cs"]
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

        // Find @toon in /** */ block comments
        let block_pattern = Regex::new(r"/\*\*[\s\S]*?@toon[\s\S]*?\*/").unwrap();

        if let Some(mat) = block_pattern.find(source) {
            let comment = mat.as_str();
            let content = comment.trim_start_matches("/**").trim_end_matches("*/");

            let content = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim().trim_start_matches('*').trim();
                    if trimmed == "@toon" {
                        ""
                    } else {
                        trimmed
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            result.file_block = Some(toon_comment::parse_toon_block(&content));
        }

        // Also check for /// @toon XML doc comments
        if result.file_block.is_none() {
            let doc_pattern = Regex::new(r"///\s*@toon\s*\n((?:///[^\n]*\n)*)").unwrap();
            if let Some(caps) = doc_pattern.captures(source) {
                if let Some(block) = caps.get(1) {
                    let content = block
                        .as_str()
                        .lines()
                        .map(|line| line.trim_start_matches('/').trim())
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

        // Replace /** */ @toon blocks
        let block_pattern = Regex::new(r"/\*\*[\s\S]*?@toon[\s\S]*?\*/").unwrap();
        result = block_pattern
            .replace_all(&result, &format!("// @toon -> {}", toon_path))
            .to_string();

        // Remove /// @toon comment blocks
        let doc_pattern = Regex::new(r"///\s*@toon[^\n]*\n(?:///[^\n]*\n)*").unwrap();
        result = doc_pattern
            .replace_all(&result, &format!("// @toon -> {}\n", toon_path))
            .to_string();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CS_FIXTURE: &str = include_str!("../../test_fixtures/sample.cs");

    #[test]
    fn test_extract_ast_info() {
        let parser = CSharpParser::new();
        let info = parser
            .extract_ast_info(CS_FIXTURE, Path::new("sample.cs"))
            .unwrap();

        let mut exports: Vec<_> = info
            .exports
            .iter()
            .map(|e| (&e.name[..], &e.kind[..]))
            .collect();
        exports.sort();
        assert_eq!(
            exports,
            vec![
                ("BaseEntity", "class"),
                ("Constants", "class"),
                ("DeleteAsync", "method"),
                ("Get", "method"),
                ("GetAllAsync", "method"),
                ("GetByIdAsync", "method"),
                ("IRepository", "interface"),
                ("SaveAsync", "method"),
                ("Set", "method"),
                ("StringExtensions", "class"),
                ("ToCamelCase", "method"),
                ("ToPascalCase", "method"),
                ("Truncate", "method"),
                ("UserConfig", "record"),
                ("UserService", "class"),
                ("UserStatus", "enum"),
            ]
        );

        let mut imports: Vec<_> = info.imports.iter().map(|i| &i.from[..]).collect();
        imports.sort();
        assert_eq!(
            imports,
            vec![
                "System",
                "System.Collections.Generic",
                "System.IO",
                "System.Linq",
                "System.Text.Json",
                "System.Threading.Tasks",
            ]
        );
    }

    #[test]
    fn test_toon_comments() {
        let parser = CSharpParser::new();
        let comments = parser.extract_toon_comments(CS_FIXTURE).unwrap();
        let block = comments.file_block.unwrap();
        assert_eq!(block.purpose.unwrap(), "Sample C# fixture for testing the luny C# parser. This file contains various C# constructs including classes, interfaces, records, and async patterns to verify extraction works correctly.");

        let stripped = parser
            .strip_toon_comments(CS_FIXTURE, "sample.cs.toon")
            .unwrap();
        assert!(stripped.starts_with("// @toon -> sample.cs.toon"));
    }
}
