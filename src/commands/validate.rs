//! @toon
//! purpose: This module implements the validate command that checks .toon files for
//!     correctness and consistency with their source files. It verifies exports match,
//!     token counts are within limits, and required fields are present.
//!
//! when-editing:
//!     - !Validation compares TOON exports against current source file exports
//!     - !Token thresholds can be customized via command-line args
//!     - Errors cause command failure; warnings are logged but don't fail by default
//!
//! invariants:
//!     - A TOON file is valid only if its source file exists
//!     - The purpose field is always required
//!     - Export mismatches generate warnings, not errors
//!
//! do-not:
//!     - Never modify TOON files during validation
//!     - Never fail on warnings unless --strict is specified
//!
//! gotchas:
//!     - TOON path to source path conversion strips .ai/ prefix and .toon suffix
//!     - Only purpose is required; all other semantic fields are optional
//!
//! flows:
//!     - Walk: Find all .toon files in .ai/ directory
//!     - Validate: For each TOON, parse it, find source, compare exports, check thresholds

use crate::cli::ValidateArgs;
use crate::formatter::{format_toon, parse_toon};
use crate::parser::ParserFactory;
use crate::types::{ToonData, ValidationResult};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

pub fn run_validate(args: &ValidateArgs, root: &Path, verbose: bool) -> Result<()> {
    let factory = ParserFactory::new();
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    // Determine paths to process
    let paths = if args.paths.is_empty() {
        vec![root.join(".ai")]
    } else {
        args.paths.clone()
    };

    let mut valid = 0;
    let mut invalid = 0;
    let mut warnings = 0;

    for path in paths {
        let full_path = if path.is_absolute() {
            path
        } else {
            root.join(&path)
        };

        if !full_path.exists() {
            if verbose {
                println!("No .ai directory found at {}", full_path.display());
            }
            continue;
        }

        // Deterministic ordering: collect all .toon paths and sort before validation.
        let mut toon_files: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&full_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                !is_excluded_dir(e) && is_allowed_symlink_target(e, &root_canon, args.unsafe_follow)
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "toon").unwrap_or(false) {
                toon_files.push(path.to_path_buf());
            }
        }
        toon_files.sort();

        for toon_path in toon_files {
            match validate_toon_file(&toon_path, &factory, args, root, verbose) {
                Ok(mut result) => {
                    // Optional fix-up pass: regenerate invalid TOON files and re-validate.
                    if args.fix && !result.is_valid() {
                        if verbose {
                            println!("Fixing: {}", toon_path.display());
                        }
                        if let Err(e) = fix_toon_file(&toon_path, &factory, root) {
                            eprintln!("Error fixing {}: {}", toon_path.display(), e);
                        } else {
                            // Re-validate after fix attempt (counts reflect final state).
                            result = validate_toon_file(&toon_path, &factory, args, root, verbose)?;
                        }
                    }

                    if result.errors.is_empty() {
                        valid += 1;
                        if !result.warnings.is_empty() {
                            warnings += result.warnings.len();
                        }
                    } else {
                        invalid += 1;
                        for err in &result.errors {
                            eprintln!("ERROR [{}]: {}", toon_path.display(), err);
                        }
                    }
                    for warn in &result.warnings {
                        if verbose || args.strict {
                            eprintln!("WARN [{}]: {}", toon_path.display(), warn);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error validating {}: {}", toon_path.display(), e);
                    invalid += 1;
                }
            }
        }
    }

    println!(
        "Valid: {}, Invalid: {}, Warnings: {}",
        valid, invalid, warnings
    );

    if invalid > 0 || (args.strict && warnings > 0) {
        anyhow::bail!("Validation failed");
    }

    Ok(())
}

fn is_excluded_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    matches!(
        entry.file_name().to_string_lossy().as_ref(),
        ".git" | "node_modules" | "target" | "__pycache__"
    )
}

fn is_allowed_symlink_target(entry: &DirEntry, root_canon: &Path, unsafe_follow: bool) -> bool {
    if unsafe_follow {
        return true;
    }
    if !entry.path_is_symlink() {
        return true;
    }
    match entry.path().canonicalize() {
        Ok(real) => real.starts_with(root_canon),
        Err(_) => false,
    }
}

