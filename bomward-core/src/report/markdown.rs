// SPDX-License-Identifier: AGPL-3.0-only

//! GitHub-flavored Markdown. Untrusted text is written only in code spans.

use super::assemble::{Entry, Line, Prepared};
use super::escape::push_value;
use super::text::{Phrase, counted};

/// Upper bound on the Markdown comment, in Unicode scalars.
pub const MARKDOWN_CHAR_BUDGET: usize = 60_000;

pub(crate) fn render(prepared: &Prepared) -> String {
    let header = render_header(&prepared.lines);
    let notes = render_notes(&prepared.notes);
    let mut changes = Vec::new();
    let mut denied = Vec::new();
    let mut warnings = Vec::new();
    let mut allowed = Vec::new();
    for group in &prepared.groups {
        let rendered: Vec<String> = group.entries.iter().map(render_entry).collect();
        match group.title {
            "Denied" => denied = rendered,
            "Warnings" => warnings = rendered,
            "Allowed" => allowed = rendered,
            _ => changes = rendered,
        }
    }
    fit(&header, &changes, &denied, &warnings, &allowed, &notes)
}

fn render_header(lines: &[Line]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        let Line::Text(text) = line;
        if index == 0 {
            out.push_str("# ");
            out.push_str(text);
        } else if let Some((label, rest)) = text.split_once(": ") {
            out.push_str("**");
            out.push_str(label);
            out.push_str(":** ");
            out.push_str(rest);
        } else {
            out.push_str(text);
        }
        out.push_str("\n\n");
    }
    out
}

fn render_entry(entry: &Entry) -> String {
    let mut out = render_phrases(&entry.heading);
    out.push_str("\n\n");
    out.push_str(&render_phrases(&entry.phrases));
    out.push('\n');
    if !entry.rules.is_empty() {
        out.push('\n');
        out.push_str(&render_phrases(&entry.rules));
        out.push('\n');
    }
    out.push('\n');
    out
}

fn render_notes(notes: &[Vec<Phrase>]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut body = String::new();
    for note in notes {
        body.push_str(&render_phrases(note));
        body.push_str("\n\n");
    }
    format!("<details>\n<summary>Notes</summary>\n\n{body}</details>\n\n")
}

fn render_phrases(phrases: &[Phrase]) -> String {
    let mut out = String::new();
    for phrase in phrases {
        match phrase {
            Phrase::Text(text) => out.push_str(text),
            Phrase::Value(value) => push_value(&mut out, value, true),
        }
    }
    out
}

struct Parts<'a> {
    header: &'a str,
    changes: &'a [String],
    denied: &'a [String],
    warnings: &'a [String],
    allowed: &'a [String],
    notes: &'a str,
}

#[derive(Clone, Copy)]
struct Keep {
    changes: usize,
    denied: usize,
    warnings: usize,
    allowed: usize,
    notes: bool,
}

struct Cost {
    header: usize,
    changes: usize,
    denied: usize,
    warnings: usize,
    allowed: usize,
    notes: usize,
    omitted_allowed: usize,
    omitted_warnings: usize,
    omitted_denied: usize,
}

fn fit(
    header: &str,
    changes: &[String],
    denied: &[String],
    warnings: &[String],
    allowed: &[String],
    notes: &str,
) -> String {
    let parts = Parts {
        header,
        changes,
        denied,
        warnings,
        allowed,
        notes,
    };
    let header_len = chars(header);
    let notes_len = chars(notes);
    let changes_full = section_len(changes, "## Changes\n\n");
    let denied_full = section_len(denied, "## Denied\n\n");
    let warnings_full = section_len(warnings, "## Warnings\n\n");
    let allowed_full = folded_len(allowed, allowed.len());
    let total = header_len + notes_len + changes_full + denied_full + warnings_full + allowed_full;
    if total <= MARKDOWN_CHAR_BUDGET {
        return compose(
            &parts,
            Keep {
                changes: changes.len(),
                denied: denied.len(),
                warnings: warnings.len(),
                allowed: allowed.len(),
                notes: true,
            },
        );
    }
    shrink_allowed(
        &parts,
        header_len,
        notes_len,
        changes_full,
        denied_full,
        warnings_full,
    )
}

fn shrink_allowed(
    parts: &Parts<'_>,
    header_len: usize,
    notes_len: usize,
    changes_full: usize,
    denied_full: usize,
    warnings_full: usize,
) -> String {
    let fixed = header_len + notes_len + changes_full + denied_full + warnings_full;
    let allowed_keep = prefix_keep(
        parts.allowed,
        fixed,
        true,
        "",
        "allowed entry",
        "allowed entries",
    );
    let allowed_cost = Cost {
        header: header_len,
        changes: changes_full,
        denied: denied_full,
        warnings: warnings_full,
        allowed: folded_len(parts.allowed, allowed_keep),
        notes: notes_len,
        omitted_allowed: parts.allowed.len() - allowed_keep,
        omitted_warnings: 0,
        omitted_denied: 0,
    };
    if allowed_keep < parts.allowed.len() && cost_len(&allowed_cost) <= MARKDOWN_CHAR_BUDGET {
        return compose(
            parts,
            Keep {
                changes: parts.changes.len(),
                denied: parts.denied.len(),
                warnings: parts.warnings.len(),
                allowed: allowed_keep,
                notes: true,
            },
        );
    }
    shrink_warnings(parts, header_len, notes_len, changes_full, denied_full)
}

