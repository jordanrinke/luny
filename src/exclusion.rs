//! @dose
//! purpose: This module provides file exclusion functionality for directory walking, supporting
//!     gitignore files and patterns from luny.toml/CLI --exclude.
//!
//! when-editing:
//!     - !Override patterns use ! prefix to negate (exclude), so we add ! to user patterns
//!     - The ignore crate handles gitignore parsing automatically
//!     - Patterns come from luny.toml exclude array and CLI --exclude flags
//!
//! invariants:
//!     - Default exclusions (node_modules, .git, etc.) are always applied
//!     - CLI --exclude patterns are combined with luny.toml exclude patterns
//!     - Gitignore is respected by default unless --no-gitignore is passed
//!
//! do-not:
//!     - Never remove default exclusions without explicit user override
//!
//! gotchas:
//!     - The ignore crate's override patterns are inclusive by default, so we negate them
//!     - Symlink handling is separate from this module (handled in commands)

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::path::Path;

/// Configuration for file exclusion during directory walking
pub struct ExclusionConfig {
    /// Glob patterns to exclude (from --exclude flags)
    pub patterns: Vec<String>,
    /// Whether to respect .gitignore files (default: true)
    pub respect_gitignore: bool,
}

impl Default for ExclusionConfig {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            respect_gitignore: true,
        }
    }
}

/// Default directories that are always excluded
const DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "__pycache__",
    ".ai",
    "vendor",
    ".venv",
    "venv",
    "dist",
    "build",
    ".next",
    ".nuxt",
];

/// Build a WalkBuilder with the given exclusion configuration
pub fn build_walker(root: &Path, config: &ExclusionConfig) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);

    // Configure gitignore handling
    builder.git_ignore(config.respect_gitignore);
    builder.git_global(config.respect_gitignore);
    builder.git_exclude(config.respect_gitignore);

    // Don't respect hidden files filter (we handle .git explicitly)
    builder.hidden(false);

    // Build override patterns for default exclusions and exclude patterns
    let mut overrides = OverrideBuilder::new(root);

    // Add default directory exclusions
    for dir in DEFAULT_EXCLUDED_DIRS {
        // Exclude the directory and all its contents
        let pattern = format!("!{}/**", dir);
        let _ = overrides.add(&pattern);
        let pattern = format!("!{}", dir);
        let _ = overrides.add(&pattern);
    }

    // Add user patterns as exclusions (! prefix makes them exclude)
    for pattern in &config.patterns {
        let exclude_pattern = format!("!{}", pattern);
        if let Err(e) = overrides.add(&exclude_pattern) {
            eprintln!("Warning: invalid exclude pattern '{}': {}", pattern, e);
        }
    }

    if let Ok(built) = overrides.build() {
        builder.overrides(built);
    }

    builder
}

/// Build a GlobSet from patterns for additional filtering
pub fn build_exclude_globset(patterns: &[String]) -> Option<GlobSet> {
    if patterns.is_empty() {
        return None;
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(e) => {
                eprintln!("Warning: invalid exclude pattern '{}': {}", pattern, e);
            }
        }
    }

    builder.build().ok()
}

