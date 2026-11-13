// SPDX-License-Identifier: AGPL-3.0-only

//! Markdown stays a tree of code spans when every report field is hostile.

use std::collections::{BTreeMap, BTreeSet};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use crate::diff::{
    BoundedNodes, Change, ChangeCounts, ChangeDetail, ChangeKind, Diff, Summary, WarningSides,
};
use crate::model::{ComponentKey, ContentHash, DependencyRole, VersionAttributes, VersionId};
use crate::policy::{Decision, DecisionCounts, Mode, Outcome, Subject, Unit, UnitCounts, Verdict};

use super::assemble::{Entry, Prepared, prepare};
use super::escape::show_value;
use super::text::Phrase;

const ALPHABET: [&str; 9] = [
    "",
    " ",
    "`",
    "``",
    "```",
    "a",
    "\n",
    "[x](https://e.example)",
    "<b>",
];

const PURGE_SLOTS: usize = 12;

#[test]
fn empty_name_does_not_open_a_fence() -> Result<(), String> {
    let supplier = "[click here to approve](https://evil.example) ![x](https://evil.example/t.png)";
    let license = "[click here to approve](https://evil.example)";
    let slots = name_case("", "", "`", license, supplier);
    assert_case(&slots)?;
    assert_json_keeps_empty(&slots, "name")?;
    Ok(())
}

#[test]
fn backtick_name_and_empty_version_stay_in_spans() -> Result<(), String> {
    let slots = name_case("`", "", "", "mit", "acme");
    let slots = Slots {
        name: "`".to_owned(),
        version: String::new(),
        ..slots
    };
    assert_case(&slots)?;
    assert_json_keeps_empty(&slots, "version")?;
    Ok(())
}

#[test]
fn empty_license_beside_a_backtick_stays_in_a_span() -> Result<(), String> {
    let mut slots = name_case("lib", "pkg", "1", "", "acme");
    slots.extra_licenses.push("`".to_owned());
    assert_case(&slots)?;
    assert_json_keeps_empty(&slots, "license")?;
    Ok(())
}

#[test]
fn alphabet_values_stay_inside_code_spans() -> Result<(), String> {
    let mut batch = Batch::default();
    for index in 0..PURGE_SLOTS {
        for value in ALPHABET {
            let mut slots = Slots::purl();
            slots.set_purl(index, value);
            batch.push(&slots)?;
        }
    }
    for left in 0..PURGE_SLOTS - 1 {
        for first in ALPHABET {
            for second in ALPHABET {
                let mut slots = Slots::purl();
                slots.set_purl(left, first);
                slots.set_purl(left + 1, second);
                batch.push(&slots)?;
            }
        }
    }
    for left in 0..PURGE_SLOTS - 2 {
        for first in ALPHABET {
            for second in ALPHABET {
                for third in ALPHABET {
                    let mut slots = Slots::purl();
                    slots.set_purl(left, first);
                    slots.set_purl(left + 1, second);
                    slots.set_purl(left + 2, third);
                    batch.push(&slots)?;
                }
            }
        }
    }
    push_name_combinations(&mut batch)?;
    push_namespace_combinations(&mut batch)?;
    push_rule_combinations(&mut batch)?;
    batch.finish()?;
    if batch.cases != 10_746 {
        return Err(format!("systematic cases: {}", batch.cases));
    }
    Ok(())
}

fn push_name_combinations(batch: &mut Batch) -> Result<(), String> {
    for group in ALPHABET {
        let mut slots = Slots::named();
        slots.group = Some(group.to_owned());
        batch.push(&slots)?;
    }
    for name in ALPHABET {
        let mut slots = Slots::named();
        slots.group = Some(String::new());
        slots.name = name.to_owned();
        batch.push(&slots)?;
    }
    for group in ALPHABET {
        for name in ALPHABET {
            let mut slots = Slots::named();
            slots.group = Some(group.to_owned());
            slots.name = name.to_owned();
            batch.push(&slots)?;
        }
    }
    for group in ALPHABET {
        for name in ALPHABET {
            for version in ALPHABET {
                let mut slots = Slots::named();
                slots.group = Some(group.to_owned());
                slots.name = name.to_owned();
                slots.version = version.to_owned();
                batch.push(&slots)?;
            }
        }
    }
    Ok(())
}

