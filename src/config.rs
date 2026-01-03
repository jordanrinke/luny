//! @dose
//! purpose: Configuration file parsing for luny.toml. Handles exclusion patterns,
//!     default token thresholds, and per-pattern threshold overrides.
//!
//! when-editing:
//!     - !Config is loaded once at startup and passed through the call chain
//!     - !Threshold overrides use glob patterns matched against relative paths
//!     - false in TOML means "disable this threshold check"
//!
//! invariants:
//!     - Config::load returns default config if luny.toml doesn't exist
//!     - Pattern matching for overrides uses the same glob syntax as exclusions
//!
//! gotchas:
//!     - Patterns are matched against paths relative to project root
//!     - First matching override wins (order matters in TOML array)

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Main configuration structure matching luny.toml
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Exclusion patterns (gitignore-style)
    pub exclude: Vec<String>,

    /// Always clean .ai directory before generating (removes stale files)
    pub clean: bool,

    /// Token threshold configuration
    pub tokens: TokenConfig,
}

/// Token threshold configuration
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TokenConfig {
    /// Default warning threshold
    pub warn: usize,

    /// Default error threshold
    pub error: usize,

    /// Per-pattern overrides
    #[serde(rename = "override")]
    pub overrides: Vec<ThresholdOverride>,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            warn: 500,
            error: 1000,
            overrides: Vec::new(),
        }
    }
}

/// A threshold override for specific file patterns
#[derive(Debug, Deserialize, Clone)]
pub struct ThresholdOverride {
    /// Glob pattern to match (relative to root)
    pub pattern: String,

    /// Warning threshold (None = disabled)
    #[serde(default, deserialize_with = "deserialize_threshold")]
    pub warn: Option<usize>,

    /// Error threshold (None = disabled)
    #[serde(default, deserialize_with = "deserialize_threshold")]
    pub error: Option<usize>,
}

/// Deserialize threshold that can be a number or false (disabled)
fn deserialize_threshold<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ThresholdValue {
        Number(usize),
        Bool(bool),
    }

    match ThresholdValue::deserialize(deserializer)? {
        ThresholdValue::Number(n) => Ok(Some(n)),
        ThresholdValue::Bool(false) => Ok(None),
        ThresholdValue::Bool(true) => Ok(None), // true also disables (use number for specific value)
    }
}

/// Compiled threshold overrides for efficient matching
pub struct ThresholdMatcher {
    overrides: Vec<(GlobSet, Option<usize>, Option<usize>)>,
    default_warn: usize,
    default_error: usize,
}

/// Result of threshold lookup for a file
#[derive(Debug, Clone, Copy)]
pub struct FileThresholds {
    /// Warning threshold (None = disabled)
    pub warn: Option<usize>,
    /// Error threshold (None = disabled)
    pub error: Option<usize>,
}

impl ThresholdMatcher {
    /// Create a new matcher from config
    pub fn new(config: &TokenConfig) -> Self {
        let mut overrides = Vec::new();

        for ov in &config.overrides {
            let mut builder = GlobSetBuilder::new();
            if let Ok(glob) = Glob::new(&ov.pattern) {
                builder.add(glob);
                if let Ok(globset) = builder.build() {
                    overrides.push((globset, ov.warn, ov.error));
                }
            }
        }

        Self {
            overrides,
            default_warn: config.warn,
            default_error: config.error,
        }
    }

    /// Get thresholds for a file path (relative to root)
    pub fn get_thresholds(&self, relative_path: &Path) -> FileThresholds {
        // Check overrides in order (first match wins)
        for (globset, warn, error) in &self.overrides {
            if globset.is_match(relative_path) {
                return FileThresholds {
                    warn: *warn,
                    error: *error,
                };
            }
        }

        // Return defaults
        FileThresholds {
            warn: Some(self.default_warn),
            error: Some(self.default_error),
        }
    }
}

impl Config {
    /// Load configuration from luny.toml in the given root directory
    pub fn load(root: &Path) -> Self {
        let config_path = root.join("luny.toml");

        if !config_path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: Failed to parse luny.toml: {}", e);
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to read luny.toml: {}", e);
                Self::default()
            }
        }
    }

    /// Create a ThresholdMatcher from this config
    pub fn threshold_matcher(&self) -> ThresholdMatcher {
        ThresholdMatcher::new(&self.tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.exclude.is_empty());
        assert_eq!(config.tokens.warn, 500);
        assert_eq!(config.tokens.error, 1000);
        assert!(config.tokens.overrides.is_empty());
    }

    #[test]
    fn test_load_missing_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::load(temp_dir.path());
        assert_eq!(config.tokens.warn, 500);
        assert_eq!(config.tokens.error, 1000);
    }

    #[test]
    fn test_load_basic_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
