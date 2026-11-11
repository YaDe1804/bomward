// SPDX-License-Identifier: AGPL-3.0-only

//! Command-line interface for the bomward SBOM review gate.
//!
//! The binary parses arguments and maps library errors to process exit codes.
//! Review logic lives in `bomward-core`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use bomward_core::Error as CoreError;
use clap::{Parser, Subcommand, ValueEnum};

/// SBOM review gate for pull requests and releases.
#[derive(Debug, Parser)]
#[command(
    name = "bomward",
    version,
    about = "SBOM review gate for pull requests and releases.",
    long_about = "\
SBOM review gate for pull requests and releases.

The diff subcommand compares two CycloneDX JSON documents:
  bomward diff <OLD> <NEW> [--policy <FILE>] [--format <terminal|markdown|json>] [--max-file-bytes N] [--max-components N] [--max-depth N]

Without --policy the run is report-only and exits 0. Every output then includes the sentence \"No policy was applied.\"
Limits default to 33554432 bytes, 100000 components, and depth 32. A violation names the limit, the value found, and the flag.

Status: pre-release, not usable yet. diff exits 2.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compare two `CycloneDX` JSON documents and judge the change.
    Diff {
        /// Baseline `CycloneDX` JSON document.
        old: PathBuf,
        /// Candidate `CycloneDX` JSON document.
        new: PathBuf,
        /// Policy file used to judge the diff.
        ///
        /// Omit this flag for report-only mode, which exits 0.
        /// Every output then includes the sentence "No policy was applied."
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Report format.
        #[arg(long, value_enum, default_value = "terminal")]
        format: OutputFormat,
        /// Maximum size in bytes of one input file. Default: 33554432 (32 MiB).
        ///
        /// A violation names this limit, the size that was found, and `--max-file-bytes`.
        #[arg(long, default_value_t = 33_554_432)]
        max_file_bytes: u64,
        /// Maximum number of component objects, including nested components. Default: 100000.
        ///
        /// A violation names this limit, the count that was found, and `--max-components`.
        #[arg(long, default_value_t = 100_000)]
        max_components: u64,
        /// Maximum nesting of components inside components, and of JSON arrays and objects while parsing. Default: 32.
        ///
        /// Does not limit dependency-graph path length. A violation names this limit, the depth that was found, and `--max-depth`.
        #[arg(long, default_value_t = 32)]
        max_depth: u32,
    },
}

/// Report format selected by `--format`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    /// Plain text for a terminal, without ANSI color.
    Terminal,
    /// Markdown suitable for a pull-request comment.
    Markdown,
    /// One JSON document.
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Diff {
            old,
            new,
            policy,
            format,
            max_file_bytes,
            max_components,
            max_depth,
        } => run_diff(
            &old,
            &new,
            policy.as_deref(),
            format,
            (max_file_bytes, max_components, max_depth),
        ),
    }
}

fn run_diff(
    _old: &Path,
    _new: &Path,
    _policy: Option<&Path>,
    _format: OutputFormat,
    _limits: (u64, u64, u32),
) -> Result<()> {
    Err(CoreError::NotImplemented.into())
}
