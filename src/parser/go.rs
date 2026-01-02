//! @toon
//! purpose: This module parses Go source files to extract exported types, functions,
//!     methods, constants, and variables. It uses tree-sitter for robust parsing and
//!     follows Go's capitalization convention for determining exports.
//!
//! when-editing:
//!     - !Go exports are determined by first letter capitalization, not keywords
//!     - !Methods have receivers that provide type context in the kind field
//!     - Import aliases need special handling via import_spec's name field
//!
//! invariants:
//!     - Only identifiers starting with uppercase letters are considered exports
//!     - Method signatures include their receiver type in parentheses
//!     - Import paths are trimmed of quotes and the package name is the last path segment
//!
//! do-not:
//!     - Never export identifiers starting with lowercase letters
//!     - Never assume all imports have aliases
//!
//! gotchas:
//!     - Go has no explicit export keyword; capitalization determines visibility
//!     - The @toon block uses /* */ block comments, not // single-line comments
//!     - Receiver types may be pointer types (prefixed with *)
//!     - Const and var declarations can declare multiple identifiers at once
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set Go language, parse source
//!     - Extract exports: Walk AST finding function_declaration, method_declaration,
//!       type_declaration, const_declaration, var_declaration nodes with uppercase names
//!     - Extract imports: Walk AST finding import_declaration with import_spec children

