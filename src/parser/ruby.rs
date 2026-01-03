//! @toon
//! purpose: This module parses Ruby source files to extract classes, modules, methods,
//!     and require/include statements. It uses tree-sitter for robust parsing and follows
//!     Ruby conventions for determining which items are public exports.
//!
//! when-editing:
//!     - !Methods starting with underscore are considered private by convention
//!     - !Singleton methods (self.method) are extracted as class_method kind
//!     - Modules and classes are always exported
//!
//! invariants:
//!     - Private methods (underscore prefix) are excluded from exports
//!     - Require, require_relative, include, and extend statements are all captured as imports
//!     - The parser handles both instance methods and class (singleton) methods
//!
//! do-not:
//!     - Never treat instance variables as exports
//!     - Never extract methods defined inside other methods
//!
//! gotchas:
//!     - Ruby has no explicit export mechanism; all defined items are potentially public
//!     - The @toon block can be in =begin/=end blocks or hash comment blocks
//!     - Singleton methods have "self." prefix in the exported name
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set Ruby language, parse source
//!     - Extract exports: Walk AST collecting method, singleton_method, class, and module nodes
//!     - Extract imports: Walk AST finding call nodes for require/include/extend

use crate::parser::{toon_comment, LanguageParser, ParseError};
use crate::types::{ASTInfo, ExportInfo, ExtractedComments, ImportInfo};
use regex::Regex;
use std::path::Path;
use tree_sitter::Parser;

/// Parser for Ruby files
#[derive(Clone)]
pub struct RubyParser;

impl RubyParser {
    pub fn new() -> Self {
        Self
    }

    fn create_parser(&self) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
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
                "method" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        // Skip private methods (by convention, methods starting with _)
                        if !name.starts_with('_') {
                            exports.push(ExportInfo {
                                name,
                                kind: "method".to_string(),
                            });
                        }
                    }
                }
                "singleton_method" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        exports.push(ExportInfo {
                            name: format!("self.{}", name),
                            kind: "class_method".to_string(),
                        });
                    }
                }
                "class" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        exports.push(ExportInfo {
                            name,
                            kind: "class".to_string(),
                        });
                    }
                }
                "module" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        exports.push(ExportInfo {
                            name,
                            kind: "module".to_string(),
                        });
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

            if node.kind() == "call" {
                // Look for require, require_relative, include, extend
                if let Some(method) = node.child_by_field_name("method") {
                    let method_name = self.node_text(method, source);
                    if method_name == "require"
                        || method_name == "require_relative"
                        || method_name == "include"
                        || method_name == "extend"
                    {
                        if let Some(args) = node.child_by_field_name("arguments") {
                            for i in 0..args.child_count() {
                                if let Some(arg) = args.child(i) {
                                    if arg.kind() == "string" || arg.kind() == "simple_symbol" {
                                        let value = self
                                            .node_text(arg, source)
                                            .trim_matches('"')
                                            .trim_matches('\'')
                                            .trim_start_matches(':')
                                            .to_string();
                                        imports.push(ImportInfo {
                                            from: value.clone(),
                                            items: vec![value],
                                        });
                                    }
                                }
                            }
                        }
                    }
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

impl Default for RubyParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for RubyParser {
    fn language_name(&self) -> &'static str {
        "ruby"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["rb"]
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

        // Find @toon in =begin/=end blocks
        let block_pattern = Regex::new(r"=begin[\s\S]*?@toon[\s\S]*?=end").unwrap();

        if let Some(mat) = block_pattern.find(source) {
            let comment = mat.as_str();
            let content = comment.trim_start_matches("=begin").trim_end_matches("=end");

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

            result.file_block = Some(toon_comment::parse_toon_block(&content));
        }

        // Also check for # @toon comments
        if result.file_block.is_none() {
            let comment_pattern = Regex::new(r"#\s*@toon\s*\n((?:#[^\n]*\n)*)").unwrap();
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

        // Replace =begin/=end @toon blocks
        let block_pattern = Regex::new(r"=begin[\s\S]*?@toon[\s\S]*?=end").unwrap();
        result = block_pattern
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

    const RB_FIXTURE: &str = include_str!("../../test_fixtures/sample.rb");

    #[test]
    fn test_extract_ast_info() {
        let parser = RubyParser::new();
        let info = parser.extract_ast_info(RB_FIXTURE, Path::new("sample.rb")).unwrap();

        // Extract just classes and modules for clear assertion
        let mut classes: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "class" || e.kind == "module")
            .map(|e| (&e.name[..], &e.kind[..]))
            .collect();
        classes.sort();
        assert_eq!(classes, vec![
            ("BaseService", "class"),
            ("Loggable", "module"),
            ("StringUtils", "module"),
            ("UserConfig", "class"),
            ("UserFactory", "module"),
            ("UserService", "class"),
        ]);

        let mut imports: Vec<_> = info.imports.iter().map(|i| &i.from[..]).collect();
        imports.sort();
        assert_eq!(imports, vec!["fileutils", "helper", "json"]);
    }

    #[test]
    fn test_toon_comments() {
        let parser = RubyParser::new();
        let comments = parser.extract_toon_comments(RB_FIXTURE).unwrap();
        let block = comments.file_block.unwrap();
        assert_eq!(block.purpose.unwrap(), "Sample Ruby fixture for testing the luny Ruby parser. This file contains various Ruby constructs including classes, modules, methods, and metaprogramming patterns to verify extraction works correctly.");

        let stripped = parser.strip_toon_comments(RB_FIXTURE, "sample.rb.toon").unwrap();
        assert!(stripped.starts_with("# @toon -> sample.rb.toon"));
    }
}
