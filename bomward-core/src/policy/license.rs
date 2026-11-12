// SPDX-License-Identifier: AGPL-3.0-only

//! SPDX license identifiers.
//!
//! List entries and expression tokens are matched on the SPDX id, not as a
//! substring. Comparison ignores ASCII case. `GPL-3.0-only` does not match
//! `LGPL-3.0-only`, and `GPL-3.0` does not match `GPL-3.0-or-later`.
//! `OR`, `AND`, and `WITH` still match the id: a choice in the expression
//! does not hide it.
//!
//! A string that is not a valid expression is split on the same character
//! class as [`canonicalize_expression`]. Every token that is an SPDX id
//! counts. A trailing `+` with no space before it is the SPDX "or later"
//! operator: the id, `{id}-or-later` when that identifier exists, and the
//! deprecated `{id}+` identifier when the list still has it. SPDX 2.3
//! allows `+` after any license id. License list 3.29 embeds `+` only in
//! `GPL-1.0+`, `GPL-2.0+`, `GPL-3.0+`, `LGPL-2.0+`, `LGPL-2.1+`, and
//! `LGPL-3.0+`.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::OnceLock;

use spdx::LicenseItem;
use spdx::ParseMode;
use spdx::expression::Expression;

/// Parse mode for a license expression stored on a component.
///
/// Deprecated ids stay distinct, so `GPL-3.0` is not rewritten to
/// `GPL-3.0-only`. Imprecise names stay off for the same reason. GNU `+`
/// stays off here: the crate would rewrite `GPL-3.0+` to `GPL-3.0-or-later`
/// and drop the deprecated id. The token scan keeps both.
const EXPRESSION: ParseMode = ParseMode {
    allow_slash_as_or_operator: false,
    allow_imprecise_license_names: false,
    allow_postfix_plus_on_gpl: false,
    allow_deprecated: true,
    allow_unknown: false,
};

/// Ids found in one license string, and whether the string is recognizable.
pub(crate) struct LicenseFacts {
    /// Canonical SPDX ids the string contains.
    ///
    /// A failed parse still contributes every token that is an SPDX id.
    pub(crate) ids: BTreeSet<&'static str>,
    /// True when the whole string is one SPDX id or a valid SPDX expression.
    ///
    /// Tokens found beside other text do not set this. Those ids still
    /// match `license` rules, and the string matches `license_status`.
    pub(crate) recognized: bool,
}

/// Canonical SPDX id, or `None` when `name` is not on the SPDX list.
pub(crate) fn canonical_license(name: &str) -> Option<&'static str> {
    index().get(&name.to_ascii_lowercase()).copied()
}

/// Ids in `text`, and whether the whole string is one id or an expression.
///
/// A parse failure falls back to whole tokens. A token that is not an SPDX
/// id is ignored. `LGPL-3.0-only` is not `GPL-3.0-only`. The string is
/// recognized only when it is exactly one SPDX id, or when it parses.
/// Tokens beside other text stay in [`LicenseFacts::ids`] and leave
/// `recognized` false.
pub(crate) fn analyze_license(text: &str) -> LicenseFacts {
    let canonical = canonicalize_expression(text);
    let mut ids = BTreeSet::new();
    let parsed = if let Ok(expression) = Expression::parse_mode(&canonical, EXPRESSION) {
        for requirement in expression.requirements() {
            if let LicenseItem::Spdx { id, or_later } = &requirement.req.license {
                if *or_later {
                    expand_plus(id.name, &mut ids);
                } else {
                    ids.insert(id.name);
                }
            }
        }
        true
    } else {
        scan_tokens(&canonical, &mut ids);
        false
    };
    LicenseFacts {
        ids,
        recognized: parsed || canonical_license(text).is_some(),
    }
}

fn expand_plus(base: &'static str, ids: &mut BTreeSet<&'static str>) {
    ids.insert(base);
    let or_later = format!("{base}-or-later");
    if let Some(id) = canonical_license(&or_later) {
        ids.insert(id);
    }
    let plus = format!("{base}+");
    if let Some(id) = canonical_license(&plus) {
        ids.insert(id);
    }
}

fn scan_tokens(input: &str, ids: &mut BTreeSet<&'static str>) {
    let mut token = String::new();
    for ch in input.chars() {
        if is_license_char(ch) {
            token.push(ch);
            continue;
        }
        take_token(&mut token, ch == '+', ids);
    }
    take_token(&mut token, false, ids);
}

fn take_token(token: &mut String, plus: bool, ids: &mut BTreeSet<&'static str>) {
    if token.is_empty() {
        return;
    }
    if let Some(canonical) = canonical_license(token) {
        if plus {
            expand_plus(canonical, ids);
        } else {
            ids.insert(canonical);
        }
    }
    token.clear();
}

fn canonicalize_expression(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut token = String::new();
    for ch in input.chars() {
        if is_license_char(ch) {
            token.push(ch);
        } else {
            flush_token(&mut token, &mut out);
            out.push(ch);
        }
    }
    flush_token(&mut token, &mut out);
    out
}

fn flush_token(token: &mut String, out: &mut String) {
    if token.is_empty() {
        return;
    }
    if let Some(canonical) = canonical_license(token) {
        out.push_str(canonical);
    } else {
        out.push_str(token);
    }
    token.clear();
}

fn is_license_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '.'
}

fn index() -> &'static HashMap<String, &'static str> {
    static INDEX: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map = HashMap::new();
        for license in spdx::identifiers::LICENSES {
            map.entry(license.name.to_ascii_lowercase())
                .or_insert(license.name);
        }
        map
    })
}

#[cfg(test)]
mod tests {
    use super::analyze_license;

    fn contains(text: &str, id: &str) -> bool {
        analyze_license(text).ids.contains(id)
    }

    #[test]
    fn plus_expands_on_the_parse_path_and_the_token_path() {
        assert!(contains("GPL-3.0+", "GPL-3.0"));
        assert!(contains("GPL-3.0+", "GPL-3.0-or-later"));
        assert!(contains("GPL-3.0+", "GPL-3.0+"));
        assert!(!contains("GPL-3.0+", "GPL-3.0-only"));
        assert!(contains("CDDL-1.0+", "CDDL-1.0"));
        assert!(!contains("CDDL-1.0+", "CDDL-1.0-or-later"));
        assert!(contains("AGPL-3.0+", "AGPL-3.0"));
        assert!(contains("AGPL-3.0+", "AGPL-3.0-or-later"));
        assert!(!contains("AGPL-3.0+", "AGPL-3.0+"));
        assert!(!analyze_license("GNU General Public License v3.0 only").recognized);
        assert!(!analyze_license("GPL-3.0-only AND MyCustomLicense").recognized);
        assert!(
            analyze_license("GPL-3.0-only AND MyCustomLicense")
                .ids
                .contains("GPL-3.0-only")
        );
    }
}