/// Check if a directory name should be excluded by default
pub fn is_default_excluded_dir(name: &str) -> bool {
    DEFAULT_EXCLUDED_DIRS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_excluded_dirs() {
        assert!(is_default_excluded_dir("node_modules"));
        assert!(is_default_excluded_dir(".git"));
        assert!(is_default_excluded_dir("target"));
        assert!(is_default_excluded_dir("__pycache__"));
        assert!(is_default_excluded_dir(".ai"));
        assert!(!is_default_excluded_dir("src"));
        assert!(!is_default_excluded_dir("lib"));
    }

    #[test]
    fn test_exclusion_config_default() {
        let config = ExclusionConfig::default();
        assert!(config.patterns.is_empty());
        assert!(config.respect_gitignore);
    }

    #[test]
    fn test_build_walker_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = ExclusionConfig::default();

        let walker = build_walker(temp_dir.path(), &config);
        // Walker should be created successfully
        let _iter = walker.build();
    }

    #[test]
    fn test_build_walker_with_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let config = ExclusionConfig {
            patterns: vec!["*.test.ts".to_string(), "docs/**".to_string()],
            respect_gitignore: true,
        };

        let walker = build_walker(temp_dir.path(), &config);
        let _iter = walker.build();
    }

    #[test]
    fn test_build_walker_no_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let config = ExclusionConfig {
            patterns: vec![],
            respect_gitignore: false,
        };

        let walker = build_walker(temp_dir.path(), &config);
        let _iter = walker.build();
    }

    #[test]
    fn test_build_exclude_globset_empty() {
        let result = build_exclude_globset(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_exclude_globset_valid_patterns() {
        let patterns = vec!["*.test.ts".to_string(), "docs/**".to_string()];
        let result = build_exclude_globset(&patterns);
        assert!(result.is_some());

        let globset = result.unwrap();
        assert!(globset.is_match("foo.test.ts"));
        assert!(globset.is_match("docs/readme.md"));
        assert!(!globset.is_match("main.ts"));
    }

    #[test]
    fn test_gitignore_respected() {
        let temp_dir = TempDir::new().unwrap();

        // Create a .git directory to make it recognized as a git repo
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create a .gitignore file
        fs::write(
            temp_dir.path().join(".gitignore"),
            "ignored_dir/\n*.ignored",
        )
        .unwrap();

        // Create directories and files
        fs::create_dir(temp_dir.path().join("ignored_dir")).unwrap();
        fs::write(temp_dir.path().join("ignored_dir/file.ts"), "content").unwrap();
        fs::write(temp_dir.path().join("test.ignored"), "content").unwrap();
        fs::write(temp_dir.path().join("main.ts"), "content").unwrap();

        let config = ExclusionConfig {
            patterns: vec![],
            respect_gitignore: true,
        };

        let walker = build_walker(temp_dir.path(), &config);
        let files: Vec<_> = walker
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Should only find main.ts (ignored_dir and *.ignored should be excluded)
        // .gitignore file itself may or may not be included depending on ignore crate behavior
        let ts_files: Vec<_> = files
            .iter()
            .filter(|p| p.extension().map(|e| e == "ts").unwrap_or(false))
            .collect();
        assert_eq!(ts_files.len(), 1);
        assert!(ts_files[0].to_string_lossy().contains("main.ts"));
    }

    #[test]
    fn test_gitignore_not_respected() {
        let temp_dir = TempDir::new().unwrap();

        // Create a .gitignore file
        fs::write(temp_dir.path().join(".gitignore"), "ignored_dir/").unwrap();

        // Create directories and files
        fs::create_dir(temp_dir.path().join("ignored_dir")).unwrap();
        fs::write(temp_dir.path().join("ignored_dir/file.ts"), "content").unwrap();
        fs::write(temp_dir.path().join("main.ts"), "content").unwrap();

        let config = ExclusionConfig {
            patterns: vec![],
            respect_gitignore: false, // Disable gitignore
        };

        let walker = build_walker(temp_dir.path(), &config);
        let files: Vec<_> = walker
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Should find both files when gitignore is disabled
        // Note: .gitignore file itself is also found
        assert!(files.len() >= 2);
    }

    #[test]
    fn test_config_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();

        // Create files
        fs::write(temp_dir.path().join("main.ts"), "content").unwrap();
        fs::write(temp_dir.path().join("main.test.ts"), "content").unwrap();
        fs::create_dir(temp_dir.path().join("docs")).unwrap();
        fs::write(temp_dir.path().join("docs/readme.md"), "content").unwrap();

        // Exclude patterns from luny.toml would be passed here
        let config = ExclusionConfig {
            patterns: vec!["*.test.ts".to_string(), "docs/**".to_string()],
            respect_gitignore: true,
        };

        let walker = build_walker(temp_dir.path(), &config);
        let files: Vec<_> = walker
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Should only find main.ts (test files and docs excluded)
        let ts_files: Vec<_> = files
            .iter()
            .filter(|p| p.extension().map(|e| e == "ts").unwrap_or(false))
            .collect();
        assert_eq!(ts_files.len(), 1);
        assert!(ts_files[0].to_string_lossy().contains("main.ts"));
    }
}
