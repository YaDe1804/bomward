// SPDX-License-Identifier: AGPL-3.0-only

//! Report rendering: one catalog, three formats, hostile input, and size.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use bomward_core::Limits;
use bomward_core::diff::{
    BoundedNodes, Change, ChangeCounts, ChangeDetail, ChangeKind, Diff, Summary, WarningSides,
};
use bomward_core::model::{
    ComponentKey, ContentHash, DependencyRole, Document, GraphNode, VersionAttributes, VersionId,
};
use bomward_core::policy::{
    Decision, DecisionCounts, Mode, Outcome, Subject, Unit, UnitCounts, Verdict,
};
use bomward_core::report::{Format, MARKDOWN_CHAR_BUDGET, render};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn load_pair(policy: Option<&str>, old: &str) -> Result<(Diff, Option<Verdict>), String> {
    let old_document = bomward_core::ingest::read_path(&root(old), &Limits::default())
        .map_err(|error| error.to_string())?;
    let new_document = bomward_core::ingest::read_path(
        &root("tests/fixtures/lockward-new.cdx.json"),
        &Limits::default(),
    )
    .map_err(|error| error.to_string())?;
    let diff = bomward_core::diff::diff(&old_document, &new_document);
    let verdict = match policy {
        None => None,
        Some(name) => {
            let loaded = bomward_core::policy::read_path(
                &root(&format!("../examples/policies/{name}.yaml")),
                &bomward_core::policy::PolicyLimits::default(),
            )
            .map_err(|error| error.to_string())?;
            Some(bomward_core::policy::evaluate(&loaded, &diff))
        }
    };
    Ok((diff, verdict))
}

fn formats(diff: &Diff, verdict: Option<&Verdict>) -> (String, String, String) {
    (
        render(diff, verdict, Format::Terminal),
        render(diff, verdict, Format::Markdown),
        render(diff, verdict, Format::Json),
    )
}

fn scenario(name: &str, policy: Option<&str>, old: &str) -> Result<(), String> {
    let (diff, verdict) = load_pair(policy, old)?;
    let (terminal, markdown, json) = formats(&diff, verdict.as_ref());
    assert_consistent(&terminal, &markdown, &json, verdict.as_ref())?;
    store(name, "txt", &terminal)?;
    store(name, "md", &markdown)?;
    store(name, "json", &json)?;
    Ok(())
}

fn store(name: &str, extension: &str, actual: &str) -> Result<(), String> {
    let path = root(&format!("../examples/reports/{name}.{extension}"));
    if std::env::var_os("BOMWARD_UPDATE_GOLDENS").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&path, actual).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let expected = fs::read_to_string(&path).map_err(|error| {
        format!(
            "missing {} ({error}); set BOMWARD_UPDATE_GOLDENS=1 to write it",
            path.display()
        )
    })?;
    if expected == actual {
        Ok(())
    } else {
        Err(format!("{} diverged from the golden file", path.display()))
    }
}

fn assert_consistent(
    terminal: &str,
    markdown: &str,
    json: &str,
    verdict: Option<&Verdict>,
) -> Result<(), String> {
    if terminal.contains('\r') || markdown.contains('\r') || json.contains('\r') {
        return Err("a report contains CR".to_owned());
    }
    if !terminal.ends_with('\n') || !markdown.ends_with('\n') || !json.ends_with('\n') {
        return Err("a report does not end with LF".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let exit_code = value
        .pointer("/gate/exit_code")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "JSON gate.exit_code is missing".to_owned())?;
    let expected_exit = verdict.map_or(0, Verdict::exit_code);
    if exit_code != u64::from(expected_exit) {
        return Err(format!("JSON exit code {exit_code} is not {expected_exit}"));
    }
    if !terminal.contains(&format!("Exit code: {expected_exit}")) {
        return Err("terminal exit code does not match the verdict".to_owned());
    }
    let units = value
        .pointer("/units")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "JSON units is missing".to_owned())?;
    let denied = units
        .iter()
        .filter(|unit| unit.get("effective").and_then(serde_json::Value::as_str) == Some("deny"))
        .count();
    if verdict.is_none() {
        if !terminal.contains("No policy was applied.")
            || !markdown.contains("No policy was applied.")
        {
            return Err("report-only sentence is missing".to_owned());
        }
        if value.get("notice").and_then(serde_json::Value::as_str) != Some("No policy was applied.")
        {
            return Err("JSON notice is not the report-only sentence".to_owned());
        }
        if value
            .get("policy_applied")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        {
            return Err("policy_applied is not false".to_owned());
        }
    } else if terminal.contains("No policy was applied.")
        || markdown.contains("No policy was applied.")
    {
        return Err("a judged report contains the report-only sentence".to_owned());
    }
    let terminal_denied = terminal.matches("\nDeny ").count();
    let markdown_denied = denied_list_items(markdown);
    if verdict.is_some() && (terminal_denied != denied || markdown_denied != denied) {
        return Err(format!(
            "denied counts differ: json {denied}, terminal {terminal_denied}, markdown {markdown_denied}"
        ));
    }
    Ok(())
}

