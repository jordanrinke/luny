//! @toon
//! purpose: This module implements the generate command that creates .toon DOSE files
//!     for source code files. It uses a two-pass algorithm to build dependency graphs and
//!     generate comprehensive documentation.
//!
//! when-editing:
//!     - !The two-pass algorithm is critical: first pass builds dependency graph, second generates TOON
//!     - !TOON files are placed in .ai/ directory mirroring source structure
//!     - File collection excludes node_modules, .git, target, and __pycache__ directories
//!
//! invariants:
//!     - Every source file gets its TOON file in .ai/path/to/file.ext.toon
//!     - The dependency graph tracks both imported_by and called_by relationships
//!     - Token thresholds trigger warnings or errors during generation
//!
//! do-not:
//!     - Never overwrite existing TOON files unless --force is specified
//!     - Never process files in excluded directories
//!
//! gotchas:
//!     - Import path resolution tries multiple variants (with/without extension, with ./ prefix)
//!     - Relative imports are resolved relative to the importing file
//!     - Package imports are stored as-is without resolution
//!
//! flows:
//!     - Collect: Walk directory tree finding supported source files
//!     - Build graph: Parse each file, extract imports and calls, build reverse dependency maps
//!     - Generate: For each file, extract AST + comments, merge with graph data, format TOON

use crate::cli::GenerateArgs;
use crate::formatter::format_toon;
use crate::parser::ParserFactory;
use crate::types::{CalledByInfo, ToonData};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Dependency graph for reverse lookups
struct DependencyGraph {
    /// Map from import target (relative path) to list of files that import it
    imported_by: HashMap<String, Vec<String>>,
    /// Map from call target (relative path) to list of (file, function) that call it
    called_by: HashMap<String, Vec<CalledByInfo>>,
}

impl DependencyGraph {
    fn new() -> Self {
        Self {
            imported_by: HashMap::new(),
            called_by: HashMap::new(),
        }
    }

    fn get_imported_by(&self, file_path: &str) -> Vec<String> {
        self.imported_by.get(file_path).cloned().unwrap_or_default()
    }

    fn get_called_by(&self, file_path: &str) -> Vec<CalledByInfo> {
        self.called_by.get(file_path).cloned().unwrap_or_default()
    }
}

pub fn run_generate(args: &GenerateArgs, root: &Path, verbose: bool) -> Result<()> {
    let factory = ParserFactory::new();

    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    // Collect all files to process
    let files = collect_files(args, root, &root_canon, &factory);

    if verbose {
        println!("Building dependency graph for {} files...", files.len());
    }

    // First pass: build dependency graph
    let dep_graph = build_dependency_graph(&files, root, &factory, verbose);

    if verbose {
        println!(
            "Found {} import relationships, {} call relationships",
            dep_graph.imported_by.len(),
            dep_graph.called_by.len()
        );
    }

    // Second pass: generate TOON files
    let mut processed = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for path in &files {
        match process_file(path, &factory, args, root, &dep_graph, verbose) {
            Ok(true) => processed += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                eprintln!("Error processing {}: {}", path.display(), e);
                errors += 1;
            }
        }
    }

    println!(
        "Generated: {}, Skipped: {}, Errors: {}",
        processed, skipped, errors
    );

    if errors > 0 {
        anyhow::bail!("{} files failed to process", errors);
    }

    Ok(())
}

fn collect_files(
    args: &GenerateArgs,
    root: &Path,
    root_canon: &Path,
    factory: &ParserFactory,
) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let paths = if args.paths.is_empty() {
        vec![root.to_path_buf()]
    } else {
        args.paths.clone()
    };

    for path in paths {
        let full_path = if path.is_absolute() {
            path
        } else {
            root.join(&path)
        };

        if full_path.is_file() {
            files.push(full_path);
        } else if full_path.is_dir() {
            for entry in WalkDir::new(&full_path)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| {
                    !is_excluded_dir(e)
                        && is_allowed_symlink_target(e, root_canon, args.unsafe_follow)
                })
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() && factory.is_supported(path) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    // Deterministic ordering + de-dup (e.g. overlapping input paths)
    files.sort();
    files.dedup();
    files
}