fn push_namespace_combinations(batch: &mut Batch) -> Result<(), String> {
    for namespace in ALPHABET {
        let mut slots = Slots::purl();
        slots.namespace = Some(namespace.to_owned());
        batch.push(&slots)?;
    }
    for namespace in ALPHABET {
        for name in ALPHABET {
            let mut slots = Slots::purl();
            slots.namespace = Some(namespace.to_owned());
            slots.name = name.to_owned();
            batch.push(&slots)?;
        }
    }
    for namespace in ALPHABET {
        for name in ALPHABET {
            for version in ALPHABET {
                let mut slots = Slots::purl();
                slots.namespace = Some(namespace.to_owned());
                slots.name = name.to_owned();
                slots.version = version.to_owned();
                batch.push(&slots)?;
            }
        }
    }
    Ok(())
}

fn push_rule_combinations(batch: &mut Batch) -> Result<(), String> {
    for first in ALPHABET {
        for second in ALPHABET {
            let mut slots = Slots::purl();
            slots.rules = vec![first.to_owned(), second.to_owned()];
            batch.push(&slots)?;
        }
    }
    for first in ALPHABET {
        for second in ALPHABET {
            for third in ALPHABET {
                let mut slots = Slots::purl();
                slots.rules = vec![first.to_owned(), second.to_owned(), third.to_owned()];
                batch.push(&slots)?;
            }
        }
    }
    Ok(())
}

#[test]
fn every_invisible_range_is_visible_in_text_and_raw_in_json() -> Result<(), String> {
    for &(start, end) in super::escape::ENCODED_RANGES {
        check_invisible(start)?;
        if end != start {
            check_invisible(end)?;
        }
    }
    Ok(())
}

fn check_invisible(code: u32) -> Result<(), String> {
    let ch = char::from_u32(code).ok_or_else(|| format!("not a scalar {code:X}"))?;
    let mut slots = Slots::named();
    slots.license = ch.to_string();
    let (diff, verdict) = slots.build();
    let prepared = prepare(&diff, Some(&verdict), "0.1.0");
    let markdown = super::markdown::render(&prepared);
    let terminal = super::terminal::render(&prepared);
    let json = super::render_json(&prepared.json);
    let escaped = format!("\\u{{{code:04X}}}");
    let markdown_clean = markdown.lines().all(|line| !line.contains(ch));
    let terminal_clean = terminal.lines().all(|line| !line.contains(ch));
    if !markdown.contains(&escaped) || !markdown_clean {
        return Err(format!("markdown kept U+{code:X} raw"));
    }
    if !terminal.contains(&escaped) || !terminal_clean {
        return Err(format!("terminal kept U+{code:X} raw"));
    }
    if json.contains(&escaped) || (code >= 0x20 && !json.contains(ch)) {
        return Err(format!("JSON changed U+{code:X}"));
    }
    Ok(())
}

fn name_case(name: &str, group: &str, version: &str, license: &str, supplier: &str) -> Slots {
    let mut slots = Slots::named();
    slots.group = Some(group.to_owned());
    slots.name = name.to_owned();
    slots.version = version.to_owned();
    slots.license = license.to_owned();
    slots.supplier = supplier.to_owned();
    slots
}

fn assert_case(slots: &Slots) -> Result<(), String> {
    let (diff, verdict) = slots.build();
    let prepared = prepare(&diff, Some(&verdict), "0.1.0");
    let markdown = super::markdown::render(&prepared);
    let terminal = super::terminal::render(&prepared);
    let (expected, empties) = written(&prepared);
    let actual = code_literals(&markdown)?;
    if actual != expected {
        return Err(format!(
            "code spans diverged\nactual: {actual:?}\nexpected: {expected:?}"
        ));
    }
    if markdown.matches("(empty)").count() != empties {
        return Err(format!("empty marker missing\n{markdown}"));
    }
    if empties > 0 && !terminal.contains("(empty)") {
        return Err("terminal did not spell an empty value".to_owned());
    }
    Ok(())
}

fn assert_json_keeps_empty(slots: &Slots, field: &str) -> Result<(), String> {
    let (diff, verdict) = slots.build();
    let prepared = prepare(&diff, Some(&verdict), "0.1.0");
    let json = super::render_json(&prepared.json);
    if json.contains("(empty)") {
        return Err("JSON replaced an empty string".to_owned());
    }
    let needle = match field {
        "name" => "\"name\": \"\"",
        "version" => "\"version\": \"\"",
        "license" => "\"licenses\": [\n            \"\"",
        _ => return Err("unknown field".to_owned()),
    };
    if !json.contains(needle) {
        return Err(format!("JSON dropped the empty {field}\n{json}"));
    }
    Ok(())
}

