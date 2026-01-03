//! @dose
//! purpose: This module parses TypeScript and JavaScript source files to extract AST
//!     information including exports, imports, function calls, and type signatures.
//!     It uses tree-sitter for robust parsing and handles both ES modules and CommonJS.
//!
//! when-editing:
//!     - !The tree-sitter language selection depends on file extension (tsx vs ts)
//!     - !Export kind detection has special logic for React components and hooks
//!     - JSX detection is done by walking the AST looking for jsx_element nodes
//!     - Hook detection uses the naming convention: starts with "use" followed by uppercase
//!
//! invariants:
//!     - The parser always returns valid ASTInfo even for empty or malformed files
//!     - Exports are only extracted from export statements, not from internal definitions
//!     - Import items include both named imports and namespace imports (e.g., "* as foo")
//!
//! do-not:
//!     - Never assume all TypeScript files have type annotations
//!     - Never panic on parse errors; return ParseError instead
//!     - Never use regex for AST parsing; always use tree-sitter
//!
//! gotchas:
//!     - TSX files need the tsx language variant for JSX support
//!     - Arrow function components may not be detected if they don't return JSX
//!     - Re-exports like "export { foo } from 'bar'" are handled differently
//!     - Type-only imports should be distinguished from value imports
//!
//! flows:
//!     - Parse: Create tree-sitter parser, set language, parse source into AST
//!     - Extract exports: Walk AST collecting export_statement nodes
//!     - Extract imports: Walk AST collecting import_statement nodes
//!     - Extract calls: Map imported names to modules, then find call_expression nodes
//!     - Extract signatures: For each export, build signature from params and return type

