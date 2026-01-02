//! @toon
//! purpose: This module parses Rust source files to extract pub items including functions,
//!     structs, enums, traits, type aliases, constants, statics, modules, and macros.
//!     It uses tree-sitter for robust parsing and respects Rust's visibility modifiers.
//!
//! when-editing:
//!     - !Only items with visibility_modifier nodes are considered public exports
//!     - !Impl blocks need special handling to extract pub methods with their receiver type
//!     - Use declarations have multiple patterns that all need handling
//!
//! invariants:
//!     - Private items (no pub modifier) are never exported
//!     - Method exports include their receiver type in the kind field
//!     - Macro definitions are always exported (they have no visibility modifier)
//!
//! do-not:
//!     - Never export items without pub modifier (except macros)
//!     - Never assume all use statements follow simple patterns
//!
//! gotchas:
//!     - Rust uses explicit pub modifier for visibility; default is private
//!     - The @toon block can use //! or /*! doc comment syntax
//!     - Use statements can have complex nested patterns with aliases
//!     - Impl blocks contain methods but the impl itself is not an export
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set Rust language, parse source
//!     - Extract exports: Walk AST finding items with visibility_modifier nodes
//!     - Extract imports: Walk AST collecting use_declaration nodes with complex patterns

use crate::parser::{LanguageParser, ParseError};
use crate::types::{
    ASTInfo, CallInfo, ExportInfo, ExtractedComments, ImportInfo, SignatureInfo,
    ToonCommentBlock, WhenEditingItem,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Parser for Rust files
#[derive(Clone)]
pub struct RustParser;

impl RustParser {
    pub fn new() -> Self {
        Self
    }

    fn create_parser(&self) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;
        Ok(parser)
    }

    fn node_text(&self, node: Node, source: &str) -> String {
        source[node.start_byte()..node.end_byte()].to_string()
    }

    /// Check if a node has `pub` visibility
    fn is_public(&self, node: Node) -> bool {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "visibility_modifier" {
                    return true;
                }
            }
        }
        false
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
                "function_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            let kind = if name.starts_with("test_") {
                                "test".to_string()
                            } else {
                                "fn".to_string()
                            };
                            exports.push(ExportInfo { name, kind });
                        }
                    }
                }
                "struct_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "struct".to_string(),
                            });
                        }
                    }
                }
                "enum_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "enum".to_string(),
                            });
                        }
                    }
                }
                "trait_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "trait".to_string(),
                            });
                        }
                    }
                }
                "type_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "type".to_string(),
                            });
                        }
                    }
                }
                "const_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "const".to_string(),
                            });
                        }
                    }
                }
                "static_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "static".to_string(),
                            });
                        }
                    }
                }
                "impl_item" => {
                    // Check for pub methods in impl blocks
                    self.extract_impl_exports(node, source, exports);
                }
                "mod_item" => {
                    if self.is_public(node) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "mod".to_string(),
                            });
                        }
                    }
                }
                "macro_definition" => {
                    // macro_rules! name { ... }
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            if child.kind() == "identifier" {
                                let name = self.node_text(child, source);
                                exports.push(ExportInfo {
                                    name,
                                    kind: "macro".to_string(),
                                });
                                break;
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

    fn extract_impl_exports(&self, impl_node: Node, source: &str, exports: &mut Vec<ExportInfo>) {
        // Get the type being implemented
        let impl_type = impl_node.child_by_field_name("type")
            .map(|t| self.node_text(t, source))
            .unwrap_or_default();

        // Find the body and extract pub methods
        if let Some(body) = impl_node.child_by_field_name("body") {
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i) {
                    if child.kind() == "function_item" && self.is_public(child) {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            let kind = if impl_type.is_empty() {
                                "method".to_string()
                            } else {
                                format!("method({})", impl_type)
                            };
                            exports.push(ExportInfo { name, kind });
                        }
                    }
                }
            }
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

            if node.kind() == "use_declaration" {
                self.extract_use_items(node, source, imports);
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

    fn extract_use_items(&self, use_node: Node, source: &str, imports: &mut Vec<ImportInfo>) {
        // Handle various use patterns:
        // use crate::module::Item;
        // use crate::module::{Item1, Item2};
        // use std::collections::HashMap;

        if let Some(arg) = use_node.child_by_field_name("argument") {
            let (path, items) = self.parse_use_tree(arg, source);
            if !path.is_empty() && !items.is_empty() {
                imports.push(ImportInfo { from: path, items });
            }
        }
    }

    fn parse_use_tree(&self, node: Node, source: &str) -> (String, Vec<String>) {
        match node.kind() {
            "scoped_identifier" | "scoped_type_identifier" => {
                let text = self.node_text(node, source);
                let parts: Vec<&str> = text.rsplitn(2, "::").collect();
                if parts.len() == 2 {
                    (parts[1].to_string(), vec![parts[0].to_string()])
                } else {
                    (text.clone(), vec![text])
                }
            }
            "identifier" => {
                let name = self.node_text(node, source);
                (name.clone(), vec![name])
            }
            "use_list" => {
                let mut items = Vec::new();
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "identifier" || child.kind() == "scoped_identifier" {
                            let name = self.node_text(child, source);
                            let last_part = name.rsplit("::").next().unwrap_or(&name);
                            items.push(last_part.to_string());
                        }
                    }
                }
                (String::new(), items)
            }
            "use_as_clause" => {
                if let Some(alias) = node.child_by_field_name("alias") {
                    let alias_name = self.node_text(alias, source);
                    (String::new(), vec![alias_name])
                } else {
                    (String::new(), Vec::new())
                }
            }
            "scoped_use_list" => {
                let mut path = String::new();
                let mut items = Vec::new();

                if let Some(path_node) = node.child_by_field_name("path") {
                    path = self.node_text(path_node, source);
                }

                if let Some(list) = node.child_by_field_name("list") {
                    for i in 0..list.child_count() {
                        if let Some(child) = list.child(i) {
                            match child.kind() {
                                "identifier" => {
                                    items.push(self.node_text(child, source));
                                }
                                "use_as_clause" => {
                                    if let Some(alias) = child.child_by_field_name("alias") {
                                        items.push(self.node_text(alias, source));
                                    } else if let Some(path) = child.child_by_field_name("path") {
                                        let text = self.node_text(path, source);
                                        let last = text.rsplit("::").next().unwrap_or(&text);
                                        items.push(last.to_string());
                                    }
                                }
                                "self" => {
                                    items.push("self".to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }

                (path, items)
            }
            _ => {
                let text = self.node_text(node, source);
                (text.clone(), vec![text])
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
                    // Handle path::to::function() calls
                    let func_text = self.node_text(func, source);
                    let parts: Vec<&str> = func_text.split("::").collect();

                    if parts.len() >= 2 {
                        let first = parts[0];
                        let method = parts.last().unwrap_or(&"");

                        if let Some(&target) = import_map.get(first) {
                            calls.push(CallInfo {
                                target: target.to_string(),
                                method: method.to_string(),
                            });
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
                "function_item" => {
                    if let Some(sig) = self.extract_function_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "struct_item" => {
                    if let Some(sig) = self.extract_struct_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "enum_item" => {
                    if let Some(sig) = self.extract_enum_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "trait_item" => {
                    if let Some(sig) = self.extract_trait_signature(node, source, export_names) {
                        signatures.push(sig);
                    }
                }
                "type_item" => {
                    if let Some(sig) = self.extract_type_alias_signature(node, source, export_names) {
                        signatures.push(sig);
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

        let type_params = node.child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        let params = node.child_by_field_name("parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_else(|| "()".to_string());

        let return_type = node.child_by_field_name("return_type")
            .map(|r| format!(" {}", self.node_text(r, source)))
            .unwrap_or_default();

        let signature = format!("{}{}{}", type_params, params, return_type);

        Some(SignatureInfo {
            name,
            kind: "fn".to_string(),
            signature,
        })
    }

    fn extract_struct_signature(
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

        let type_params = node.child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        // Summarize fields
        let body = node.child_by_field_name("body");
        let fields = if let Some(body_node) = body {
            self.summarize_struct_fields(body_node, source)
        } else {
            // Tuple struct or unit struct
            "".to_string()
        };

        let signature = format!("{} {{ {} }}", type_params, fields).trim().to_string();

        Some(SignatureInfo {
            name,
            kind: "struct".to_string(),
            signature,
        })
    }

    fn summarize_struct_fields(&self, body: Node, source: &str) -> String {
        let mut fields = Vec::new();

        for i in 0..body.child_count() {
            if let Some(child) = body.child(i) {
                if child.kind() == "field_declaration" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let field_name = self.node_text(name_node, source);
                        let field_type = child.child_by_field_name("type")
                            .map(|t| self.node_text(t, source))
                            .unwrap_or_default();
                        fields.push(format!("{}: {}", field_name, field_type));
                    }
                }
            }

            if fields.len() >= 5 {
                fields.push("...".to_string());
                break;
            }
        }

        fields.join(", ")
    }

    fn extract_enum_signature(
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

        let type_params = node.child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        // Get variant names
        let mut variants = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i) {
                    if child.kind() == "enum_variant" {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            variants.push(self.node_text(name_node, source));
                        }
                    }
                }
                if variants.len() >= 5 {
                    variants.push("...".to_string());
                    break;
                }
            }
        }

        let signature = format!("{} {{ {} }}", type_params, variants.join(" | ")).trim().to_string();

        Some(SignatureInfo {
            name,
            kind: "enum".to_string(),
            signature,
        })
    }

    fn extract_trait_signature(
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

        let type_params = node.child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        // Get method names
        let mut methods = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i) {
                    if child.kind() == "function_signature_item" || child.kind() == "function_item" {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let method_name = self.node_text(name_node, source);
                            let params = child.child_by_field_name("parameters")
                                .map(|p| self.node_text(p, source))
                                .unwrap_or_else(|| "()".to_string());
                            methods.push(format!("{}{}", method_name, params));
                        }
                    }
                }
                if methods.len() >= 5 {
                    methods.push("...".to_string());
                    break;
                }
            }
        }

        let signature = format!("{} {{ {} }}", type_params, methods.join("; ")).trim().to_string();

        Some(SignatureInfo {
            name,
            kind: "trait".to_string(),
            signature,
        })
    }

    fn extract_type_alias_signature(
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

        let type_params = node.child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        let type_value = node.child_by_field_name("type")
            .map(|t| self.node_text(t, source))
            .unwrap_or_default();

        let signature = if type_params.is_empty() {
            type_value
        } else {
            format!("{} = {}", type_params, type_value)
        };

        Some(SignatureInfo {
            name,
            kind: "type".to_string(),
            signature,
        })
    }

    fn parse_toon_block(&self, content: &str) -> ToonCommentBlock {
        let mut block = ToonCommentBlock::default();
        let lines: Vec<&str> = content.lines().collect();
        let mut current_section: Option<&str> = None;
        let mut current_items: Vec<String> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            // Strip Rust doc comment prefixes
            let trimmed = line.trim()
                .trim_start_matches("///")
                .trim_start_matches("//!")
                .trim_start_matches('*')
                .trim();

            if trimmed.is_empty() {
                continue;
            }

            if let Some((key, value)) = trimmed.split_once(':') {
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
                block.purpose = Some(trimmed.to_string());
            }
        }

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

impl LanguageParser for RustParser {
    fn language_name(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["rs"]
    }

    fn extract_ast_info(&self, source: &str, _file_path: &Path) -> Result<ASTInfo, ParseError> {
        let mut parser = self.create_parser()?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse Rust source".to_string()))?;

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

        // Match /*! @toon ... */ or /** @toon ... */ block comments
        let block_pattern = Regex::new(r"/\*[!\*]?\s*@toon\b([\s\S]*?)\*/").unwrap();

        if let Some(captures) = block_pattern.captures(source) {
            if let Some(content) = captures.get(1) {
                comments.file_block = Some(self.parse_toon_block(content.as_str()));
            }
        }

        // Match //! @toon or /// @toon doc comments
        let doc_pattern = Regex::new(r"(?m)^[ \t]*//[!/]\s*@toon\b(.*)$").unwrap();
        if let Some(captures) = doc_pattern.captures(source) {
            if let Some(content) = captures.get(1) {
                // Collect subsequent doc comment lines
                let start = captures.get(0).unwrap().end();
                let rest = &source[start..];
                let mut full_content = content.as_str().to_string();

                let continuation = Regex::new(r"(?m)^[ \t]*//[!/](.*)$").unwrap();
                for cap in continuation.captures_iter(rest) {
                    if let Some(line) = cap.get(1) {
                        let line_text = line.as_str().trim();
                        if line_text.starts_with("@") || line_text.is_empty() {
                            break;
                        }
                        full_content.push('\n');
                        full_content.push_str(line_text);
                    } else {
                        break;
                    }
                }

                comments.file_block = Some(self.parse_toon_block(&full_content));
            }
        }

        Ok(comments)
    }

    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError> {
        let mut result = source.to_string();

        // Replace block @toon comments with reference
        let block_pattern = Regex::new(r"/\*[!\*]?\s*@toon\b[\s\S]*?\*/\s*\n?").unwrap();
        result = block_pattern
            .replace_all(&result, &format!("// @toon -> {}\n", toon_path))
            .to_string();

        // Remove doc comment @toon lines
        let doc_pattern = Regex::new(r"(?m)^[ \t]*//[!/]\s*@toon[^\n]*\n?").unwrap();
        result = doc_pattern
            .replace_all(&result, &format!("// @toon -> {}\n", toon_path))
            .to_string();

        Ok(result)
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RS_FIXTURE: &str = include_str!("../../test_fixtures/sample.rs");

    #[test]
    fn test_language_name() {
        let parser = RustParser::new();
        assert_eq!(parser.language_name(), "rust");
    }

    #[test]
    fn test_file_extensions() {
        let parser = RustParser::new();
        let exts = parser.file_extensions();
        assert!(exts.contains(&"rs"));
        assert_eq!(exts.len(), 1);
    }

    #[test]
    fn test_is_public() {
        let parser = RustParser::new();
        let source = r#"
pub fn public_fn() {}
fn private_fn() {}
"#;
        let mut tree_parser = Parser::new();
        tree_parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = tree_parser.parse(source, None).unwrap();
        let root = tree.root_node();

        let mut found_public = false;
        let mut found_private = false;

        for i in 0..root.child_count() {
            if let Some(child) = root.child(i) {
                if child.kind() == "function_item" {
                    if parser.is_public(child) {
                        found_public = true;
                    } else {
                        found_private = true;
                    }
                }
            }
        }

        assert!(found_public);
        assert!(found_private);
    }

    #[test]
    fn test_extract_exports() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        assert!(!info.exports.is_empty());

        // Check for pub functions
        assert!(info.exports.iter().any(|e| e.name == "create_user" && e.kind == "fn"));
        assert!(info.exports.iter().any(|e| e.name == "validate_email" && e.kind == "fn"));

        // Check for pub structs
        assert!(info.exports.iter().any(|e| e.name == "UserConfig" && e.kind == "struct"));
        assert!(info.exports.iter().any(|e| e.name == "UserService" && e.kind == "struct"));
        assert!(info.exports.iter().any(|e| e.name == "Cache" && e.kind == "struct"));

        // Check for pub traits
        assert!(info.exports.iter().any(|e| e.name == "Repository" && e.kind == "trait"));

        // Check for pub enums
        assert!(info.exports.iter().any(|e| e.name == "Error" && e.kind == "enum"));
        assert!(info.exports.iter().any(|e| e.name == "UserStatus" && e.kind == "enum"));

        // Check for pub type aliases
        assert!(info.exports.iter().any(|e| e.name == "UserId" && e.kind == "type"));
        assert!(info.exports.iter().any(|e| e.name == "Result" && e.kind == "type"));

        // Check for pub constants
        assert!(info.exports.iter().any(|e| e.name == "VERSION" && e.kind == "const"));
        assert!(info.exports.iter().any(|e| e.name == "DEFAULT_TIMEOUT" && e.kind == "const"));
    }

    #[test]
    fn test_private_items_excluded() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        // Private items should NOT be in exports
        assert!(!info.exports.iter().any(|e| e.name == "generate_id"));
        assert!(!info.exports.iter().any(|e| e.name == "INTERNAL_BUFFER_SIZE"));
    }

    #[test]
    fn test_extract_imports() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        assert!(!info.imports.is_empty());

        // Check for standard library imports
        assert!(info.imports.iter().any(|i| i.from.contains("collections") && i.items.contains(&"HashMap".to_string())));
        assert!(info.imports.iter().any(|i| i.from.contains("path") && i.items.contains(&"Path".to_string())));
    }

    #[test]
    fn test_extract_impl_methods() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        // Pub methods in impl blocks should be exported with receiver type
        let methods: Vec<_> = info.exports.iter()
            .filter(|e| e.kind.starts_with("method"))
            .collect();
        assert!(!methods.is_empty());

        // UserConfig::new, UserConfig::with_email, etc.
        assert!(info.exports.iter().any(|e| e.name == "new" && e.kind.contains("UserConfig")));
    }

    #[test]
    fn test_extract_signatures() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        assert!(!info.signatures.is_empty());

        // Check struct signature
        let user_config_sig = info.signatures.iter()
            .find(|s| s.name == "UserConfig");
        assert!(user_config_sig.is_some());
        assert_eq!(user_config_sig.unwrap().kind, "struct");

        // Check trait signature
        let repo_sig = info.signatures.iter()
            .find(|s| s.name == "Repository");
        assert!(repo_sig.is_some());
        assert_eq!(repo_sig.unwrap().kind, "trait");
    }

    #[test]
    fn test_extract_toon_comments() {
        let parser = RustParser::new();
        let comments = parser.extract_toon_comments(RS_FIXTURE).unwrap();

        assert!(comments.file_block.is_some());
        let block = comments.file_block.unwrap();
        assert!(block.purpose.is_some());
        assert!(block.purpose.unwrap().contains("Sample Rust"));
    }

    #[test]
    fn test_strip_toon_comments() {
        let parser = RustParser::new();
        let stripped = parser.strip_toon_comments(RS_FIXTURE, "sample.rs.toon").unwrap();

        assert!(stripped.contains("// @toon ->"));
        assert!(stripped.contains("sample.rs.toon"));
    }

    #[test]
    fn test_empty_source() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info("", Path::new("empty.rs")).unwrap();

        assert!(info.exports.is_empty());
        assert!(info.imports.is_empty());
    }

    #[test]
    fn test_default_impl() {
        let parser = RustParser::new();
        assert_eq!(parser.language_name(), "rust");
    }

    #[test]
    fn test_macro_export() {
        let parser = RustParser::new();
        let info = parser.extract_ast_info(RS_FIXTURE, Path::new("sample.rs")).unwrap();

        // Macros should be exported
        assert!(info.exports.iter().any(|e| e.name == "log_info" && e.kind == "macro"));
    }

    #[test]
    fn test_generic_struct() {
        let parser = RustParser::new();
        let source = r#"
pub struct Cache<K, V> {
    data: HashMap<K, V>,
}
"#;
        let info = parser.extract_ast_info(source, Path::new("test.rs")).unwrap();

        let cache_export = info.exports.iter().find(|e| e.name == "Cache");
        assert!(cache_export.is_some());
        assert_eq!(cache_export.unwrap().kind, "struct");
    }

    #[test]
    fn test_use_patterns() {
        let parser = RustParser::new();
        let source = r#"
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::types::*;
"#;
        let info = parser.extract_ast_info(source, Path::new("test.rs")).unwrap();

        assert!(info.imports.iter().any(|i| i.items.contains(&"HashMap".to_string())));
        assert!(info.imports.iter().any(|i| i.items.contains(&"Path".to_string())));
        assert!(info.imports.iter().any(|i| i.items.contains(&"PathBuf".to_string())));
    }

    #[test]
    fn test_parse_toon_block_sections() {
        let parser = RustParser::new();
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

    #[test]
    fn test_enum_variants() {
        let parser = RustParser::new();
        let source = r#"
pub enum Status {
    Active,
    Inactive,
    Pending,
}
"#;
        let info = parser.extract_ast_info(source, Path::new("test.rs")).unwrap();

        let status_sig = info.signatures.iter().find(|s| s.name == "Status");
        assert!(status_sig.is_some());
        assert!(status_sig.unwrap().signature.contains("Active"));
    }
}
