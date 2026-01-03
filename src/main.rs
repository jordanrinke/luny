//! @dose
//! purpose: This is the CLI entry point for luny. It parses command-line arguments using clap,
//!     determines the project root directory, and dispatches to the appropriate command handler
//!     (generate, validate, or strip).
//!
//! when-editing:
//!     - !All command handlers are imported from the luny crate
//!     - !The root directory defaults to current working directory if not specified
//!     - Error messages are printed to stderr and exit with code 1
//!
//! invariants:
//!     - One and only one subcommand is always executed per invocation
//!     - The process exits with 0 on success, 1 on any error
//!
//! do-not:
//!     - Never add business logic here - delegate to command modules
//!     - Never panic - always use proper error handling
//!
//! gotchas:
//!     - The --root flag can be placed before or after the subcommand due to global flag
//!     - Verbose mode is also a global flag that affects all commands

use anyhow::Context;
use clap::Parser;
use luny::cli::{Cli, Commands};
use luny::commands::{run_generate, run_strip, run_validate};
use std::env;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Determine root directory
    let root = match cli.root {
        Some(root) => root,
        None => env::current_dir().context("Failed to get current directory")?,
    };

    match cli.command {
        Commands::Generate(args) => run_generate(&args, &root, cli.verbose),
        Commands::Validate(args) => run_validate(&args, &root, cli.verbose),
        Commands::Strip(args) => run_strip(&args, &root, cli.verbose),
    }
}