use crate::parser::{toon_comment, LanguageParser, ParseError};
use crate::types::{
    ASTInfo, CallInfo, ExportInfo, ExtractedComments, FunctionAnnotation, ImportInfo, SignatureInfo,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Parser for TypeScript and JavaScript files
#[derive(Clone)]
pub struct TypeScriptParser {
    // Tree-sitter parser is not Clone, so we create it on demand
}

impl TypeScriptParser {
    pub fn new() -> Self {
        Self {}
    }

    fn create_parser(&self, is_tsx: bool) -> Result<Parser, ParseError> {
        let mut parser = Parser::new();
        let language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        parser
            .set_language(&language)
            .map_err(|e| ParseError::ParseError(e.to_string()))?;
        Ok(parser)
    }

    fn is_tsx(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| ext == "tsx" || ext == "jsx")
            .unwrap_or(false)
    }

    fn extract_exports(&self, root: Node, source: &str) -> Vec<ExportInfo> {
        // First pass: collect all top-level definitions
        let definitions = self.collect_definitions(root, source);

        // Second pass: collect exports
        let mut exports = Vec::new();
        let mut cursor = root.walk();

        self.visit_exports(&mut cursor, source, &mut exports, &definitions);
        exports
    }

    /// Collect all top-level definitions to look up export kinds
    fn collect_definitions(&self, root: Node, source: &str) -> HashMap<String, String> {
        let mut defs = HashMap::new();

        // Iterate through top-level children of the program
        for i in 0..root.child_count() {
            let Some(node) = root.child(i) else { continue };

            match node.kind() {
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        let kind = if self.returns_jsx(node, source) {
                            "component"
                        } else if name.starts_with("use")
                            && name
                                .chars()
                                .nth(3)
                                .map(|c| c.is_uppercase())
                                .unwrap_or(false)
                        {
                            "hook"
                        } else {
                            "fn"
                        };
                        defs.insert(name, kind.to_string());
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    for j in 0..node.child_count() {
                        if let Some(declarator) = node.child(j) {
                            if declarator.kind() == "variable_declarator" {
                                if let Some(name_node) = declarator.child_by_field_name("name") {
                                    let name = self.node_text(name_node, source);
                                    let kind =
                                        self.infer_kind_from_declarator(declarator, &name, source);
                                    defs.insert(name, kind);
                                }
                            }
                        }
                    }
                }
                "class_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        defs.insert(name, "class".to_string());
                    }
                }
                "type_alias_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        defs.insert(name, "type".to_string());
                    }
                }
                "interface_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        defs.insert(name, "interface".to_string());
                    }
                }
                "enum_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = self.node_text(name_node, source);
                        defs.insert(name, "enum".to_string());
                    }
                }
                _ => {}
            }
        }

        defs
    }

    /// Check if a function returns JSX (is a React component) by walking AST
    fn returns_jsx(&self, node: Node, _source: &str) -> bool {
        self.contains_jsx_element(node)
    }

    /// Recursively check if node or its descendants contain JSX elements
    fn contains_jsx_element(&self, node: Node) -> bool {
        // Check if this node is a JSX type
        match node.kind() {
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => {
                return true;
            }
            _ => {}
        }

        // Recurse into children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if self.contains_jsx_element(child) {
                    return true;
                }
            }
        }

        false
    }

    /// Infer kind from a variable declarator by looking at its value
    fn infer_kind_from_declarator(&self, node: Node, name: &str, source: &str) -> String {
        // Check for type annotation first - look for React component types in AST
        if let Some(type_node) = node.child_by_field_name("type") {
            if self.is_react_component_type(type_node, source) {
                return "component".to_string();
            }
        }

        // Check the value
        if let Some(value) = node.child_by_field_name("value") {
            match value.kind() {
                "arrow_function" | "function" | "function_expression" => {
                    // Check if it's a hook by naming convention (useXxx)
                    if name.starts_with("use")
                        && name
                            .chars()
                            .nth(3)
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false)
                    {
                        return "hook".to_string();
                    }
                    // Check if it returns JSX (component) by AST analysis
                    if self.returns_jsx(value, source) {
                        return "component".to_string();
                    }
                    return "fn".to_string();
                }
                "call_expression" => {
                    // Analyze the call expression AST
                    if let Some(kind) = self.infer_kind_from_call(value, source) {
                        return kind;
                    }
                    return "const".to_string();
                }
                _ => {}
            }
        }

        "const".to_string()
    }

    /// Check if type annotation is a React component type
    fn is_react_component_type(&self, type_node: Node, source: &str) -> bool {
        // Look for type_identifier or generic_type nodes
        self.find_type_name(
            type_node,
            source,
            &["FC", "FunctionComponent", "ComponentType", "Element"],
        )
    }

    /// Recursively search for specific type names in a type annotation
    fn find_type_name(&self, node: Node, source: &str, names: &[&str]) -> bool {
        match node.kind() {
            "type_identifier" => {
                let name = self.node_text(node, source);
                return names.contains(&name.as_str());
            }
            "member_expression" | "nested_type_identifier" => {
                // Check the property/right side for React.FC etc
                if let Some(prop) = node
                    .child_by_field_name("property")
                    .or_else(|| node.child_by_field_name("name"))
                {
                    let name = self.node_text(prop, source);
                    return names.contains(&name.as_str());
                }
            }
            "generic_type" => {
                // Check the base type name
                if let Some(name_node) = node.child_by_field_name("name") {
                    return self.find_type_name(name_node, source, names);
                }
            }
            _ => {}
        }

        // Recurse into children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if self.find_type_name(child, source, names) {
                    return true;
                }
            }
        }

        false
    }

    /// Infer kind from a call expression
    fn infer_kind_from_call(&self, node: Node, source: &str) -> Option<String> {
        // Get the function being called
        let callee = node.child(0)?;

        match callee.kind() {
            "identifier" => {
                let name = self.node_text(callee, source);
                match name.as_str() {
                    "createContext" => return Some("context".to_string()),
                    "memo" | "forwardRef" | "lazy" => return Some("component".to_string()),
                    _ => {}
                }
            }
            "member_expression" => {
                // Handle React.createContext, React.memo, etc.
                if let Some(prop) = callee.child_by_field_name("property") {
                    let method = self.node_text(prop, source);
                    match method.as_str() {
                        "createContext" => return Some("context".to_string()),
                        "memo" | "forwardRef" | "lazy" => return Some("component".to_string()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn visit_exports(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        exports: &mut Vec<ExportInfo>,
        definitions: &HashMap<String, String>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "export_statement" {
                exports.extend(self.parse_export_statement(node, source, definitions));
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.visit_exports(cursor, source, exports, definitions);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_export_statement(
        &self,
        node: Node,
        source: &str,
        definitions: &HashMap<String, String>,
    ) -> Vec<ExportInfo> {
        let mut exports = Vec::new();

        // Look for declaration child
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "function_declaration" | "function_signature" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            // Use collected definition kind, or infer from function
                            let kind = definitions.get(&name).cloned().unwrap_or_else(|| {
                                if self.returns_jsx(child, source) {
                                    "component".to_string()
                                } else if name.starts_with("use")
                                    && name
                                        .chars()
                                        .nth(3)
                                        .map(|c| c.is_uppercase())
                                        .unwrap_or(false)
                                {
                                    "hook".to_string()
                                } else {
                                    "fn".to_string()
                                }
                            });
                            exports.push(ExportInfo { name, kind });
                        }
                    }
                    "class_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "class".to_string(),
                            });
                        }
                    }
                    "lexical_declaration" | "variable_declaration" => {
                        // export const foo = ..., bar = ...
                        for j in 0..child.child_count() {
                            if let Some(declarator) = child.child(j) {
                                if declarator.kind() == "variable_declarator" {
                                    if let Some(name_node) = declarator.child_by_field_name("name")
                                    {
                                        let name = self.node_text(name_node, source);
                                        // Use collected definition kind
                                        let kind =
                                            definitions.get(&name).cloned().unwrap_or_else(|| {
                                                self.infer_kind_from_declarator(
                                                    declarator, &name, source,
                                                )
                                            });
                                        exports.push(ExportInfo { name, kind });
                                    }
                                }
                            }
                        }
                    }
                    "type_alias_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "type".to_string(),
                            });
                        }
                    }
                    "interface_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "interface".to_string(),
                            });
                        }
                    }
                    "enum_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.node_text(name_node, source);
                            exports.push(ExportInfo {
                                name,
                                kind: "enum".to_string(),
                            });
                        }
                    }
                    // Handle export { Name1, Name2 }
                    "export_clause" => {
                        for j in 0..child.child_count() {
                            if let Some(spec) = child.child(j) {
                                if spec.kind() == "export_specifier" {
                                    if let Some(name_node) = spec.child_by_field_name("name") {
                                        let name = self.node_text(name_node, source);
                                        // Look up the actual definition kind
                                        let kind = definitions
                                            .get(&name)
                                            .cloned()
                                            .unwrap_or_else(|| "const".to_string());
                                        exports.push(ExportInfo { name, kind });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        exports
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

            if node.kind() == "import_statement" {
                if let Some(import) = self.parse_import_statement(node, source) {
                    imports.push(import);
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

    fn parse_import_statement(&self, node: Node, source: &str) -> Option<ImportInfo> {
        let mut from = String::new();
        let mut items = Vec::new();

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "string" | "string_fragment" => {
                        from = self
                            .node_text(child, source)
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                    }
                    "import_clause" => {
                        self.extract_import_items(child, source, &mut items);
                    }
                    _ => {}
                }
            }
        }

        if !from.is_empty() {
            Some(ImportInfo { from, items })
        } else {
            None
        }
    }

    fn extract_import_items(&self, node: Node, source: &str, items: &mut Vec<String>) {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "identifier" => {
                        items.push(self.node_text(child, source));
                    }
                    "named_imports" => {
                        self.extract_named_imports(child, source, items);
                    }
                    "namespace_import" => {
                        // import * as foo
                        if let Some(name) = child.child_by_field_name("name") {
                            items.push(format!("* as {}", self.node_text(name, source)));
                        }
                    }
                    _ => {
                        self.extract_import_items(child, source, items);
                    }
                }
            }
        }
    }

    fn extract_named_imports(&self, node: Node, source: &str, items: &mut Vec<String>) {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "import_specifier" {
                    if let Some(name) = child.child_by_field_name("name") {
                        items.push(self.node_text(name, source));
                    }
                }
            }
        }
    }

    /// Extract function/method calls from AST, grouped by import source
    fn extract_calls(&self, root: Node, source: &str, imports: &[ImportInfo]) -> Vec<CallInfo> {
        // Build a map of imported names to their source modules
        let mut import_map: HashMap<String, String> = HashMap::new();
        for imp in imports {
            for item in &imp.items {
                // Handle "* as X" imports
                let name = if item.starts_with("* as ") {
                    item.strip_prefix("* as ").unwrap_or(item)
                } else {
                    item.as_str()
                };
                import_map.insert(name.to_string(), imp.from.clone());
            }
        }

        let mut calls: Vec<CallInfo> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut cursor = root.walk();

        self.visit_calls(&mut cursor, source, &import_map, &mut calls, &mut seen);
        calls
    }

    fn visit_calls(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        import_map: &HashMap<String, String>,
        calls: &mut Vec<CallInfo>,
        seen: &mut HashSet<(String, String)>,
    ) {
        loop {
            let node = cursor.node();

            if node.kind() == "call_expression" {
                if let Some(call) = self.parse_call_expression(node, source, import_map) {
                    let key = (call.target.clone(), call.method.clone());
                    if !seen.contains(&key) {
                        seen.insert(key);
                        calls.push(call);
                    }
                }
            }

            if cursor.goto_first_child() {
                self.visit_calls(cursor, source, import_map, calls, seen);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    fn parse_call_expression(
        &self,
        node: Node,
        source: &str,
        import_map: &HashMap<String, String>,
    ) -> Option<CallInfo> {
        // Get the function being called (first child)
        let callee = node.child(0)?;

        match callee.kind() {
            // Direct call: foo()
            "identifier" => {
                let name = self.node_text(callee, source);
                if let Some(target) = import_map.get(&name) {
                    return Some(CallInfo {
                        target: target.clone(),
                        method: name,
                    });
                }
            }
            // Member call: foo.bar() or foo.bar.baz()
            "member_expression" => {
                let (obj_name, method_name) = self.parse_member_expression(callee, source)?;
                if let Some(target) = import_map.get(&obj_name) {
                    return Some(CallInfo {
                        target: target.clone(),
                        method: method_name,
                    });
                }
            }
            _ => {}
        }
        None
    }

    fn parse_member_expression(&self, node: Node, source: &str) -> Option<(String, String)> {
        let obj = node.child_by_field_name("object")?;
        let prop = node.child_by_field_name("property")?;

        let method = self.node_text(prop, source);

        // Get the root object name (handles nested member expressions)
        let obj_name = match obj.kind() {
            "identifier" => self.node_text(obj, source),
            "member_expression" => {
                // For nested like a.b.c(), get 'a'
                self.get_root_identifier(obj, source)?
            }
            _ => return None,
        };

        Some((obj_name, method))
    }

    fn get_root_identifier(&self, node: Node, source: &str) -> Option<String> {
        match node.kind() {
            "identifier" => Some(self.node_text(node, source)),
            "member_expression" => {
                let obj = node.child_by_field_name("object")?;
                self.get_root_identifier(obj, source)
            }
            _ => None,
        }
    }

    /// Extract signatures for exported functions/components
    fn extract_signatures(
        &self,
        root: Node,
        source: &str,
        exports: &[ExportInfo],
    ) -> Vec<SignatureInfo> {
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
                "lexical_declaration" | "variable_declaration" => {
                    // Handle: const Foo = (props) => ... or const Foo: Type = ...
                    for i in 0..node.child_count() {
                        if let Some(declarator) = node.child(i) {
                            if declarator.kind() == "variable_declarator" {
                                if let Some(sig) = self.extract_variable_signature(
                                    declarator,
                                    source,
                                    export_names,
                                ) {
                                    signatures.push(sig);
                                }
                            }
                        }
                    }
                }
                "type_alias_declaration" => {
                    if let Some(sig) = self.extract_type_alias_signature(node, source, export_names)
                    {
                        signatures.push(sig);
                    }
                }
                "interface_declaration" => {
                    if let Some(sig) = self.extract_interface_signature(node, source, export_names)
                    {
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

        // Determine kind from function analysis
        let kind = if self.returns_jsx(node, source) {
            "component".to_string()
        } else if name.starts_with("use")
            && name
                .chars()
                .nth(3)
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
        {
            "hook".to_string()
        } else {
            "fn".to_string()
        };

        // Build signature from parameters and return type
        let params = node
            .child_by_field_name("parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_else(|| "()".to_string());

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| self.node_text(r, source))
            .unwrap_or_default();

        let signature = if return_type.is_empty() {
            params
        } else {
            format!("{} {}", params, return_type)
        };

        Some(SignatureInfo {
            name,
            kind,
            signature,
        })
    }

    fn extract_variable_signature(
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

        // Determine kind from declarator analysis
        let kind = self.infer_kind_from_declarator(node, &name, source);

        // Try to get type annotation
        let type_ann = node.child_by_field_name("type");

        // Try to get value (arrow function, function expression, etc.)
        let value = node.child_by_field_name("value");

        let signature = if let Some(type_node) = type_ann {
            // Has explicit type annotation
            self.node_text(type_node, source)
                .trim_start_matches(':')
                .trim()
                .to_string()
        } else if let Some(val) = value {
            // Infer from value
            self.infer_signature_from_value(val, source)
        } else {
            String::new()
        };

        if signature.is_empty() {
            return None;
        }

        Some(SignatureInfo {
            name,
            kind,
            signature,
        })
    }

    fn infer_signature_from_value(&self, node: Node, source: &str) -> String {
        match node.kind() {
            "arrow_function" => {
                let params = node
                    .child_by_field_name("parameters")
                    .or_else(|| node.child_by_field_name("parameter"))
                    .map(|p| self.node_text(p, source))
                    .unwrap_or_else(|| "()".to_string());

                let return_type = node
                    .child_by_field_name("return_type")
                    .map(|r| self.node_text(r, source))
                    .unwrap_or_default();

                if return_type.is_empty() {
                    format!("{} => ...", params)
                } else {
                    format!("{} {} => ...", params, return_type)
                }
            }
            "function" | "function_expression" => {
                let params = node
                    .child_by_field_name("parameters")
                    .map(|p| self.node_text(p, source))
                    .unwrap_or_else(|| "()".to_string());
                params
            }
            _ => String::new(),
        }
    }

    fn extract_type_alias_signature(
        &self,
        node: Node,
        source: &str,
        export_names: &HashSet<&str>,
    ) -> Option<SignatureInfo> {
        // type Foo = ... or type Foo<T> = ...
        let name_node = node.child_by_field_name("name")?;
        let name = self.node_text(name_node, source);

        if !export_names.contains(name.as_str()) {
            return None;
        }

        // Get type parameters if present (e.g., <T, U>)
        let type_params = node
            .child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        // Get the value (right side of =)
        let value = node
            .child_by_field_name("value")
            .map(|v| self.node_text(v, source))
            .unwrap_or_default();

        let signature = if type_params.is_empty() {
            value
        } else {
            format!("{} = {}", type_params, value)
        };

        if signature.is_empty() {
            return None;
        }

        Some(SignatureInfo {
            name,
            kind: "type".to_string(),
            signature,
        })
    }

    fn extract_interface_signature(
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

        // Get type parameters if present
        let type_params = node
            .child_by_field_name("type_parameters")
            .map(|p| self.node_text(p, source))
            .unwrap_or_default();

        // Get extends clause if present
        let mut extends_clause = String::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "extends_type_clause" {
                    extends_clause = format!(" {}", self.node_text(child, source));
                    break;
                }
            }
        }

        // Get interface body - summarize fields
        let body = node.child_by_field_name("body");
        let fields_summary = if let Some(body_node) = body {
            self.summarize_interface_body(body_node, source)
        } else {
            "{}".to_string()
        };

        let signature = format!("{}{} {}", type_params, extends_clause, fields_summary);

        Some(SignatureInfo {
            name,
            kind: "interface".to_string(),
            signature: signature.trim().to_string(),
        })
    }

    fn summarize_interface_body(&self, body: Node, source: &str) -> String {
        // Extract field names and types, limit to keep signature manageable
        let mut fields: Vec<String> = Vec::new();

        for i in 0..body.child_count() {
            if let Some(child) = body.child(i) {
                match child.kind() {
                    "property_signature" | "method_signature" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let field_name = self.node_text(name_node, source);
                            let optional = child.child_by_field_name("optional").is_some();
                            let type_ann = child
                                .child_by_field_name("type")
                                .map(|t| self.node_text(t, source))
                                .unwrap_or_default();

                            let field = if optional {
                                format!("{}?{}", field_name, type_ann)
                            } else {
                                format!("{}{}", field_name, type_ann)
                            };
                            fields.push(field);
                        }
                    }
                    _ => {}
                }
            }

            // Limit to 5 fields to keep signature readable
            if fields.len() >= 5 {
                fields.push("...".to_string());
                break;
            }
        }

        if fields.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ {} }}", fields.join("; "))
        }
    }

    fn node_text(&self, node: Node, source: &str) -> String {
        source[node.start_byte()..node.end_byte()].to_string()
    }

    /// Find the name of the next function/export declaration after a given byte position
    fn find_next_function_name(
        &self,
        root: &Node,
        source: &str,
        after_pos: usize,
    ) -> Option<String> {
        let mut cursor = root.walk();
        self.find_next_function_recursive(&mut cursor, source, after_pos)
    }

    fn find_next_function_recursive(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        after_pos: usize,
    ) -> Option<String> {
        loop {
            let node = cursor.node();

            // Check if this node starts after our position and is a function/export
            if node.start_byte() >= after_pos {
                let kind = node.kind();
                if kind == "function_declaration"
                    || kind == "export_statement"
                    || kind == "lexical_declaration"
                {
                    // Extract the function/variable name
                    return self.extract_declaration_name(&node, source);
                }
            }

            // Recurse into children
            if cursor.goto_first_child() {
                if let Some(name) = self.find_next_function_recursive(cursor, source, after_pos) {
                    return Some(name);
                }
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                return None;
            }
        }
    }

    fn extract_declaration_name(&self, node: &Node, source: &str) -> Option<String> {
        let kind = node.kind();

        if kind == "function_declaration" {
            // Look for the identifier child
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "identifier" {
                        return Some(self.node_text(child, source));
                    }
                }
            }
        } else if kind == "export_statement" {
            // Look for function_declaration or variable_declaration inside
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if let Some(name) = self.extract_declaration_name(&child, source) {
                        return Some(name);
                    }
                }
            }
        } else if kind == "lexical_declaration" {
            // Look for variable_declarator -> identifier
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "variable_declarator" {
                        for j in 0..child.child_count() {
                            if let Some(grandchild) = child.child(j) {
                                if grandchild.kind() == "identifier" {
                                    return Some(self.node_text(grandchild, source));
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

impl Default for TypeScriptParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for TypeScriptParser {
    fn language_name(&self) -> &'static str {
        "typescript"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn extract_ast_info(&self, source: &str, file_path: &Path) -> Result<ASTInfo, ParseError> {
        let mut parser = self.create_parser(Self::is_tsx(file_path))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse source".to_string()))?;

        let root = tree.root_node();

        let exports = self.extract_exports(root, source);
        let imports = self.extract_imports(root, source);
        let calls = self.extract_calls(root, source, &imports);
        let signatures = self.extract_signatures(root, source, &exports);

        // Calculate tokens using tiktoken
        let tokens = super::tokens::count_tokens(source);

        Ok(ASTInfo {
            tokens,
            exports,
            imports,
            calls,
            signatures,
        })
    }

    fn extract_toon_comments(&self, source: &str) -> Result<ExtractedComments, ParseError> {
        let mut result = ExtractedComments::default();

        // Find @dose block comments using regex (file-level, multi-line)
        let block_pattern = Regex::new(r"/\*\*[\s\S]*?@dose[\s\S]*?\*/").unwrap();

        if let Some(mat) = block_pattern.find(source) {
            let comment = mat.as_str();
            // Check if this is a file-level block (contains sections like when-editing, invariants, etc.)
            // vs a single-line inline comment
            let is_file_block = comment.contains("when-editing")
                || comment.contains("invariants")
                || comment.contains("do-not")
                || comment.contains("gotchas")
                || comment.contains("flows")
                || comment.lines().count() > 3;

            if is_file_block {
                // Extract content between /** and */
                let content = comment
                    .trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .trim();

                // Remove @dose marker
                let content = content
                    .lines()
                    .map(|line| {
                        let trimmed = line.trim().trim_start_matches('*').trim();
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
        }

        // Find inline @dose comments (single-line annotations for specific functions)
        // Pattern matches: /** @dose field: value */ or // @dose field: value
        let inline_pattern = Regex::new(
            r"(?:/\*\*\s*@dose\s+(invariant|gotcha|do-not|constraint|error-handling):\s*([^*]+)\s*\*/|//\s*@dose\s+(invariant|gotcha|do-not|constraint|error-handling):\s*(.+))"
        ).unwrap();

        // Collect all inline annotations with their positions
        let mut inline_annotations: Vec<(usize, String, String)> = Vec::new();
        for cap in inline_pattern.captures_iter(source) {
            let pos = cap.get(0).unwrap().end();
            let (field, value) = if let Some(f) = cap.get(1) {
                (
                    f.as_str().to_string(),
                    cap.get(2).unwrap().as_str().trim().to_string(),
                )
            } else {
                (
                    cap.get(3).unwrap().as_str().to_string(),
                    cap.get(4).unwrap().as_str().trim().to_string(),
                )
            };
            inline_annotations.push((pos, field, value));
        }

        if !inline_annotations.is_empty() {
            // Parse source with tree-sitter to find function names
            let mut parser = Parser::new();
            let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
            parser.set_language(&language.into()).ok();

            if let Some(tree) = parser.parse(source, None) {
                let root = tree.root_node();

                for (pos, field, value) in inline_annotations {
                    // Find the next function/export after this position
                    if let Some(func_name) = self.find_next_function_name(&root, source, pos) {
                        let annotation = result
                            .function_annotations
                            .entry(func_name.clone())
                            .or_insert_with(|| FunctionAnnotation {
                                name: func_name,
                                invariants: None,
                                gotchas: None,
                                do_not: None,
                                error_handling: None,
                                constraints: None,
                            });

                        match field.as_str() {
                            "invariant" => {
                                annotation
                                    .invariants
                                    .get_or_insert_with(Vec::new)
                                    .push(value);
                            }
                            "gotcha" => {
                                annotation.gotchas.get_or_insert_with(Vec::new).push(value);
                            }
                            "do-not" => {
                                annotation.do_not.get_or_insert_with(Vec::new).push(value);
                            }
                            "constraint" => {
                                annotation
                                    .constraints
                                    .get_or_insert_with(Vec::new)
                                    .push(value);
                            }
                            "error-handling" => {
                                annotation
                                    .error_handling
                                    .get_or_insert_with(Vec::new)
                                    .push(value);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    fn strip_toon_comments(&self, source: &str, toon_path: &str) -> Result<String, ParseError> {
        let mut result = source.to_string();

        // Replace block @dose comments with stub
        let block_pattern = Regex::new(r"/\*\*[\s\S]*?@dose[\s\S]*?\*/").unwrap();
        result = block_pattern
            .replace_all(&result, &format!("// @dose -> {}", toon_path))
            .to_string();

        // Remove single-line @dose comments
        let single_pattern = Regex::new(r"//\s*@dose:[^\n]*\n?").unwrap();
        result = single_pattern.replace_all(&result, "").to_string();

        Ok(result)
    }

    fn get_string_ranges(&self, source: &str) -> Result<Vec<(usize, usize)>, ParseError> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .map_err(|e| ParseError::ParseError(e.to_string()))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::ParseError("Failed to parse source".to_string()))?;

        let mut ranges = Vec::new();
        let mut cursor = tree.walk();
        collect_string_ranges(&mut cursor, source.as_bytes(), &mut ranges);
        Ok(ranges)
    }
}

/// Collect byte ranges of string literals recursively
fn collect_string_ranges(
    cursor: &mut tree_sitter::TreeCursor,
    _source: &[u8],
    ranges: &mut Vec<(usize, usize)>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        // TypeScript/JavaScript string types
        if kind == "string" || kind == "template_string" || kind == "string_fragment" {
            ranges.push((node.start_byte(), node.end_byte()));
        }

        if cursor.goto_first_child() {
            collect_string_ranges(cursor, _source, ranges);
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

    const TS_FIXTURE: &str = include_str!("../../test_fixtures/sample.ts");
    const TSX_FIXTURE: &str = include_str!("../../test_fixtures/sample.tsx");

    #[test]
    fn test_extract_ast_info() {
        let parser = TypeScriptParser::new();
        let info = parser
            .extract_ast_info(TS_FIXTURE, Path::new("sample.ts"))
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
                ("DEFAULT_CONFIG", "const"),
                ("MemoizedComponent", "component"),
                ("MyComponent", "component"),
                ("ReactMemoComponent", "component"),
                ("Result", "type"),
                ("Status", "enum"),
                ("ThemeContext", "context"),
                ("UserConfig", "interface"),
                ("UserId", "type"),
                ("UserService", "class"),
                ("createUser", "fn"),
                ("defaultExport", "const"),
                ("memo", "fn"),
                ("saveUser", "fn"),
                ("validateUser", "fn"),
            ]
        );

        let mut imports: Vec<_> = info.imports.iter().map(|i| &i.from[..]).collect();
        imports.sort();
        assert_eq!(
            imports,
            vec!["./other-module", "./types", "fs/promises", "path"]
        );

        // Also verify TSX hooks/components detection
        let tsx_info = parser
            .extract_ast_info(TSX_FIXTURE, Path::new("sample.tsx"))
            .unwrap();
        let hooks: Vec<_> = tsx_info
            .exports
            .iter()
            .filter(|e| e.kind == "hook")
            .map(|e| &e.name[..])
            .collect();
        assert!(hooks.contains(&"useUser"));
        assert!(hooks.contains(&"useToggle"));

        let components: Vec<_> = tsx_info
            .exports
            .iter()
            .filter(|e| e.kind == "component")
            .map(|e| &e.name[..])
            .collect();
        assert!(components.contains(&"Button"));
        assert!(components.contains(&"UserCard"));
    }

    #[test]
    fn test_toon_comments() {
        let parser = TypeScriptParser::new();
        let comments = parser.extract_toon_comments(TS_FIXTURE).unwrap();
        let block = comments.file_block.unwrap();
        assert_eq!(block.purpose.unwrap(), "Sample TypeScript fixture for testing the luny parser. This file contains various TypeScript constructs to verify extraction works correctly.");

        let stripped = parser
            .strip_toon_comments(TS_FIXTURE, "sample.ts.toon")
            .unwrap();
        assert!(stripped.contains("// @dose"));
        assert!(stripped.contains("sample.ts.toon"));
    }
}
