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

use crate::parser::{LanguageParser, ParseError};
use crate::types::{
    ASTInfo, ExportInfo, ExtractedComments, ImportInfo, ToonCommentBlock, WhenEditingItem,
};
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

    fn parse_toon_block(&self, content: &str) -> ToonCommentBlock {
        let mut block = ToonCommentBlock::default();

        let lines: Vec<&str> = content.lines().collect();
        let mut current_section: Option<&str> = None;
        let mut current_items: Vec<String> = Vec::new();

        for line in lines {
            let trimmed = line.trim().trim_start_matches('*').trim_start_matches('/').trim();

            if trimmed.is_empty() {
                continue;
            }

            if let Some(header) = self.parse_section_header(trimmed) {
                self.save_section(&mut block, current_section, &current_items);
                current_section = Some(header);
                current_items.clear();
            } else if current_section.is_none() && block.purpose.is_none() {
                // Strip "purpose:" prefix if present
                let purpose_text = if trimmed.to_lowercase().starts_with("purpose:") {
                    trimmed[8..].trim()
                } else {
                    trimmed
                };
                block.purpose = Some(purpose_text.to_string());
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

            result.file_block = Some(self.parse_toon_block(&content));
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
                    result.file_block = Some(self.parse_toon_block(&content));
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
    fn test_language_name() {
        let parser = CSharpParser::new();
        assert_eq!(parser.language_name(), "csharp");
    }

    #[test]
    fn test_file_extensions() {
        let parser = CSharpParser::new();
        let exts = parser.file_extensions();
        assert!(exts.contains(&"cs"));
        assert_eq!(exts.len(), 1);
    }

    #[test]
    fn test_extract_exports() {
        let parser = CSharpParser::new();
        let info = parser.extract_ast_info(CS_FIXTURE, Path::new("sample.cs")).unwrap();

        assert!(!info.exports.is_empty());

        // Check for class exports
        let class_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "class")
            .collect();
        assert!(class_exports.iter().any(|e| e.name == "UserService"));

        // Check for interface exports
        let interface_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "interface")
            .collect();
        assert!(interface_exports.iter().any(|e| e.name == "IRepository"));

        // Check for record exports
        let record_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "record")
            .collect();
        assert!(record_exports.iter().any(|e| e.name == "UserConfig"));

        // Check for enum exports
        let enum_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "enum")
            .collect();
        assert!(enum_exports.iter().any(|e| e.name == "UserStatus"));
    }

    #[test]
    fn test_private_class_excluded() {
        let parser = CSharpParser::new();
        let source = r#"
public class PublicClass { }
private class PrivateClass { }
internal class InternalClass { }
class DefaultClass { }
"#;
        let info = parser.extract_ast_info(source, Path::new("test.cs")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "PublicClass"));
        assert!(!info.exports.iter().any(|e| e.name == "PrivateClass"));
        assert!(!info.exports.iter().any(|e| e.name == "InternalClass"));
        assert!(!info.exports.iter().any(|e| e.name == "DefaultClass"));
    }

    #[test]
    fn test_extract_imports() {
        let parser = CSharpParser::new();
        let info = parser.extract_ast_info(CS_FIXTURE, Path::new("sample.cs")).unwrap();

        assert!(!info.imports.is_empty());

        // Check for System import
        let system_import = info.imports.iter()
            .find(|i| i.from == "System");
        assert!(system_import.is_some());

        // Check for collections import
        let collections_import = info.imports.iter()
            .find(|i| i.from.contains("Collections"));
        assert!(collections_import.is_some());
    }

    #[test]
    fn test_extract_toon_comments() {
        let parser = CSharpParser::new();
        let comments = parser.extract_toon_comments(CS_FIXTURE).unwrap();

        assert!(comments.file_block.is_some());
        let block = comments.file_block.unwrap();
        assert!(block.purpose.is_some());
        assert!(block.purpose.unwrap().contains("Sample C#"));
    }

    #[test]
    fn test_strip_toon_comments() {
        let parser = CSharpParser::new();
        let stripped = parser.strip_toon_comments(CS_FIXTURE, "sample.cs.toon").unwrap();

        assert!(stripped.contains("// @toon ->"));
        assert!(stripped.contains("sample.cs.toon"));
    }

    #[test]
    fn test_empty_source() {
        let parser = CSharpParser::new();
        let info = parser.extract_ast_info("", Path::new("empty.cs")).unwrap();

        assert!(info.exports.is_empty());
        assert!(info.imports.is_empty());
    }

    #[test]
    fn test_default_impl() {
        let parser = CSharpParser::new();
        assert_eq!(parser.language_name(), "csharp");
    }

    #[test]
    fn test_public_static_class() {
        let parser = CSharpParser::new();
        let source = r#"
public static class StringExtensions
{
    public static string ToCamelCase(this string str) { return str; }
}
"#;
        let info = parser.extract_ast_info(source, Path::new("test.cs")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "StringExtensions"));
    }

    #[test]
    fn test_abstract_class() {
        let parser = CSharpParser::new();
        let source = r#"
public abstract class BaseEntity
{
    public string Id { get; set; }
}
"#;
        let info = parser.extract_ast_info(source, Path::new("test.cs")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "BaseEntity"));
    }

    #[test]
    fn test_struct_export() {
        let parser = CSharpParser::new();
        let source = r#"
public struct Point
{
    public int X;
    public int Y;
}
"#;
        let info = parser.extract_ast_info(source, Path::new("test.cs")).unwrap();

        let point_export = info.exports.iter().find(|e| e.name == "Point");
        assert!(point_export.is_some());
        assert_eq!(point_export.unwrap().kind, "struct");
    }

    #[test]
    fn test_parse_toon_block_sections() {
        let parser = CSharpParser::new();
        let content = r#"
purpose: Test purpose

when-editing:
    - !Important rule

do-not:
    - Avoid this
"#;
        let block = parser.parse_toon_block(content);

        assert!(block.purpose.is_some());
        assert!(block.when_editing.is_some());
        assert!(block.do_not.is_some());
    }
}
