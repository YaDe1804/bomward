// SPDX-License-Identifier: AGPL-3.0-only

//! Typed errors for the bomward library.
//!
//! [`Error::NotImplemented`] remains so older callers keep a stable message.
//! Ingest, policy, and the report use the other variants.
//! Display text for a rejected `specVersion` is the sentence from ADR 0004.
//! Display text for a limit is one of the sentences from ADR 0006.
//!
//! Messages do not copy SBOM strings. The one exception is `specVersion`:
//! control characters are written as `\u{XXXX}` and the value is cut at 64
//! characters. That escaping is the rule in ADR 0006.

use std::path::PathBuf;

use thiserror::Error;

/// Failure returned by the bomward library.
///
/// Library functions return this type instead of panicking. The variants are
/// not a stable set: the crate is not published.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A module that this milestone does not implement.
    ///
    /// Kept so the sentence `not implemented yet` stays stable.
    /// The CLI maps every error to exit code 2.
    #[error("not implemented yet")]
    NotImplemented,

    /// The policy file is not a valid version-1 policy, or it exceeds a limit.
    ///
    /// The CLI maps this to exit code 2. No verdict is produced.
    #[error(transparent)]
    InvalidPolicy(#[from] crate::policy::PolicyError),

    /// The path the caller passed could not be read.
    ///
    /// `path` is that caller path. It is never a string taken from a SBOM.
    #[error("failed to read {}", path.display())]
    Io {
        /// Path the caller asked the library to open.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// The bytes are not a `CycloneDX` document this version can read.
    #[error(transparent)]
    InvalidDocument(#[from] DocumentError),

    /// `specVersion` is absent or not one of 1.4, 1.5, 1.6, 1.7.
    ///
    /// `found` is `missing` when the field is absent. Otherwise it is the
    /// sanitized spec value described on this module.
    #[error("unsupported specVersion {found}; supported versions: 1.4, 1.5, 1.6, 1.7")]
    UnsupportedSpecVersion {
        /// Sanitized value, or `missing`.
        found: String,
    },

    /// A size, component-count, or depth limit was exceeded.
    ///
    /// The document is not truncated. Callers match on [`LimitExceeded::kind`].
    #[error(transparent)]
    LimitExceeded(#[from] LimitExceeded),

    /// `--max-depth` is above the depth the JSON parser can hold.
    ///
    /// This is a usage error. No file is parsed.
    #[error(transparent)]
    UnsupportedDepth(#[from] UnsupportedDepth),
}

/// Why a document was rejected before it became a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DocumentError {
    /// The bytes are not UTF-8. A leading UTF-8 BOM is not this error.
    #[error("invalid UTF-8")]
    Utf8,

    /// The bytes are not a JSON value.
    ///
    /// `line` and `column` locate the syntax error. They are not a copy of
    /// the document.
    #[error("invalid JSON at line {line} column {column}")]
    Json {
        /// 1-based line reported by the JSON parser.
        line: usize,
        /// 1-based column reported by the JSON parser.
        column: usize,
    },

    /// `bomFormat` is missing or is not the string `CycloneDX`.
    #[error("bomFormat must be CycloneDX")]
    BomFormat,

    /// A component with no usable purl also has no `name`.
    #[error("component is missing a name")]
    MissingName,

    /// A required JSON shape is wrong: a component is not an object, a
    /// `components` or `dependencies` value is not an array, or a
    /// `dependsOn` entry is not a string.
    #[error("invalid document")]
    Shape,
}

/// Which limit a [`LimitExceeded`] error reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// `--max-file-bytes`.
    FileBytes,
    /// `--max-components`.
    ComponentCount,
    /// `--max-depth`: nesting of components inside components.
    NestingDepth,
    /// JSON arrays and objects nested past the parser cap.
    ///
    /// This is not `--max-depth`. The sentence does not name that flag.
    JsonNesting,
}

/// `--max-depth` is above [`crate::Limits::MAX_SUPPORTED_DEPTH`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("--max-depth {requested} exceeds supported maximum {maximum}")]
pub struct UnsupportedDepth {
    /// The value passed as `--max-depth`.
    pub requested: u32,
    /// The largest value this build can parse.
    pub maximum: u32,
}

/// A limit from ADR 0006 was exceeded.
///
/// `found` and `limit` are the numbers in the sentence. For component
/// nesting, `limit` is the flag. For JSON container depth, `limit` is
/// [`crate::Limits::SERDE_JSON_MAX_DEPTH`] and the sentence does not name
/// `--max-depth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{}", limit_message(*kind, *found, *limit))]
pub struct LimitExceeded {
    /// Which counter fired.
    pub kind: LimitKind,
    /// The value that crossed the limit.
    pub found: u64,
    /// The limit that was in force for that counter.
    pub limit: u64,
}

fn limit_message(kind: LimitKind, found: u64, limit: u64) -> String {
    match kind {
        LimitKind::FileBytes => {
            format!("file size {found} exceeds limit {limit} (--max-file-bytes)")
        }
        LimitKind::ComponentCount => {
            format!("component count {found} exceeds limit {limit} (--max-components)")
        }
        LimitKind::NestingDepth => {
            format!("nesting depth {found} exceeds limit {limit} (--max-depth)")
        }
        LimitKind::JsonNesting => format!("json nesting {found} exceeds limit {limit}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, LimitExceeded, LimitKind, UnsupportedDepth};

    #[test]
    fn not_implemented_display_is_stable() {
        let err = Error::NotImplemented;
        assert_eq!(err.to_string(), "not implemented yet");
    }

    #[test]
    fn limit_sentences_match_the_adr() {
        let file = LimitExceeded {
            kind: LimitKind::FileBytes,
            found: 33_554_433,
            limit: 33_554_432,
        };
        assert_eq!(
            file.to_string(),
            "file size 33554433 exceeds limit 33554432 (--max-file-bytes)"
        );
        let count = LimitExceeded {
            kind: LimitKind::ComponentCount,
            found: 3,
            limit: 2,
        };
        assert_eq!(
            count.to_string(),
            "component count 3 exceeds limit 2 (--max-components)"
        );
        let depth = LimitExceeded {
            kind: LimitKind::NestingDepth,
            found: 33,
            limit: 32,
        };
        assert_eq!(
            depth.to_string(),
            "nesting depth 33 exceeds limit 32 (--max-depth)"
        );
        let json = LimitExceeded {
            kind: LimitKind::JsonNesting,
            found: 128,
            limit: 127,
        };
        let json_text = json.to_string();
        assert_eq!(json_text, "json nesting 128 exceeds limit 127");
        assert!(!json_text.contains("--max-depth"));
    }

    #[test]
    fn an_unsupported_depth_names_the_flag_and_the_maximum() {
        let error = Error::from(UnsupportedDepth {
            requested: 64,
            maximum: 63,
        });
        assert_eq!(
            error.to_string(),
            "--max-depth 64 exceeds supported maximum 63"
        );
    }
}
