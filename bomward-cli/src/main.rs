// SPDX-License-Identifier: AGPL-3.0-only

//! Command-line interface for the bomward SBOM review gate.
//!
//! The binary parses arguments, calls the library, and maps the verdict to
//! an exit code. Review logic lives in `bomward-core`.

#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bomward_core::policy::{Policy, PolicyLimits};
use bomward_core::report::{Format, render};
use bomward_core::{Error as CoreError, Limits};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

/// Width used for `--help`, so the golden file does not follow the console.
const HELP_WIDTH: usize = 100;

/// SBOM review gate for pull requests and releases.
#[derive(Debug, Parser)]
#[command(
    name = "bomward",
    bin_name = "bomward",
    version,
    about = "SBOM review gate for pull requests and releases.",
    long_about = "\
SBOM review gate for pull requests and releases.

The diff subcommand compares two CycloneDX JSON documents:
  bomward diff <OLD> <NEW> [--policy <FILE>] [--format <terminal|markdown|json>] [--output <FILE>] [--max-file-bytes N] [--max-components N] [--max-depth N]

Without --policy the run is report-only and exits 0. Every output then includes the sentence \"No policy was applied.\"
Limits default to 33554432 bytes, 100000 components, and depth 32. A violation names the limit, the value found, and the flag.

Status: pre-release.",
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
        /// Write the report to this file instead of stdout.
        ///
        /// The file is created or truncated. Parent directories are not created.
        /// Stdout stays empty.
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
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
        /// Maximum nesting of components inside components. Default: 32. Maximum: 63.
        ///
        /// Does not limit dependency-graph path length or JSON container depth. JSON containers are capped at 127, and that error does not name this flag. A value above 63 is rejected before the files are read. A component-nesting violation names this limit, the depth that was found, and `--max-depth`.
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

enum CliError {
    Message(anyhow::Error),
    ClosedPipe,
}

fn message(text: impl std::fmt::Display) -> CliError {
    CliError::Message(anyhow::Error::msg(text.to_string()))
}

fn main() -> ExitCode {
    let matches = Cli::command().term_width(HELP_WIDTH).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    match execute(cli) {
        Ok(code) => ExitCode::from(code),
        Err(CliError::ClosedPipe) => ExitCode::SUCCESS,
        Err(CliError::Message(error)) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn execute(cli: Cli) -> Result<u8, CliError> {
    match cli.command {
        Command::Diff {
            old,
            new,
            policy,
            format,
            output,
            max_file_bytes,
            max_components,
            max_depth,
        } => run_diff(
            &old,
            &new,
            policy.as_deref(),
            format,
            output.as_deref(),
            (max_file_bytes, max_components, max_depth),
        ),
    }
}

fn run_diff(
    old: &Path,
    new: &Path,
    policy: Option<&Path>,
    format: OutputFormat,
    output: Option<&Path>,
    limits: (u64, u64, u32),
) -> Result<u8, CliError> {
    let limits = Limits::new(limits.0, limits.1, limits.2).map_err(message)?;
    let old_document = read_document("OLD", old, &limits)?;
    let new_document = read_document("NEW", new, &limits)?;
    let diff = bomward_core::diff::diff(&old_document, &new_document);
    let verdict = match policy {
        Some(path) => Some(bomward_core::policy::evaluate(&read_policy(path)?, &diff)),
        None => None,
    };
    let text = render(&diff, verdict.as_ref(), report_format(format));
    emit(&text, output)?;
    Ok(verdict
        .as_ref()
        .map_or(0, bomward_core::policy::Verdict::exit_code))
}

fn read_document(
    which: &str,
    path: &Path,
    limits: &Limits,
) -> Result<bomward_core::Document, CliError> {
    bomward_core::read_path(path, limits).map_err(|error| message(explain(which, path, error)))
}

fn read_policy(path: &Path) -> Result<Policy, CliError> {
    bomward_core::policy::read_path(path, &PolicyLimits::default())
        .map_err(|error| message(explain("policy", path, error)))
}

fn explain(which: &str, path: &Path, error: CoreError) -> String {
    let shown = path.display();
    match error {
        CoreError::Io { source, .. } => format!("failed to read {which} {shown}: {source}"),
        CoreError::InvalidDocument(inner) => format!("{which} {shown}: {inner}"),
        CoreError::UnsupportedSpecVersion { found } => format!(
            "{which} {shown}: unsupported specVersion {found}; supported versions: 1.4, 1.5, 1.6, 1.7"
        ),
        CoreError::LimitExceeded(inner) => format!("{which} {shown}: {inner}"),
        CoreError::InvalidPolicy(inner) => format!("policy {shown}: {inner}"),
        CoreError::UnsupportedDepth(inner) => inner.to_string(),
        other => format!("{which} {shown}: {other}"),
    }
}

fn report_format(format: OutputFormat) -> Format {
    match format {
        OutputFormat::Terminal => Format::Terminal,
        OutputFormat::Markdown => Format::Markdown,
        OutputFormat::Json => Format::Json,
    }
}

fn emit(text: &str, output: Option<&Path>) -> Result<(), CliError> {
    let bytes = text.as_bytes();
    match output {
        Some(path) => write_file(path, bytes),
        None => write_stdout(bytes),
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|source| message(format!("failed to write {}: {source}", path.display())))?;
    file.write_all(bytes)
        .map_err(|source| message(format!("failed to write {}: {source}", path.display())))
}

fn write_stdout(bytes: &[u8]) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::BrokenPipe => Err(CliError::ClosedPipe),
        Err(source) => Err(message(format!("failed to write stdout: {source}"))),
    }
}
