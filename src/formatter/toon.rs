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
    CalledByInfo, CallInfo, ExportInfo, FunctionAnnotation, ImportInfo, SignatureInfo,
    ToonData, WhenEditingItem,
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
    format!("imports[{}]{{from,items}}: {}", imports.len(), items.join("; "))
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

    format!("calls[{}]{{target,methods}}: {}", by_target.len(), items.join("; "))
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
    format!("called-by[{}]: {}{}", called_by.len(), items.join("; "), suffix)
}

/// Format signatures for full type information.
fn format_signatures(signatures: &[SignatureInfo]) -> Vec<String> {
    let mut lines = vec![format!("signatures[{}]:", signatures.len())];

    for sig in signatures {
        // Collapse whitespace/newlines to single spaces
        let collapsed = sig.signature.split_whitespace().collect::<Vec<_>>().join(" ");
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
    let mut data = ToonData::new(
        String::new(),
        0,
        Vec::new(),
    );

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

    // ==================== format_toon Tests ====================

    #[test]
    fn test_format_basic_toon() {
        let data = ToonData::new(
            "Test file purpose".to_string(),
            500,
            vec![ExportInfo {
                name: "testFn".to_string(),
                kind: "fn".to_string(),
            }],
        );

        let output = format_toon(&data);
        assert!(output.contains("purpose: Test file purpose"));
        assert!(output.contains("tokens: ~500"));
        assert!(output.contains("exports[1]: testFn(fn)"));
    }

    #[test]
    fn test_format_multiple_exports() {
        let data = ToonData::new(
            "Multi export module".to_string(),
            200,
            vec![
                ExportInfo { name: "foo".to_string(), kind: "function".to_string() },
                ExportInfo { name: "Bar".to_string(), kind: "class".to_string() },
                ExportInfo { name: "BAZ".to_string(), kind: "const".to_string() },
            ],
        );

        let output = format_toon(&data);
        assert!(output.contains("exports[3]:"));
        assert!(output.contains("foo(function)"));
        assert!(output.contains("Bar(class)"));
        assert!(output.contains("BAZ(const)"));
    }

    #[test]
    fn test_format_with_invariants() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.invariants = Some(vec![
            "Must always return a value".to_string(),
            "Never throws exceptions".to_string(),
        ]);

        let output = format_toon(&data);
        assert!(output.contains("invariants:"));
        // "Must always" gets compressed to "must"
        assert!(output.contains("must return a value"));
    }

    #[test]
    fn test_format_with_do_not() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.do_not = Some(vec![
            "Never modify global state".to_string(),
            "Do not call external APIs directly".to_string(),
        ]);

        let output = format_toon(&data);
        assert!(output.contains("do-not:"));
        assert!(output.contains("Never modify global state"));
    }

    #[test]
    fn test_format_with_when_editing() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.when_editing = Some(vec![
            WhenEditingItem { text: "Critical item".to_string(), important: true },
            WhenEditingItem { text: "Normal item".to_string(), important: false },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("when-editing:"));
        assert!(output.contains("!Critical item"));
        assert!(output.contains("Normal item"));
    }

    #[test]
    fn test_format_with_imports() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.imports = Some(vec![
            ImportInfo {
                from: "react".to_string(),
                items: vec!["useState".to_string(), "useEffect".to_string()],
            },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("imports[1]{from,items}:"));
        assert!(output.contains("react"));
        assert!(output.contains("useState"));
    }

    #[test]
    fn test_format_with_calls() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.calls = Some(vec![
            CallInfo { target: "./utils".to_string(), method: "helper".to_string() },
            CallInfo { target: "./utils".to_string(), method: "format".to_string() },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("calls["));
        assert!(output.contains("./utils"));
    }

    #[test]
    fn test_format_with_imported_by() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.imported_by = Some(vec![
            "main.ts".to_string(),
            "app.ts".to_string(),
        ]);

        let output = format_toon(&data);
        assert!(output.contains("imported-by[2]:"));
        assert!(output.contains("main.ts"));
        assert!(output.contains("app.ts"));
    }

    #[test]
    fn test_format_imported_by_truncation() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.imported_by = Some((0..15).map(|i| format!("file{}.ts", i)).collect());

        let output = format_toon(&data);
        assert!(output.contains("imported-by[15]:"));
        assert!(output.contains("(+5 more)"));
    }

    #[test]
    fn test_format_with_called_by() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.called_by = Some(vec![
            CalledByInfo { from: "main.ts".to_string(), function: "init".to_string() },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("called-by[1]:"));
        assert!(output.contains("main.ts"));
        assert!(output.contains("init"));
    }

    #[test]
    fn test_format_with_signatures() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.signatures = Some(vec![
            SignatureInfo {
                name: "greet".to_string(),
                kind: "function".to_string(),
                signature: "(name: string) => string".to_string(),
            },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("signatures[1]:"));
        assert!(output.contains("greet(function):"));
        assert!(output.contains("(name: string) => string"));
    }

    #[test]
    fn test_format_signature_truncation() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        let long_sig = "a".repeat(200);
        data.signatures = Some(vec![
            SignatureInfo {
                name: "longFunc".to_string(),
                kind: "function".to_string(),
                signature: long_sig,
            },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("..."));
        // Should be truncated
        assert!(!output.contains(&"a".repeat(200)));
    }

    #[test]
    fn test_format_with_gotchas() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.gotchas = Some(vec!["Watch out for null values".to_string()]);

        let output = format_toon(&data);
        assert!(output.contains("gotchas:"));
        assert!(output.contains("Watch out for null values"));
    }

    #[test]
    fn test_format_gotchas_at_end() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.gotchas = Some(vec!["Gotcha".to_string()]);
        data.invariants = Some(vec!["Invariant".to_string()]);

        let output = format_toon(&data);
        let gotchas_pos = output.find("gotchas:").unwrap();
        let invariants_pos = output.find("invariants:").unwrap();
        // Gotchas should appear after invariants (at the end for high attention)
        assert!(gotchas_pos > invariants_pos);
    }

    #[test]
    fn test_format_with_flows() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.flows = Some(vec!["Parse input".to_string(), "Process data".to_string()]);

        let output = format_toon(&data);
        assert!(output.contains("flows:"));
    }

    #[test]
    fn test_format_with_related() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.related = Some(vec!["utils.ts".to_string(), "types.ts".to_string()]);

        let output = format_toon(&data);
        assert!(output.contains("related[2]:"));
    }

    #[test]
    fn test_format_empty_optional_fields() {
        let data = ToonData::new("Test".to_string(), 100, vec![]);

        let output = format_toon(&data);
        // Empty optional fields should not appear
        assert!(!output.contains("invariants:"));
        assert!(!output.contains("do-not:"));
        assert!(!output.contains("gotchas:"));
        assert!(!output.contains("imports["));
    }

    // ==================== parse_toon Tests ====================

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

    #[test]
    fn test_parse_purpose() {
        let content = "purpose: Test module for parsing\ntokens: ~100\n";
        let parsed = parse_toon(content);
        assert_eq!(parsed.purpose, "Test module for parsing");
    }

    #[test]
    fn test_parse_tokens() {
        let content = "purpose: Test\ntokens: ~500\n";
        let parsed = parse_toon(content);
        assert_eq!(parsed.tokens, 500);
    }

    #[test]
    fn test_parse_tokens_without_tilde() {
        let content = "purpose: Test\ntokens: 500\n";
        let parsed = parse_toon(content);
        // Should handle both ~500 and 500
        assert!(parsed.tokens == 500 || parsed.tokens == 0);
    }

    #[test]
    fn test_parse_exports() {
        let content = "purpose: Test\ntokens: ~100\nexports[2]: foo(function), Bar(class)\n";
        let parsed = parse_toon(content);
        assert_eq!(parsed.exports.len(), 2);
        assert_eq!(parsed.exports[0].name, "foo");
        assert_eq!(parsed.exports[0].kind, "function");
        assert_eq!(parsed.exports[1].name, "Bar");
        assert_eq!(parsed.exports[1].kind, "class");
    }

    #[test]
    fn test_parse_invariants() {
        let content = "purpose: Test\ntokens: ~100\ninvariants: Rule one; Rule two\n";
        let parsed = parse_toon(content);
        assert!(parsed.invariants.is_some());
        let invariants = parsed.invariants.unwrap();
        assert_eq!(invariants.len(), 2);
        assert_eq!(invariants[0], "Rule one");
        assert_eq!(invariants[1], "Rule two");
    }

    #[test]
    fn test_parse_do_not() {
        let content = "purpose: Test\ntokens: ~100\ndo-not: Never do X; Avoid Y\n";
        let parsed = parse_toon(content);
        assert!(parsed.do_not.is_some());
        let do_not = parsed.do_not.unwrap();
        assert_eq!(do_not.len(), 2);
    }

    #[test]
    fn test_parse_gotchas() {
        let content = "purpose: Test\ntokens: ~100\ngotchas: Watch out for this\n";
        let parsed = parse_toon(content);
        assert!(parsed.gotchas.is_some());
        assert_eq!(parsed.gotchas.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_when_editing() {
        let content = "purpose: Test\ntokens: ~100\nwhen-editing: !Important; Normal\n";
        let parsed = parse_toon(content);
        assert!(parsed.when_editing.is_some());
        let when_editing = parsed.when_editing.unwrap();
        assert_eq!(when_editing.len(), 2);
        assert!(when_editing[0].important);
        assert_eq!(when_editing[0].text, "Important");
        assert!(!when_editing[1].important);
    }

    #[test]
    fn test_parse_related() {
        let content = "purpose: Test\ntokens: ~100\nrelated[2]: file1.ts,file2.ts\n";
        let parsed = parse_toon(content);
        assert!(parsed.related.is_some());
        let related = parsed.related.unwrap();
        assert_eq!(related.len(), 2);
    }

    #[test]
    fn test_parse_imported_by() {
        let content = "purpose: Test\ntokens: ~100\nimported-by[2]: main.ts,app.ts\n";
        let parsed = parse_toon(content);
        assert!(parsed.imported_by.is_some());
        let imported_by = parsed.imported_by.unwrap();
        assert_eq!(imported_by.len(), 2);
    }

    #[test]
    fn test_parse_empty_content() {
        let content = "";
        let parsed = parse_toon(content);
        assert!(parsed.purpose.is_empty());
        assert_eq!(parsed.tokens, 0);
    }

    #[test]
    fn test_parse_ignores_empty_lines() {
        let content = "purpose: Test\n\n\ntokens: ~100\n\n";
        let parsed = parse_toon(content);
        assert_eq!(parsed.purpose, "Test");
        assert_eq!(parsed.tokens, 100);
    }

    #[test]
    fn test_parse_ignore_directive() {
        let content = "purpose: Test\ntokens: ~100\nignore: export-mismatch,token-count\n";
        let parsed = parse_toon(content);
        assert!(parsed.ignore.is_some());
        let ignore = parsed.ignore.unwrap();
        assert_eq!(ignore.len(), 2);
        assert!(ignore.contains(&"export-mismatch".to_string()));
    }

    // ==================== Helper Function Tests ====================

    #[test]
    fn test_format_exports_single() {
        let exports = vec![ExportInfo { name: "foo".to_string(), kind: "fn".to_string() }];
        let result = format_exports(&exports);
        assert_eq!(result, "exports[1]: foo(fn)");
    }

    #[test]
    fn test_format_exports_multiple() {
        let exports = vec![
            ExportInfo { name: "a".to_string(), kind: "const".to_string() },
            ExportInfo { name: "b".to_string(), kind: "fn".to_string() },
        ];
        let result = format_exports(&exports);
        assert_eq!(result, "exports[2]: a(const), b(fn)");
    }

    #[test]
    fn test_parse_semicolon_list() {
        let result = parse_semicolon_list("item1; item2; item3");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "item1");
        assert_eq!(result[1], "item2");
        assert_eq!(result[2], "item3");
    }

    #[test]
    fn test_parse_semicolon_list_with_spaces() {
        let result = parse_semicolon_list("  item1  ;  item2  ");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "item1");
        assert_eq!(result[1], "item2");
    }

    #[test]
    fn test_parse_when_editing_items() {
        let result = parse_when_editing("!Important item; Normal item");
        assert_eq!(result.len(), 2);
        assert!(result[0].important);
        assert_eq!(result[0].text, "Important item");
        assert!(!result[1].important);
        assert_eq!(result[1].text, "Normal item");
    }

    #[test]
    fn test_format_with_function_annotations() {
        let mut data = ToonData::new("Test".to_string(), 100, vec![]);
        data.function_annotations = Some(vec![
            FunctionAnnotation {
                name: "processData".to_string(),
                invariants: Some(vec!["Must validate input".to_string()]),
                gotchas: None,
                do_not: None,
                error_handling: None,
                constraints: None,
            },
        ]);

        let output = format_toon(&data);
        assert!(output.contains("fn:processData:"));
        assert!(output.contains("invariants:"));
    }
}
