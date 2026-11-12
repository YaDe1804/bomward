// SPDX-License-Identifier: AGPL-3.0-only

//! Package patterns from ADR 0013.
//!
//! `*` is any run of characters other than `/`, `?`, or `#`. `**` is any
//! run other than `?` or `#`, including `/`. The exact pattern `**` matches
//! every key, including one with qualifiers or a subpath. Every other
//! character is literal. The match is a walk with a visited table, so a
//! long pattern cannot backtrack exponentially. Qualifiers and the
//! subpath are literal and must already be in the canonical form the key
//! stores. A run of three or more `*` is rejected.

use std::str::FromStr;

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
    /// `*`: any run that does not contain `/`, `?`, or `#`, including empty.
    Star,
    /// `**`: any run that does not contain `?` or `#`, including `/` and empty.
    Glob,
}

/// Why a pattern was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PatternError {
    /// The pattern is not `**` and does not start with `pkg:`.
    Prefix,
    /// A namespace or name character that the canonical key percent-encodes.
    Unencoded {
        /// The character the author wrote.
        found: char,
        /// The encoding the key stores, such as `%40`.
        write: &'static str,
    },
    /// A control character in the namespace or name.
    Control,
    /// Three or more `*` in a row.
    Stars,
    /// Qualifiers are missing or not in the key's canonical form.
    Qualifiers {
        /// The qualifier text the key would store.
        expected: String,
    },
    /// A `#` with nothing after it.
    EmptySubpath,
}

impl Pattern {
    pub(crate) fn compile(pattern: &str) -> Result<Self, PatternError> {
        check_star_run(pattern)?;
        if pattern != "**" && !pattern.starts_with("pkg:") {
            return Err(PatternError::Prefix);
        }
        check_purl_encoding(pattern)?;
        check_qualifiers(pattern)?;
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
        if self.universal {
            return true;
        }
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
                    Some(Atom::Glob) => glob_matches(&dp, width, bytes, i, j),
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
            Some(b'/' | b'?' | b'#') | None => return false,
            Some(_) => k += 1,
        }
    }
}

fn glob_matches(dp: &[bool], width: usize, bytes: &[u8], atom: usize, start: usize) -> bool {
    let mut k = start;
    loop {
        if get(dp, width, atom + 1, k) {
            return true;
        }
        match bytes.get(k) {
            Some(b'?' | b'#') | None => return false,
            Some(_) => k += 1,
        }
    }
}

fn check_star_run(pattern: &str) -> Result<(), PatternError> {
    let mut run = 0u32;
    for ch in pattern.chars() {
        if ch == '*' {
            run = run.saturating_add(1);
            if run >= 3 {
                return Err(PatternError::Stars);
            }
        } else {
            run = 0;
        }
    }
    Ok(())
}

/// Qualifier text in the form `PackageUrl` stores it, as in the policy docs.
const QUALIFIER_EXAMPLE: &str = "repository_url=https:%2F%2Fevil.example";

fn check_qualifiers(pattern: &str) -> Result<(), PatternError> {
    if pattern == "**" {
        return Ok(());
    }
    let Some((_, after_query)) = pattern.split_once('?') else {
        return check_bare_subpath(pattern);
    };
    let (query, subpath) = match after_query.split_once('#') {
        Some((query, subpath)) => (query, Some(subpath)),
        None => (after_query, None),
    };
    if query.is_empty() {
        return Err(PatternError::Qualifiers {
            expected: QUALIFIER_EXAMPLE.to_owned(),
        });
    }
    let synthetic = format!("pkg:generic/name?{query}");
    let rendered = packageurl::PackageUrl::from_str(&synthetic)
        .ok()
        .and_then(|purl| {
            purl.to_string()
                .split_once('?')
                .map(|(_, qualifiers)| qualifiers.to_owned())
        });
    match rendered {
        Some(expected) if expected == query => {}
        Some(expected) => return Err(PatternError::Qualifiers { expected }),
        None => {
            return Err(PatternError::Qualifiers {
                expected: QUALIFIER_EXAMPLE.to_owned(),
            });
        }
    }
    if let Some(subpath) = subpath {
        check_subpath_text(subpath)?;
    }
    Ok(())
}

