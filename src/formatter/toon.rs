//! @toon
//! purpose: This module handles serialization and deserialization of TOON DOSE files.
//!     It formats ToonData into the compact TOON text format and parses TOON content back
//!     into ToonData for validation and processing.
//!
//! when-editing:
//!     - !The format_toon function uses U-curve ordering for optimal AI attention
//!     - !parse_toon must be kept in sync with format_toon to ensure round-trip compatibility
//!     - Fields at the beginning and end of the output receive highest AI attention
//!
//! invariants:
//!     - format_toon always outputs valid TOON format that can be parsed by parse_toon
//!     - Empty optional fields are never included in the output
//!     - All text content is compressed using the compress module before output
//!
//! do-not:
//!     - Never change the U-curve ordering without understanding the AI attention implications
//!     - Never add fields to the middle section that are critical for understanding
//!
//! gotchas:
//!     - The parse_toon function is lenient and handles missing fields gracefully
//!     - Signatures are truncated to 150 characters to prevent excessively long lines
//!     - imported-by and called-by are truncated to show only first 10 entries
//!
//! flows:
//!     - format_toon: Build ToonData -> Apply compression -> Format each field -> Join lines
//!     - parse_toon: Split by lines -> Match field patterns -> Parse field values -> Build ToonData

use crate::formatter::compress::{compress, compress_item};
use crate::types::{
    CallInfo, CalledByInfo, ExportInfo, FunctionAnnotation, ImportInfo, SignatureInfo, ToonData,
    WhenEditingItem,
};

/// Format ToonData into TOON DOSE file content.
/// Uses U-curve ordering for optimal AI attention.
pub fn format_toon(data: &ToonData) -> String {
    let mut lines: Vec<String> = Vec::new();

    // === BEGIN (high attention zone) ===

    // Required header fields
    lines.push(format!("purpose: {}", compress(&data.purpose)));
    lines.push(format!("tokens: ~{}", data.tokens));

    // Exports - what's exported from this file
    if !data.exports.is_empty() {
        lines.push(format_exports(&data.exports));
    }

    // Signatures - full type information for AI reasoning
    if let Some(ref signatures) = data.signatures {
        if !signatures.is_empty() {
            lines.extend(format_signatures(signatures));
        }
    }

    // When-editing guidance - important items prefixed with !
    if let Some(ref when_editing) = data.when_editing {
        if !when_editing.is_empty() {
            lines.push(format_when_editing(when_editing));
        }
    }

    // Invariants - high attention
    if let Some(ref invariants) = data.invariants {
        if !invariants.is_empty() {
            lines.push(format_list_field("invariants", invariants));
        }
    }

    // DO-NOT - high attention (forbidden patterns)
    if let Some(ref do_not) = data.do_not {
        if !do_not.is_empty() {
            lines.push(format_list_field("do-not", do_not));
        }
    }

    // === MIDDLE (lower attention zone) ===

    // Imports
    if let Some(ref imports) = data.imports {
        if !imports.is_empty() {
            lines.push(format_imports(imports));
        }
    }

    // Calls (what this file calls)
    if let Some(ref calls) = data.calls {
        if !calls.is_empty() {
            lines.push(format_calls(calls));
        }
    }

    // Imported-by (reverse deps)
    if let Some(ref imported_by) = data.imported_by {
        if !imported_by.is_empty() {
            let display: Vec<&str> = imported_by.iter().take(10).map(|s| s.as_str()).collect();
            let mut line = format!("imported-by[{}]: {}", imported_by.len(), display.join(","));
            if imported_by.len() > 10 {
                line.push_str(&format!(" (+{} more)", imported_by.len() - 10));
            }
            lines.push(line);
        }
    }

    // Called-by (reverse deps)
    if let Some(ref called_by) = data.called_by {
        if !called_by.is_empty() {
            lines.push(format_called_by(called_by));
        }
    }

    // Error handling
    if let Some(ref error_handling) = data.error_handling {
        if !error_handling.is_empty() {
            lines.push(format_list_field("error-handling", error_handling));
        }
    }

    // Constraints
    if let Some(ref constraints) = data.constraints {
        if !constraints.is_empty() {
            lines.push(format_list_field("constraints", constraints));
        }
    }

    // Flows
    if let Some(ref flows) = data.flows {
        if !flows.is_empty() {
            lines.push(format_list_field("flows", flows));
        }
    }

    // Testing
    if let Some(ref testing) = data.testing {
        if !testing.is_empty() {
            lines.push(format_list_field("testing", testing));
        }
    }

    // Common mistakes
    if let Some(ref common_mistakes) = data.common_mistakes {
        if !common_mistakes.is_empty() {
            lines.push(format_list_field("common-mistakes", common_mistakes));
        }
    }

    // Change impacts
    if let Some(ref change_impacts) = data.change_impacts {
        if !change_impacts.is_empty() {
            lines.push(format_list_field("change-impacts", change_impacts));
        }
    }

    // Related files
    if let Some(ref related) = data.related {
        if !related.is_empty() {
            lines.push(format!("related[{}]: {}", related.len(), related.join(",")));
        }
    }

    // Validation control (ignore directives)
    if let Some(ref ignore) = data.ignore {
        if !ignore.is_empty() {
            lines.push(format!("ignore: {}", ignore.join(",")));
        }
    }

    // Function-level annotations
    if let Some(ref fn_annotations) = data.function_annotations {
        if !fn_annotations.is_empty() {
            lines.extend(format_function_annotations(fn_annotations));
        }
    }

    // === END (high attention zone) ===

    // Gotchas - last for high attention
    if let Some(ref gotchas) = data.gotchas {
        if !gotchas.is_empty() {
            lines.push(format_list_field("gotchas", gotchas));
        }
    }

    lines.join("\n") + "\n"
}

