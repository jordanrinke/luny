//! @dose
//! purpose: Implements the watch command that monitors source files for changes
//!     and incrementally regenerates affected .toon files using the dependency graph.
//!
//! when-editing:
//!     - !The dependency graph must be kept in sync with file system state
//!     - !Debouncing is critical for handling rapid file changes (IDE saves)
//!     - Uses notify crate for cross-platform file system watching
//!
//! invariants:
//!     - Initial full generation must complete before watching starts
//!     - Config file changes trigger full regeneration
//!     - Deleted source files result in deleted .toon files
//!
//! flows:
//!     - Initial: Run full generate, build complete dependency graph
//!     - Watch: Receive notify events, debounce, determine affected files
//!     - Update: Regenerate only affected .toon files, update graph

use crate::cli::{GenerateArgs, WatchArgs};
use crate::commands::run_generate;
use crate::config::Config;
use crate::dependency::{
    get_path_variants, normalize_separators, resolve_import_path, DependencyGraph,
};
use crate::exclusion::build_walker;
use crate::formatter::format_toon;
use crate::parser::ParserFactory;
use crate::types::{CalledByInfo, ToonData};
use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The kind of change detected for a file
#[derive(Clone, Copy, PartialEq, Debug)]
enum ChangeKind {
    Create,
    Modify,
    Delete,
}

