// SPDX-License-Identifier: AGPL-3.0-only

//! Default and per-run limits from ADR 0006.
//!
//! The numbers are the v0.1 defaults. A caller, later the CLI, may raise one
//! of them for a single run. Changing a default is an amendment of that ADR.

/// How large a document may be, and how deep it may nest.
///
/// JSON container depth uses [`Limits::json_depth_limit`], not
/// [`Limits::max_depth`] by itself. ADR 0008 defines that ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum size of one document, in bytes. Default: 33,554,432.
    pub max_file_bytes: u64,
    /// Maximum number of component objects, including nested ones.
    /// Default: 100,000.
    pub max_components: u64,
    /// Maximum nesting of `components` inside `components`. Default: 32.
    ///
    /// This does not limit the length of a dependency chain.
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

    /// The defaults from ADR 0006.
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
            max_components: Self::DEFAULT_MAX_COMPONENTS,
            max_depth: Self::DEFAULT_MAX_DEPTH,
        }
    }

    /// Limits for one run. There is no further clamping.
    #[must_use]
    pub const fn new(max_file_bytes: u64, max_components: u64, max_depth: u32) -> Self {
        Self {
            max_file_bytes,
            max_components,
            max_depth,
        }
    }

    /// Maximum JSON array and object nesting for this flag.
    ///
    /// The formula is `min(4 * max_depth + 8, 127)`. See ADR 0008.
    #[must_use]
    pub const fn json_depth_limit(self) -> u32 {
        let scaled = self.max_depth.saturating_mul(4).saturating_add(8);
        if scaled < Self::SERDE_JSON_MAX_DEPTH {
            scaled
        } else {
            Self::SERDE_JSON_MAX_DEPTH
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::defaults()
    }
}
