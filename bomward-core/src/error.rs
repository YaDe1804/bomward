// SPDX-License-Identifier: AGPL-3.0-only

//! Typed errors for the bomward library.
//!
//! The foundation enum is a seam, not a stable error set. Variants will change
//! before any release. The crate is not published.

use thiserror::Error;

/// Failure returned by the bomward library.
///
/// Library functions return this type instead of panicking.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No library operation is implemented yet.
    ///
    /// The foundation CLI maps this variant to exit code 2. Later work
    /// packages replace it with errors for rejected input and policy.
    #[error("not implemented yet")]
    NotImplemented,
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn not_implemented_display_is_stable() {
        let err = Error::NotImplemented;
        assert_eq!(err.to_string(), "not implemented yet");
    }
}