use crate::parser::{LanguageParser, ParseError};
use crate::types::{
    ASTInfo, CallInfo, ExportInfo, ExtractedComments, ImportInfo, SignatureInfo,
    ToonCommentBlock, WhenEditingItem,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Parser for Go files
#[derive(Clone)]
pub struct GoParser;

impl GoParser {
    pub fn new() -> Self {
        Self
    }

    fn create_parser(&self) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;
        Ok(parser)
    }

    fn node_text(&self, node: Node, source: &str) -> String {
        source[node.start_byte()..node.end_byte()].to_string()
    }

    /// Check if identifier is exported (starts with uppercase)
    fn is_exported(&self, name: &str) -> bool {
        name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
    }

    fn extract_exports(&self, root: Node, source: &str) -> Vec<ExportInfo> {
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
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        if self.is_exported(&name) {
                            exports.push(ExportInfo {
                                name,
                                kind: "fn".to_string(),
                            });
                        }
                    }
                }
                "method_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        if self.is_exported(&name) {
                            // Get receiver type for context
                            let receiver = node.child_by_field_name("receiver")
                                .map(|r| self.extract_receiver_type(r, source))
                                .unwrap_or_default();

                            let kind = if receiver.is_empty() {
                                "method".to_string()
                            } else {
                                format!("method({})", receiver)
                            };

                            exports.push(ExportInfo { name, kind });
                        }
                    }
                }
                "type_declaration" => {
                    // Handle type Foo struct/interface/alias
                    for i in 0..node.child_count() {
                        if let Some(spec) = node.child(i) {
                            if spec.kind() == "type_spec" {
                                if let Some(name_node) = spec.child_by_field_name("name") {
                                    let name = self.node_text(name_node, source);
                                    if self.is_exported(&name) {
                                        let kind = self.infer_type_kind(spec, source);
                                        exports.push(ExportInfo { name, kind });
                                    }
                                }
                            }
                        }
                    }
                }
                "const_declaration" | "var_declaration" => {
                    // Handle const/var blocks
                    for i in 0..node.child_count() {
                        if let Some(spec) = node.child(i) {
                            if spec.kind() == "const_spec" || spec.kind() == "var_spec" {
                                // Can have multiple names: const A, B = 1, 2
                                for j in 0..spec.child_count() {
                                    if let Some(child) = spec.child(j) {
                                        if child.kind() == "identifier" {
                                            let name = self.node_text(child, source);
                                            if self.is_exported(&name) {
                                                let kind = if node.kind() == "const_declaration" {
                                                    "const".to_string()
                                                } else {
                                                    "var".to_string()
                                                };
                                                exports.push(ExportInfo { name, kind });
                                            }
                                        }
                                    }
                                }
                            }
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

    fn extract_receiver_type(&self, receiver: Node, source: &str) -> String {
        // Extract type from (r *Receiver) or (r Receiver)
        for i in 0..receiver.child_count() {
            if let Some(child) = receiver.child(i) {
                if child.kind() == "parameter_declaration" {
                    if let Some(type_node) = child.child_by_field_name("type") {
                        let type_text = self.node_text(type_node, source);
                        return type_text.trim_start_matches('*').to_string();
                    }
                }
            }
        }
        String::new()
    }

    fn infer_type_kind(&self, spec: Node, _source: &str) -> String {
        if let Some(type_node) = spec.child_by_field_name("type") {
            match type_node.kind() {
                "struct_type" => "struct".to_string(),
                "interface_type" => "interface".to_string(),
                _ => "type".to_string(),
            }
        } else {
            "type".to_string()
        }
    }

    fn extract_imports(&self, root: Node, source: &str) -> Vec<ImportInfo> {
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

            if node.kind() == "import_declaration" {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        match child.kind() {
                            "import_spec" => {
                                if let Some(path_node) = child.child_by_field_name("path") {
                                    let path = self.node_text(path_node, source)
                                        .trim_matches('"')
                                        .to_string();

                                    // Get alias if present
                                    let alias = child.child_by_field_name("name")
                                        .map(|n| self.node_text(n, source));

                                    let items = if let Some(a) = alias {
                                        vec![a]
                                    } else {
                                        // Use last part of path as package name
                                        let pkg = path.rsplit('/').next().unwrap_or(&path);
                                        vec![pkg.to_string()]
                                    };

                                    imports.push(ImportInfo { from: path, items });
                                }
                            }
                            "import_spec_list" => {
                                // Multiple imports in ( )
                                for j in 0..child.child_count() {
                                    if let Some(spec) = child.child(j) {
                                        if spec.kind() == "import_spec" {
                                            if let Some(path_node) = spec.child_by_field_name("path") {
                                                let path = self.node_text(path_node, source)
                                                    .trim_matches('"')
                                                    .to_string();

                                                let alias = spec.child_by_field_name("name")
                                                    .map(|n| self.node_text(n, source));

                                                let items = if let Some(a) = alias {
                                                    vec![a]
                                                } else {
                                                    let pkg = path.rsplit('/').next().unwrap_or(&path);
                                                    vec![pkg.to_string()]
                                                };

                                                imports.push(ImportInfo { from: path, items });
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
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

    fn extract_calls(&self, root: Node, source: &str, imports: &[ImportInfo]) -> Vec<CallInfo> {
        let mut calls = Vec::new();
        let import_map: HashMap<&str, &str> = imports
            .iter()
            .flat_map(|i| i.items.iter().map(move |item| (item.as_str(), i.from.as_str())))
            .collect();

        let mut cursor = root.walk();
        self.visit_calls(&mut cursor, source, &import_map, &mut calls);

        // Deduplicate
        let mut seen = HashSet::new();
        calls.retain(|c| seen.insert((c.target.clone(), c.method.clone())));
        calls
    }

    fn visit_calls(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        import_map: &HashMap<&str, &str>,
        calls: &mut Vec<CallInfo>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                if let Some(func) = node.child_by_field_name("function") {
                    if func.kind() == "selector_expression" {
                        // pkg.Function() or obj.Method()
                        if let Some(operand) = func.child_by_field_name("operand") {
                            if let Some(field) = func.child_by_field_name("field") {
                                let pkg = self.node_text(operand, source);
                                let method = self.node_text(field, source);

                                if let Some(&target) = import_map.get(pkg.as_str()) {
                                    calls.push(CallInfo {
                                        target: target.to_string(),
                                        method,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            if cursor.goto_first_child() {
                self.visit_calls(cursor, source, import_map, calls);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_signatures(&self, root: Node, source: &str, exports: &[ExportInfo]) -> Vec<SignatureInfo> {
        let export_names: HashSet<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        let mut signatures = Vec::new();
        let mut cursor = root.walk();

        self.visit_signatures(&mut cursor, source, &export_names, &mut signatures);
        signatures
    }

    fn visit_signatures(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        export_names: &HashSet<&str>,
        signatures: &mut Vec<SignatureInfo>,
    ) {
        loop {
            let node = cursor.node();

            match node.kind() {
                "function_declaration" => {
                    if let Some(sig) = self.extract_function_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "method_declaration" => {
                    if let Some(sig) = self.extract_method_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "type_declaration" => {
                    for i in 0..node.child_count() {
                        if let Some(spec) = node.child(i) {
                            if spec.kind() == "type_spec" {
                                if let Some(sig) = self.extract_type_signature(spec, source, export_names) {
                                    signatures.push(sig);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                self.visit_signatures(cursor, source, export_names, signatures);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn extract_function_signature(
        &self,
        node: Node,
        source: &str,
        export_names: &HashSet<&str>,
    ) -> Option<SignatureInfo> {
        let name_node = node.child_by_field_name("name")?;
        let name = self.node_text(name_node, source);

        if !export_names.contains(name.as_str()) {
            return None;
        }

        let params = node.child_by_field_name("parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_else(|| "()".to_string());

        let result = node.child_by_field_name("result")
            .map(|r| format!(" {}", self.node_text(r, source)))
            .unwrap_or_default();

        let signature = format!("{}{}", params, result);

        Some(SignatureInfo {
            name,
            kind: "fn".to_string(),
            signature,
        })
    }

    fn extract_method_signature(
        &self,
        node: Node,
        source: &str,
        export_names: &HashSet<&str>,
    ) -> Option<SignatureInfo> {
        let name_node = node.child_by_field_name("name")?;
        let name = self.node_text(name_node, source);

        if !export_names.contains(name.as_str()) {
            return None;
        }

        let receiver = node.child_by_field_name("receiver")
            .map(|r| self.node_text(r, source))
            .unwrap_or_default();

        let params = node.child_by_field_name("parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_else(|| "()".to_string());

        let result = node.child_by_field_name("result")
            .map(|r| format!(" {}", self.node_text(r, source)))
            .unwrap_or_default();

        let signature = format!("{} {}{}", receiver, params, result);

        Some(SignatureInfo {
            name,
            kind: "method".to_string(),
            signature,
        })
    }

    fn extract_type_signature(
        &self,
        spec: Node,
        source: &str,
        export_names: &HashSet<&str>,
    ) -> Option<SignatureInfo> {
        let name_node = spec.child_by_field_name("name")?;
        let name = self.node_text(name_node, source);

        if !export_names.contains(name.as_str()) {
            return None;
        }

        let type_node = spec.child_by_field_name("type")?;
        let kind = match type_node.kind() {
            "struct_type" => "struct",
            "interface_type" => "interface",
            _ => "type",
        };

        let signature = self.summarize_type(type_node, source);

        Some(SignatureInfo {
            name,
            kind: kind.to_string(),
            signature,
        })
    }

    fn summarize_type(&self, type_node: Node, source: &str) -> String {
        match type_node.kind() {
            "struct_type" => {
                let mut fields = Vec::new();
                if let Some(body) = type_node.child_by_field_name("body") {
                    for i in 0..body.child_count() {
                        if let Some(field) = body.child(i) {
                            if field.kind() == "field_declaration" {
                                let field_text = self.node_text(field, source)
                                    .split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                fields.push(field_text);
                                if fields.len() >= 5 {
                                    fields.push("...".to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
                if fields.is_empty() {
                    "struct{}".to_string()
                } else {
                    format!("struct {{ {} }}", fields.join("; "))
                }
            }
            "interface_type" => {
                let mut methods = Vec::new();
                for i in 0..type_node.child_count() {
                    if let Some(child) = type_node.child(i) {
                        if child.kind() == "method_spec" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = self.node_text(name_node, source);
                                let params = child.child_by_field_name("parameters")
                                    .map(|p| self.node_text(p, source))
                                    .unwrap_or_else(|| "()".to_string());
                                methods.push(format!("{}{}", name, params));
                                if methods.len() >= 5 {
                                    methods.push("...".to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
                if methods.is_empty() {
                    "interface{}".to_string()
                } else {
                    format!("interface {{ {} }}", methods.join("; "))
                }
            }
            _ => self.node_text(type_node, source),
        }
    }

    fn parse_toon_block(&self, content: &str) -> ToonCommentBlock {
        let mut block = ToonCommentBlock::default();
        let lines: Vec<&str> = content.lines().collect();
        let mut current_section: Option<&str> = None;
        let mut current_items: Vec<String> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim().trim_start_matches('*').trim();

            if trimmed.is_empty() {
                continue;
            }

            // Check for section headers
            if let Some((key, value)) = trimmed.split_once(':') {
                // Save previous section
                if let Some(section) = current_section {
                    self.save_section(&mut block, section, &current_items);
                }

                current_section = Some(key.trim());
                current_items = if value.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![value.trim().to_string()]
                };
            } else if current_section.is_some() {
                current_items.push(trimmed.to_string());
            } else if i == 0 {
                // First line without section header is purpose - strip "purpose:" prefix if present
                let purpose_text = if trimmed.to_lowercase().starts_with("purpose:") {
                    trimmed[8..].trim()
                } else {
                    trimmed
                };
                block.purpose = Some(purpose_text.to_string());
            }
        }

        // Save last section
        if let Some(section) = current_section {
            self.save_section(&mut block, section, &current_items);
        }

        block
    }

    fn save_section(&self, block: &mut ToonCommentBlock, section: &str, items: &[String]) {
        let combined = items.join(" ").trim().to_string();
        if combined.is_empty() {
            return;
        }

        let list: Vec<String> = combined.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

        match section.to_lowercase().as_str() {
            "purpose" => block.purpose = Some(combined),
            "when-editing" => {
                block.when_editing = Some(
                    list.into_iter()
                        .map(|s| {
                            let important = s.starts_with('!');
                            let text = if important { s[1..].trim().to_string() } else { s };
                            WhenEditingItem { text, important }
                        })
                        .collect(),
                );
            }
            "do-not" => block.do_not = Some(list),
            "invariants" | "invariant" => block.invariants = Some(list),
            "error-handling" => block.error_handling = Some(list),
            "constraints" | "constraint" => block.constraints = Some(list),
            "gotchas" | "gotcha" => block.gotchas = Some(list),
            "flows" | "flow" => block.flows = Some(list),
            "testing" => block.testing = Some(list),
            "common-mistakes" => block.common_mistakes = Some(list),
            "change-impacts" => block.change_impacts = Some(list),
            "related" => block.related = Some(list),
            _ => {}
        }
    }
}

impl LanguageParser for GoParser {
    fn language_name(&self) -> &'static str {
        "go"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["go"]
    }

    fn extract_ast_info(&self, source: &str, _file_path: &Path) -> Result<ASTInfo, ParseError> {
        let mut parser = self.create_parser()?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse Go source".to_string()))?;

        let root = tree.root_node();
        let tokens = source.len() / 4;

        let exports = self.extract_exports(root, source);
        let imports = self.extract_imports(root, source);
        let calls = self.extract_calls(root, source, &imports);
        let signatures = self.extract_signatures(root, source, &exports);

        Ok(ASTInfo {
            tokens,
            exports,
            imports,
            calls,
            signatures,
        })
    }

    fn extract_toon_comments(&self, source: &str) -> Result<ExtractedComments, ParseError> {
        let mut comments = ExtractedComments::default();

        // Match /* @toon ... */ block comments
        let block_pattern = Regex::new(r"/\*\s*@toon\b([\s\S]*?)\*/").unwrap();

        if let Some(captures) = block_pattern.captures(source) {
            if let Some(content) = captures.get(1) {
                comments.file_block = Some(self.parse_toon_block(content.as_str()));
            }
        }

        // Match // @toon: key: value single-line comments
        let single_pattern = Regex::new(r"//\s*@toon:\s*(\w+):\s*(.+)").unwrap();
        for cap in single_pattern.captures_iter(source) {
            if let (Some(key), Some(value)) = (cap.get(1), cap.get(2)) {
                let block = comments.file_block.get_or_insert_with(ToonCommentBlock::default);
                self.save_section(block, key.as_str(), &[value.as_str().to_string()]);
            }
        }

        Ok(comments)
    }

    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError> {
        let mut result = source.to_string();

        // Replace block @toon comments with reference
        let block_pattern = Regex::new(r"/\*\s*@toon\b[\s\S]*?\*/\s*\n?").unwrap();
        result = block_pattern
            .replace_all(&result, &format!("// @toon -> {}\n", toon_path))
            .to_string();

        // Remove single-line @toon comments
        let single_pattern = Regex::new(r"//\s*@toon:[^\n]*\n?").unwrap();
        result = single_pattern.replace_all(&result, "").to_string();

        Ok(result)
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GO_FIXTURE: &str = include_str!("../../test_fixtures/sample.go");

    #[test]
    fn test_language_name() {
        let parser = GoParser::new();
        assert_eq!(parser.language_name(), "go");
    }

    #[test]
    fn test_file_extensions() {
        let parser = GoParser::new();
        let exts = parser.file_extensions();
        assert!(exts.contains(&"go"));
        assert_eq!(exts.len(), 1);
    }

    #[test]
    fn test_is_exported() {
        let parser = GoParser::new();
        assert!(parser.is_exported("Foo"));
        assert!(parser.is_exported("UserService"));
        assert!(!parser.is_exported("foo"));
        assert!(!parser.is_exported("privateFunc"));
        assert!(!parser.is_exported("_internal"));
    }

    #[test]
    fn test_extract_exports() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        assert!(!info.exports.is_empty());

        // Check for exported functions
        assert!(info.exports.iter().any(|e| e.name == "NewUserService" && e.kind == "fn"));
        assert!(info.exports.iter().any(|e| e.name == "CreateUser" && e.kind == "fn"));
        assert!(info.exports.iter().any(|e| e.name == "ValidateEmail" && e.kind == "fn"));
        assert!(info.exports.iter().any(|e| e.name == "Filter" && e.kind == "fn"));

        // Check for exported structs
        assert!(info.exports.iter().any(|e| e.name == "UserConfig" && e.kind == "struct"));
        assert!(info.exports.iter().any(|e| e.name == "UserService" && e.kind == "struct"));
        assert!(info.exports.iter().any(|e| e.name == "Cache" && e.kind == "struct"));

        // Check for exported interfaces
        assert!(info.exports.iter().any(|e| e.name == "Repository" && e.kind == "interface"));
        assert!(info.exports.iter().any(|e| e.name == "Logger" && e.kind == "interface"));

        // Check for exported constants
        assert!(info.exports.iter().any(|e| e.name == "Version" && e.kind == "const"));
        assert!(info.exports.iter().any(|e| e.name == "DefaultTimeout" && e.kind == "const"));
        assert!(info.exports.iter().any(|e| e.name == "MaxRetries" && e.kind == "const"));
    }

    #[test]
    fn test_unexported_items_excluded() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        // Unexported items should NOT be in exports
        assert!(!info.exports.iter().any(|e| e.name == "generateID"));
        assert!(!info.exports.iter().any(|e| e.name == "validate"));
        assert!(!info.exports.iter().any(|e| e.name == "internalBufferSize"));
    }

    #[test]
    fn test_extract_imports() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        assert!(!info.imports.is_empty());

        // Check for standard library imports
        assert!(info.imports.iter().any(|i| i.from == "encoding/json"));
        assert!(info.imports.iter().any(|i| i.from == "fmt"));
        assert!(info.imports.iter().any(|i| i.from == "sync"));
    }

    #[test]
    fn test_extract_methods() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        // Methods should have their receiver type in kind
        let methods: Vec<_> = info.exports.iter()
            .filter(|e| e.kind.starts_with("method"))
            .collect();
        assert!(!methods.is_empty());

        // UserService.Get, UserService.Save, etc.
        assert!(info.exports.iter().any(|e| e.name == "Get" && e.kind.contains("UserService")));
        assert!(info.exports.iter().any(|e| e.name == "Save" && e.kind.contains("UserService")));
    }

    #[test]
    fn test_extract_calls() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        // Should have calls to imported packages
        let fmt_calls: Vec<_> = info.calls.iter()
            .filter(|c| c.target == "fmt")
            .collect();
        assert!(!fmt_calls.is_empty());
    }

    #[test]
    fn test_extract_signatures() {
        let parser = GoParser::new();
        let info = parser.extract_ast_info(GO_FIXTURE, Path::new("sample.go")).unwrap();

        assert!(!info.signatures.is_empty());

        // Check struct signature
        let user_config_sig = info.signatures.iter()
            .find(|s| s.name == "UserConfig");
        assert!(user_config_sig.is_some());
        assert_eq!(user_config_sig.unwrap().kind, "struct");

        // Check interface signature
        let repo_sig = info.signatures.iter()
            .find(|s| s.name == "Repository");
        assert!(repo_sig.is_some());
        assert_eq!(repo_sig.unwrap().kind, "interface");
    }

    #[test]
    fn test_extract_toon_comments() {
        let parser = GoParser::new();
        let comments = parser.extract_toon_comments(GO_FIXTURE).unwrap();

        assert!(comments.file_block.is_some());
        let block = comments.file_block.unwrap();
        assert!(block.purpose.is_some());
        assert!(block.purpose.unwrap().contains("Sample Go"));
    }

    #[test]
    fn test_strip_toon_comments() {
        let parser = GoParser::new();
        let stripped = parser.strip_toon_comments(GO_FIXTURE, "sample.go.toon").unwrap();

        assert!(stripped.contains("// @toon ->"));
        assert!(stripped.contains("sample.go.toon"));
    }

    #[test]
    fn test_empty_source() {
        let parser = GoParser::new();
        let source = "package main";
        let info = parser.extract_ast_info(source, Path::new("empty.go")).unwrap();

        assert!(info.exports.is_empty());
    }

    #[test]
    fn test_default_impl() {
        let parser = GoParser::new();
        assert_eq!(parser.language_name(), "go");
    }

    #[test]
    fn test_type_alias() {
        let parser = GoParser::new();
        let source = r#"
package main

type UserID string
"#;
        let info = parser.extract_ast_info(source, Path::new("test.go")).unwrap();

        let type_export = info.exports.iter().find(|e| e.name == "UserID");
        assert!(type_export.is_some());
        assert_eq!(type_export.unwrap().kind, "type");
    }

    #[test]
    fn test_const_block() {
        let parser = GoParser::new();
        let source = r#"
package main

const (
    Version = "1.0.0"
    Name    = "test"
)
"#;
        let info = parser.extract_ast_info(source, Path::new("test.go")).unwrap();

        assert!(info.exports.iter().any(|e| e.name == "Version" && e.kind == "const"));
        assert!(info.exports.iter().any(|e| e.name == "Name" && e.kind == "const"));
    }

    #[test]
    fn test_import_with_alias() {
        let parser = GoParser::new();
        let source = r#"
package main

import (
    f "fmt"
    . "strings"
)
"#;
        let info = parser.extract_ast_info(source, Path::new("test.go")).unwrap();

        let fmt_import = info.imports.iter().find(|i| i.from == "fmt");
        assert!(fmt_import.is_some());
        assert!(fmt_import.unwrap().items.contains(&"f".to_string()));
    }

    #[test]
    fn test_parse_toon_block_sections() {
        let parser = GoParser::new();
        let content = r#"
purpose: Test purpose

when-editing: !Important rule; Normal rule

invariants: Rule one; Rule two
"#;
        let block = parser.parse_toon_block(content);

        assert!(block.purpose.is_some());
        assert!(block.when_editing.is_some());
        assert!(block.invariants.is_some());
    }
}