fn check_bare_subpath(pattern: &str) -> Result<(), PatternError> {
    if let Some((_, subpath)) = pattern.split_once('#') {
        check_subpath_text(subpath)?;
    }
    Ok(())
}

fn check_subpath_text(subpath: &str) -> Result<(), PatternError> {
    if subpath.is_empty() {
        return Err(PatternError::EmptySubpath);
    }
    Ok(())
}

fn check_purl_encoding(pattern: &str) -> Result<(), PatternError> {
    if pattern == "**" {
        return Ok(());
    }
    let path = pattern.split(['?', '#']).next().unwrap_or(pattern);
    let mut chars = path.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hex = chars.next().is_some_and(|item| item.is_ascii_hexdigit())
                && chars.next().is_some_and(|item| item.is_ascii_hexdigit());
            if !hex {
                return Err(PatternError::Unencoded {
                    found: '%',
                    write: "%25",
                });
            }
            continue;
        }
        if ch == '/' || ch == '*' || ch == ':' {
            continue;
        }
        if ch.is_control() {
            return Err(PatternError::Control);
        }
        if let Some(write) = encoded_literal(ch) {
            return Err(PatternError::Unencoded { found: ch, write });
        }
    }
    Ok(())
}

/// Percent-encoding of a character in a canonical namespace or name.
///
/// `packageurl` 0.7.1 encodes these, plus `/` (a segment separator here),
/// `#` and `?` (the qualifier and subpath markers), and `%` (an escape).
fn encoded_literal(ch: char) -> Option<&'static str> {
    Some(match ch {
        ' ' => "%20",
        '"' => "%22",
        '<' => "%3C",
        '>' => "%3E",
        '`' => "%60",
        '{' => "%7B",
        '}' => "%7D",
        ';' => "%3B",
        '=' => "%3D",
        '+' => "%2B",
        '@' => "%40",
        '\\' => "%5C",
        '[' => "%5B",
        ']' => "%5D",
        '^' => "%5E",
        '|' => "%7C",
        _ => return None,
    })
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
        assert_eq!(Pattern::compile("***"), Err(PatternError::Stars));
        Ok(())
    }

    #[test]
    fn a_pattern_must_name_a_purl_or_everything() -> Result<(), PatternError> {
        assert_eq!(Pattern::compile("npm/*"), Err(PatternError::Prefix));
        Pattern::compile("pkg:npm/%40acme/*")?;
        Ok(())
    }

    #[test]
    fn wildcards_do_not_cross_qualifiers_or_subpath() -> Result<(), PatternError> {
        let qualified = "pkg:npm/%40acme/tool?repository_url=https:%2F%2Fevil.example";
        let star = Pattern::compile("pkg:npm/%40acme/*")?;
        assert!(!star.matches(qualified));
        assert!(!star.matches("pkg:npm/%40acme/tool#src"));
        assert!(star.matches("pkg:npm/%40acme/tool"));
        let glob = Pattern::compile("pkg:npm/**")?;
        assert!(!glob.matches(qualified));
        assert!(glob.matches("pkg:npm/%40acme/tool"));
        let explicit =
            Pattern::compile("pkg:npm/%40acme/*?repository_url=https:%2F%2Fevil.example")?;
        assert!(explicit.matches(qualified));
        let all = Pattern::compile("**")?;
        assert!(all.matches(qualified));
        let pkg_glob = Pattern::compile("pkg:**")?;
        assert!(!pkg_glob.matches(qualified));
        assert!(pkg_glob.matches("pkg:npm/%40acme/tool"));
        Ok(())
    }
}
