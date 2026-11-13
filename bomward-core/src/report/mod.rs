// SPDX-License-Identifier: AGPL-3.0-only

//! Render a diff, and an optional verdict, as terminal text, Markdown, or JSON.
//!
//! Rendering is deterministic: one sentence catalog, `LF` line endings, no
//! timestamps, and no local paths. Untrusted strings are escaped here and
//! nowhere else. See ADR 0015.

mod assemble;
mod escape;
mod explain;
mod json;
mod markdown;
mod terminal;
mod text;

#[cfg(test)]
mod safety;

use assemble::prepare;
use json::JsonReport;

pub use escape::DISPLAY_VALUE_CHARS;
pub use markdown::MARKDOWN_CHAR_BUDGET;

/// Which bytes [`render`] writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Plain text for a terminal, without ANSI color.
    Terminal,
    /// Markdown for a pull-request comment.
    Markdown,
    /// One JSON document, `report_version` 1.
    Json,
}

/// Render `diff`. `verdict` is `None` in report-only mode.
///
/// The same inputs and format always produce the same bytes. The result
/// ends with `LF` and contains no `CR`.
#[must_use]
pub fn render(
    diff: &crate::diff::Diff,
    verdict: Option<&crate::policy::Verdict>,
    format: Format,
) -> String {
    let prepared = prepare(diff, verdict, env!("CARGO_PKG_VERSION"));
    match format {
        Format::Terminal => terminal::render(&prepared),
        Format::Markdown => markdown::render(&prepared),
        Format::Json => render_json(&prepared.json),
    }
}

fn render_json(report: &JsonReport) -> String {
    let mut text = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_owned());
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