fn is_excluded_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }

    match entry.file_name().to_string_lossy().as_ref() {
        // Common heavyweight/irrelevant dirs
        "node_modules" | ".git" | "target" | "__pycache__" => true,
        // Avoid reading/writing generated outputs back into inputs
        ".ai" => true,
        _ => false,
    }
}

fn is_allowed_symlink_target(entry: &DirEntry, root_canon: &Path, unsafe_follow: bool) -> bool {
    if unsafe_follow {
        return true;
    }
    if !entry.path_is_symlink() {
        return true;
    }
    // Only follow symlinks whose resolved targets stay within the root.
    match entry.path().canonicalize() {
        Ok(real) => real.starts_with(root_canon),
        // If we can't resolve it, treat it as not allowed (avoids surprising escapes).
        Err(_) => false,
    }
}

fn build_dependency_graph(
    files: &[PathBuf],
    root: &Path,
    factory: &ParserFactory,
    _verbose: bool,
) -> DependencyGraph {
    let mut graph = DependencyGraph::new();

    for path in files {
        let Some(parser) = factory.get_parser(path) else {
            continue;
        };

        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };

        let Ok(ast_info) = parser.extract_ast_info(&source, path) else {
            continue;
        };

        let Ok(file_relative) = path.strip_prefix(root) else {
            // Should never happen if collection is rooted correctly.
            // Avoid leaking absolute paths into the dependency graph.
            continue;
        };
        let file_relative = file_relative.to_string_lossy().to_string();

        // Process imports to build imported_by
        for import in &ast_info.imports {
            let target = resolve_import_path(&import.from, path, root);
            graph
                .imported_by
                .entry(target)
                .or_default()
                .push(file_relative.clone());
        }

        // Process calls to build called_by
        for call in &ast_info.calls {
            let target = resolve_import_path(&call.target, path, root);
            graph
                .called_by
                .entry(target)
                .or_default()
                .push(CalledByInfo {
                    from: file_relative.clone(),
                    function: call.method.clone(),
                });
        }
    }

    graph
}

/// Resolve an import path to a canonical relative path
fn resolve_import_path(import_from: &str, from_file: &Path, root: &Path) -> String {
    if import_from.starts_with('.') {
        // Relative import - resolve relative to the importing file
        let from_dir = from_file.parent().unwrap_or(from_file);
        let resolved = from_dir.join(import_from);

        // Normalize (purely lexical) and make relative to root.
        // Avoid canonicalize(): it performs filesystem IO and follows symlinks, which is slow and
        // can escape the intended root in surprising ways.
        let normalized = normalize_path(&resolved);
        if let Ok(rel) = normalized.strip_prefix(root) {
            return rel.to_string_lossy().to_string();
        }
        normalized.to_string_lossy().to_string()
    } else {
        // Package import - return as-is
        import_from.to_string()
    }
}

/// Simple path normalization (resolve . and ..)
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => {
                result.push(component);
            }
        }
    }
    result
}

