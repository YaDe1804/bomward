// SPDX-License-Identifier: AGPL-3.0-only

//! Read a review policy and, later, judge a diff against it.
//!
//! The file is configuration. Unknown fields, duplicate keys, and duplicate
//! rule ids are errors. Evaluation does not run until the file has loaded.
//!
//! ```
//! use bomward_core::diff::diff;
//! use bomward_core::policy::{self, PolicyLimits};
//! use bomward_core::Document;
//!
//! let text = "version: 1\nmode: enforce\ndefault: allow\nrules: []\n";
//! let policy = policy::read_str(text, &PolicyLimits::default())?;
//! let document = Document {
//!     components: Default::default(),
//!     dependencies: Default::default(),
//!     warnings: Vec::new(),
//! };
//! let verdict = policy::evaluate(&policy, &diff(&document, &document));
//! assert_eq!(verdict.exit_code(), 0);
//! # Ok::<(), bomward_core::Error>(())
//! ```

mod error;
mod evaluate;
mod license;
mod limits;
mod load;
mod pattern;
mod schema;

use std::path::Path;

pub use error::PolicyError;
pub use evaluate::{DecisionCounts, Outcome, Subject, Unit, UnitCounts, Verdict, evaluate};
pub use limits::PolicyLimits;
pub use load::Policy;
pub use schema::{Decision, DocumentCondition, Mode};

use crate::error::Error;

/// Read a policy from UTF-8 text.
///
/// # Errors
///
/// [`Error::InvalidPolicy`] when the text is not a valid version-1 policy
/// or it exceeds `limits`. A leading UTF-8 BOM is ignored.
pub fn read_str(text: &str, limits: &PolicyLimits) -> Result<Policy, Error> {
    read_bytes(text.as_bytes(), limits)
}

/// Read a policy from bytes.
///
/// # Errors
///
/// [`Error::InvalidPolicy`] when the bytes are not valid UTF-8, are larger
/// than `limits.max_file_bytes`, or are not a valid version-1 policy.
/// The size is checked before the bytes are parsed. A leading UTF-8 BOM
/// is ignored.
pub fn read_bytes(bytes: &[u8], limits: &PolicyLimits) -> Result<Policy, Error> {
    let found = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if found > limits.max_file_bytes {
        return Err(PolicyError::FileTooLarge {
            found,
            limit: limits.max_file_bytes,
        }
        .into());
    }
    load::parse(decode(bytes)?, limits).map_err(Error::from)
}

/// Read a policy from a path the caller passed.
///
/// # Errors
///
/// [`Error::Io`] when the path cannot be read. [`Error::InvalidPolicy`]
/// for the same reasons as [`read_bytes`]. A file whose metadata already
/// exceeds the byte limit is not read.
pub fn read_path(path: &Path, limits: &PolicyLimits) -> Result<Policy, Error> {
    let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > limits.max_file_bytes {
        return Err(PolicyError::FileTooLarge {
            found: metadata.len(),
            limit: limits.max_file_bytes,
        }
        .into());
    }
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    read_bytes(&bytes, limits)
}

fn decode(bytes: &[u8]) -> Result<&str, PolicyError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    std::str::from_utf8(bytes).map_err(|_| PolicyError::Utf8)
}