fn validate_toon_file(
    toon_path: &Path,
    factory: &ParserFactory,
    args: &ValidateArgs,
    root: &Path,
    verbose: bool,
) -> Result<ValidationResult> {
    // Compute source path from TOON path
    // .ai/path/to/file.ts.toon -> path/to/file.ts
    let Some(source_path) = try_toon_path_to_source_path(toon_path, root) else {
        let mut result = ValidationResult::new(
            "<unknown>".to_string(),
            toon_path.to_string_lossy().to_string(),
        );
        result.add_error("TOON file is outside the .ai/ directory for this root");
        return Ok(result);
    };

    let mut result = ValidationResult::new(
        source_path.to_string_lossy().to_string(),
        toon_path.to_string_lossy().to_string(),
    );

    // Read TOON file
    let toon_content = fs::read_to_string(toon_path).context("Failed to read TOON file")?;
    let toon_data = parse_toon(&toon_content);

    // Check if source file exists
    if !source_path.exists() {
        result.add_error("Source file no longer exists");
        return Ok(result);
    }

    // Validate required fields
    if toon_data.purpose.is_empty() {
        result.add_error("Missing required field: purpose");
    }

    // Get parser for source file
    if let Some(parser) = factory.get_parser(&source_path) {
        // Read and parse source
        let source = fs::read_to_string(&source_path).context("Failed to read source file")?;
        let ast_info = parser.extract_ast_info(&source, &source_path)?;

        // Check token count
        if ast_info.tokens > args.token_error {
            result.add_error(format!(
                "Token count {} exceeds error threshold {}",
                ast_info.tokens, args.token_error
            ));
        } else if ast_info.tokens > args.token_warn {
            result.add_warning(format!(
                "Token count {} exceeds warning threshold {}",
                ast_info.tokens, args.token_warn
            ));
        }

        // Check exports match
        let toon_exports: std::collections::HashSet<_> =
            toon_data.exports.iter().map(|e| &e.name).collect();
        let source_exports: std::collections::HashSet<_> =
            ast_info.exports.iter().map(|e| &e.name).collect();

        for missing in source_exports.difference(&toon_exports) {
            result.add_warning(format!("Export '{}' not documented in TOON", missing));
        }

        for extra in toon_exports.difference(&source_exports) {
            result.add_warning(format!(
                "TOON documents '{}' which no longer exists in source",
                extra
            ));
        }

        if verbose {
            println!(
                "Validated: {} ({} tokens, {} exports)",
                toon_path.display(),
                ast_info.tokens,
                ast_info.exports.len()
            );
        }
    } else {
        result.add_warning("Could not find parser for source file");
    }

    Ok(result)
}

fn try_toon_path_to_source_path(toon_path: &Path, root: &Path) -> Option<PathBuf> {
    let relative = toon_path.strip_prefix(root.join(".ai")).ok()?;
    Some(root.join(relative.to_string_lossy().trim_end_matches(".toon")))
}

