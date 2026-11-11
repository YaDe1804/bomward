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
  bomward diff <OLD> <NEW> [--policy <FILE>] [--format <terminal|markdown|json>]

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
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Report format.
        #[arg(long, value_enum, default_value = "terminal")]
        format: OutputFormat,
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
        } => run_diff(&old, &new, policy.as_deref(), format),
    }
}

fn run_diff(_old: &Path, _new: &Path, _policy: Option<&Path>, _format: OutputFormat) -> Result<()> {
    Err(CoreError::NotImplemented.into())
}
