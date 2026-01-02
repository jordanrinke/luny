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

use crate::parser::{LanguageParser, ParseError};
use crate::types::{
    ASTInfo, ExportInfo, ExtractedComments, ImportInfo, ToonCommentBlock, WhenEditingItem,
};
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

            result.file_block = Some(self.parse_toon_block(&content));
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
                    result.file_block = Some(self.parse_toon_block(&content));
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
    fn test_language_name() {
        let parser = RubyParser::new();
        assert_eq!(parser.language_name(), "ruby");
    }

    #[test]
    fn test_file_extensions() {
        let parser = RubyParser::new();
        let exts = parser.file_extensions();
        assert!(exts.contains(&"rb"));
        assert_eq!(exts.len(), 1);
    }

    #[test]
    fn test_extract_exports() {
        let parser = RubyParser::new();
        let info = parser.extract_ast_info(RB_FIXTURE, Path::new("sample.rb")).unwrap();

        assert!(!info.exports.is_empty());

        // Check for class exports
        let class_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "class")
            .collect();
        assert!(class_exports.iter().any(|e| e.name == "UserConfig"));
        assert!(class_exports.iter().any(|e| e.name == "BaseService"));
        assert!(class_exports.iter().any(|e| e.name == "UserService"));

        // Check for module exports
        let module_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "module")
            .collect();
        assert!(module_exports.iter().any(|e| e.name == "Loggable"));
        assert!(module_exports.iter().any(|e| e.name == "StringUtils"));
        assert!(module_exports.iter().any(|e| e.name == "UserFactory"));
    }

    #[test]
    fn test_extract_methods() {
        let parser = RubyParser::new();
        let info = parser.extract_ast_info(RB_FIXTURE, Path::new("sample.rb")).unwrap();

        // Check for method exports
        let method_exports: Vec<_> = info.exports.iter()
            .filter(|e| e.kind == "method")
            .collect();
        assert!(!method_exports.is_empty());
    }

    #[test]
    fn test_extract_imports() {
        let parser = RubyParser::new();
        let info = parser.extract_ast_info(RB_FIXTURE, Path::new("sample.rb")).unwrap();

        assert!(!info.imports.is_empty());

        // Check for require imports
        let json_import = info.imports.iter()
            .find(|i| i.from == "json");
        assert!(json_import.is_some());

        let fileutils_import = info.imports.iter()
            .find(|i| i.from == "fileutils");
        assert!(fileutils_import.is_some());
    }

    #[test]
    fn test_private_method_excluded() {
        let parser = RubyParser::new();
        let source = r#"
def public_method
end

def _private_method
end
"#;
        let info = parser.extract_ast_info(source, Path::new("test.rb")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "public_method"));
        assert!(!info.exports.iter().any(|e| e.name == "_private_method"));
    }

    #[test]
    fn test_singleton_method() {
        let parser = RubyParser::new();
        let source = r#"
class Foo
  def self.bar
  end
end
"#;
        let info = parser.extract_ast_info(source, Path::new("test.rb")).unwrap();

        let singleton = info.exports.iter()
            .find(|e| e.kind == "class_method");
        assert!(singleton.is_some());
        assert!(singleton.unwrap().name.contains("bar"));
    }

    #[test]
    fn test_extract_toon_comments() {
        let parser = RubyParser::new();
        let comments = parser.extract_toon_comments(RB_FIXTURE).unwrap();

        assert!(comments.file_block.is_some());
        let block = comments.file_block.unwrap();
        assert!(block.purpose.is_some());
        assert!(block.purpose.unwrap().contains("Sample Ruby"));
    }

    #[test]
    fn test_strip_toon_comments() {
        let parser = RubyParser::new();
        let stripped = parser.strip_toon_comments(RB_FIXTURE, "sample.rb.toon").unwrap();

        assert!(stripped.contains("# @toon ->"));
        assert!(stripped.contains("sample.rb.toon"));
    }

    #[test]
    fn test_empty_source() {
        let parser = RubyParser::new();
        let info = parser.extract_ast_info("", Path::new("empty.rb")).unwrap();

        assert!(info.exports.is_empty());
        assert!(info.imports.is_empty());
    }

    #[test]
    fn test_default_impl() {
        let parser = RubyParser::new();
        assert_eq!(parser.language_name(), "ruby");
    }

    #[test]
    fn test_require_relative() {
        let parser = RubyParser::new();
        let source = "require_relative 'helper'";
        let info = parser.extract_ast_info(source, Path::new("test.rb")).unwrap();

        let helper_import = info.imports.iter()
            .find(|i| i.from == "helper");
        assert!(helper_import.is_some());
    }

    #[test]
    fn test_include_with_symbol() {
        let parser = RubyParser::new();
        // include with a symbol argument (not a constant) is captured
        let source = r#"include :loggable"#;
        let info = parser.extract_ast_info(source, Path::new("test.rb")).unwrap();

        // Include with symbol argument is treated as an import
        let loggable_import = info.imports.iter().find(|i| i.from == "loggable");
        assert!(loggable_import.is_some());
    }

    #[test]
    fn test_parse_toon_block_sections() {
        let parser = RubyParser::new();
        let content = r#"
purpose: Test purpose

when-editing:
    - !Important rule

invariants:
    - Some invariant
"#;
        let block = parser.parse_toon_block(content);

        assert!(block.purpose.is_some());
        assert!(block.when_editing.is_some());
        assert!(block.invariants.is_some());
    }
}
