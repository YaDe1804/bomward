// SPDX-License-Identifier: AGPL-3.0-only

//! Render a verdict as terminal text, Markdown, or JSON.
//!
//! Rendering is deterministic: sorted entries, `LF` line endings, no
//! timestamps, and no local paths. Untrusted strings are escaped here and
//! nowhere else. Rendering is not implemented in this milestone.