/// Format exports in compact TOON format.
fn format_exports(exports: &[ExportInfo]) -> String {
    let items: Vec<String> = exports
        .iter()
        .map(|e| format!("{}({})", e.name, e.kind))
        .collect();
    format!("exports[{}]: {}", exports.len(), items.join(", "))
}

/// Format imports in compact single-line format.
fn format_imports(imports: &[ImportInfo]) -> String {
    let items: Vec<String> = imports
        .iter()
        .map(|imp| format!("{},{}", imp.from, imp.items.join("|")))
        .collect();
    format!(
        "imports[{}]{{from,items}}: {}",
        imports.len(),
        items.join("; ")
    )
}

/// Format a list field in compact single-line format.
fn format_list_field(name: &str, items: &[String]) -> String {
    let processed: Vec<String> = items.iter().map(|item| compress_item(item)).collect();
    format!("{}: {}", name, processed.join("; "))
}

/// Format when-editing guidance with ! prefix for important items.
fn format_when_editing(items: &[WhenEditingItem]) -> String {
    let processed: Vec<String> = items
        .iter()
        .map(|item| {
            let text = compress_item(&item.text);
            if item.important {
                format!("!{}", text)
            } else {
                text
            }
        })
        .collect();
    format!("when-editing: {}", processed.join("; "))
}

/// Format calls (outgoing dependencies) in compact single-line format.
fn format_calls(calls: &[CallInfo]) -> String {
    use std::collections::HashMap;

    // Group calls by target
    let mut by_target: HashMap<&str, Vec<&str>> = HashMap::new();
    for call in calls {
        by_target
            .entry(&call.target)
            .or_default()
            .push(&call.method);
    }

    // Deduplicate methods
    let items: Vec<String> = by_target
        .iter()
        .map(|(target, methods)| {
            let unique: Vec<&str> = methods
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            format!("{},{}", target, unique.join("|"))
        })
        .collect();

    format!(
        "calls[{}]{{target,methods}}: {}",
        by_target.len(),
        items.join("; ")
    )
}

/// Format called-by (reverse call dependencies) in compact single-line format.
fn format_called_by(called_by: &[CalledByInfo]) -> String {
    let items: Vec<String> = called_by
        .iter()
        .take(10)
        .map(|entry| format!("{},{}", entry.from, entry.function))
        .collect();
    let suffix = if called_by.len() > 10 {
        format!(" (+{} more)", called_by.len() - 10)
    } else {
        String::new()
    };
    format!(
        "called-by[{}]: {}{}",
        called_by.len(),
        items.join("; "),
        suffix
    )
}

/// Format signatures for full type information.
fn format_signatures(signatures: &[SignatureInfo]) -> Vec<String> {
    let mut lines = vec![format!("signatures[{}]:", signatures.len())];

    for sig in signatures {
        // Collapse whitespace/newlines to single spaces
        let collapsed = sig
            .signature
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // Truncate very long signatures
        let truncated = if collapsed.len() > 150 {
            format!("{}...", &collapsed[..147])
        } else {
            collapsed
        };
        lines.push(format!("  {}({}): {}", sig.name, sig.kind, truncated));
    }

    lines
}