exclude = ["*.test.ts", "docs/**"]

[tokens]
warn = 600
error = 1200
"#;
        fs::write(temp_dir.path().join("luny.toml"), config_content).unwrap();

        let config = Config::load(temp_dir.path());
        assert_eq!(config.exclude, vec!["*.test.ts", "docs/**"]);
        assert_eq!(config.tokens.warn, 600);
        assert_eq!(config.tokens.error, 1200);
    }

    #[test]
    fn test_load_config_with_overrides() {
        let temp_dir = TempDir::new().unwrap();
        let config_content = r#"
[tokens]
warn = 500
error = 1000

[[tokens.override]]
pattern = "src/generated/**"
warn = 5000
error = 10000

[[tokens.override]]
pattern = "src/legacy/**"
warn = false
error = false
"#;
        fs::write(temp_dir.path().join("luny.toml"), config_content).unwrap();

        let config = Config::load(temp_dir.path());
        assert_eq!(config.tokens.overrides.len(), 2);
        assert_eq!(config.tokens.overrides[0].pattern, "src/generated/**");
        assert_eq!(config.tokens.overrides[0].warn, Some(5000));
        assert_eq!(config.tokens.overrides[0].error, Some(10000));
        assert_eq!(config.tokens.overrides[1].pattern, "src/legacy/**");
        assert_eq!(config.tokens.overrides[1].warn, None);
        assert_eq!(config.tokens.overrides[1].error, None);
    }

    #[test]
    fn test_threshold_matcher_defaults() {
        let config = TokenConfig::default();
        let matcher = ThresholdMatcher::new(&config);

        let thresholds = matcher.get_thresholds(Path::new("src/main.rs"));
        assert_eq!(thresholds.warn, Some(500));
        assert_eq!(thresholds.error, Some(1000));
    }

    #[test]
    fn test_threshold_matcher_override() {
        let config = TokenConfig {
            warn: 500,
            error: 1000,
            overrides: vec![ThresholdOverride {
                pattern: "src/generated/**".to_string(),
                warn: Some(5000),
                error: Some(10000),
            }],
        };
        let matcher = ThresholdMatcher::new(&config);

        // Matching file
        let thresholds = matcher.get_thresholds(Path::new("src/generated/types.rs"));
        assert_eq!(thresholds.warn, Some(5000));
        assert_eq!(thresholds.error, Some(10000));

        // Non-matching file
        let thresholds = matcher.get_thresholds(Path::new("src/main.rs"));
        assert_eq!(thresholds.warn, Some(500));
        assert_eq!(thresholds.error, Some(1000));
    }

    #[test]
    fn test_threshold_matcher_disabled() {
        let config = TokenConfig {
            warn: 500,
            error: 1000,
            overrides: vec![ThresholdOverride {
                pattern: "**/*.min.js".to_string(),
                warn: None,
                error: None,
            }],
        };
        let matcher = ThresholdMatcher::new(&config);

        let thresholds = matcher.get_thresholds(Path::new("dist/bundle.min.js"));
        assert_eq!(thresholds.warn, None);
        assert_eq!(thresholds.error, None);
    }

    #[test]
    fn test_threshold_matcher_first_match_wins() {
        let config = TokenConfig {
            warn: 500,
            error: 1000,
            overrides: vec![
                ThresholdOverride {
                    pattern: "src/**".to_string(),
                    warn: Some(1000),
                    error: Some(2000),
                },
                ThresholdOverride {
                    pattern: "src/generated/**".to_string(),
                    warn: Some(5000),
                    error: Some(10000),
                },
            ],
        };
        let matcher = ThresholdMatcher::new(&config);

        // First pattern matches, so we get its values (not the more specific one)
        let thresholds = matcher.get_thresholds(Path::new("src/generated/types.rs"));
        assert_eq!(thresholds.warn, Some(1000));
        assert_eq!(thresholds.error, Some(2000));
    }
}
