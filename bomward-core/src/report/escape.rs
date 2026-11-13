// SPDX-License-Identifier: AGPL-3.0-only

//! Encode characters that a terminal or a Markdown renderer would act on.

use std::fmt::Write as _;

/// Word the terminal and Markdown use for a value with no characters.
///
/// An empty code span is two backticks. `CommonMark` reads that as an opening
/// fence, so the next span can close the wrong one.
pub(crate) const EMPTY_VALUE: &str = "(empty)";

/// Longest untrusted value written into terminal or Markdown text.
///
/// JSON keeps the original string. The count is Unicode scalars.
pub const DISPLAY_VALUE_CHARS: usize = 256;

/// A value prepared for terminal text or a Markdown code span.
pub(crate) struct ShownValue {
    /// Escaped text, cut at [`DISPLAY_VALUE_CHARS`] when the source is longer.
    pub(crate) text: String,
    /// Original number of scalars, set only when `text` was cut.
    pub(crate) total: Option<usize>,
}

/// `true` when `ch` can change terminal state or leave a line.
pub(crate) fn affects_terminal(ch: char) -> bool {
    let code = u32::from(ch);
    code <= 0x1F
        || (0x7F..=0x9F).contains(&code)
        || matches!(
            code,
            0x061C | 0x200E | 0x200F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
        )
}

/// Encode terminal-affecting characters as `\u{XXXX}`.
pub(crate) fn encode_controls(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if affects_terminal(ch) {
            let _ = write!(out, "\\u{{{:04X}}}", u32::from(ch));
        } else {
            out.push(ch);
        }
    }
    out
}

/// Escape `raw`, and cut it when it is longer than [`DISPLAY_VALUE_CHARS`].
pub(crate) fn show_value(raw: &str) -> ShownValue {
    let total = raw.chars().count();
    if total <= DISPLAY_VALUE_CHARS {
        return ShownValue {
            text: encode_controls(raw),
            total: None,
        };
    }
    let prefix: String = raw.chars().take(DISPLAY_VALUE_CHARS).collect();
    let mut text = encode_controls(&prefix);
    text.push('…');
    ShownValue {
        text,
        total: Some(total),
    }
}

/// Write one value. An empty value is [`EMPTY_VALUE`], not a code span.
pub(crate) fn push_value(out: &mut String, value: &str, span: bool) {
    if value.is_empty() {
        out.push_str(EMPTY_VALUE);
        return;
    }
    let shown = show_value(value);
    if span {
        out.push_str(&code_span(&shown.text));
    } else {
        out.push_str(&shown.text);
    }
    if let Some(total) = shown.total {
        let _ = write!(out, " ({total} characters)");
    }
}

/// Wrap `escaped` in a `CommonMark` code span.
///
/// The fence is longer than any backtick run in `escaped`. A value that
/// starts or ends with a backtick or a space is padded with one space.
/// `CommonMark` drops one leading and one trailing space when a span is not
/// entirely spaces, and a backtick run must not touch the fence.
pub(crate) fn code_span(escaped: &str) -> String {
    let fence = "`".repeat(longest_backtick_run(escaped) + 1);
    let pad = if needs_space_pad(escaped) { " " } else { "" };
    format!("{fence}{pad}{escaped}{pad}{fence}")
}

fn needs_space_pad(text: &str) -> bool {
    if text.is_empty() || text.chars().all(|ch| ch == ' ') {
        return false;
    }
    text.starts_with('`') || text.ends_with('`') || text.starts_with(' ') || text.ends_with(' ')
}

fn longest_backtick_run(text: &str) -> usize {
    let mut best = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::{DISPLAY_VALUE_CHARS, affects_terminal, code_span, encode_controls, show_value};

    #[test]
    fn controls_and_bidi_are_encoded() {
        let encoded = encode_controls("a\nb\u{001B}c\u{007F}d\u{0085}e\u{202E}f\u{2066}");
        assert_eq!(
            encoded,
            "a\\u{000A}b\\u{001B}c\\u{007F}d\\u{0085}e\\u{202E}f\\u{2066}"
        );
        assert!(affects_terminal('\u{202E}'));
        assert!(!affects_terminal('A'));
    }

    #[test]
    fn a_long_value_keeps_its_total() {
        let raw = "x".repeat(DISPLAY_VALUE_CHARS + 4);
        let shown = show_value(&raw);
        assert_eq!(shown.total, Some(DISPLAY_VALUE_CHARS + 4));
        assert!(shown.text.ends_with('…'));
        assert_eq!(
            shown.text.chars().filter(|ch| *ch == 'x').count(),
            DISPLAY_VALUE_CHARS
        );
    }

    #[test]
    fn the_fence_outgrows_backticks() {
        let span = code_span("``a`");
        assert!(span.starts_with("```"));
        assert!(span.contains("``a`"));
    }

    #[test]
    fn a_value_with_end_spaces_is_padded() {
        assert_eq!(code_span(" / "), "`  /  `");
        assert_eq!(code_span(" "), "` `");
        assert_eq!(code_span("`"), "`` ` ``");
    }
}
