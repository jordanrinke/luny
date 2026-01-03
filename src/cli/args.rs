//! @dose
//! purpose: This module defines the command-line interface for luny using the clap derive
//!     macros. It specifies all commands (generate, validate, strip) and their arguments.
//!
//! when-editing:
//!     - !Each command struct must derive Args and be added to the Commands enum
//!     - !Global flags (root, verbose) are defined on Cli and propagate to all subcommands
//!     - Default values for token thresholds are specified in the derive attributes
//!
//! invariants:
//!     - The Cli struct is the root parser that clap uses to parse command-line arguments
//!     - Each subcommand has its own Args struct with typed fields
//!     - PathBuf is used for all file/directory path arguments to ensure proper path handling
//!
//! do-not:
//!     - Never add positional arguments that could conflict with subcommands
//!     - Never change default threshold values without updating documentation
//!
//! gotchas:
//!     - The strip command accepts "-" as input to read from stdin
//!     - Token thresholds have separate warn and error levels that can be customized
//!     - The --root flag is global but optional; defaults to current directory in main.rs

use crate::exclusion::ExclusionConfig;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "luny")]
#[command(author, version, about = "Multi-language TOON DOSE generator")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to project root (defaults to current directory)
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate TOON DOSE files from source
    Generate(GenerateArgs),

    /// Validate existing TOON DOSE files against source
    Validate(ValidateArgs),

    /// Strip @dose comments from source files
    Strip(StripArgs),

    /// Watch for file changes and regenerate TOON DOSE files
    Watch(WatchArgs),
}

/// Common options shared between generate and validate commands
#[derive(Args, Clone)]
pub struct CommonOptions {
    /// Follow symlinks that resolve outside of the project root (DANGEROUS).
    ///
    /// By default, luny follows symlinks but will only process targets whose resolved paths
    /// stay within `--root`. This flag disables that safety boundary.
    #[arg(long)]
    pub unsafe_follow: bool,

    /// Token count warning threshold
    #[arg(long, default_value = "500")]
    #[arg(default_value_t = 500)]
    pub token_warn: usize,

    /// Token count error threshold
    #[arg(long, default_value = "1000")]
    #[arg(default_value_t = 1000)]
    pub token_error: usize,

    /// Exclude files/directories matching glob pattern (can be repeated)
    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,

    /// Don't respect .gitignore files
    #[arg(long)]
    pub no_gitignore: bool,
}

impl Default for CommonOptions {
    fn default() -> Self {
        Self {
            unsafe_follow: false,
            token_warn: 500,
            token_error: 1000,
            exclude: Vec::new(),
            no_gitignore: false,
        }
    }
}

impl CommonOptions {
    /// Create an ExclusionConfig from these options, merging with config file patterns
    pub fn exclusion_config(&self, config_patterns: &[String]) -> ExclusionConfig {
        let mut patterns = config_patterns.to_vec();
        patterns.extend(self.exclude.clone());
        ExclusionConfig {
            patterns,
            respect_gitignore: !self.no_gitignore,
        }
    }
}

#[derive(Args, Default)]
pub struct GenerateArgs {
    /// Specific files or directories to process
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Dry run - show what would be generated without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Force regeneration even if TOON file exists
    #[arg(short, long)]
    pub force: bool,

    /// Clean the .ai directory before generating (removes stale files)
    #[arg(long)]
    pub clean: bool,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Args, Default)]
pub struct ValidateArgs {
    /// Specific TOON files to validate
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Attempt to fix validation errors by regenerating
    #[arg(long)]
    pub fix: bool,

    /// Strict mode - treat warnings as errors
    #[arg(long)]
    pub strict: bool,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Args)]
pub struct StripArgs {
    /// Source file to strip (use "-" for stdin)
    pub input: Option<PathBuf>,

    /// Output file (defaults to stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Language extension hint when reading from stdin
    #[arg(long)]
    pub ext: Option<String>,

    /// Minify output to reduce tokens (removes blank lines; also strips indentation for non-whitespace-sensitive languages)
    #[arg(short, long)]
    pub minify: bool,

    /// Experimental: aggressive minification using AST-aware whitespace removal (preserves string literals)
    #[arg(long)]
    pub minify_extreme: bool,
}

#[derive(Args, Default)]
pub struct WatchArgs {
    /// Specific files or directories to watch
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Debounce delay in milliseconds
    #[arg(long, default_value_t = 100)]
    pub debounce: u64,

    /// Clear screen before each update
    #[arg(long)]
    pub clear: bool,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_verify() {
        Cli::command().debug_assert();
    }

    /// Comprehensive test for generate command and all its options
    #[test]
    fn test_parse_generate() {
        // Default values
        let cli = Cli::try_parse_from(["luny", "generate"]).unwrap();
        let Commands::Generate(args) = cli.command else {
            panic!("Expected Generate")
        };
        assert!(args.paths.is_empty());
        assert!(!args.dry_run);
        assert!(!args.force);
        assert_eq!(args.common.token_warn, 500);
        assert_eq!(args.common.token_error, 1000);

        // With paths
        let cli = Cli::try_parse_from(["luny", "generate", "src/", "lib/"]).unwrap();
        let Commands::Generate(args) = cli.command else {
            panic!("Expected Generate")
        };
        assert_eq!(args.paths.len(), 2);

        // Flags: --dry-run, --force, -f
        let cli = Cli::try_parse_from(["luny", "generate", "--dry-run"]).unwrap();
        let Commands::Generate(args) = cli.command else {
            panic!("Expected Generate")
        };
        assert!(args.dry_run);

        let cli = Cli::try_parse_from(["luny", "generate", "-f"]).unwrap();
        let Commands::Generate(args) = cli.command else {
            panic!("Expected Generate")
        };
        assert!(args.force);

        // Token thresholds
        let cli = Cli::try_parse_from([
            "luny",
            "generate",
            "--token-warn",
            "300",
            "--token-error",
            "600",
        ])
        .unwrap();
        let Commands::Generate(args) = cli.command else {
            panic!("Expected Generate")
        };
        assert_eq!(args.common.token_warn, 300);
        assert_eq!(args.common.token_error, 600);
    }