fn shrink_warnings(
    parts: &Parts<'_>,
    header_len: usize,
    notes_len: usize,
    changes_full: usize,
    denied_full: usize,
) -> String {
    let warning_keep = prefix_keep(
        parts.warnings,
        header_len + notes_len + changes_full + denied_full,
        false,
        "## Warnings\n\n",
        "warning",
        "warnings",
    );
    if warning_keep < parts.warnings.len()
        && cost_len(&Cost {
            header: header_len,
            changes: changes_full,
            denied: denied_full,
            warnings: section_len(take(parts.warnings, warning_keep), "## Warnings\n\n"),
            allowed: 0,
            notes: notes_len,
            omitted_allowed: parts.allowed.len(),
            omitted_warnings: parts.warnings.len() - warning_keep,
            omitted_denied: 0,
        }) <= MARKDOWN_CHAR_BUDGET
    {
        return compose(
            parts,
            Keep {
                changes: parts.changes.len(),
                denied: parts.denied.len(),
                warnings: warning_keep,
                allowed: 0,
                notes: true,
            },
        );
    }
    let denied_keep = prefix_keep(
        parts.denied,
        header_len + changes_full,
        false,
        "## Denied\n\n",
        "denied entry",
        "denied entries",
    );
    compose(
        parts,
        Keep {
            changes: parts.changes.len(),
            denied: denied_keep,
            warnings: 0,
            allowed: 0,
            notes: false,
        },
    )
}

fn compose(parts: &Parts<'_>, keep: Keep) -> String {
    let mut out = String::new();
    out.push_str(parts.header);
    out.push_str(&visible_section(parts.changes, keep.changes, "Changes"));
    out.push_str(&visible_section(parts.denied, keep.denied, "Denied"));
    out.push_str(&omission(
        parts.denied.len().saturating_sub(keep.denied),
        parts.denied.len(),
        "denied entry",
        "denied entries",
    ));
    out.push_str(&visible_section(parts.warnings, keep.warnings, "Warnings"));
    out.push_str(&omission(
        parts.warnings.len().saturating_sub(keep.warnings),
        parts.warnings.len(),
        "warning",
        "warnings",
    ));
    out.push_str(&folded_section(parts.allowed, keep.allowed));
    out.push_str(&omission(
        parts.allowed.len().saturating_sub(keep.allowed),
        parts.allowed.len(),
        "allowed entry",
        "allowed entries",
    ));
    if keep.notes {
        out.push_str(parts.notes);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn cost_len(cost: &Cost) -> usize {
    cost.header
        + cost.changes
        + cost.denied
        + cost.warnings
        + cost.allowed
        + cost.notes
        + chars(&omission(
            cost.omitted_allowed,
            cost.omitted_allowed,
            "allowed entry",
            "allowed entries",
        ))
        + chars(&omission(
            cost.omitted_warnings,
            cost.omitted_warnings,
            "warning",
            "warnings",
        ))
        + chars(&omission(
            cost.omitted_denied,
            cost.omitted_denied,
            "denied entry",
            "denied entries",
        ))
}

fn prefix_keep(
    entries: &[String],
    fixed: usize,
    folded: bool,
    heading: &str,
    one: &str,
    many: &str,
) -> usize {
    let total = entries.len();
    let mut low = 0;
    let mut high = total;
    let mut best = 0;
    while low <= high {
        let mid = low + (high - low) / 2;
        let body = if folded {
            folded_len(entries, mid)
        } else if mid == 0 {
            0
        } else {
            section_len(take(entries, mid), heading)
        };
        let cost = fixed + body + chars(&omission(total - mid, total, one, many));
        if cost <= MARKDOWN_CHAR_BUDGET {
            best = mid;
            if mid == total {
                break;
            }
            low = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    best
}

fn section_len(entries: &[String], heading: &str) -> usize {
    if entries.is_empty() {
        return 0;
    }
    chars(heading) + entries.iter().map(|entry| chars(entry)).sum::<usize>()
}

fn folded_len(entries: &[String], keep: usize) -> usize {
    if keep == 0 {
        return 0;
    }
    let body: usize = entries.iter().take(keep).map(|entry| chars(entry)).sum();
    let summary = format!("Allowed ({keep})");
    chars("<details>\n<summary>")
        + chars(&summary)
        + chars("</summary>\n\n")
        + body
        + chars("</details>\n\n")
}

fn visible_section(entries: &[String], keep: usize, title: &str) -> String {
    if keep == 0 {
        return String::new();
    }
    let mut out = format!("## {title}\n\n");
    for entry in entries.iter().take(keep) {
        out.push_str(entry);
    }
    out
}

fn folded_section(entries: &[String], keep: usize) -> String {
    if keep == 0 {
        return String::new();
    }
    let mut body = String::new();
    for entry in entries.iter().take(keep) {
        body.push_str(entry);
    }
    format!("<details>\n<summary>Allowed ({keep})</summary>\n\n{body}</details>\n\n")
}

fn omission(omitted: usize, total: usize, one: &str, many: &str) -> String {
    if omitted == 0 {
        return String::new();
    }
    let omitted = u64::try_from(omitted).unwrap_or(u64::MAX);
    format!(
        "{} omitted of {total}. The JSON report is complete.\n\n",
        counted(omitted, one, many)
    )
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

fn take(entries: &[String], count: usize) -> &[String] {
    entries.get(..count).unwrap_or(entries)
}
