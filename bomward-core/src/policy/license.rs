// SPDX-License-Identifier: AGPL-3.0-only

//! SPDX license identifiers.
//!
//! List entries and expression tokens are matched on the SPDX id, not as a
//! substring. Comparison ignores ASCII case. `GPL-3.0-only` does not match
//! `LGPL-3.0-only`, and `GPL-3.0` does not match `GPL-3.0-or-later`.
//! `OR`, `AND`, and `WITH` still match the id: a choice in the expression
//! does not hide it.

use std::collections::HashMap;
use std::sync::OnceLock;

use spdx::expression::Expression;
use spdx::{ParseMode, license_id};

/// Parse mode for a license expression stored on a component.
///
/// Deprecated ids stay distinct, so `GPL-3.0` is not rewritten to
/// `GPL-3.0-only`. Imprecise names stay off for the same reason.
const EXPRESSION: ParseMode = ParseMode {
    allow_slash_as_or_operator: false,
    allow_imprecise_license_names: false,
    allow_postfix_plus_on_gpl: false,
    allow_deprecated: true,
    allow_unknown: false,
};

/// Canonical SPDX id, or `None` when `name` is not on the SPDX list.
pub(crate) fn canonical_license(name: &str) -> Option<&'static str> {
    index().get(&name.to_ascii_lowercase()).copied()
}

/// Whether `text` is an SPDX expression that contains `wanted`.
///
/// `wanted` is a canonical id stored at load. A string that does not parse
/// contains nothing.
pub(crate) fn expression_contains(text: &str, wanted: &str) -> bool {
    let Some(wanted_id) = license_id(wanted) else {
        return false;
    };
    let canonical = canonicalize_expression(text);
    let Ok(expression) = Expression::parse_mode(&canonical, EXPRESSION) else {
        return false;
    };
    expression
        .requirements()
        .any(|requirement| requirement.req.license.id() == Some(wanted_id))
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