    /// Comprehensive test for validate command and all its options
    #[test]
    fn test_parse_validate() {
        // Default values
        let cli = Cli::try_parse_from(["luny", "validate"]).unwrap();
        let Commands::Validate(args) = cli.command else {
            panic!("Expected Validate")
        };
        assert!(args.paths.is_empty());
        assert!(!args.fix);
        assert!(!args.strict);
        assert_eq!(args.common.token_warn, 500);
        assert_eq!(args.common.token_error, 1000);

        // With path
        let cli = Cli::try_parse_from(["luny", "validate", ".ai/src/main.ts.toon"]).unwrap();
        let Commands::Validate(args) = cli.command else {
            panic!("Expected Validate")
        };
        assert_eq!(args.paths.len(), 1);

        // Flags: --fix, --strict
        let cli = Cli::try_parse_from(["luny", "validate", "--fix"]).unwrap();
        let Commands::Validate(args) = cli.command else {
            panic!("Expected Validate")
        };
        assert!(args.fix);

        let cli = Cli::try_parse_from(["luny", "validate", "--strict"]).unwrap();
        let Commands::Validate(args) = cli.command else {
            panic!("Expected Validate")
        };
        assert!(args.strict);
    }

    /// Comprehensive test for strip command and all its options
    #[test]
    fn test_parse_strip() {
        // Basic usage
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts"]).unwrap();
        let Commands::Strip(args) = cli.command else {
            panic!("Expected Strip")
        };
        assert_eq!(args.input, Some(PathBuf::from("test.ts")));
        assert!(args.output.is_none());
        assert!(args.ext.is_none());

        // With output
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts", "-o", "output.ts"]).unwrap();
        let Commands::Strip(args) = cli.command else {
            panic!("Expected Strip")
        };
        assert_eq!(args.output, Some(PathBuf::from("output.ts")));

        // With ext (for stdin)
        let cli = Cli::try_parse_from(["luny", "strip", "--ext", "ts"]).unwrap();
        let Commands::Strip(args) = cli.command else {
            panic!("Expected Strip")
        };
        assert_eq!(args.ext, Some("ts".to_string()));

        // Stdin mode
        let cli = Cli::try_parse_from(["luny", "strip", "-", "--ext", "py"]).unwrap();
        let Commands::Strip(args) = cli.command else {
            panic!("Expected Strip")
        };
        assert_eq!(args.input, Some(PathBuf::from("-")));
    }

    /// Test global flags (-v, --verbose, -r, --root)
    #[test]
    fn test_global_flags() {
        // -v and --verbose
        let cli = Cli::try_parse_from(["luny", "-v", "generate"]).unwrap();
        assert!(cli.verbose);
        let cli = Cli::try_parse_from(["luny", "--verbose", "generate"]).unwrap();
        assert!(cli.verbose);

        // -r and --root
        let cli = Cli::try_parse_from(["luny", "-r", "/tmp/project", "generate"]).unwrap();
        assert_eq!(cli.root, Some(PathBuf::from("/tmp/project")));
        let cli = Cli::try_parse_from(["luny", "--root", "/tmp/project", "validate"]).unwrap();
        assert_eq!(cli.root, Some(PathBuf::from("/tmp/project")));

        // Flags after command
        let cli = Cli::try_parse_from(["luny", "generate", "-v"]).unwrap();
        assert!(cli.verbose);
    }

    /// Comprehensive test for watch command and all its options
    #[test]
    fn test_parse_watch() {
        // Default values
        let cli = Cli::try_parse_from(["luny", "watch"]).unwrap();
        let Commands::Watch(args) = cli.command else {
            panic!("Expected Watch")
        };
        assert!(args.paths.is_empty());
        assert_eq!(args.debounce, 100);
        assert!(!args.clear);

        // With paths
        let cli = Cli::try_parse_from(["luny", "watch", "src/", "lib/"]).unwrap();
        let Commands::Watch(args) = cli.command else {
            panic!("Expected Watch")
        };
        assert_eq!(args.paths.len(), 2);

        // Flags: --debounce, --clear
        let cli = Cli::try_parse_from(["luny", "watch", "--debounce", "200"]).unwrap();
        let Commands::Watch(args) = cli.command else {
            panic!("Expected Watch")
        };
        assert_eq!(args.debounce, 200);

        let cli = Cli::try_parse_from(["luny", "watch", "--clear"]).unwrap();
        let Commands::Watch(args) = cli.command else {
            panic!("Expected Watch")
        };
        assert!(args.clear);
    }

    /// Test error cases
    #[test]
    fn test_error_cases() {
        assert!(Cli::try_parse_from(["luny"]).is_err()); // Missing command
        assert!(Cli::try_parse_from(["luny", "invalid"]).is_err()); // Invalid command
        assert!(Cli::try_parse_from(["luny", "generate", "--token-warn", "not_a_number"]).is_err());
    }

    /// Test help output
    #[test]
    fn test_help_output() {
        let mut cmd = Cli::command();
        let help = format!("{}", cmd.render_help());
        assert!(help.contains("generate"));
        assert!(help.contains("validate"));
        assert!(help.contains("strip"));
        assert!(help.contains("watch"));
        assert!(help.contains("TOON") || help.contains("DOSE"));
    }
}
