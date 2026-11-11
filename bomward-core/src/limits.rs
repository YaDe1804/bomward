// SPDX-License-Identifier: AGPL-3.0-only

//! Default and per-run limits from ADR 0006.
//!
//! The numbers are the v0.1 defaults. A caller, later the CLI, may raise one
//! of them for a single run. Changing a default is an amendment of that ADR.
//! `--max-depth` limits components inside components. JSON container depth
//! is the fixed parser cap and does not follow the flag.

use crate::error::UnsupportedDepth;

/// How large a document may be, and how deep its components may nest.
///
/// JSON container depth uses [`Limits::json_depth_limit`], which does not
/// move with [`Limits::max_depth`]. ADR 0008 defines that cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum size of one document, in bytes. Default: 33,554,432.
    pub max_file_bytes: u64,
    /// Maximum number of component objects, including nested ones.
    /// Default: 100,000.
    pub max_components: u64,
    /// Maximum nesting of `components` inside `components`. Default: 32.
    ///
    /// This does not limit the length of a dependency chain, and it does
    /// not limit JSON container depth. Values above
    /// [`Self::MAX_SUPPORTED_DEPTH`] are rejected.
    pub max_depth: u32,
}

impl Limits {
    /// Default maximum size of one input, in bytes (32 MiB).
    pub const DEFAULT_MAX_FILE_BYTES: u64 = 33_554_432;

    /// Default maximum number of component objects.
    pub const DEFAULT_MAX_COMPONENTS: u64 = 100_000;

    /// Default maximum component-nesting depth.
    pub const DEFAULT_MAX_DEPTH: u32 = 32;

    /// Deepest container tree `serde_json` 1.0.154 will build.
    ///
    /// Its recursion budget starts at 128 and the 128th container is rejected,
    /// so 127 succeeds. ADR 0008 caps the JSON measurement here.
    pub const SERDE_JSON_MAX_DEPTH: u32 = 127;

    /// Largest `--max-depth` whose component tree still fits in
    /// [`Self::SERDE_JSON_MAX_DEPTH`].
    ///
    /// A component at depth `D` sits at JSON container depth `2 * D + 1`:
    /// the root object, then one array and one object per level.
    /// `2 * 63 + 1 = 127`.
    pub const MAX_SUPPORTED_DEPTH: u32 = (Self::SERDE_JSON_MAX_DEPTH - 1) / 2;

    /// The defaults from ADR 0006.
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
            max_components: Self::DEFAULT_MAX_COMPONENTS,
            max_depth: Self::DEFAULT_MAX_DEPTH,
        }
    }

    /// Limits for one run.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedDepth`] when `max_depth` is above
    /// [`Self::MAX_SUPPORTED_DEPTH`].
    pub const fn new(
        max_file_bytes: u64,
        max_components: u64,
        max_depth: u32,
    ) -> Result<Self, UnsupportedDepth> {
        let limits = Self {
            max_file_bytes,
            max_components,
            max_depth,
        };
        match limits.check_depth() {
            Ok(()) => Ok(limits),
            Err(error) => Err(error),
        }
    }

    /// Rejects a depth the JSON parser cannot hold.
    ///
    /// A struct literal can bypass [`Self::new`]. Every read calls this
    /// so that path still fails before the bytes are parsed.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedDepth`] when `max_depth` is above
    /// [`Self::MAX_SUPPORTED_DEPTH`].
    pub const fn check_depth(self) -> Result<(), UnsupportedDepth> {
        if self.max_depth > Self::MAX_SUPPORTED_DEPTH {
            Err(UnsupportedDepth {
                requested: self.max_depth,
                maximum: Self::MAX_SUPPORTED_DEPTH,
            })
        } else {
            Ok(())
        }
    }

    /// JSON container depth the depth scan rejects.
    ///
    /// This is the parser cap. It does not move with `--max-depth`.
    #[must_use]
    pub const fn json_depth_limit() -> u32 {
        Self::SERDE_JSON_MAX_DEPTH
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::defaults()
    }
}

const _: () = assert!(Limits::DEFAULT_MAX_DEPTH <= Limits::MAX_SUPPORTED_DEPTH);
const _: () = assert!(2 * Limits::MAX_SUPPORTED_DEPTH + 1 == Limits::SERDE_JSON_MAX_DEPTH);

#[cfg(test)]
mod tests {
    use super::Limits;

    #[test]
    fn the_supported_depth_is_the_largest_chain_the_json_cap_holds() -> Result<(), String> {
        let depth = Limits::MAX_SUPPORTED_DEPTH;
        if 2 * depth + 1 != Limits::SERDE_JSON_MAX_DEPTH {
            return Err(format!("depth {depth} does not fill the json cap"));
        }
        if Limits::new(1, 1, depth).is_err() {
            return Err("the supported depth was rejected".to_owned());
        }
        match Limits::new(1, 1, depth + 1) {
            Ok(_) => Err("one past the supported depth was accepted".to_owned()),
            Err(error) => {
                let text = error.to_string();
                if text == "--max-depth 64 exceeds supported maximum 63" {
                    Ok(())
                } else {
                    Err(text)
                }
            }
        }
    }
}
