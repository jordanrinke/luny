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

    // ==================== CLI Parsing Tests ====================

    #[test]
    fn test_cli_verify() {
        // Verify that the CLI definition is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_generate_command() {
        let cli = Cli::try_parse_from(["luny", "generate"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert!(args.paths.is_empty());
                assert!(!args.dry_run);
                assert!(!args.force);
                assert!(!args.unsafe_follow);
                assert_eq!(args.token_warn, 500);
                assert_eq!(args.token_error, 1000);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_with_paths() {
        let cli = Cli::try_parse_from(["luny", "generate", "src/", "lib/"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.paths.len(), 2);
                assert_eq!(args.paths[0], PathBuf::from("src/"));
                assert_eq!(args.paths[1], PathBuf::from("lib/"));
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_dry_run() {
        let cli = Cli::try_parse_from(["luny", "generate", "--dry-run"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert!(args.dry_run);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_force() {
        let cli = Cli::try_parse_from(["luny", "generate", "--force"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert!(args.force);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_force_short() {
        let cli = Cli::try_parse_from(["luny", "generate", "-f"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert!(args.force);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_generate_token_thresholds() {
        let cli =
            Cli::try_parse_from(["luny", "generate", "--token-warn", "300", "--token-error", "600"])
                .unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.token_warn, 300);
                assert_eq!(args.token_error, 600);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_parse_validate_command() {
        let cli = Cli::try_parse_from(["luny", "validate"]).unwrap();
        match cli.command {
            Commands::Validate(args) => {
                assert!(args.paths.is_empty());
                assert!(!args.fix);
                assert!(!args.strict);
                assert!(!args.unsafe_follow);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test]
    fn test_parse_validate_with_paths() {
        let cli = Cli::try_parse_from(["luny", "validate", ".ai/src/main.ts.toon"]).unwrap();
        match cli.command {
            Commands::Validate(args) => {
                assert_eq!(args.paths.len(), 1);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test]
    fn test_parse_validate_fix() {
        let cli = Cli::try_parse_from(["luny", "validate", "--fix"]).unwrap();
        match cli.command {
            Commands::Validate(args) => {
                assert!(args.fix);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test]
    fn test_parse_validate_strict() {
        let cli = Cli::try_parse_from(["luny", "validate", "--strict"]).unwrap();
        match cli.command {
            Commands::Validate(args) => {
                assert!(args.strict);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test]
    fn test_parse_strip_command() {
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts"]).unwrap();
        match cli.command {
            Commands::Strip(args) => {
                assert_eq!(args.input, Some(PathBuf::from("test.ts")));
                assert!(args.output.is_none());
                assert!(args.ext.is_none());
            }
            _ => panic!("Expected Strip command"),
        }
    }

    #[test]
    fn test_parse_strip_with_output() {
        let cli = Cli::try_parse_from(["luny", "strip", "test.ts", "-o", "output.ts"]).unwrap();
        match cli.command {
            Commands::Strip(args) => {
                assert_eq!(args.input, Some(PathBuf::from("test.ts")));
                assert_eq!(args.output, Some(PathBuf::from("output.ts")));
            }
            _ => panic!("Expected Strip command"),
        }
    }

    #[test]
    fn test_parse_strip_with_ext() {
        let cli = Cli::try_parse_from(["luny", "strip", "--ext", "ts"]).unwrap();
        match cli.command {
            Commands::Strip(args) => {
                assert!(args.input.is_none());
                assert_eq!(args.ext, Some("ts".to_string()));
            }
            _ => panic!("Expected Strip command"),
        }
    }

    #[test]
    fn test_parse_strip_stdin() {
        let cli = Cli::try_parse_from(["luny", "strip", "-", "--ext", "py"]).unwrap();
        match cli.command {
            Commands::Strip(args) => {
                assert_eq!(args.input, Some(PathBuf::from("-")));
                assert_eq!(args.ext, Some("py".to_string()));
            }
            _ => panic!("Expected Strip command"),
        }
    }

    // ==================== Global Flags Tests ====================

    #[test]
    fn test_global_verbose_flag() {
        let cli = Cli::try_parse_from(["luny", "-v", "generate"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_global_verbose_long_flag() {
        let cli = Cli::try_parse_from(["luny", "--verbose", "generate"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_global_root_flag() {
        let cli = Cli::try_parse_from(["luny", "-r", "/tmp/project", "generate"]).unwrap();
        assert_eq!(cli.root, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn test_global_root_long_flag() {
        let cli = Cli::try_parse_from(["luny", "--root", "/tmp/project", "validate"]).unwrap();
        assert_eq!(cli.root, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn test_global_flags_after_command() {
        // Global flags can appear after the command
        let cli = Cli::try_parse_from(["luny", "generate", "-v"]).unwrap();
        assert!(cli.verbose);
    }

    // ==================== Default Values Tests ====================

    #[test]
    fn test_default_token_warn() {
        let cli = Cli::try_parse_from(["luny", "generate"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.token_warn, 500);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_default_token_error() {
        let cli = Cli::try_parse_from(["luny", "generate"]).unwrap();
        match cli.command {
            Commands::Generate(args) => {
                assert_eq!(args.token_error, 1000);
            }
            _ => panic!("Expected Generate command"),
        }
    }

    #[test]
    fn test_validate_default_thresholds() {
        let cli = Cli::try_parse_from(["luny", "validate"]).unwrap();
        match cli.command {
            Commands::Validate(args) => {
                assert_eq!(args.token_warn, 500);
                assert_eq!(args.token_error, 1000);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    // ==================== Error Cases Tests ====================

    #[test]
    fn test_missing_command() {
        let result = Cli::try_parse_from(["luny"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_command() {
        let result = Cli::try_parse_from(["luny", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_token_warn() {
        let result = Cli::try_parse_from(["luny", "generate", "--token-warn", "not_a_number"]);
        assert!(result.is_err());
    }

    // ==================== Help Text Tests ====================

    #[test]
    fn test_help_contains_commands() {
        let mut cmd = Cli::command();
        let help = format!("{}", cmd.render_help());
        assert!(help.contains("generate"));
        assert!(help.contains("validate"));
        assert!(help.contains("strip"));
    }

    #[test]
    fn test_help_contains_description() {
        let mut cmd = Cli::command();
        let help = format!("{}", cmd.render_help());
        assert!(help.contains("TOON") || help.contains("DOSE"));
    }
}