fn fix_toon_file(toon_path: &Path, factory: &ParserFactory, root: &Path) -> Result<()> {
    let source_path = try_toon_path_to_source_path(toon_path, root)
        .context("TOON file is outside the .ai/ directory for this root")?;
    if !source_path.exists() {
        anyhow::bail!("Source file no longer exists");
    }

    let parser = factory
        .get_parser(&source_path)
        .context("Could not find parser for source file")?;

    let source = fs::read_to_string(&source_path).context("Failed to read source file")?;
    let ast_info = parser.extract_ast_info(&source, &source_path)?;
    let comments = parser.extract_toon_comments(&source)?;

    let purpose = comments
        .file_block
        .as_ref()
        .and_then(|b| b.purpose.clone())
        .unwrap_or_else(|| {
            let filename = source_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            format!("{} module", filename)
        });

    let mut toon_data = ToonData::new(purpose, ast_info.tokens, ast_info.exports);

    if !ast_info.imports.is_empty() {
        toon_data.imports = Some(ast_info.imports);
    }
    if !ast_info.calls.is_empty() {
        toon_data.calls = Some(ast_info.calls);
    }
    if !ast_info.signatures.is_empty() {
        toon_data.signatures = Some(ast_info.signatures);
    }

    if let Some(ref block) = comments.file_block {
        toon_data.when_editing = block.when_editing.clone();
        toon_data.do_not = block.do_not.clone();
        toon_data.invariants = block.invariants.clone();
        toon_data.error_handling = block.error_handling.clone();
        toon_data.constraints = block.constraints.clone();
        toon_data.gotchas = block.gotchas.clone();
        toon_data.flows = block.flows.clone();
        toon_data.testing = block.testing.clone();
        toon_data.common_mistakes = block.common_mistakes.clone();
        toon_data.change_impacts = block.change_impacts.clone();
        toon_data.related = block.related.clone();
    }

    let content = format_toon(&toon_data);

    if let Some(parent) = toon_path.parent() {
        fs::create_dir_all(parent).context("Failed to create output directory")?;
    }
    fs::write(toon_path, content).context("Failed to write TOON file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== ValidationResult Tests ====================

    #[test]
    fn test_validation_result_new() {
        let result = ValidationResult::new(
            "src/main.ts".to_string(),
            ".ai/src/main.ts.toon".to_string(),
        );
        assert_eq!(result.source_path, "src/main.ts");
        assert_eq!(result.toon_path, ".ai/src/main.ts.toon");
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validation_result_add_error() {
        let mut result = ValidationResult::new("test.ts".to_string(), "test.ts.toon".to_string());
        result.add_error("Missing required field");
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0], "Missing required field");
    }

    #[test]
    fn test_validation_result_add_warning() {
        let mut result = ValidationResult::new("test.ts".to_string(), "test.ts.toon".to_string());
        result.add_warning("Token count is high");
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0], "Token count is high");
    }

    // ==================== validate_toon_file Tests ====================

    fn create_test_env() -> (TempDir, ParserFactory) {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create .ai directory
        fs::create_dir(temp_dir.path().join(".ai")).unwrap();

        (temp_dir, factory)
    }

    #[test]
    fn test_validate_missing_source_file() {
        let (temp_dir, factory) = create_test_env();

        // Create TOON file without source
        let toon_content = "purpose: Test module\ntokens: ~100\nexports[0]:";
        fs::write(temp_dir.path().join(".ai/missing.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/missing.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("no longer exists"));
    }

    #[test]
    fn test_validate_missing_purpose() {
        let (temp_dir, factory) = create_test_env();

        // Create source file
        fs::write(temp_dir.path().join("test.ts"), "export const x = 1;").unwrap();

        // Create TOON file without purpose
        let toon_content = "tokens: ~100\nexports[1]: x(const)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| e.contains("purpose")));
    }

    #[test]
    fn test_validate_token_warning() {
        let (temp_dir, factory) = create_test_env();

        // Create source file with content
        let source = "export const x = 1;\nexport function foo() { return 42; }";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create valid TOON file
        let toon_content = "purpose: Test module\ntokens: ~50\nexports[2]: x(const), foo(function)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 10, // Very low threshold to trigger warning
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Should have warning about token count
        assert!(!result.warnings.is_empty() || !result.errors.is_empty());
    }

    #[test]
    fn test_validate_token_error() {
        let (temp_dir, factory) = create_test_env();

        // Create source file
        let source = "export const x = 1;\nexport function foo() { return 42; }";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create valid TOON file
        let toon_content = "purpose: Test module\ntokens: ~50\nexports[2]: x(const), foo(function)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 5,
            token_error: 10, // Very low threshold to trigger error
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Should have error about token count
        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| e.contains("Token count")));
    }

    #[test]
    fn test_validate_missing_export_warning() {
        let (temp_dir, factory) = create_test_env();

        // Create source file with exports
        let source = "export const x = 1;\nexport function foo() {}";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create TOON file missing one export
        let toon_content = "purpose: Test module\ntokens: ~50\nexports[1]: x(const)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Should have warning about missing export 'foo'
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("foo") && w.contains("not documented")));
    }

    #[test]
    fn test_validate_extra_export_warning() {
        let (temp_dir, factory) = create_test_env();

        // Create source file with one export
        let source = "export const x = 1;";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create TOON file with extra export
        let toon_content =
            "purpose: Test module\ntokens: ~50\nexports[2]: x(const), removed(function)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Should have warning about extra export
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("removed") && w.contains("no longer exists")));
    }

    #[test]
    fn test_validate_purpose_only_is_valid() {
        let (temp_dir, factory) = create_test_env();

        // Create source file
        let source = "export const x = 1;";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create TOON file with only purpose (all other semantic fields optional)
        let toon_content = "purpose: Test module\ntokens: ~50\nexports[1]: x(const)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Purpose-only TOON files are valid - no warnings for missing semantic content
        assert!(result.is_valid());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_valid_toon_file() {
        let (temp_dir, factory) = create_test_env();

        // Create source file
        let source = "export const x = 1;";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create valid TOON file with semantic content
        let toon_content = r#"purpose: Test module
tokens: ~50
exports[1]: x(const)
invariants: Must always export x
gotchas: None
"#;
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = validate_toon_file(
            &temp_dir.path().join(".ai/test.ts.toon"),
            &factory,
            &args,
            temp_dir.path(),
            false,
        )
        .unwrap();

        // Should have no errors (may have some minor warnings)
        assert!(result.errors.is_empty());
    }

    // ==================== run_validate Tests ====================

    #[test]
    fn test_run_validate_no_ai_directory() {
        let temp_dir = TempDir::new().unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        // Should succeed even without .ai directory
        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_empty_ai_directory() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".ai")).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_with_valid_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".ai")).unwrap();

        // Create source file
        let source = "export const x = 1;";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create valid TOON file
        let toon_content = r#"purpose: Test module
