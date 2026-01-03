//! @dose
//! purpose: Shared dependency graph module for tracking import/call relationships
//!     between source files. Used by both generate and watch commands.
//!
//! when-editing:
//!     - !Both generate.rs and watch.rs depend on this module
//!     - !Path normalization must use forward slashes for cross-platform consistency
//!     - The graph maintains both forward and reverse lookups for efficient updates
//!
//! invariants:
//!     - Forward and reverse maps must stay in sync
//!     - All paths are normalized with forward slashes
//!
//! do-not:
//!     - Never use filesystem IO for path resolution (use lexical normalization only)

use crate::types::CalledByInfo;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Dependency graph for tracking relationships between source files.
/// Maintains both forward (what a file imports) and reverse (what imports a file) lookups.
#[derive(Default)]
pub struct DependencyGraph {
    /// Reverse: maps file path to files that import it
    pub imported_by: HashMap<String, Vec<String>>,
    /// Reverse: maps file path to files/functions that call it
    pub called_by: HashMap<String, Vec<CalledByInfo>>,
    /// Forward: maps file path to files it imports
    pub imports: HashMap<String, Vec<String>>,
    /// Forward: maps file path to files it calls
    pub calls_to: HashMap<String, Vec<String>>,
}

/// Tracks which files need regeneration after a change
#[derive(Default, Debug)]
pub struct AffectedFiles {
    /// Files whose own content changed - regenerate their .toon
    pub direct: HashSet<String>,
    /// Files whose imported_by/called_by data changed - regenerate their .toon
    pub indirect: HashSet<String>,
}

impl AffectedFiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all affected files (direct + indirect)
    pub fn all(&self) -> HashSet<String> {
        self.direct.union(&self.indirect).cloned().collect()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get list of files that import the given file
    pub fn get_imported_by(&self, file_path: &str) -> Vec<String> {
        self.imported_by.get(file_path).cloned().unwrap_or_default()
    }

    /// Get list of files/functions that call the given file
    pub fn get_called_by(&self, file_path: &str) -> Vec<CalledByInfo> {
        self.called_by.get(file_path).cloned().unwrap_or_default()
    }

    /// Add a file to the dependency graph with its import and call relationships.
    /// This populates both forward and reverse maps.
    pub fn add_file(
        &mut self,
        file_path: &str,
        file_imports: Vec<String>,
        file_calls: Vec<(String, String)>,
    ) {
        let file_path = file_path.to_string();

        // Store forward maps
        self.imports.insert(file_path.clone(), file_imports.clone());
        let call_targets: Vec<String> = file_calls
            .iter()
            .map(|(target, _)| target.clone())
            .collect();
        self.calls_to.insert(file_path.clone(), call_targets);

        // Update reverse maps: imported_by
        for import_target in &file_imports {
            self.imported_by
                .entry(import_target.clone())
                .or_default()
                .push(file_path.clone());
        }

        // Update reverse maps: called_by
        for (call_target, function) in &file_calls {
            self.called_by
                .entry(call_target.clone())
                .or_default()
                .push(CalledByInfo {
                    from: file_path.clone(),
                    function: function.clone(),
                });
        }
    }

    /// Remove a file from the dependency graph. Returns the set of affected files
    /// whose .toon files need regeneration (their imported_by/called_by changed).
    pub fn remove_file(&mut self, file_path: &str) -> AffectedFiles {
        let mut affected = AffectedFiles::new();

        // Get files this file imported (they lose an entry in their imported_by)
        if let Some(file_imports) = self.imports.remove(file_path) {
            for import_target in file_imports {
                if let Some(importers) = self.imported_by.get_mut(&import_target) {
                    importers.retain(|f| f != file_path);
                    affected.indirect.insert(import_target);
                }
            }
        }

        // Get files this file called (they lose an entry in their called_by)
        if let Some(file_calls) = self.calls_to.remove(file_path) {
            for call_target in file_calls {
                if let Some(callers) = self.called_by.get_mut(&call_target) {
                    callers.retain(|c| c.from != file_path);
                    affected.indirect.insert(call_target);
                }
            }
        }

        // Remove this file from imported_by (for files that import it)
        self.imported_by.remove(file_path);

        // Remove this file from called_by (for files that call it)
        self.called_by.remove(file_path);

        affected
    }

    /// Update a file in the dependency graph. Removes old relationships and adds new ones.
    /// Returns the set of affected files whose .toon files need regeneration.
    pub fn update_file(
        &mut self,
        file_path: &str,
        file_imports: Vec<String>,
        file_calls: Vec<(String, String)>,
    ) -> AffectedFiles {
        // Remove old relationships (this adds old import/call targets to indirect)
        let mut affected = self.remove_file(file_path);

        // Add new relationships
        self.add_file(file_path, file_imports.clone(), file_calls.clone());

        // The file itself is directly affected
        affected.direct.insert(file_path.to_string());

        // Files that this file now imports are indirectly affected (their imported_by changed)
        for import_target in &file_imports {
            affected.indirect.insert(import_target.clone());
        }

        // Files that this file now calls are indirectly affected (their called_by changed)
        for (call_target, _) in &file_calls {
            affected.indirect.insert(call_target.clone());
        }

        affected
    }

    /// Get files that would be affected if the given file changes.
    /// This is useful for preview/dry-run operations.
    pub fn get_affected_files(&self, file_path: &str) -> AffectedFiles {
        let mut affected = AffectedFiles::new();
        affected.direct.insert(file_path.to_string());

        // Files that import this file
        for importer in self.get_imported_by(file_path) {
            affected.indirect.insert(importer);
        }

        // Files this file imports (their imported_by list includes this file)
        if let Some(imports) = self.imports.get(file_path) {
            for import_target in imports {
                affected.indirect.insert(import_target.clone());
            }
        }

        // Files that call this file
        for caller in self.get_called_by(file_path) {
            affected.indirect.insert(caller.from);
        }

        // Files this file calls (their called_by list includes this file)
        if let Some(calls) = self.calls_to.get(file_path) {
            for call_target in calls {
                affected.indirect.insert(call_target.clone());
            }
        }

        affected
    }
}