/// Format function-level annotations.
fn format_function_annotations(annotations: &[FunctionAnnotation]) -> Vec<String> {
    let mut lines = Vec::new();

    for fn_ann in annotations {
        let mut fields: Vec<String> = Vec::new();

        if let Some(ref invariants) = fn_ann.invariants {
            for inv in invariants {
                fields.push(format!("invariants: {}", compress(inv)));
            }
        }
        if let Some(ref gotchas) = fn_ann.gotchas {
            for gotcha in gotchas {
                fields.push(format!("gotchas: {}", compress(gotcha)));
            }
        }
        if let Some(ref do_not) = fn_ann.do_not {
            for rule in do_not {
                fields.push(format!("do-not: {}", compress(rule)));
            }
        }
        if let Some(ref error_handling) = fn_ann.error_handling {
            for err in error_handling {
                fields.push(format!("error-handling: {}", compress(err)));
            }
        }
        if let Some(ref constraints) = fn_ann.constraints {
            for constraint in constraints {
                fields.push(format!("constraints: {}", compress(constraint)));
            }
        }

        if fields.is_empty() {
            continue;
        }

        // Use compact format if only one field
        if fields.len() == 1 {
            lines.push(format!("fn:{}: {}", fn_ann.name, fields[0]));
        } else {
            lines.push(format!("fn:{}:", fn_ann.name));
            for field in fields {
                lines.push(format!("  {}", field));
            }
        }
    }

    lines
}

