// SPDX-License-Identifier: AGPL-3.0-only

//! Failures while reading a policy file.
//!
//! The policy is configuration. A typo, an unknown field, or a repeated key
//! is an error here, not a warning and not a quiet allow. Text copied from
//! the file is cut and control characters are escaped, the same way ADR 0006
//! treats a rejected `specVersion`.

use std::fmt::Write as _;

use thiserror::Error;

/// The policy file cannot be used.
///
/// The CLI maps this to exit code 2. No verdict is produced.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyError {
    /// The bytes are not UTF-8. A leading UTF-8 BOM is not this error.
    #[error("invalid UTF-8")]
    Utf8,

    /// The file is larger than the configured byte limit.
    #[error("policy file is {found} bytes; the limit is {limit}")]
    FileTooLarge {
        /// Size that was found, in bytes.
        found: u64,
        /// Configured limit, in bytes.
        limit: u64,
    },

    /// `rules` has more entries than the limit.
    #[error("policy has {found} rules; the limit is {limit}")]
    TooManyRules {
        /// Number of rules in the file.
        found: usize,
        /// Configured limit.
        limit: usize,
    },

    /// One list inside a rule is longer than the limit.
    #[error("policy rule {rule} field {field} has {found} entries; the limit is {limit}")]
    ListTooLong {
        /// Rule id, already checked against the id grammar.
        rule: String,
        /// Field name from the schema.
        field: &'static str,
        /// Number of entries found.
        found: usize,
        /// Configured limit.
        limit: usize,
    },

    /// `description` is longer than the limit.
    #[error("policy rule {rule} description is {found} characters; the limit is {limit}")]
    DescriptionTooLong {
        /// Rule id, already checked against the id grammar.
        rule: String,
        /// Character count.
        found: usize,
        /// Configured limit.
        limit: usize,
    },

    /// A package pattern is longer than the limit.
    #[error("policy pattern is {found} characters; the limit is {limit}")]
    PatternTooLong {
        /// Character count.
        found: usize,
        /// Configured limit.
        limit: usize,
    },

    /// The file is not a version-1 policy.
    ///
    /// `line` and `column` are set when the YAML parser reported them.
    /// Semantic checks name the rule and the field in `message` and leave
    /// the location empty.
    #[error("{}", located_message(*line, *column, message))]
    Invalid {
        /// 1-based line, when the parser has one.
        line: Option<usize>,
        /// 1-based column, when the parser has one.
        column: Option<usize>,
        /// What is wrong. File text inside it is sanitized.
        message: String,
    },
}

fn located_message(line: Option<usize>, column: Option<usize>, message: &str) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!("line {line} column {column}: {message}"),
        (Some(line), None) => format!("line {line}: {message}"),
        _ => message.to_owned(),
    }
}

/// Cut file text and escape control characters.
///
/// U+0000 through U+001F and U+007F become `\u{XXXX}`. The result is at most
/// 64 of the original characters, then `...` when the input was longer.
pub(crate) fn sanitize(text: &str) -> String {
    let mut out = String::new();
    for (count, ch) in text.chars().enumerate() {
        if count == 64 {
            out.push_str("...");
            break;
        }
        if ch.is_control() {
            let _ = write!(out, "\\u{{{:04X}}}", u32::from(ch));
        } else {
            out.push(ch);
        }
    }
    out
}