/// Resolve an import path to a canonical relative path
pub fn resolve_import_path(import_from: &str, from_file: &Path, root: &Path) -> String {
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
pub fn normalize_path(path: &Path) -> PathBuf {
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

/// Normalize path separators to forward slashes for cross-platform consistency
pub fn normalize_separators(path: &str) -> String {
    path.replace('\\', "/")
}

/// Get various path forms to match against imports
pub fn get_path_variants(path: &str) -> Vec<String> {
    let normalized = normalize_separators(path);
    let mut variants = vec![normalized.clone()];

    // Without extension (for JS/TS files)
    if let Some(without_ext) = normalized
        .strip_suffix(".ts")
        .or_else(|| normalized.strip_suffix(".tsx"))
        .or_else(|| normalized.strip_suffix(".js"))
        .or_else(|| normalized.strip_suffix(".jsx"))
    {
        variants.push(without_ext.to_string());
    }

    // With ./ prefix
    variants.push(format!("./{}", normalized));

    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph_new() {
        let graph = DependencyGraph::new();
        assert!(graph.imported_by.is_empty());
        assert!(graph.called_by.is_empty());
        assert!(graph.imports.is_empty());
        assert!(graph.calls_to.is_empty());
    }

    #[test]
    fn test_add_file_updates_forward_maps() {
        let mut graph = DependencyGraph::new();
        graph.add_file(
            "main.ts",
            vec!["utils.ts".to_string(), "api.ts".to_string()],
            vec![("api.ts".to_string(), "fetchData".to_string())],
        );

        assert_eq!(graph.imports.get("main.ts").unwrap().len(), 2);
        assert_eq!(graph.calls_to.get("main.ts").unwrap().len(), 1);
    }

    #[test]
    fn test_add_file_updates_reverse_maps() {
        let mut graph = DependencyGraph::new();
        graph.add_file(
            "main.ts",
            vec!["utils.ts".to_string()],
            vec![("api.ts".to_string(), "fetch".to_string())],
        );

        assert_eq!(graph.get_imported_by("utils.ts"), vec!["main.ts"]);
        assert_eq!(graph.get_called_by("api.ts").len(), 1);
        assert_eq!(graph.get_called_by("api.ts")[0].from, "main.ts");
    }

    #[test]
    fn test_remove_file_cleans_maps() {
        let mut graph = DependencyGraph::new();
        graph.add_file("main.ts", vec!["utils.ts".to_string()], vec![]);

        let affected = graph.remove_file("main.ts");

        assert!(!graph.imports.contains_key("main.ts"));
        assert!(graph.get_imported_by("utils.ts").is_empty());
        assert!(affected.indirect.contains("utils.ts"));
    }

    #[test]
    fn test_update_file_returns_affected() {
        let mut graph = DependencyGraph::new();
        graph.add_file("main.ts", vec!["utils.ts".to_string()], vec![]);
        graph.add_file("utils.ts", vec![], vec![]);

        let affected = graph.update_file("main.ts", vec!["api.ts".to_string()], vec![]);

        assert!(affected.direct.contains("main.ts"));
        assert!(affected.indirect.contains("utils.ts")); // no longer imported
        assert!(affected.indirect.contains("api.ts")); // now imported
    }

    #[test]
    fn test_get_affected_files() {
        let mut graph = DependencyGraph::new();
        graph.add_file("main.ts", vec!["utils.ts".to_string()], vec![]);
        graph.add_file("app.ts", vec!["utils.ts".to_string()], vec![]);

        let affected = graph.get_affected_files("utils.ts");

        assert!(affected.direct.contains("utils.ts"));
        assert!(affected.indirect.contains("main.ts"));
        assert!(affected.indirect.contains("app.ts"));
    }

    #[test]
    fn test_normalize_separators() {
        assert_eq!(
            normalize_separators("src\\utils\\file.ts"),
            "src/utils/file.ts"
        );
        assert_eq!(
            normalize_separators("src/utils/file.ts"),
            "src/utils/file.ts"
        );
    }

    #[test]
    fn test_get_path_variants_typescript() {
        let variants = get_path_variants("src/utils.ts");
        assert!(variants.contains(&"src/utils.ts".to_string()));
        assert!(variants.contains(&"src/utils".to_string()));
        assert!(variants.contains(&"./src/utils.ts".to_string()));
    }

    #[test]
    fn test_normalize_path() {
        let path = PathBuf::from("/a/b/../c/./d");
        let normalized = normalize_path(&path);
        assert_eq!(normalized, PathBuf::from("/a/c/d"));
    }
}