#[test]
fn lockward_recommended() -> Result<(), String> {
    scenario(
        "lockward-recommended",
        Some("recommended"),
        "tests/fixtures/lockward-old.cdx.json",
    )
}

#[test]
fn lockward_strict() -> Result<(), String> {
    scenario(
        "lockward-strict",
        Some("strict"),
        "tests/fixtures/lockward-old.cdx.json",
    )
}

#[test]
fn lockward_licenses() -> Result<(), String> {
    scenario(
        "lockward-licenses",
        Some("licenses"),
        "tests/fixtures/lockward-old.cdx.json",
    )
}

#[test]
fn lockward_report_only() -> Result<(), String> {
    scenario(
        "lockward-report-only",
        None,
        "tests/fixtures/lockward-old.cdx.json",
    )
}

#[test]
fn old_format_recommended() -> Result<(), String> {
    scenario(
        "lockward-old-format-recommended",
        Some("recommended"),
        "tests/fixtures/lockward-old-format.cdx.json",
    )
}

#[test]
fn old_format_strict() -> Result<(), String> {
    scenario(
        "lockward-old-format-strict",
        Some("strict"),
        "tests/fixtures/lockward-old-format.cdx.json",
    )
}

#[test]
fn old_format_licenses() -> Result<(), String> {
    scenario(
        "lockward-old-format-licenses",
        Some("licenses"),
        "tests/fixtures/lockward-old-format.cdx.json",
    )
}

#[test]
fn old_format_report_only() -> Result<(), String> {
    scenario(
        "lockward-old-format-report-only",
        None,
        "tests/fixtures/lockward-old-format.cdx.json",
    )
}

#[test]
fn the_same_input_is_byte_identical() -> Result<(), String> {
    let (diff, verdict) = load_pair(Some("strict"), "tests/fixtures/lockward-old.cdx.json")?;
    let first = formats(&diff, verdict.as_ref());
    let second = formats(&diff, verdict.as_ref());
    if first != second {
        return Err("a second render changed bytes".to_owned());
    }
    Ok(())
}

#[test]
fn an_empty_diff_has_one_sentence() -> Result<(), String> {
    let document = Document {
        components: BTreeMap::default(),
        dependencies: BTreeMap::default(),
        warnings: Vec::new(),
    };
    let diff = bomward_core::diff::diff(&document, &document);
    let (terminal, markdown, _) = formats(&diff, None);
    if !terminal.contains("No dependency changes.") || !markdown.contains("No dependency changes.")
    {
        return Err("the empty-diff sentence is missing".to_owned());
    }
    if terminal.contains("## ") {
        return Err("the empty diff built a section scaffold".to_owned());
    }
    Ok(())
}

#[test]
fn decision_words_cover_zero_one_and_two() -> Result<(), String> {
    for count in [0_u64, 1, 2] {
        let (diff, verdict) = counted_denials(count);
        let (terminal, _, _) = formats(&diff, Some(&verdict));
        let word = match count {
            0 => "0 denials",
            1 => "1 denial",
            _ => "2 denials",
        };
        if !terminal.contains(word) {
            return Err(format!("missing {word}"));
        }
        let warnings = match count {
            0 => "0 warnings",
            1 => "1 warning",
            _ => "2 warnings",
        };
        if !terminal.contains(warnings) {
            return Err(format!("missing {warnings}"));
        }
    }
    Ok(())
}

