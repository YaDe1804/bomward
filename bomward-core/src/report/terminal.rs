// SPDX-License-Identifier: AGPL-3.0-only

//! Plain text. No ANSI color, no wrapping, LF line endings.

use super::assemble::{Entry, Group, Line, Prepared};
use super::escape::push_value;
use super::text::Phrase;

pub(crate) fn render(prepared: &Prepared) -> String {
    let mut out = String::new();
    for line in &prepared.lines {
        push_line(&mut out, &render_line(line));
    }
    if !prepared.groups.is_empty() {
        push_line(&mut out, "");
    }
    for (index, group) in prepared.groups.iter().enumerate() {
        if index > 0 {
            push_line(&mut out, "");
        }
        push_group(&mut out, group);
    }
    if !prepared.notes.is_empty() {
        push_line(&mut out, "");
        push_line(&mut out, "Notes");
        for note in &prepared.notes {
            push_line(&mut out, &render_phrases(note));
        }
    }
    out
}

fn push_group(out: &mut String, group: &Group) {
    push_line(out, group.title);
    for entry in &group.entries {
        push_line(out, "");
        push_entry(out, entry);
    }
}

fn push_entry(out: &mut String, entry: &Entry) {
    push_line(out, &render_phrases(&entry.heading));
    push_line(out, &render_phrases(&entry.phrases));
    if !entry.rules.is_empty() {
        push_line(out, &render_phrases(&entry.rules));
    }
}

fn render_line(line: &Line) -> String {
    match line {
        Line::Text(text) => text.clone(),
    }
}

fn render_phrases(phrases: &[Phrase]) -> String {
    let mut out = String::new();
    for phrase in phrases {
        match phrase {
            Phrase::Text(text) => out.push_str(text),
            Phrase::Value(value) => push_value(&mut out, value, false),
        }
    }
    out
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}