struct Slots {
    ecosystem: String,
    namespace: Option<String>,
    group: Option<String>,
    name: String,
    version: String,
    qualifier_key: String,
    qualifier_value: String,
    subpath: Option<String>,
    license: String,
    extra_licenses: Vec<String>,
    supplier: String,
    scope: String,
    hash_alg: String,
    hash_content: String,
    rules: Vec<String>,
}

impl Slots {
    fn purl() -> Self {
        Self {
            ecosystem: "a".to_owned(),
            namespace: None,
            group: None,
            name: "a".to_owned(),
            version: "a".to_owned(),
            qualifier_key: "a".to_owned(),
            qualifier_value: "a".to_owned(),
            subpath: Some("a".to_owned()),
            license: "a".to_owned(),
            extra_licenses: Vec::new(),
            supplier: "a".to_owned(),
            scope: "a".to_owned(),
            hash_alg: "a".to_owned(),
            hash_content: "a".to_owned(),
            rules: vec!["a".to_owned()],
        }
    }

    fn named() -> Self {
        let mut slots = Self::purl();
        slots.group = Some("a".to_owned());
        slots.subpath = None;
        slots
    }

    fn set_purl(&mut self, index: usize, value: &str) {
        let owned = value.to_owned();
        match index {
            0 => self.ecosystem = owned,
            1 => self.name = owned,
            2 => self.version = owned,
            3 => self.qualifier_key = owned,
            4 => self.qualifier_value = owned,
            5 => self.subpath = Some(owned),
            6 => self.license = owned,
            7 => self.supplier = owned,
            8 => self.scope = owned,
            9 => self.hash_alg = owned,
            10 => self.hash_content = owned,
            11 => self.rules = vec![owned],
            _ => {}
        }
    }

    fn component_key(&self) -> ComponentKey {
        if let Some(group) = &self.group {
            return ComponentKey::Name {
                group: group.clone(),
                name: self.name.clone(),
            };
        }
        let mut qualifiers = BTreeMap::new();
        qualifiers.insert(self.qualifier_key.clone(), self.qualifier_value.clone());
        ComponentKey::Purl {
            ty: self.ecosystem.clone(),
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            qualifiers,
            subpath: self.subpath.clone(),
        }
    }

    fn build(&self) -> (Diff, Verdict) {
        let version = VersionId {
            version: self.version.clone(),
            epoch: None,
        };
        let key = self.component_key();
        let mut licenses = BTreeSet::new();
        licenses.insert(self.license.clone());
        licenses.extend(self.extra_licenses.iter().cloned());
        let mut suppliers = BTreeSet::new();
        suppliers.insert(self.supplier.clone());
        let mut scopes = BTreeSet::new();
        scopes.insert(self.scope.clone());
        let mut hashes = BTreeSet::new();
        hashes.insert(ContentHash {
            alg: self.hash_alg.clone(),
            content: self.hash_content.clone(),
        });
        let change = Change {
            key: key.clone(),
            kind: ChangeKind::Added,
            version: version.clone(),
            old_version: None,
            new_version: Some(version.clone()),
            old_role: None,
            new_role: Some(DependencyRole::Direct),
            detail: ChangeDetail::Added {
                attributes: Box::new(VersionAttributes {
                    licenses,
                    hashes,
                    suppliers,
                    scopes,
                    ..VersionAttributes::default()
                }),
                introduced_by: BoundedNodes {
                    nodes: Vec::new(),
                    total: 0,
                },
                path_from_root: None,
            },
            judged: VersionAttributes::default(),
        };
        let unit = Unit {
            subject: Subject::Change {
                key,
                version,
                kind: ChangeKind::Added,
            },
            configured: Decision::Deny,
            effective: Decision::Deny,
            rules: self.rules.clone(),
            used_default: false,
        };
        let diff = Diff {
            changes: vec![change],
            root: None,
            summary: Summary {
                counts: ChangeCounts {
                    added: 1,
                    ..ChangeCounts::default()
                },
                by_role: Vec::new(),
            },
            roles_comparable: true,
            warnings: WarningSides {
                old: Vec::new(),
                new: Vec::new(),
            },
        };
        let counts = DecisionCounts {
            deny: 1,
            warn: 0,
            allow: 0,
        };
        let verdict = Verdict {
            mode: Mode::Enforce,
            configured: Outcome::Deny,
            effective: Outcome::Deny,
            units: vec![unit],
            counts: UnitCounts {
                configured: counts.clone(),
                effective: counts,
            },
            old_warnings: Vec::new(),
        };
        (diff, verdict)
    }
}

