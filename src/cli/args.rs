//! @toon
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

    /// Strip @toon comments from source files
    Strip(StripArgs),
}

#[derive(Args)]
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

    /// Follow symlinks that resolve outside of the project root (DANGEROUS).
    ///
    /// By default, luny follows symlinks but will only process targets whose resolved paths
    /// stay within `--root`. This flag disables that safety boundary.
    #[arg(long)]
    pub unsafe_follow: bool,

    /// Token count warning threshold
    #[arg(long, default_value = "500")]
    pub token_warn: usize,

    /// Token count error threshold
    #[arg(long, default_value = "1000")]
    pub token_error: usize,
}

#[derive(Args)]
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

    /// Follow symlinks that resolve outside of the project root (DANGEROUS).
    ///
    /// By default, luny follows symlinks but will only process targets whose resolved paths
    /// stay within `--root`. This flag disables that safety boundary.
    #[arg(long)]
    pub unsafe_follow: bool,

    /// Token count warning threshold
    #[arg(long, default_value = "500")]
    pub token_warn: usize,

    /// Token count error threshold
    #[arg(long, default_value = "1000")]
    pub token_error: usize,
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
        let Commands::Generate(args) = cli.command else { panic!("Expected Generate") };
        assert!(args.paths.is_empty());
        assert!(!args.dry_run);
        assert!(!args.force);
        assert_eq!(args.token_warn, 500);
        assert_eq!(args.token_error, 1000);

        // With paths
        let cli = Cli::try_parse_from(["luny", "generate", "src/", "lib/"]).unwrap();
        let Commands::Generate(args) = cli.command else { panic!("Expected Generate") };
        assert_eq!(args.paths.len(), 2);

        // Flags: --dry-run, --force, -f
        let cli = Cli::try_parse_from(["luny", "generate", "--dry-run"]).unwrap();
        let Commands::Generate(args) = cli.command else { panic!("Expected Generate") };
        assert!(args.dry_run);

        let cli = Cli::try_parse_from(["luny", "generate", "-f"]).unwrap();
        let Commands::Generate(args) = cli.command else { panic!("Expected Generate") };
        assert!(args.force);

        // Token thresholds
        let cli = Cli::try_parse_from(["luny", "generate", "--token-warn", "300", "--token-error", "600"]).unwrap();
        let Commands::Generate(args) = cli.command else { panic!("Expected Generate") };
        assert_eq!(args.token_warn, 300);
        assert_eq!(args.token_error, 600);
    }

    /// Comprehensive test for validate command and all its options
    #[test]
    fn test_parse_validate() {
        // Default values
        let cli = Cli::try_parse_from(["luny", "validate"]).unwrap();
        let Commands::Validate(args) = cli.command else { panic!("Expected Validate") };
        assert!(args.paths.is_empty());
        assert!(!args.fix);
        assert!(!args.strict);
        assert_eq!(args.token_warn, 500);
        assert_eq!(args.token_error, 1000);

        // With path
        let cli = Cli::try_parse_from(["luny", "validate", ".ai/src/main.ts.toon"]).unwrap();
        let Commands::Validate(args) = cli.command else { panic!("Expected Validate") };
        assert_eq!(args.paths.len(), 1);

        // Flags: --fix, --strict
        let cli = Cli::try_parse_from(["luny", "validate", "--fix"]).unwrap();
        let Commands::Validate(args) = cli.command else { panic!("Expected Validate") };
        assert!(args.fix);

        let cli = Cli::try_parse_from(["luny", "validate", "--strict"]).unwrap();
        let Commands::Validate(args) = cli.command else { panic!("Expected Validate") };
        assert!(args.strict);
    }

    /// Comprehensive test for strip command and all its options
    #[test]
    fn test_parse_strip() {
        // Basic usage
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts"]).unwrap();
        let Commands::Strip(args) = cli.command else { panic!("Expected Strip") };
        assert_eq!(args.input, Some(PathBuf::from("test.ts")));
        assert!(args.output.is_none());
        assert!(args.ext.is_none());

        // With output
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts", "-o", "output.ts"]).unwrap();
        let Commands::Strip(args) = cli.command else { panic!("Expected Strip") };
        assert_eq!(args.output, Some(PathBuf::from("output.ts")));

        // With ext (for stdin)
        let cli = Cli::try_parse_from(["luny", "strip", "--ext", "ts"]).unwrap();
        let Commands::Strip(args) = cli.command else { panic!("Expected Strip") };
        assert_eq!(args.ext, Some("ts".to_string()));

        // Stdin mode
        let cli = Cli::try_parse_from(["luny", "strip", "-", "--ext", "py"]).unwrap();
        let Commands::Strip(args) = cli.command else { panic!("Expected Strip") };
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
        assert!(help.contains("TOON") || help.contains("DOSE"));
    }
}