tokens: ~50
exports[1]: x(const)
invariants: Always export x
"#;
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_fails_with_errors() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".ai")).unwrap();

        // Create TOON file without source
        let toon_content = "purpose: Test module\ntokens: ~100\nexports[0]:";
        fs::write(temp_dir.path().join(".ai/missing.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_validate_strict_mode_fails_on_warnings() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".ai")).unwrap();

        // Create source file with two exports
        let source = "export const x = 1;\nexport const y = 2;";
        fs::write(temp_dir.path().join("test.ts"), source).unwrap();

        // Create TOON file missing one export (causes warning)
        let toon_content = "purpose: Test module\ntokens: ~50\nexports[1]: x(const)";
        fs::write(temp_dir.path().join(".ai/test.ts.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: true, // Strict mode
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_validate(&args, temp_dir.path(), false);
        // Should fail because of export mismatch warning in strict mode
        assert!(result.is_err());
    }

    #[test]
    fn test_run_validate_specific_path() {
        let temp_dir = TempDir::new().unwrap();

        // Create two .ai subdirectories
        let ai_src = temp_dir.path().join(".ai/src");
        let ai_lib = temp_dir.path().join(".ai/lib");
        fs::create_dir_all(&ai_src).unwrap();
        fs::create_dir_all(&ai_lib).unwrap();

        // Create source files
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::create_dir(temp_dir.path().join("lib")).unwrap();
        fs::write(temp_dir.path().join("src/main.ts"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("lib/utils.ts"), "export const y = 2;").unwrap();

        // Create TOON files
        let toon_content = "purpose: Test\ntokens: ~50\nexports[1]: x(const)\ninvariants: Test";
        fs::write(ai_src.join("main.ts.toon"), toon_content).unwrap();

        // Create invalid TOON file in lib (missing source)
        fs::write(
            ai_lib.join("missing.ts.toon"),
            "purpose: Missing\ntokens: ~50\nexports[0]:",
        )
        .unwrap();

        let args = ValidateArgs {
            paths: vec![ai_src.clone()], // Only validate src
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        // Should succeed because we only validate src, not lib
        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_validate_nested_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested structure
        let ai_nested = temp_dir.path().join(".ai/src/components");
        fs::create_dir_all(&ai_nested).unwrap();

        // Create source file
        let src_nested = temp_dir.path().join("src/components");
        fs::create_dir_all(&src_nested).unwrap();
        fs::write(src_nested.join("Button.tsx"), "export function Button() {}").unwrap();

        // Create TOON file
        let toon_content = "purpose: Button component\ntokens: ~50\nexports[1]: Button(function)\ninvariants: Must be a function";
        fs::write(ai_nested.join("Button.tsx.toon"), toon_content).unwrap();

        let args = ValidateArgs {
            paths: vec![],
            fix: false,
            strict: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_validate(&args, temp_dir.path(), false);
        assert!(result.is_ok());
    }
}
