// SPDX-License-Identifier: AGPL-3.0-only

//! Size limits for a policy file.
//!
//! The numbers are the v0.1 defaults. A caller can lower them. Raising them
//! is a deliberate choice of that caller; the parser still refuses aliases.

/// How large a policy file may be.
///
/// A value past the limit is [`crate::PolicyError`], not a truncated file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyLimits {
    /// Maximum file size in bytes, before parsing.
    pub max_file_bytes: u64,
    /// Maximum number of rules.
    pub max_rules: usize,
    /// Maximum entries in one list (`change`, `license`, `package`, …).
    pub max_list_entries: usize,
    /// Maximum characters in one `description`.
    pub max_description_chars: usize,
    /// Maximum characters in one package pattern.
    pub max_pattern_chars: usize,
}

impl PolicyLimits {
    /// 1 MiB.
    pub const DEFAULT_MAX_FILE_BYTES: u64 = 1024 * 1024;
    /// 1000 rules.
    pub const DEFAULT_MAX_RULES: usize = 1000;
    /// 256 entries in one list.
    pub const DEFAULT_MAX_LIST_ENTRIES: usize = 256;
    /// 500 characters.
    pub const DEFAULT_MAX_DESCRIPTION_CHARS: usize = 500;
    /// 256 characters.
    pub const DEFAULT_MAX_PATTERN_CHARS: usize = 256;
}

impl Default for PolicyLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: Self::DEFAULT_MAX_FILE_BYTES,
            max_rules: Self::DEFAULT_MAX_RULES,
            max_list_entries: Self::DEFAULT_MAX_LIST_ENTRIES,
            max_description_chars: Self::DEFAULT_MAX_DESCRIPTION_CHARS,
            max_pattern_chars: Self::DEFAULT_MAX_PATTERN_CHARS,
        }
    }
}
