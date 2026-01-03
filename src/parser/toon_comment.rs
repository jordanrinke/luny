//! Shared @dose comment parsing logic used by all language parsers.
//!
//! This module extracts the common parsing code that was duplicated across
//! TypeScript, Python, Ruby, C#, Go, and Rust parsers.

use crate::types::{ToonCommentBlock, WhenEditingItem};

/// Normalize a section name: lowercase, spaces to dashes, strip trailing colon.
fn normalize_section(s: &str) -> String {
    s.to_lowercase()
        .replace(' ', "-")
        .trim_end_matches(':')
        .to_string()
}

/// Check if a line starts with a section header. Returns (header_name, inline_content) if found.
/// Handles both "header:" and "header: inline content" formats.
fn parse_section_start(line: &str) -> Option<(String, Option<&str>)> {
    let colon_pos = line.find(':')?;
    let name = line[..colon_pos].trim();

    // Must be 1-3 words, letters/dashes/spaces only
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphabetic() || c == '-' || c == ' ')
        || name.split_whitespace().count() > 3
    {
        return None;
    }

    let after_colon = line[colon_pos + 1..].trim();
    let inline_content = if after_colon.is_empty() {
        None
    } else {
        Some(after_colon)
    };

    Some((normalize_section(name), inline_content))
}

/// Parse semicolon-separated items from a string.
fn parse_inline_items(s: &str) -> Vec<String> {
    s.split(';')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Parse @dose block content into a ToonCommentBlock.
/// Supports both multi-line format with - prefixes and compact semicolon-separated format.
pub fn parse_toon_block(content: &str) -> ToonCommentBlock {
    let mut block = ToonCommentBlock::default();
    let mut current_section: Option<String> = None;
    let mut current_items: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((header, inline_content)) = parse_section_start(trimmed) {
            save_section(&mut block, current_section.as_deref(), &current_items);
            current_items.clear();

            // Handle inline semicolon-separated items (but not for purpose - keep as single string)
            if let Some(content) = inline_content {
                if header == "purpose" {
                    current_items.push(content.to_string());
                } else {
                    current_items.extend(parse_inline_items(content));
                }
            }
            current_section = Some(header);
        } else if current_section.is_none() && block.purpose.is_none() {
            // First non-section line is purpose
            block.purpose = Some(trimmed.to_string());
        } else if trimmed.starts_with('-') || trimmed.starts_with('•') {
            let item = trimmed
                .trim_start_matches('-')
                .trim_start_matches('•')
                .trim();
            if !item.is_empty() {
                // Also support semicolons within bulleted items
                current_items.extend(parse_inline_items(item));
            }
        } else if current_section.is_some() {
            // Continuation line - also support semicolons
            current_items.extend(parse_inline_items(trimmed));
        }
    }

    save_section(&mut block, current_section.as_deref(), &current_items);
    block
}

/// Check if a line is a section header and return the normalized header name.
pub fn parse_section_header(line: &str) -> Option<String> {
    parse_section_start(line).map(|(header, _)| header)
}

/// Save accumulated items to the appropriate field in ToonCommentBlock.
pub fn save_section(block: &mut ToonCommentBlock, section: Option<&str>, items: &[String]) {
    if items.is_empty() {
        return;
    }

    let section = match section {
        Some(s) => s,
        None => return,
    };

    // Normalize: already lowercase with dashes, just handle singular/plural
    let normalized = section.trim_end_matches('s');

    match normalized {
        "purpose" => {
            // Join items into single string (purpose doesn't split on semicolons)
            block.purpose = Some(items.join(" "));
        }
        "when-editing" => {
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
        "do-not" => block.do_not = Some(items.to_vec()),
        "invariant" => block.invariants = Some(items.to_vec()),
        "error-handling" => block.error_handling = Some(items.to_vec()),
        "constraint" => block.constraints = Some(items.to_vec()),
        "gotcha" => block.gotchas = Some(items.to_vec()),
        "flow" => block.flows = Some(items.to_vec()),
        "testing" => block.testing = Some(items.to_vec()),
        "common-mistake" => block.common_mistakes = Some(items.to_vec()),
        "change-impact" => block.change_impacts = Some(items.to_vec()),
        "related" => block.related = Some(items.to_vec()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toon_block() {
        let content = r#"purpose: Test module
When Editing: !Critical; Normal
DO-NOT: Never do this
invariant: Must hold
"#;
        let block = parse_toon_block(content);

        assert_eq!(block.purpose, Some("Test module".to_string()));

        let we = block.when_editing.unwrap();
        assert_eq!(we.len(), 2);
        assert_eq!(
            we[0],
            WhenEditingItem {
                text: "Critical".to_string(),
                important: true
            }
        );
        assert_eq!(
            we[1],
            WhenEditingItem {
                text: "Normal".to_string(),
                important: false
            }
        );

        assert_eq!(block.do_not, Some(vec!["Never do this".to_string()]));
        assert_eq!(block.invariants, Some(vec!["Must hold".to_string()]));
    }
}