#[derive(Default)]
struct Batch {
    markdown: String,
    expected: Vec<String>,
    empties: usize,
    pending: usize,
    cases: usize,
}

impl Batch {
    fn push(&mut self, slots: &Slots) -> Result<(), String> {
        let (diff, verdict) = slots.build();
        let prepared = prepare(&diff, Some(&verdict), "0.1.0");
        let (values, empties) = written(&prepared);
        self.expected.extend(values);
        self.empties += empties;
        self.markdown.push_str(&super::markdown::render(&prepared));
        self.pending += 1;
        self.cases += 1;
        if self.pending == 200 {
            self.finish_pending()?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.pending > 0 {
            self.finish_pending()?;
        }
        Ok(())
    }

    fn finish_pending(&mut self) -> Result<(), String> {
        let actual = code_literals(&self.markdown)?;
        if actual != self.expected {
            let index = actual
                .iter()
                .zip(self.expected.iter())
                .position(|(left, right)| left != right);
            let found = index.and_then(|index| actual.get(index));
            let wanted = index.and_then(|index| self.expected.get(index));
            return Err(format!(
                "code spans diverged at {index:?} (spans {}, values {}, cases {}): found {found:?} wanted {wanted:?}",
                actual.len(),
                self.expected.len(),
                self.cases
            ));
        }
        if self.markdown.matches("(empty)").count() != self.empties {
            return Err("an empty value was written as a code span".to_owned());
        }
        self.markdown.clear();
        self.expected.clear();
        self.empties = 0;
        self.pending = 0;
        Ok(())
    }
}

fn written(prepared: &Prepared) -> (Vec<String>, usize) {
    let mut values = Vec::new();
    let mut empties = 0usize;
    for group in &prepared.groups {
        for entry in &group.entries {
            take(&entry.heading, &mut values, &mut empties);
            take(&entry.phrases, &mut values, &mut empties);
            take_rules(entry, &mut values, &mut empties);
        }
    }
    for note in &prepared.notes {
        take(note, &mut values, &mut empties);
    }
    (values, empties)
}

fn take(phrases: &[Phrase], values: &mut Vec<String>, empties: &mut usize) {
    for phrase in phrases {
        if let Phrase::Value(value) = phrase {
            if value.is_empty() {
                *empties += 1;
            } else {
                values.push(show_value(value).text);
            }
        }
    }
}

fn take_rules(entry: &Entry, values: &mut Vec<String>, empties: &mut usize) {
    take(&entry.rules, values, empties);
}

fn code_literals(markdown: &str) -> Result<Vec<String>, String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.table = true;
    let root = parse_document(&arena, markdown, &options);
    let mut literals = Vec::new();
    walk(root, &mut literals)?;
    Ok(literals)
}

fn walk<'a>(node: &'a AstNode<'a>, literals: &mut Vec<String>) -> Result<(), String> {
    match &node.data.borrow().value {
        NodeValue::Link(_) | NodeValue::Image(_) => {
            return Err("markdown contains a link or an image".to_owned());
        }
        NodeValue::HtmlBlock(block) => {
            if !html_is_ours(&block.literal) {
                return Err(format!("unexpected HTML block {}", block.literal));
            }
        }
        NodeValue::HtmlInline(html) => {
            if !html_is_ours(html) {
                return Err(format!("unexpected HTML inline {html}"));
            }
        }
        NodeValue::Code(code) => literals.push(code.literal.clone()),
        _ => {}
    }
    for child in node.children() {
        walk(child, literals)?;
    }
    Ok(())
}

fn html_is_ours(html: &str) -> bool {
    let stripped = html
        .replace("<details>", "")
        .replace("</details>", "")
        .replace("<summary>", "")
        .replace("</summary>", "")
        .replace('\n', "");
    !stripped.contains('<') && !stripped.contains('>')
}