pub fn run_watch(args: &WatchArgs, root: &Path, verbose: bool) -> Result<()> {
    let factory = ParserFactory::new();
    let config = Config::load(root);

    // Initial full generation
    println!("Running initial generation...");
    let generate_args = GenerateArgs {
        paths: args.paths.clone(),
        force: true, // Force regeneration to ensure consistency
        clean: true, // Clean stale files
        common: args.common.clone(),
        ..Default::default()
    };
    run_generate(&generate_args, root, verbose)?;

    // Build initial dependency graph
    println!("Building dependency graph...");
    let mut dep_graph = build_full_dependency_graph(root, &args.paths, &factory, &config, verbose)?;

    // Set up file watcher
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;

    // Determine paths to watch
    let watch_paths = if args.paths.is_empty() {
        vec![root.to_path_buf()]
    } else {
        args.paths.iter().map(|p| root.join(p)).collect()
    };

    for path in &watch_paths {
        watcher.watch(path, RecursiveMode::Recursive)?;
        if verbose {
            println!("Watching: {}", path.display());
        }
    }

    println!("Watching for changes... (press Ctrl+C to stop)");

    // Event loop with debouncing
    let mut pending: HashMap<PathBuf, ChangeKind> = HashMap::new();
    let mut last_event = Instant::now();
    let debounce = Duration::from_millis(args.debounce);
    let poll_interval = Duration::from_millis(50);

    loop {
        match rx.recv_timeout(poll_interval) {
            Ok(event) => {
                process_event(&event, &mut pending, root, &factory);
                last_event = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check if debounce period has elapsed and we have pending changes
                if !pending.is_empty() && last_event.elapsed() >= debounce {
                    if args.clear {
                        // Clear terminal (ANSI escape code)
                        print!("\x1B[2J\x1B[1;1H");
                    }
                    process_pending_changes(
                        &mut pending,
                        &mut dep_graph,
                        &factory,
                        &config,
                        root,
                        verbose,
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("Watcher disconnected");
                break;
            }
        }
    }

    Ok(())
}

/// Build dependency graph for all source files
fn build_full_dependency_graph(
    root: &Path,
    paths: &[PathBuf],
    factory: &ParserFactory,
    config: &Config,
    _verbose: bool,
) -> Result<DependencyGraph> {
    let mut graph = DependencyGraph::new();
    let files = collect_source_files(root, paths, factory, config);

    for path in &files {
        if let Ok(source) = fs::read_to_string(path) {
            if let Some(parser) = factory.get_parser(path) {
                if let Ok(ast_info) = parser.extract_ast_info(&source, path) {
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                    let rel_path = normalize_separators(&rel_path);

                    // Extract imports
                    let imports: Vec<String> = ast_info
                        .imports
                        .iter()
                        .map(|imp| {
                            normalize_separators(&resolve_import_path(&imp.from, path, root))
                        })
                        .collect();

                    // Extract calls
                    let calls: Vec<(String, String)> = ast_info
                        .calls
                        .iter()
                        .map(|call| {
                            (
                                normalize_separators(&resolve_import_path(
                                    &call.target,
                                    path,
                                    root,
                                )),
                                call.method.clone(),
                            )
                        })
                        .collect();

                    graph.add_file(&rel_path, imports, calls);
                }
            }
        }
    }

    Ok(graph)
}

/// Collect all source files to process
fn collect_source_files(
    root: &Path,
    paths: &[PathBuf],
    factory: &ParserFactory,
    config: &Config,
) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let search_paths = if paths.is_empty() {
        vec![root.to_path_buf()]
    } else {
        paths.iter().map(|p| root.join(p)).collect()
    };

    let exclusion_config = crate::exclusion::ExclusionConfig {
        patterns: config.exclude.clone(),
        respect_gitignore: true,
    };

    for search_path in search_paths {
        if search_path.is_file() && factory.is_supported(&search_path) {
            files.push(search_path);
        } else if search_path.is_dir() {
            let mut walker = build_walker(&search_path, &exclusion_config);
            walker.follow_links(true);

            for entry in walker.build().filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                if entry_path.is_file() && factory.is_supported(entry_path) {
                    files.push(entry_path.to_path_buf());
                }
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

/// Process a notify event and add to pending changes
fn process_event(
    event: &Event,
    pending: &mut HashMap<PathBuf, ChangeKind>,
    root: &Path,
    factory: &ParserFactory,
) {
    let kind = match &event.kind {
        EventKind::Create(_) => ChangeKind::Create,
        EventKind::Modify(_) => ChangeKind::Modify,
        EventKind::Remove(_) => ChangeKind::Delete,
        _ => return, // Ignore other events
    };

    for path in &event.paths {
        // Skip non-source files (but include luny.toml for config changes)
        let is_config = path.file_name().map(|n| n == "luny.toml").unwrap_or(false);
        if !is_config && !factory.is_supported(path) {
            continue;
        }

        // Skip files in .ai directory
        if path
            .strip_prefix(root)
            .map(|p| p.starts_with(".ai"))
            .unwrap_or(false)
        {
            continue;
        }

        // Coalesce events: Create + Modify = Create, Modify + Delete = Delete
        pending
            .entry(path.clone())
            .and_modify(|existing| {
                *existing = match (*existing, kind) {
                    (ChangeKind::Create, ChangeKind::Modify) => ChangeKind::Create,
                    (ChangeKind::Create, ChangeKind::Delete) => ChangeKind::Delete,
                    (ChangeKind::Modify, ChangeKind::Delete) => ChangeKind::Delete,
                    (_, new) => new,
                };
            })
            .or_insert(kind);
    }
}

/// Process all pending file changes
fn process_pending_changes(
    pending: &mut HashMap<PathBuf, ChangeKind>,
    dep_graph: &mut DependencyGraph,
    factory: &ParserFactory,
    config: &Config,
    root: &Path,
    verbose: bool,
) {
    // Check for config file change -> full regen
    let config_path = root.join("luny.toml");
    if pending.contains_key(&config_path) {
        println!("Config changed, running full regeneration...");
        pending.clear();
        // Reload config and regenerate everything
        let new_config = Config::load(root);
        if let Ok(new_graph) = build_full_dependency_graph(root, &[], factory, &new_config, verbose)
        {
            *dep_graph = new_graph;
        }
        let generate_args = GenerateArgs {
            force: true,
            clean: true,
            ..Default::default()
        };
        let _ = run_generate(&generate_args, root, verbose);
        return;
    }

    // Calculate all affected files
    let mut to_regenerate: HashSet<PathBuf> = HashSet::new();
    let mut to_delete: HashSet<PathBuf> = HashSet::new();

    let changes: Vec<_> = pending.drain().collect();
    let timestamp = chrono_lite_timestamp();

    for (path, kind) in changes {
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let rel_path = normalize_separators(&rel_path);

        match kind {
            ChangeKind::Delete => {
                // Get affected files before removing from graph
                let affected = dep_graph.remove_file(&rel_path);

                // Queue .toon file for deletion
                let toon_path = get_toon_path(root, &path);
                to_delete.insert(toon_path);

                // Queue affected files for regeneration
                for affected_path in affected.indirect {
                    let source_path = root.join(&affected_path);
                    if source_path.exists() {
                        to_regenerate.insert(source_path);
                    }
                }

                println!("[{}] Deleted: {}", timestamp, rel_path);
            }
            ChangeKind::Create | ChangeKind::Modify => {
                // Re-parse and update graph
                if let Ok(source) = fs::read_to_string(&path) {
                    if let Some(parser) = factory.get_parser(&path) {
                        if let Ok(ast_info) = parser.extract_ast_info(&source, &path) {
                            // Extract imports and calls
                            let imports: Vec<String> = ast_info
                                .imports
                                .iter()
                                .map(|imp| {
                                    normalize_separators(&resolve_import_path(
                                        &imp.from, &path, root,
                                    ))
                                })
                                .collect();

                            let calls: Vec<(String, String)> = ast_info
                                .calls
                                .iter()
                                .map(|call| {
                                    (
                                        normalize_separators(&resolve_import_path(
                                            &call.target,
                                            &path,
                                            root,
                                        )),
                                        call.method.clone(),
                                    )
                                })
                                .collect();

                            // Update graph and get affected files
                            let affected = dep_graph.update_file(&rel_path, imports, calls);

                            // Queue direct file
                            to_regenerate.insert(path.clone());

                            // Queue indirectly affected files
                            for affected_path in affected.indirect {
                                let source_path = root.join(&affected_path);
                                if source_path.exists() {
                                    to_regenerate.insert(source_path);
                                }
                            }
                        }
                    }
                }

                let action = if kind == ChangeKind::Create {
                    "Created"
                } else {
                    "Modified"
                };
                println!("[{}] {}: {}", timestamp, action, rel_path);
            }
        }
    }

    // Delete removed .toon files
    for toon_path in &to_delete {
        if toon_path.exists() {
            if let Err(e) = fs::remove_file(toon_path) {
                eprintln!("Failed to delete {}: {}", toon_path.display(), e);
            } else if verbose {
                println!("  Deleted: {}", toon_path.display());
            }
        }
    }

    // Regenerate affected files
    let mut regenerated = 0;
    let threshold_matcher = config.threshold_matcher();

    for source_path in &to_regenerate {
        match regenerate_single_file(
            source_path,
            dep_graph,
            factory,
            &threshold_matcher,
            root,
            verbose,
        ) {
            Ok(_) => regenerated += 1,
            Err(e) => eprintln!("Failed to regenerate {}: {}", source_path.display(), e),
        }
    }

    if regenerated > 0 || !to_delete.is_empty() {
        println!(
            "[{}] Regenerated {} file(s), deleted {} file(s)",
            timestamp,
            regenerated,
            to_delete.len()
        );
    }
}

/// Get the .toon file path for a source file
fn get_toon_path(root: &Path, source_path: &Path) -> PathBuf {
    let relative = source_path.strip_prefix(root).unwrap_or(source_path);
    let toon_filename = format!(
        "{}.toon",
        relative.file_name().unwrap_or_default().to_string_lossy()
    );
    let toon_relative = relative.with_file_name(toon_filename);
    root.join(".ai").join(toon_relative)
}

/// Regenerate a single .toon file
fn regenerate_single_file(
    path: &Path,
    dep_graph: &DependencyGraph,
    factory: &ParserFactory,
    threshold_matcher: &crate::config::ThresholdMatcher,
    root: &Path,
    verbose: bool,
) -> Result<()> {
    let parser = factory
        .get_parser(path)
        .context("No parser available for file")?;

    let source = fs::read_to_string(path).context("Failed to read source file")?;
    let ast_info = parser.extract_ast_info(&source, path)?;

    // Compute TOON output path
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("File {} is outside root {}", path.display(), root.display()))?;
    let relative_str = normalize_separators(&relative.to_string_lossy());

    // Check token limits
    let thresholds = threshold_matcher.get_thresholds(relative);
    if let Some(error_threshold) = thresholds.error {
        if ast_info.tokens > error_threshold {
            eprintln!(
                "ERROR: {} has {} tokens (exceeds error threshold of {})",
                path.display(),
                ast_info.tokens,
                error_threshold
            );
        }
    }
    if let Some(warn_threshold) = thresholds.warn {
        if ast_info.tokens > warn_threshold && thresholds.error.is_none_or(|e| ast_info.tokens <= e)
        {
            eprintln!(
                "WARNING: {} has {} tokens (exceeds warning threshold of {})",
                path.display(),
                ast_info.tokens,
                warn_threshold
            );
        }
    }

    // Extract @dose comments
    let comments = parser.extract_toon_comments(&source)?;

    // Build purpose from comments or generate default
    let purpose = comments
        .file_block
        .as_ref()
        .and_then(|b| b.purpose.clone())
        .unwrap_or_else(|| {
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

    // Add function-level annotations
    if !comments.function_annotations.is_empty() {
        toon_data.function_annotations =
            Some(comments.function_annotations.values().cloned().collect());
    }

    // Format and write TOON content
    let content = format_toon(&toon_data);
    let toon_path = get_toon_path(root, path);

    if let Some(parent) = toon_path.parent() {
        fs::create_dir_all(parent).context("Failed to create output directory")?;
    }

    fs::write(&toon_path, &content).context("Failed to write TOON file")?;

    if verbose {
        println!("  Regenerated: {}", toon_path.display());
    }

    Ok(())
}

/// Simple timestamp without external crate
fn chrono_lite_timestamp() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
