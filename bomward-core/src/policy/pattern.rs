// SPDX-License-Identifier: AGPL-3.0-only

//! Package patterns from ADR 0013.
//!
//! `*` is any run of characters other than `/`. `**` is any run, including
//! `/`. Every other character is literal. The match is a walk with a visited
//! table, so a long pattern cannot backtrack exponentially.

/// A pattern compiled once at load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Pattern {
    /// True only for the exact pattern `**`.
    ///
    /// A name key matches this and nothing else.
    pub(crate) universal: bool,
    atoms: Vec<Atom>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Atom {
    Text(String),
    /// `*`: any run that does not contain `/`, including empty.
    Star,
    /// `**`: any run, including `/` and empty.
    Glob,
}

/// Why a pattern was rejected. The text is a fixed sentence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PatternError {
    /// The pattern is not `**` and does not start with `pkg:`.
    Prefix,
}

impl Pattern {
    pub(crate) fn compile(pattern: &str) -> Result<Self, PatternError> {
        if pattern != "**" && !pattern.starts_with("pkg:") {
            return Err(PatternError::Prefix);
        }
        let mut atoms = Vec::new();
        let mut literal = String::new();
        let mut chars = pattern.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '*' {
                push_literal(&mut atoms, &mut literal);
                if chars.peek() == Some(&'*') {
                    chars.next();
                    atoms.push(Atom::Glob);
                } else {
                    atoms.push(Atom::Star);
                }
            } else {
                literal.push(ch);
            }
        }
        push_literal(&mut atoms, &mut literal);
        Ok(Self {
            universal: pattern == "**",
            atoms,
        })
    }

    pub(crate) fn matches(&self, text: &str) -> bool {
        let n = self.atoms.len();
        let m = text.len();
        let Some(width) = m.checked_add(1) else {
            return false;
        };
        let Some(cells) = n.checked_add(1).and_then(|rows| rows.checked_mul(width)) else {
            return false;
        };
        let mut dp = vec![false; cells];
        set(&mut dp, width, n, m);
        let bytes = text.as_bytes();
        for i in (0..n).rev() {
            for j in (0..=m).rev() {
                let matched = match self.atoms.get(i) {
                    Some(Atom::Text(literal)) => {
                        text_at(bytes, j, literal).is_some_and(|next| get(&dp, width, i + 1, next))
                    }
                    Some(Atom::Star) => star_matches(&dp, width, bytes, i, j),
                    Some(Atom::Glob) => glob_matches(&dp, width, m, i, j),
                    None => false,
                };
                if matched {
                    set(&mut dp, width, i, j);
                }
            }
        }
        get(&dp, width, 0, 0)
    }
}

fn push_literal(atoms: &mut Vec<Atom>, literal: &mut String) {
    if !literal.is_empty() {
        atoms.push(Atom::Text(std::mem::take(literal)));
    }
}

fn text_at(bytes: &[u8], start: usize, literal: &str) -> Option<usize> {
    let literal_bytes = literal.as_bytes();
    let end = start.checked_add(literal_bytes.len())?;
    if bytes.get(start..end) == Some(literal_bytes) {
        Some(end)
    } else {
        None
    }
}

fn star_matches(dp: &[bool], width: usize, bytes: &[u8], atom: usize, start: usize) -> bool {
    let mut k = start;
    loop {
        if get(dp, width, atom + 1, k) {
            return true;
        }
        match bytes.get(k) {
            Some(b'/') | None => return false,
            Some(_) => k += 1,
        }
    }
}

fn glob_matches(dp: &[bool], width: usize, len: usize, atom: usize, start: usize) -> bool {
    let mut k = start;
    loop {
        if get(dp, width, atom + 1, k) {
            return true;
        }
        if k >= len {
            return false;
        }
        k += 1;
    }
}

fn index(width: usize, row: usize, column: usize) -> Option<usize> {
    row.checked_mul(width)?.checked_add(column)
}

fn set(dp: &mut [bool], width: usize, row: usize, column: usize) {
    if let Some(slot) = index(width, row, column).and_then(|at| dp.get_mut(at)) {
        *slot = true;
    }
}

fn get(dp: &[bool], width: usize, row: usize, column: usize) -> bool {
    index(width, row, column)
        .and_then(|at| dp.get(at).copied())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{Pattern, PatternError};

    #[test]
    fn star_does_not_cross_a_slash() -> Result<(), PatternError> {
        let pattern = Pattern::compile("pkg:npm/*")?;
        assert!(pattern.matches("pkg:npm/left-pad"));
        assert!(!pattern.matches("pkg:npm/@acme/tool"));
        assert!(!pattern.matches("pkg:npm/a/b"));
        Ok(())
    }

    #[test]
    fn glob_crosses_slashes() -> Result<(), PatternError> {
        let pattern = Pattern::compile("pkg:npm/**")?;
        assert!(pattern.matches("pkg:npm/left-pad"));
        assert!(pattern.matches("pkg:npm/%40acme/tool"));
        Ok(())
    }

    #[test]
    fn only_the_exact_double_star_is_universal() -> Result<(), PatternError> {
        let universal = Pattern::compile("**")?;
        assert!(universal.universal);
        assert!(universal.matches("pkg:npm/a/b"));
        assert_eq!(Pattern::compile("***"), Err(PatternError::Prefix));
        Ok(())
    }

    #[test]
    fn a_pattern_must_name_a_purl_or_everything() -> Result<(), PatternError> {
        assert_eq!(Pattern::compile("npm/*"), Err(PatternError::Prefix));
        Pattern::compile("pkg:npm/%40acme/*")?;
        Ok(())
    }
}