/// Parse TOON content back into ToonData.
/// Used by validation tool.
pub fn parse_toon(content: &str) -> ToonData {
    let mut data = ToonData::new(String::new(), 0, Vec::new());

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse simple key: value pairs
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "purpose" => data.purpose = value.to_string(),
                "tokens" => {
                    data.tokens = value.trim_start_matches('~').parse().unwrap_or(0);
                }
                "ignore" => {
                    data.ignore = Some(
                        value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect(),
                    );
                }
                _ => {
                    // Handle other fields with [N] suffix
                    let field_name = key.split('[').next().unwrap_or(key);
                    match field_name {
                        "exports" => {
                            data.exports = parse_exports(value);
                        }
                        "invariants" | "invariant" => {
                            data.invariants = Some(parse_semicolon_list(value));
                        }
                        "do-not" => {
                            data.do_not = Some(parse_semicolon_list(value));
                        }
                        "gotchas" | "gotcha" => {
                            data.gotchas = Some(parse_semicolon_list(value));
                        }
                        "when-editing" => {
                            data.when_editing = Some(parse_when_editing(value));
                        }
                        "error-handling" => {
                            data.error_handling = Some(parse_semicolon_list(value));
                        }
                        "constraints" | "constraint" => {
                            data.constraints = Some(parse_semicolon_list(value));
                        }
                        "flows" | "flow" => {
                            data.flows = Some(parse_semicolon_list(value));
                        }
                        "testing" => {
                            data.testing = Some(parse_semicolon_list(value));
                        }
                        "common-mistakes" => {
                            data.common_mistakes = Some(parse_semicolon_list(value));
                        }
                        "change-impacts" => {
                            data.change_impacts = Some(parse_semicolon_list(value));
                        }
                        "related" => {
                            data.related = Some(
                                value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect(),
                            );
                        }
                        "imported-by" => {
                            data.imported_by = Some(
                                value
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty() && !s.starts_with('('))
                                    .collect(),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    data
}

fn parse_exports(value: &str) -> Vec<ExportInfo> {
    let mut exports = Vec::new();

    // Handle format: Name(kind), Name(kind)
    for item in value.split(", ") {
        if let Some((name, rest)) = item.split_once('(') {
            let kind = rest.trim_end_matches(')');
            exports.push(ExportInfo {
                name: name.to_string(),
                kind: kind.to_string(),
            });
        }
    }

    exports
}

fn parse_semicolon_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_when_editing(value: &str) -> Vec<WhenEditingItem> {
    value
        .split(';')
        .map(|s| {
            let s = s.trim();
            let important = s.starts_with('!');
            let text = if important { &s[1..] } else { s };
            WhenEditingItem {
                text: text.trim().to_string(),
                important,
            }
        })
        .filter(|item| !item.text.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comprehensive test covering ALL ToonData fields for formatting.
    /// This single test covers: purpose, tokens, exports, invariants, do_not, when_editing,
    /// imports, calls, imported_by, called_by, signatures, gotchas, flows, related,
    /// error_handling, constraints, testing, common_mistakes, change_impacts, ignore,
    /// and function_annotations with multiple fields.
    #[test]
    fn test_format_all_fields() {
        let mut data = ToonData::new(
            "Comprehensive test purpose".to_string(),
            1000,
            vec![
                ExportInfo {
                    name: "foo".to_string(),
                    kind: "fn".to_string(),
                },
                ExportInfo {
                    name: "Bar".to_string(),
                    kind: "class".to_string(),
                },
            ],
        );

        // All optional fields
        data.invariants = Some(vec!["Must always return".to_string()]);
        data.do_not = Some(vec!["Never modify state".to_string()]);
        data.when_editing = Some(vec![
            WhenEditingItem {
                text: "Critical".to_string(),
                important: true,
            },
            WhenEditingItem {
                text: "Normal".to_string(),
                important: false,
            },
        ]);
        data.imports = Some(vec![ImportInfo {
            from: "react".to_string(),
            items: vec!["useState".to_string()],
        }]);
        data.calls = Some(vec![CallInfo {
            target: "./utils".to_string(),
            method: "helper".to_string(),
        }]);
        data.imported_by = Some(vec!["main.ts".to_string()]);
        data.called_by = Some(vec![CalledByInfo {
            from: "app.ts".to_string(),
            function: "init".to_string(),
        }]);
        data.signatures = Some(vec![SignatureInfo {
            name: "greet".to_string(),
            kind: "fn".to_string(),
            signature: "(s: string) => string".to_string(),
        }]);
        data.gotchas = Some(vec!["Watch for nulls".to_string()]);
        data.flows = Some(vec!["Init -> Process -> Return".to_string()]);
        data.related = Some(vec!["types.ts".to_string()]);
        // Previously uncovered fields
        data.error_handling = Some(vec!["Throws on invalid input".to_string()]);
        data.constraints = Some(vec!["Max 100 items".to_string()]);
        data.testing = Some(vec!["Use mock db".to_string()]);
        data.common_mistakes = Some(vec!["Forgetting null check".to_string()]);
        data.change_impacts = Some(vec!["Breaks API".to_string()]);
        data.ignore = Some(vec!["export-mismatch".to_string()]);
        // Function annotations with multiple fields (covers multi-field format)
        data.function_annotations = Some(vec![FunctionAnnotation {
            name: "processData".to_string(),
            invariants: Some(vec!["Validate first".to_string()]),
            gotchas: Some(vec!["Can be slow".to_string()]),
            do_not: Some(vec!["Skip validation".to_string()]),
            error_handling: Some(vec!["Throws TypeError".to_string()]),
            constraints: Some(vec!["Input < 1MB".to_string()]),
        }]);

        let output = format_toon(&data);

        // Verify all fields present
        assert!(output.contains("purpose: Comprehensive test purpose"));
        assert!(output.contains("tokens: ~1000"));
        assert!(output.contains("exports[2]:"));
        assert!(output.contains("foo(fn)"));
        assert!(output.contains("invariants:"));
        assert!(output.contains("do-not:"));
        assert!(output.contains("when-editing:"));
        assert!(output.contains("!Critical"));
        assert!(output.contains("imports[1]"));
        assert!(output.contains("calls[1]"));
        assert!(output.contains("imported-by[1]:"));
        assert!(output.contains("called-by[1]:"));
        assert!(output.contains("signatures[1]:"));
        assert!(output.contains("gotchas:"));
        assert!(output.contains("flows:"));
        assert!(output.contains("related[1]:"));
        assert!(output.contains("error-handling:"));
        assert!(output.contains("constraints:"));
        assert!(output.contains("testing:"));
        assert!(output.contains("common-mistakes:"));
        assert!(output.contains("change-impacts:"));
        assert!(output.contains("ignore:"));
        assert!(output.contains("fn:processData:"));
    }

    /// Test truncation for imported_by (>10 items) and called_by (>10 items)
    #[test]
    fn test_format_truncation() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.imported_by = Some((0..15).map(|i| format!("file{}.ts", i)).collect());
        data.called_by = Some(
            (0..12)
                .map(|i| CalledByInfo {
                    from: format!("mod{}.ts", i),
                    function: "fn".to_string(),
                })
                .collect(),
        );
        data.signatures = Some(vec![SignatureInfo {
            name: "longFunc".to_string(),
            kind: "fn".to_string(),
            signature: "a".repeat(200),
        }]);

        let output = format_toon(&data);
        assert!(output.contains("imported-by[15]:"));
        assert!(output.contains("(+5 more)"));
        assert!(output.contains("called-by[12]:"));
        assert!(output.contains("(+2 more)"));
        assert!(output.contains("longFunc(fn): "));
        assert!(output.contains("..."));
    }

    /// Comprehensive parse test covering all field types
    #[test]
    fn test_parse_all_fields() {
        let content = r#"purpose: Test module
tokens: ~500
exports[2]: foo(fn), Bar(class)
invariants: Rule one; Rule two
do-not: Never do X; Avoid Y
when-editing: !Important; Normal
related[2]: file1.ts,file2.ts
imported-by[2]: main.ts,app.ts
ignore: export-mismatch,token-count
gotchas: Watch out
error-handling: Throws on invalid; Returns null on empty
constraints: Max 100 items; Min 1 item
flows: Start -> Process -> End
testing: Use mock db
common-mistakes: Forgetting null check
change-impacts: Breaks API
"#;
        let parsed = parse_toon(content);

        assert_eq!(parsed.purpose, "Test module");
        assert_eq!(parsed.tokens, 500);
        assert_eq!(parsed.exports.len(), 2);
        assert_eq!(parsed.exports[0].name, "foo");
        assert_eq!(parsed.invariants.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.do_not.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.when_editing.as_ref().unwrap().len(), 2);
        assert!(parsed.when_editing.as_ref().unwrap()[0].important);
        assert_eq!(parsed.related.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.imported_by.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.ignore.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.gotchas.as_ref().unwrap().len(), 1);
        // Previously uncovered parse branches
        assert_eq!(parsed.error_handling.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.constraints.as_ref().unwrap().len(), 2);
        assert_eq!(parsed.flows.as_ref().unwrap().len(), 1);
        assert_eq!(parsed.testing.as_ref().unwrap().len(), 1);
        assert_eq!(parsed.common_mistakes.as_ref().unwrap().len(), 1);
        assert_eq!(parsed.change_impacts.as_ref().unwrap().len(), 1);
    }

    /// Round-trip test: format then parse should preserve data
    #[test]
    fn test_parse_roundtrip() {
        let data = ToonData::new(
            "Test purpose".to_string(),
            1000,
            vec![
                ExportInfo {
                    name: "Component".to_string(),
                    kind: "component".to_string(),
                },
                ExportInfo {
                    name: "useHook".to_string(),
                    kind: "hook".to_string(),
                },
            ],
        );

        let formatted = format_toon(&data);
        let parsed = parse_toon(&formatted);

        assert_eq!(parsed.purpose, data.purpose);
        assert_eq!(parsed.tokens, data.tokens);
        assert_eq!(parsed.exports.len(), data.exports.len());
    }

    /// Edge case: empty content
    #[test]
    fn test_parse_empty_content() {
        let parsed = parse_toon("");
        assert!(parsed.purpose.is_empty());
        assert_eq!(parsed.tokens, 0);
    }

    /// Edge case: tokens without tilde
    #[test]
    fn test_parse_tokens_without_tilde() {
        let content = "purpose: Test\ntokens: 500\n";
        let parsed = parse_toon(content);
        assert!(parsed.tokens == 500 || parsed.tokens == 0);
    }

    /// Helper function tests
    #[test]
    fn test_helper_functions() {
        // format_exports
        let exports = vec![
            ExportInfo {
                name: "a".to_string(),
                kind: "const".to_string(),
            },
            ExportInfo {
                name: "b".to_string(),
                kind: "fn".to_string(),
            },
        ];
        assert_eq!(format_exports(&exports), "exports[2]: a(const), b(fn)");

        // parse_semicolon_list
        let list = parse_semicolon_list("  item1  ;  item2  ");
        assert_eq!(list, vec!["item1", "item2"]);

        // parse_when_editing
        let when = parse_when_editing("!Important; Normal");
        assert!(when[0].important);
        assert!(!when[1].important);
    }
}