#[test]
fn hostile_values_stay_inside_code_spans() -> Result<(), String> {
    let payloads = hostile_payloads();
    let (diff, verdict) = hostile_diff(&payloads);
    let (terminal, markdown, json) = formats(&diff, Some(&verdict));
    assert_terminal_bytes(&terminal)?;
    assert_markdown_tree(&markdown, &payloads)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let long = payloads
        .last()
        .ok_or_else(|| "no long payload".to_owned())?;
    if !contains_string(&value, long) {
        return Err("JSON dropped the long value".to_owned());
    }
    if markdown.contains(long) {
        return Err("Markdown kept the uncut long value".to_owned());
    }
    Ok(())
}

#[test]
fn markdown_keeps_denied_entries_inside_the_budget() -> Result<(), String> {
    let started = Instant::now();
    let (diff, verdict) = many(50_000, 5);
    let markdown = render(&diff, Some(&verdict), Format::Markdown);
    let elapsed = started.elapsed();
    eprintln!("size-allowed-ms {}", elapsed.as_millis());
    if markdown.chars().count() > MARKDOWN_CHAR_BUDGET {
        return Err("markdown exceeded the character budget".to_owned());
    }
    for index in 0..5 {
        if !markdown.contains(&format!("deny-item-{index}")) {
            return Err(format!("denied entry {index} was cut first"));
        }
    }
    if !markdown.contains("allowed entries omitted of 50000")
        && !markdown.contains("allowed entry omitted of 50000")
    {
        return Err("the allowed omission does not give the total".to_owned());
    }
    if markdown.contains('\r') {
        return Err("markdown contains CR".to_owned());
    }
    Ok(())
}

#[test]
fn markdown_truncates_denied_entries_last() -> Result<(), String> {
    let started = Instant::now();
    let (diff, verdict) = many(0, 20_000);
    let markdown = render(&diff, Some(&verdict), Format::Markdown);
    let elapsed = started.elapsed();
    eprintln!("size-denied-ms {}", elapsed.as_millis());
    if markdown.chars().count() > MARKDOWN_CHAR_BUDGET {
        return Err("markdown exceeded the character budget".to_owned());
    }
    if !markdown.contains("denied entries omitted of 20000") {
        return Err("the denied omission does not give the total".to_owned());
    }
    Ok(())
}

#[test]
fn every_golden_matches_the_schema() -> Result<(), String> {
    let schema_text = fs::read_to_string(root("../docs/report.schema.json"))
        .map_err(|error| error.to_string())?;
    let schema: serde_json::Value =
        serde_json::from_str(&schema_text).map_err(|error| error.to_string())?;
    let validator = jsonschema::validator_for(&schema).map_err(|error| error.to_string())?;
    for name in GOLDENS {
        let path = root(&format!("../examples/reports/{name}.json"));
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let instance: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if let Err(error) = validator.validate(&instance) {
            return Err(format!("{}: {error}", path.display()));
        }
    }
    Ok(())
}

const GOLDENS: [&str; 8] = [
    "lockward-recommended",
    "lockward-strict",
    "lockward-licenses",
    "lockward-report-only",
    "lockward-old-format-recommended",
    "lockward-old-format-strict",
    "lockward-old-format-licenses",
    "lockward-old-format-report-only",
];

fn counted_denials(deny: u64) -> (Diff, Verdict) {
    let mut changes = Vec::new();
    let mut units = Vec::new();
    for index in 0..deny {
        let (change, unit) = judged(&format!("deny-{index}"), Decision::Deny);
        changes.push(change);
        units.push(unit);
    }
    for index in 0..deny {
        let (change, unit) = judged(&format!("warn-{index}"), Decision::Warn);
        changes.push(change);
        units.push(unit);
    }
    let diff = diff_of(changes);
    let verdict = verdict_of(
        units,
        Mode::Enforce,
        if deny == 0 {
            Outcome::Pass
        } else {
            Outcome::Deny
        },
    );
    (diff, verdict)
}

fn many(allowed: u64, denied: u64) -> (Diff, Verdict) {
    let mut changes = Vec::new();
    let mut units = Vec::new();
    for index in 0..denied {
        let (change, unit) = judged(&format!("deny-item-{index}"), Decision::Deny);
        changes.push(change);
        units.push(unit);
    }
    for index in 0..allowed {
        let (change, unit) = judged(&format!("allow-item-{index}"), Decision::Allow);
        changes.push(change);
        units.push(unit);
    }
    let outcome = if denied > 0 {
        Outcome::Deny
    } else {
        Outcome::Pass
    };
    (diff_of(changes), verdict_of(units, Mode::Enforce, outcome))
}