fn process_file(
    path: &Path,
    factory: &ParserFactory,
    args: &GenerateArgs,
    root: &Path,
    dep_graph: &DependencyGraph,
    verbose: bool,
) -> Result<bool> {
    let parser = factory
        .get_parser(path)
        .context("No parser available for file")?;

    // Compute TOON output path (keep full filename, add .toon suffix)
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("File {} is outside root {}", path.display(), root.display()))?;
    let relative_str = relative.to_string_lossy().to_string();
    let toon_filename = format!(
        "{}.toon",
        relative.file_name().unwrap_or_default().to_string_lossy()
    );
    let toon_relative = relative.with_file_name(toon_filename);
    let toon_path = root.join(".ai").join(toon_relative);

    // Check if TOON file exists and we're not forcing regeneration
    if toon_path.exists() && !args.force {
        if verbose {
            println!("Skipping {} (TOON exists)", path.display());
        }
        return Ok(false);
    }

    // Read source file
    let source = fs::read_to_string(path).context("Failed to read source file")?;

    // Extract AST info
    let ast_info = parser.extract_ast_info(&source, path)?;

    // Check token limits
    if ast_info.tokens > args.token_error {
        eprintln!(
            "ERROR: {} has {} tokens (exceeds error threshold of {})",
            path.display(),
            ast_info.tokens,
            args.token_error
        );
    } else if ast_info.tokens > args.token_warn {
        eprintln!(
            "WARNING: {} has {} tokens (exceeds warning threshold of {})",
            path.display(),
            ast_info.tokens,
            args.token_warn
        );
    }

    // Extract @toon comments
    let comments = parser.extract_toon_comments(&source)?;

    // Build purpose from comments or generate default
    let purpose = comments
        .file_block
        .as_ref()
        .and_then(|b| b.purpose.clone())
        .unwrap_or_else(|| {
            // Generate default purpose from filename
            let filename = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{} module", filename)
        });

    // Build ToonData
    let mut toon_data = ToonData::new(purpose, ast_info.tokens, ast_info.exports);

    // Add AST-extracted data
    if !ast_info.imports.is_empty() {
        toon_data.imports = Some(ast_info.imports);
    }
    if !ast_info.calls.is_empty() {
        toon_data.calls = Some(ast_info.calls);
    }
    if !ast_info.signatures.is_empty() {
        toon_data.signatures = Some(ast_info.signatures);
    }

    // Add reverse dependency data from graph
    // Try multiple path variations to find matches
    let path_variants = get_path_variants(&relative_str);

    let mut imported_by: Vec<String> = Vec::new();
    let mut called_by: Vec<CalledByInfo> = Vec::new();

    for variant in &path_variants {
        imported_by.extend(dep_graph.get_imported_by(variant));
        called_by.extend(dep_graph.get_called_by(variant));
    }

    // Deduplicate
    imported_by.sort();
    imported_by.dedup();

    if !imported_by.is_empty() {
        toon_data.imported_by = Some(imported_by);
    }
    if !called_by.is_empty() {
        // Deterministic ordering and de-dup (variants can overlap).
        called_by.sort_by(|a, b| {
            (a.from.as_str(), a.function.as_str()).cmp(&(b.from.as_str(), b.function.as_str()))
        });
        called_by.dedup_by(|a, b| a.from == b.from && a.function == b.function);
        toon_data.called_by = Some(called_by);
    }

    // Add comment-extracted data
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

    // Add function-level annotations (inline @toon comments)
    if !comments.function_annotations.is_empty() {
        toon_data.function_annotations =
            Some(comments.function_annotations.values().cloned().collect());
    }

    // Format TOON content
    let content = format_toon(&toon_data);

    if args.dry_run {
        println!("Would write to: {}", toon_path.display());
        if verbose {
            println!("---\n{}\n---", content);
        }
    } else {
        // Ensure parent directory exists
        if let Some(parent) = toon_path.parent() {
            fs::create_dir_all(parent).context("Failed to create output directory")?;
        }

        fs::write(&toon_path, &content).context("Failed to write TOON file")?;

        if verbose {
            println!("Generated: {}", toon_path.display());
        }
    }

    Ok(true)
}