fn judged(name: &str, decision: Decision) -> (Change, Unit) {
    let version = VersionId {
        version: "1.0.0".to_owned(),
        epoch: None,
    };
    let key = ComponentKey::Name {
        group: String::new(),
        name: name.to_owned(),
    };
    let change = Change {
        key: key.clone(),
        kind: ChangeKind::Added,
        version: version.clone(),
        old_version: None,
        new_version: Some(version.clone()),
        old_role: None,
        new_role: Some(DependencyRole::Direct),
        detail: ChangeDetail::Added {
            attributes: Box::new(VersionAttributes::default()),
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
        configured: decision,
        effective: decision,
        rules: vec!["sample-rule".to_owned()],
        used_default: false,
    };
    (change, unit)
}

fn diff_of(changes: Vec<Change>) -> Diff {
    let counts = ChangeCounts {
        added: u64::try_from(changes.len()).unwrap_or(u64::MAX),
        ..ChangeCounts::default()
    };
    Diff {
        changes,
        root: None,
        summary: Summary {
            counts,
            by_role: Vec::new(),
        },
        roles_comparable: true,
        warnings: WarningSides {
            old: Vec::new(),
            new: Vec::new(),
        },
    }
}

fn verdict_of(units: Vec<Unit>, mode: Mode, effective: Outcome) -> Verdict {
    let deny = units
        .iter()
        .filter(|unit| unit.effective == Decision::Deny)
        .count();
    let warn = units
        .iter()
        .filter(|unit| unit.effective == Decision::Warn)
        .count();
    let allow = units
        .iter()
        .filter(|unit| unit.effective == Decision::Allow)
        .count();
    let counts = DecisionCounts {
        deny: u64::try_from(deny).unwrap_or(u64::MAX),
        warn: u64::try_from(warn).unwrap_or(u64::MAX),
        allow: u64::try_from(allow).unwrap_or(u64::MAX),
    };
    Verdict {
        mode,
        configured: effective,
        effective,
        units,
        counts: UnitCounts {
            configured: counts.clone(),
            effective: counts,
        },
        old_warnings: Vec::new(),
    }
}

fn hostile_payloads() -> Vec<String> {
    let mut payloads = vec![
        "[x](https://evil.example)".to_owned(),
        "![x](https://evil.example/a.png)".to_owned(),
        "<img src=x onerror=alert(1)>".to_owned(),
        "</details><h1>x".to_owned(),
        "@octocat".to_owned(),
        "#1".to_owned(),
        "https://evil.example".to_owned(),
        "www.evil.example".to_owned(),
        "0123456789abcdef0123456789abcdef01234567".to_owned(),
        "`".to_owned(),
        "```".to_owned(),
        "| a | b |".to_owned(),
        "\n# heading".to_owned(),
        "\u{001B}[31mred".to_owned(),
        "\u{202E}hidden".to_owned(),
    ];
    payloads.push("H".repeat(100_000));
    payloads
}

fn hostile_diff(payloads: &[String]) -> (Diff, Verdict) {
    let at = |index: usize| {
        payloads
            .get(index)
            .cloned()
            .unwrap_or_else(|| "missing".to_owned())
    };
    let version = VersionId {
        version: at(6),
        epoch: None,
    };
    let mut licenses = BTreeSet::new();
    licenses.insert(at(0));
    licenses.insert(at(15));
    let mut suppliers = BTreeSet::new();
    suppliers.insert(at(1));
    let mut scopes = BTreeSet::new();
    scopes.insert(at(2));
    let mut hashes = BTreeSet::new();
    hashes.insert(ContentHash {
        alg: at(8),
        content: at(9),
    });
    let attributes = VersionAttributes {
        licenses,
        hashes,
        suppliers,
        scopes,
        ..VersionAttributes::default()
    };
    let introduced = GraphNode {
        key: ComponentKey::Purl {
            ty: at(3),
            namespace: Some(at(4)),
            name: at(5),
            qualifiers: BTreeMap::new(),
            subpath: Some(at(7)),
        },
        version: version.clone(),
    };
    let change = Change {
        key: ComponentKey::Purl {
            ty: at(10),
            namespace: Some(at(11)),
            name: at(12),
            qualifiers: {
                let mut map = std::collections::BTreeMap::new();
                map.insert(at(13), at(14));
                map
            },
            subpath: None,
        },
        kind: ChangeKind::Added,
        version: version.clone(),
        old_version: None,
        new_version: Some(version.clone()),
        old_role: None,
        new_role: Some(DependencyRole::Transitive),
        detail: ChangeDetail::Added {
            attributes: Box::new(attributes),
            introduced_by: BoundedNodes {
                nodes: vec![introduced],
                total: 1,
            },
            path_from_root: None,
        },
        judged: VersionAttributes::default(),
    };
    let unit = Unit {
        subject: Subject::Change {
            key: change.key.clone(),
            version: version.clone(),
            kind: ChangeKind::Added,
        },
        configured: Decision::Deny,
        effective: Decision::Deny,
        rules: vec!["hostile-rule".to_owned()],
        used_default: false,
    };
    let diff = diff_of(vec![change]);
    let verdict = verdict_of(vec![unit], Mode::Enforce, Outcome::Deny);
    (diff, verdict)
}

fn assert_terminal_bytes(text: &str) -> Result<(), String> {
    for ch in text.chars() {
        let code = u32::from(ch);
        let forbidden = (code < 0x20 && ch != '\n')
            || code == 0x7F
            || (0x80..=0x9F).contains(&code)
            || matches!(
                code,
                0x061C | 0x200E | 0x200F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
            );
        if forbidden {
            return Err(format!("terminal text contains U+{code:04X}"));
        }
    }
    Ok(())
}

fn assert_markdown_tree(markdown: &str, payloads: &[String]) -> Result<(), String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.table = true;
    let root = parse_document(&arena, markdown, &options);
    let mut literals = Vec::new();
    walk(root, &mut literals)?;
    for (index, payload) in payloads.iter().enumerate() {
        let expected = if payload.chars().count() > 256 {
            let prefix: String = payload.chars().take(256).collect();
            prefix
        } else {
            encoded(payload)
        };
        if !literals.iter().any(|literal| literal.contains(&expected)) {
            return Err(format!("payload {index} is not inside one code span"));
        }
    }
    let long = payloads
        .last()
        .ok_or_else(|| "no long payload".to_owned())?;
    if !markdown.contains(&format!("({} characters)", long.chars().count())) {
        return Err("the long value has no total length".to_owned());
    }
    Ok(())
}

fn walk<'a>(node: &'a AstNode<'a>, literals: &mut Vec<String>) -> Result<(), String> {
    match &node.data.borrow().value {
        NodeValue::Link(_) | NodeValue::Image(_) => {
            return Err("markdown contains a link or an image".to_owned());
        }
        NodeValue::Table(_) | NodeValue::TableRow(_) | NodeValue::TableCell => {
            return Err("markdown contains a table".to_owned());
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
        NodeValue::Heading(_) => {
            let text = heading_text(node);
            let allowed = text.starts_with("bomward ")
                || text == "Denied"
                || text == "Warnings"
                || text == "Changes";
            if !allowed {
                return Err(format!("unexpected heading {text}"));
            }
        }
        NodeValue::Code(code) => literals.push(code.literal.clone()),
        // List and item nodes are the entry format. Anything else that is
        // not a link, a table, or unexpected HTML is report prose.
        _ => {}
    }
    for child in node.children() {
        walk(child, literals)?;
    }
    Ok(())
}

fn denied_list_items(markdown: &str) -> usize {
    let Some(section) = markdown.split("## Denied\n").nth(1) else {
        return 0;
    };
    let body = section.split("\n## ").next().unwrap_or(section);
    let body = body.split("\n<details>").next().unwrap_or(body);
    body.lines().filter(|line| line.starts_with("- ")).count()
}

fn heading_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let NodeValue::Text(value) = &child.data.borrow().value {
            text.push_str(value);
        }
    }
    text
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

fn encoded(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        let code = u32::from(ch);
        let special = code <= 0x1F
            || (0x7F..=0x9F).contains(&code)
            || matches!(
                code,
                0x061C | 0x200E | 0x200F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
            );
        if special {
            let _ = write!(out, "\\u{{{code:04X}}}");
        } else {
            out.push(ch);
        }
    }
    out
}

fn contains_string(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(needle),
        serde_json::Value::Array(items) => items.iter().any(|item| contains_string(item, needle)),
        serde_json::Value::Object(map) => map.values().any(|item| contains_string(item, needle)),
        _ => false,
    }
}