/// Get various path forms to match against imports
fn get_path_variants(path: &str) -> Vec<String> {
    let mut variants = vec![path.to_string()];

    // Without extension
    if let Some(without_ext) = path
        .strip_suffix(".ts")
        .or_else(|| path.strip_suffix(".tsx"))
        .or_else(|| path.strip_suffix(".js"))
        .or_else(|| path.strip_suffix(".jsx"))
    {
        variants.push(without_ext.to_string());
    }

    // With ./ prefix
    variants.push(format!("./{}", path));

    variants
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== DependencyGraph Tests ====================

    #[test]
    fn test_dependency_graph_new() {
        let graph = DependencyGraph::new();
        assert!(graph.imported_by.is_empty());
        assert!(graph.called_by.is_empty());
    }

    #[test]
    fn test_dependency_graph_get_imported_by_empty() {
        let graph = DependencyGraph::new();
        let result = graph.get_imported_by("nonexistent.ts");
        assert!(result.is_empty());
    }

    #[test]
    fn test_dependency_graph_get_imported_by_with_data() {
        let mut graph = DependencyGraph::new();
        graph.imported_by.insert(
            "utils.ts".to_string(),
            vec!["main.ts".to_string(), "app.ts".to_string()],
        );

        let result = graph.get_imported_by("utils.ts");
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"main.ts".to_string()));
        assert!(result.contains(&"app.ts".to_string()));
    }

    #[test]
    fn test_dependency_graph_get_called_by_empty() {
        let graph = DependencyGraph::new();
        let result = graph.get_called_by("nonexistent.ts");
        assert!(result.is_empty());
    }

    #[test]
    fn test_dependency_graph_get_called_by_with_data() {
        let mut graph = DependencyGraph::new();
        graph.called_by.insert(
            "api.ts".to_string(),
            vec![
                CalledByInfo {
                    from: "main.ts".to_string(),
                    function: "fetchData".to_string(),
                },
                CalledByInfo {
                    from: "app.ts".to_string(),
                    function: "getData".to_string(),
                },
            ],
        );

        let result = graph.get_called_by("api.ts");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].function, "fetchData");
        assert_eq!(result[1].function, "getData");
    }

    // ==================== normalize_path Tests ====================

    #[test]
    fn test_normalize_path_simple() {
        let path = PathBuf::from("/a/b/c");
        let result = normalize_path(&path);
        assert_eq!(result, PathBuf::from("/a/b/c"));
    }

    #[test]
    fn test_normalize_path_with_current_dir() {
        let path = PathBuf::from("/a/./b/./c");
        let result = normalize_path(&path);
        assert_eq!(result, PathBuf::from("/a/b/c"));
    }

    #[test]
    fn test_normalize_path_with_parent_dir() {
        let path = PathBuf::from("/a/b/../c");
        let result = normalize_path(&path);
        assert_eq!(result, PathBuf::from("/a/c"));
    }

    #[test]
    fn test_normalize_path_complex() {
        let path = PathBuf::from("/a/b/c/../d/./e/../f");
        let result = normalize_path(&path);
        assert_eq!(result, PathBuf::from("/a/b/d/f"));
    }

    #[test]
    fn test_normalize_path_relative() {
        let path = PathBuf::from("a/b/../c");
        let result = normalize_path(&path);
        assert_eq!(result, PathBuf::from("a/c"));
    }

    // ==================== resolve_import_path Tests ====================

    #[test]
    fn test_resolve_import_path_package_import() {
        let from_file = Path::new("/project/src/main.ts");
        let root = Path::new("/project");
        let result = resolve_import_path("react", from_file, root);
        assert_eq!(result, "react");
    }

    #[test]
    fn test_resolve_import_path_scoped_package() {
        let from_file = Path::new("/project/src/main.ts");
        let root = Path::new("/project");
        let result = resolve_import_path("@types/node", from_file, root);
        assert_eq!(result, "@types/node");
    }

    #[test]
    fn test_resolve_import_path_relative_same_dir() {
        let from_file = Path::new("/project/src/main.ts");
        let root = Path::new("/project");
        // Since canonicalize may fail in tests, we test the fallback path
        let result = resolve_import_path("./utils", from_file, root);
        // Should contain utils somewhere in the result
        assert!(result.contains("utils"));
    }

    #[test]
    fn test_resolve_import_path_relative_parent_dir() {
        let from_file = Path::new("/project/src/components/Button.ts");
        let root = Path::new("/project");
        let result = resolve_import_path("../utils", from_file, root);
        // Should contain utils and be normalized
        assert!(result.contains("utils"));
    }

    // ==================== get_path_variants Tests ====================

    #[test]
    fn test_get_path_variants_typescript() {
        let variants = get_path_variants("src/utils.ts");
        assert!(variants.contains(&"src/utils.ts".to_string()));
        assert!(variants.contains(&"src/utils".to_string()));
        assert!(variants.contains(&"./src/utils.ts".to_string()));
    }

    #[test]
    fn test_get_path_variants_tsx() {
        let variants = get_path_variants("components/Button.tsx");
        assert!(variants.contains(&"components/Button.tsx".to_string()));
        assert!(variants.contains(&"components/Button".to_string()));
        assert!(variants.contains(&"./components/Button.tsx".to_string()));
    }

    #[test]
    fn test_get_path_variants_javascript() {
        let variants = get_path_variants("lib/helper.js");
        assert!(variants.contains(&"lib/helper.js".to_string()));
        assert!(variants.contains(&"lib/helper".to_string()));
        assert!(variants.contains(&"./lib/helper.js".to_string()));
    }

    #[test]
    fn test_get_path_variants_jsx() {
        let variants = get_path_variants("App.jsx");
        assert!(variants.contains(&"App.jsx".to_string()));
        assert!(variants.contains(&"App".to_string()));
        assert!(variants.contains(&"./App.jsx".to_string()));
    }

    #[test]
    fn test_get_path_variants_non_js_extension() {
        let variants = get_path_variants("main.py");
        assert!(variants.contains(&"main.py".to_string()));
        assert!(variants.contains(&"./main.py".to_string()));
        // Should NOT have version without extension for non-JS files
        assert_eq!(variants.len(), 2);
    }

    #[test]
    fn test_get_path_variants_no_extension() {
        let variants = get_path_variants("Makefile");
        assert!(variants.contains(&"Makefile".to_string()));
        assert!(variants.contains(&"./Makefile".to_string()));
        assert_eq!(variants.len(), 2);
    }

    // ==================== collect_files Tests ====================

    #[test]
    fn test_collect_files_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();
        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_files_with_supported_files() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create some supported files
        fs::write(temp_dir.path().join("main.ts"), "export const x = 1;").unwrap();
        fs::write(temp_dir.path().join("utils.py"), "def foo(): pass").unwrap();
        fs::write(temp_dir.path().join("readme.md"), "# Readme").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        // Should find .ts and .py but not .md
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_files_excludes_node_modules() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create node_modules directory with a file
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("package.ts"), "export const x = 1;").unwrap();

        // Create a file outside node_modules
        fs::write(temp_dir.path().join("main.ts"), "export const y = 2;").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        // Should only find main.ts, not the one in node_modules
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("main.ts"));
    }

    #[test]
    fn test_collect_files_excludes_git() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create .git directory with a file
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("config.ts"), "export const x = 1;").unwrap();

        // Create a file outside .git
        fs::write(temp_dir.path().join("main.ts"), "export const y = 2;").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("main.ts"));
    }

    #[test]
    fn test_collect_files_excludes_target() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create target directory with a file
        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("debug.rs"), "fn main() {}").unwrap();

        // Create a file outside target
        fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("main.rs"));
    }

    #[test]
    fn test_collect_files_excludes_pycache() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create __pycache__ directory with a file
        let pycache_dir = temp_dir.path().join("__pycache__");
        fs::create_dir(&pycache_dir).unwrap();
        fs::write(pycache_dir.join("cached.py"), "def foo(): pass").unwrap();

        // Create a file outside __pycache__
        fs::write(temp_dir.path().join("main.py"), "def main(): pass").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("main.py"));
    }

    #[test]
    fn test_collect_files_specific_path() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create files in different directories
        let src_dir = temp_dir.path().join("src");
        let lib_dir = temp_dir.path().join("lib");
        fs::create_dir(&src_dir).unwrap();
        fs::create_dir(&lib_dir).unwrap();
        fs::write(src_dir.join("main.ts"), "export const x = 1;").unwrap();
        fs::write(lib_dir.join("utils.ts"), "export const y = 2;").unwrap();

        let args = GenerateArgs {
            paths: vec![src_dir.clone()],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        // Should only find files in src/
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("main.ts"));
    }

    #[test]
    fn test_collect_files_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        let file_path = temp_dir.path().join("single.ts");
        fs::write(&file_path, "export const x = 1;").unwrap();

        let args = GenerateArgs {
            paths: vec![file_path.clone()],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let files = collect_files(&args, temp_dir.path(), temp_dir.path(), &factory);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], file_path);
    }

    // ==================== build_dependency_graph Tests ====================

    #[test]
    fn test_build_dependency_graph_empty() {
        let factory = ParserFactory::new();
        let files: Vec<PathBuf> = vec![];
        let temp_dir = TempDir::new().unwrap();

        let graph = build_dependency_graph(&files, temp_dir.path(), &factory, false);
        assert!(graph.imported_by.is_empty());
        assert!(graph.called_by.is_empty());
    }

    #[test]
    fn test_build_dependency_graph_with_imports() {
        let temp_dir = TempDir::new().unwrap();
        let factory = ParserFactory::new();

        // Create files with imports
        let main_path = temp_dir.path().join("main.ts");
        let utils_path = temp_dir.path().join("utils.ts");

        fs::write(&main_path, "import { foo } from './utils';").unwrap();
        fs::write(&utils_path, "export function foo() {}").unwrap();

        let files = vec![main_path, utils_path];
        let graph = build_dependency_graph(&files, temp_dir.path(), &factory, false);

        // main.ts imports from ./utils, so utils should be in imported_by
        assert!(!graph.imported_by.is_empty());
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_run_generate_dry_run() {
        let temp_dir = TempDir::new().unwrap();

        // Create a simple TypeScript file
        fs::write(temp_dir.path().join("main.ts"), "export const x = 1;").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: true,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_generate(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // No .ai directory should be created in dry run
        assert!(!temp_dir.path().join(".ai").exists());
    }

    #[test]
    fn test_run_generate_creates_toon_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create a simple TypeScript file
        fs::write(temp_dir.path().join("main.ts"), "export const x = 1;").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_generate(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // .ai directory should be created with the toon file
        let toon_path = temp_dir.path().join(".ai/main.ts.toon");
        assert!(toon_path.exists());
    }

    #[test]
    fn test_run_generate_skips_existing_without_force() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file
        fs::write(temp_dir.path().join("main.ts"), "export const x = 1;").unwrap();

        // Create existing .ai directory and toon file
        let ai_dir = temp_dir.path().join(".ai");
        fs::create_dir(&ai_dir).unwrap();
        fs::write(ai_dir.join("main.ts.toon"), "existing content").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_generate(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Existing content should be preserved
        let content = fs::read_to_string(ai_dir.join("main.ts.toon")).unwrap();
        assert_eq!(content, "existing content");
    }

    #[test]
    fn test_run_generate_overwrites_with_force() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file
        fs::write(temp_dir.path().join("main.ts"), "export const x = 1;").unwrap();

        // Create existing .ai directory and toon file
        let ai_dir = temp_dir.path().join(".ai");
        fs::create_dir(&ai_dir).unwrap();
        fs::write(ai_dir.join("main.ts.toon"), "existing content").unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: true,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_generate(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Content should be overwritten
        let content = fs::read_to_string(ai_dir.join("main.ts.toon")).unwrap();
        assert_ne!(content, "existing content");
        assert!(content.contains("purpose:"));
    }

    #[test]
    fn test_run_generate_with_toon_comments() {
        let temp_dir = TempDir::new().unwrap();

        // Create a TypeScript file with @toon comments
        let source = r#"/** @toon
purpose: Main application entry point
when-editing:
    - !Check all imports before modifying
    - Update tests after changes
invariants:
    - Must export the main function
do-not:
    - Never call exit() directly
gotchas:
    - Configuration is loaded lazily
*/

export function main() {
    console.log("Hello");
}
"#;
        fs::write(temp_dir.path().join("main.ts"), source).unwrap();

        let args = GenerateArgs {
            paths: vec![],
            dry_run: false,
            force: false,
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
        };

        let result = run_generate(&args, temp_dir.path(), false);
        assert!(result.is_ok());

        // Check the generated toon file contains the extracted comments
        let toon_content = fs::read_to_string(temp_dir.path().join(".ai/main.ts.toon")).unwrap();
        assert!(toon_content.contains("Main application entry point"));
        assert!(toon_content.contains("when-editing:"));
        assert!(toon_content.contains("invariants:"));
    }
}
