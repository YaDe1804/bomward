// SPDX-License-Identifier: AGPL-3.0-only

//! Policy load and evaluation.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Instant;

use bomward_core::Error;
use bomward_core::diff::{ChangeKind, Direction, diff};
use bomward_core::model::{
    ComponentKey, ContentHash, DependencyRole, Document, Version, VersionAttributes, VersionId,
    Warning, WarningKind,
};
use bomward_core::policy::{
    self, Decision, Mode, Outcome, Policy, PolicyError, PolicyLimits, Subject, Unit, Verdict,
};

fn load(text: &str) -> Result<Policy, Error> {
    policy::read_str(text, &PolicyLimits::default())
}

fn policy_err(text: &str) -> Result<PolicyError, String> {
    match load(text) {
        Err(Error::InvalidPolicy(error)) => Ok(error),
        Err(other) => Err(other.to_string()),
        Ok(_) => Err("policy was accepted".to_owned()),
    }
}

fn base(rules: &str) -> String {
    format!("version: 1\nmode: enforce\ndefault: allow\nrules:\n{rules}")
}

fn rule(body: &str) -> String {
    base(&format!("  - id: sample\n{body}"))
}

fn empty_document() -> Document {
    Document {
        components: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        warnings: Vec::new(),
    }
}

fn npm(name: &str) -> ComponentKey {
    ComponentKey::Purl {
        ty: "npm".to_owned(),
        namespace: None,
        name: name.to_owned(),
        qualifiers: BTreeMap::new(),
        subpath: None,
    }
}

fn scoped(namespace: &str, name: &str) -> ComponentKey {
    ComponentKey::Purl {
        ty: "npm".to_owned(),
        namespace: Some(namespace.to_owned()),
        name: name.to_owned(),
        qualifiers: BTreeMap::new(),
        subpath: None,
    }
}

fn name_key(group: &str, name: &str) -> ComponentKey {
    ComponentKey::Name {
        group: group.to_owned(),
        name: name.to_owned(),
    }
}

fn put(
    document: &mut Document,
    key: ComponentKey,
    version: &str,
    role: DependencyRole,
) -> VersionId {
    let id = VersionId {
        version: version.to_owned(),
        epoch: None,
    };
    document.components.entry(key).or_default().versions.insert(
        id.clone(),
        Version {
            attributes: VersionAttributes::default(),
            role,
        },
    );
    id
}

fn attrs<'a>(
    document: &'a mut Document,
    key: &ComponentKey,
    version: &str,
) -> Result<&'a mut VersionAttributes, String> {
    document
        .components
        .get_mut(key)
        .and_then(|component| {
            component.versions.get_mut(&VersionId {
                version: version.to_owned(),
                epoch: None,
            })
        })
        .map(|stored| &mut stored.attributes)
        .ok_or_else(|| format!("missing {version}"))
}

fn judge(text: &str, old: &Document, new: &Document) -> Result<Verdict, Error> {
    let policy = load(text)?;
    Ok(policy::evaluate(&policy, &diff(old, new)))
}

fn change_units(verdict: &Verdict) -> Vec<&Unit> {
    verdict
        .units
        .iter()
        .filter(|unit| matches!(unit.subject, Subject::Change { .. }))
        .collect()
}

fn unit_kind(verdict: &Verdict, kind: ChangeKind) -> Result<&Unit, String> {
    change_units(verdict)
        .into_iter()
        .find(|unit| matches!(unit.subject, Subject::Change { kind: found, .. } if found == kind))
        .ok_or_else(|| format!("no {kind:?}"))
}

#[test]
fn a_minimal_policy_loads() -> Result<(), Error> {
    let policy = load("version: 1\nmode: enforce\ndefault: allow\nrules: []\n")?;
    assert_eq!(policy.mode(), Mode::Enforce);
    assert_eq!(policy.default_decision(), Decision::Allow);
    Ok(())
}

#[test]
fn version_must_be_the_integer_one() -> Result<(), String> {
    let string = policy_err("version: \"1\"\nmode: enforce\ndefault: allow\nrules: []\n")?;
    assert!(matches!(string, PolicyError::Invalid { .. }), "{string}");
    let float = policy_err("version: 1.5\nmode: enforce\ndefault: allow\nrules: []\n")?;
    assert!(matches!(float, PolicyError::Invalid { .. }), "{float}");
    let other = policy_err("version: 2\nmode: enforce\ndefault: allow\nrules: []\n")?;
    assert!(other.to_string().contains("version"), "{other}");
    Ok(())
}

#[test]
fn required_fields_and_unknown_fields_are_errors() -> Result<(), String> {
    let missing = policy_err("version: 1\ndefault: allow\nrules: []\n")?;
    assert!(
        matches!(missing, PolicyError::Invalid { line: Some(_), .. }),
        "{missing}"
    );
    let unknown = policy_err("version: 1\nmode: enforce\ndefault: allow\nrules: []\nowner: ada\n")?;
    assert!(unknown.to_string().contains("line "), "{unknown}");
    let rule = policy_err(&rule(
        "    extra: 1\n    when:\n      change: [any]\n    decision: deny\n",
    ))?;
    assert!(rule.to_string().contains("line "), "{rule}");
    Ok(())
}

#[test]
fn duplicate_keys_and_duplicate_ids_are_errors() -> Result<(), String> {
    let keys = policy_err("version: 1\nmode: enforce\ndefault: allow\ndefault: deny\nrules: []\n")?;
    assert!(
        matches!(
            keys,
            PolicyError::Invalid {
                line: Some(_),
                column: Some(_),
                ..
            }
        ),
        "{keys}"
    );
    let ids = policy_err(&format!(
        "{}{}",
        rule("    when:\n      change: [any]\n    decision: allow\n"),
        "  - id: sample\n    when:\n      change: [added]\n    decision: deny\n"
    ))?;
    assert!(ids.to_string().contains("duplicate id"), "{ids}");
    Ok(())
}

#[test]
fn illegal_combinations_are_errors() -> Result<(), String> {
    let cases = [
        ("    when: {}\n    decision: deny\n", "no matcher"),
        (
            "    when:\n      change: [added]\n      document: [root-changed]\n    decision: deny\n",
            "both set",
        ),
        (
            "    when:\n      change: [any]\n      direction: [downgrade]\n    decision: deny\n",
            "version-changed",
        ),
        (
            "    when:\n      direction: [downgrade]\n    decision: deny\n",
            "version-changed",
        ),
        (
            "    when:\n      change: [any]\n      coverage: [all-dropped]\n    decision: deny\n",
            "hash-coverage-changed",
        ),
        (
            "    when:\n      change: [any, added]\n    decision: deny\n",
            "any cannot",
        ),
        (
            "    when:\n      document: [root-changed]\n    unless: [\"**\"]\n    decision: deny\n",
            "unless",
        ),
        ("    when:\n      change: []\n    decision: deny\n", "empty"),
        (
            "    when:\n      change: [sideways]\n    decision: deny\n",
            "unknown value",
        ),
        (
            "    when:\n      change: [added]\n      role: [root]\n    decision: deny\n",
            "unknown value",
        ),
        (
            "    when:\n      change: [license-changed]\n      license: [not-a-license]\n    decision: deny\n",
            "unknown value",
        ),
        (
            "    when:\n      package: [\"npm/*\"]\n    decision: deny\n",
            "pkg:",
        ),
    ];
    for (body, needle) in cases {
        let error = policy_err(&rule(body))?;
        assert!(
            error.to_string().contains(needle),
            "expected {needle} in {error} for {body}"
        );
    }
    let bad_id = policy_err(&base(
        "  - id: Nope\n    when:\n      change: [any]\n    decision: deny\n",
    ))?;
    assert!(bad_id.to_string().contains("rule id"), "{bad_id}");
    Ok(())
}

#[test]
fn description_list_rule_and_pattern_limits() -> Result<(), String> {
    let long = "a".repeat(501);
    let description = policy_err(&rule(&format!(
        "    description: \"{long}\"\n    when:\n      change: [any]\n    decision: allow\n"
    )))?;
    assert!(
        matches!(
            description,
            PolicyError::DescriptionTooLong {
                found: 501,
                limit: 500,
                ..
            }
        ),
        "{description}"
    );
    let ok_description = "a".repeat(500);
    load(&rule(&format!(
        "    description: \"{ok_description}\"\n    when:\n      change: [any]\n    decision: allow\n"
    )))
    .map_err(|error| error.to_string())?;

    let list = vec!["added"; 257];
    let joined = list.join(", ");
    let too_many = policy_err(&rule(&format!(
        "    when:\n      change: [{joined}]\n    decision: allow\n"
    )))?;
    assert!(
        matches!(
            too_many,
            PolicyError::ListTooLong {
                found: 257,
                limit: 256,
                ..
            }
        ),
        "{too_many}"
    );

    let mut rules = String::from("version: 1\nmode: enforce\ndefault: allow\nrules:\n");
    for index in 0..1001 {
        let _ = write!(
            rules,
            "  - id: r{index:04}\n    when:\n      change: [any]\n    decision: allow\n"
        );
    }
    let count = policy_err(&rules)?;
    assert!(
        matches!(
            count,
            PolicyError::TooManyRules {
                found: 1001,
                limit: 1000
            }
        ),
        "{count}"
    );
    rules.truncate(rules.rfind("  - id: r1000").ok_or("marker")?);
    load(&rules).map_err(|error| error.to_string())?;

    let pattern = format!("pkg:{}", "a".repeat(253));
    assert_eq!(pattern.chars().count(), 257);
    let long_pattern = policy_err(&rule(&format!(
        "    when:\n      package: [\"{pattern}\"]\n    decision: deny\n"
    )))?;
    assert!(
        matches!(
            long_pattern,
            PolicyError::PatternTooLong {
                found: 257,
                limit: 256
            }
        ),
        "{long_pattern}"
    );
    Ok(())
}

#[test]
fn file_size_utf8_control_characters_and_aliases() -> Result<(), String> {
    let mut exact = b"version: 1\nmode: enforce\ndefault: allow\nrules: []\n".to_vec();
    let limit = PolicyLimits::default().max_file_bytes;
    let limit_usize = usize::try_from(limit).map_err(|_| "limit")?;
    exact.resize(limit_usize, b' ');
    policy::read_bytes(&exact, &PolicyLimits::default()).map_err(|error| error.to_string())?;
    exact.push(b' ');
    let oversized = policy::read_bytes(&exact, &PolicyLimits::default());
    match oversized {
        Err(Error::InvalidPolicy(PolicyError::FileTooLarge { found, limit: cap })) => {
            assert_eq!(found, limit + 1);
            assert_eq!(cap, limit);
        }
        other => return Err(format!("{other:?}")),
    }

    let utf8 = policy::read_bytes(&[0xFF, 0xFE], &PolicyLimits::default());
    assert!(matches!(utf8, Err(Error::InvalidPolicy(PolicyError::Utf8))));

    let control = policy_err(
        "version: 1\nmode: enforce\ndefault: allow\nrules:\n  - id: \"a\\u0000b\"\n    when:\n      change: [any]\n    decision: deny\n",
    )?;
    assert!(
        !control.to_string().chars().any(char::is_control),
        "{control}"
    );

    let bomb = "\
a: &a [\"lol\", \"lol\", \"lol\", \"lol\", \"lol\", \"lol\", \"lol\", \"lol\", \"lol\"]
b: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c]
";
    let started = Instant::now();
    let alias = policy_err(bomb)?;
    let elapsed = started.elapsed();
    assert!(matches!(alias, PolicyError::Invalid { .. }), "{alias}");
    assert!(elapsed.as_secs() < 2, "alias expansion took {elapsed:?}");
    eprintln!("billion-laughs rejected in {elapsed:?}: {alias}");
    Ok(())
}

#[test]
fn license_and_hash_changes_inside_an_update_match() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let mut old = empty_document();
    let mut new = empty_document();
    put(&mut old, key.clone(), "1.0.0", DependencyRole::Direct);
    put(&mut new, key.clone(), "2.0.0", DependencyRole::Direct);
    attrs(&mut old, &key, "1.0.0")?
        .licenses
        .insert("MIT".to_owned());
    attrs(&mut new, &key, "2.0.0")?
        .licenses
        .insert("GPL-3.0-only".to_owned());
    attrs(&mut old, &key, "1.0.0")?.hashes.insert(ContentHash {
        alg: "SHA-256".to_owned(),
        content: "aaaa".to_owned(),
    });
    attrs(&mut new, &key, "2.0.0")?.hashes.insert(ContentHash {
        alg: "SHA-256".to_owned(),
        content: "bbbb".to_owned(),
    });
    let text = rule(
        "    when:\n      change: [license-changed]\n      license: [GPL-3.0-only]\n    decision: deny\n",
    );
    let verdict = judge(&text, &old, &new)?;
    let unit = unit_kind(&verdict, ChangeKind::VersionChanged)?;
    assert_eq!(unit.configured, Decision::Deny);
    assert_eq!(unit.rules, vec!["sample".to_owned()]);

    let hash = rule("    when:\n      change: [hash-changed]\n    decision: deny\n");
    let hashed = judge(&hash, &old, &new)?;
    assert_eq!(
        unit_kind(&hashed, ChangeKind::VersionChanged)?.configured,
        Decision::Deny
    );

    let removed_only = rule(
        "    when:\n      change: [license-changed]\n      license: [MIT]\n    decision: deny\n",
    );
    let missed = judge(&removed_only, &old, &new)?;
    assert!(unit_kind(&missed, ChangeKind::VersionChanged)?.used_default);
    Ok(())
}

#[test]
fn an_unknown_role_matches_the_configured_stand_in() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let old = empty_document();
    let mut new = empty_document();
    put(&mut new, key, "1.0.0", DependencyRole::Unknown);
    let direct =
        rule("    when:\n      change: [added]\n      role: [direct]\n    decision: deny\n");
    let direct_verdict = judge(&direct, &old, &new)?;
    assert_eq!(
        unit_kind(&direct_verdict, ChangeKind::Added)?.configured,
        Decision::Deny
    );

    let transitive = "version: 1\nmode: enforce\ndefault: allow\nunknown_role: transitive\nrules:\n  - id: sample\n    when:\n      change: [added]\n      role: [direct]\n    decision: deny\n";
    let transitive_verdict = judge(transitive, &old, &new)?;
    assert!(unit_kind(&transitive_verdict, ChangeKind::Added)?.used_default);

    let as_transitive = "version: 1\nmode: enforce\ndefault: allow\nunknown_role: transitive\nrules:\n  - id: sample\n    when:\n      change: [added]\n      role: [transitive]\n    decision: warn\n";
    let matched = judge(as_transitive, &old, &new)?;
    assert_eq!(
        unit_kind(&matched, ChangeKind::Added)?.configured,
        Decision::Warn
    );
    Ok(())
}

#[test]
fn license_tokens_do_not_match_a_longer_id() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("LGPL-3.0-only", "GPL-3.0-only", false),
        ("GPL-3.0-or-later", "GPL-3.0", false),
        ("GPL-3.0-only", "GPL-3.0-only", true),
        ("mit", "MIT", true),
        ("MIT OR GPL-3.0-only", "GPL-3.0-only", true),
        ("MIT AND GPL-3.0-only", "GPL-3.0-only", true),
        (
            "GPL-2.0-only WITH Classpath-exception-2.0",
            "GPL-2.0-only",
            true,
        ),
        ("not a license", "MIT", false),
    ];
    for (expression, id, expect) in cases {
        let key = npm("widget");
        let old = empty_document();
        let mut new = empty_document();
        put(&mut new, key.clone(), "1.0.0", DependencyRole::Direct);
        attrs(&mut new, &key, "1.0.0")?
            .licenses
            .insert(expression.to_owned());
        let text = rule(&format!(
            "    when:\n      change: [added]\n      license: [{id}]\n    decision: deny\n"
        ));
        let verdict = judge(&text, &old, &new)?;
        let matched = !unit_kind(&verdict, ChangeKind::Added)?.used_default;
        assert_eq!(matched, expect, "{expression} vs {id}");
    }

    let key = npm("widget");
    let mut old = empty_document();
    let mut new = empty_document();
    put(&mut old, key.clone(), "1.0.0", DependencyRole::Direct);
    put(&mut new, key.clone(), "1.0.0", DependencyRole::Direct);
    attrs(&mut old, &key, "1.0.0")?
        .licenses
        .insert("GPL-3.0-only".to_owned());
    attrs(&mut new, &key, "1.0.0")?
        .licenses
        .insert("MIT".to_owned());
    let text = rule(
        "    when:\n      change: [license-changed]\n      license: [GPL-3.0-only]\n    decision: deny\n",
    );
    let verdict = judge(&text, &old, &new)?;
    assert!(unit_kind(&verdict, ChangeKind::LicenseChanged)?.used_default);
    Ok(())
}

#[test]
fn package_patterns_use_the_canonical_purl() -> Result<(), Box<dyn std::error::Error>> {
    let old = empty_document();
    let mut new = empty_document();
    put(&mut new, npm("left-pad"), "1.0.0", DependencyRole::Direct);
    put(
        &mut new,
        scoped("@acme", "tool"),
        "1.0.0",
        DependencyRole::Direct,
    );
    put(
        &mut new,
        name_key("acme", "local"),
        "1.0.0",
        DependencyRole::Direct,
    );

    let star = rule(
        "    when:\n      change: [added]\n      package: [\"pkg:npm/*\"]\n    decision: deny\n",
    );
    let starred = judge(&star, &old, &new)?;
    let denied = change_units(&starred)
        .into_iter()
        .filter(|unit| unit.configured == Decision::Deny)
        .count();
    assert_eq!(denied, 1);

    let scope = rule(
        "    when:\n      change: [added]\n      package: [\"pkg:npm/%40acme/*\"]\n    decision: deny\n",
    );
    let scoped_verdict = judge(&scope, &old, &new)?;
    assert_eq!(
        change_units(&scoped_verdict)
            .into_iter()
            .filter(|unit| unit.configured == Decision::Deny)
            .count(),
        1
    );
    let raw_at = rule(
        "    when:\n      change: [added]\n      package: [\"pkg:npm/@acme/*\"]\n    decision: deny\n",
    );
    let raw = judge(&raw_at, &old, &new)?;
    assert!(change_units(&raw).iter().all(|unit| unit.used_default));

    let only_all =
        rule("    when:\n      change: [added]\n      package: [\"**\"]\n    decision: warn\n");
    let all = judge(&only_all, &old, &new)?;
    assert_eq!(
        change_units(&all)
            .into_iter()
            .filter(|unit| unit.configured == Decision::Warn)
            .count(),
        3
    );
    let pkg_glob =
        rule("    when:\n      change: [added]\n      package: [\"pkg:**\"]\n    decision: deny\n");
    let globs = judge(&pkg_glob, &old, &new)?;
    let name_units = change_units(&globs);
    let name_unit = name_units
        .iter()
        .find(|unit| {
            matches!(
                unit.subject,
                Subject::Change {
                    key: ComponentKey::Name { .. },
                    ..
                }
            )
        })
        .ok_or("name")?;
    assert!(name_unit.used_default);
    Ok(())
}

#[test]
fn deny_beats_allow_and_unless_is_local() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let old = empty_document();
    let mut new = empty_document();
    put(&mut new, key, "1.0.0", DependencyRole::Direct);
    let text = "\
version: 1
mode: enforce
default: allow
rules:
  - id: allow-it
    when:
      change: [added]
    decision: allow
  - id: deny-it
    when:
      change: [added]
    decision: deny
";
    let verdict = judge(text, &old, &new)?;
    let unit = unit_kind(&verdict, ChangeKind::Added)?;
    assert_eq!(unit.configured, Decision::Deny);
    assert_eq!(
        unit.rules,
        vec!["allow-it".to_owned(), "deny-it".to_owned()]
    );
    assert!(!unit.used_default);

    let except = "\
version: 1
mode: enforce
default: deny
rules:
  - id: deny-new
    when:
      change: [added]
    unless: [\"pkg:npm/widget\"]
    decision: deny
";
    let skipped = judge(except, &old, &new)?;
    let skipped_unit = unit_kind(&skipped, ChangeKind::Added)?;
    assert!(skipped_unit.used_default);
    assert_eq!(skipped_unit.configured, Decision::Deny);
    Ok(())
}

#[test]
fn warn_mode_keeps_the_configured_decision() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let old = empty_document();
    let mut new = empty_document();
    put(&mut new, key, "1.0.0", DependencyRole::Direct);
    let text = "\
version: 1
mode: warn
default: allow
rules:
  - id: deny-new
    when:
      change: [added]
    decision: deny
";
    let verdict = judge(text, &old, &new)?;
    let unit = unit_kind(&verdict, ChangeKind::Added)?;
    assert_eq!(unit.configured, Decision::Deny);
    assert_eq!(unit.effective, Decision::Warn);
    assert_eq!(verdict.configured, Outcome::Deny);
    assert_eq!(verdict.effective, Outcome::Warn);
    assert_eq!(verdict.exit_code(), 0);
    assert_eq!(verdict.counts.configured.deny, 1);
    assert_eq!(verdict.counts.effective.warn, 1);

    let enforced = text.replace("mode: warn", "mode: enforce");
    let hard = judge(&enforced, &old, &new)?;
    assert_eq!(hard.exit_code(), 1);
    assert_eq!(hard.effective, Outcome::Deny);
    Ok(())
}

#[test]
fn document_conditions_follow_a_fixed_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut old = empty_document();
    let mut new = empty_document();
    put(&mut new, npm("root"), "1.0.0", DependencyRole::Root);
    new.warnings.push(Warning {
        key: None,
        kind: WarningKind::NoComponents,
    });
    old.warnings.push(Warning {
        key: None,
        kind: WarningKind::RoleUnknown,
    });
    let text = "\
version: 1
mode: enforce
default: allow
rules:
  - id: no-graph
    when:
      document: [graph-unavailable, no-components, root-changed]
    decision: deny
";
    let verdict = judge(text, &old, &new)?;
    let conditions: Vec<_> = verdict
        .units
        .iter()
        .filter_map(|unit| match &unit.subject {
            Subject::Document { condition } => Some(condition.id()),
            Subject::Change { .. } => None,
        })
        .collect();
    assert_eq!(
        conditions,
        vec!["root-changed", "graph-unavailable", "no-components"]
    );
    assert!(
        verdict
            .units
            .iter()
            .take(3)
            .all(|unit| unit.configured == Decision::Deny)
    );
    assert_eq!(verdict.old_warnings.len(), 1);
    assert!(verdict.units.iter().all(|unit| match &unit.subject {
        Subject::Document { condition } => condition.id() != "role-unknown",
        Subject::Change { .. } => true,
    }));
    Ok(())
}

#[test]
fn rule_order_does_not_change_the_verdict() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let old = empty_document();
    let mut new = empty_document();
    put(&mut new, key.clone(), "1.0.0", DependencyRole::Direct);
    put(&mut new, npm("other"), "1.0.0", DependencyRole::Transitive);
    let first = "\
version: 1
mode: enforce
default: allow
rules:
  - id: beta
    when:
      change: [added]
      role: [transitive]
    decision: warn
  - id: alpha
    when:
      change: [added]
      role: [direct]
    decision: deny
";
    let second = "\
version: 1
mode: enforce
default: allow
rules:
  - id: alpha
    when:
      change: [added]
      role: [direct]
    decision: deny
  - id: beta
    when:
      change: [added]
      role: [transitive]
    decision: warn
";
    let left = judge(first, &old, &new)?;
    let right = judge(second, &old, &new)?;
    assert_eq!(left.to_canonical_json(), right.to_canonical_json());
    assert_eq!(change_units(&left).len(), 2);
    let bare = judge(
        "version: 1\nmode: enforce\ndefault: warn\nrules: []\n",
        &old,
        &new,
    )?;
    assert!(
        change_units(&bare)
            .iter()
            .all(|unit| unit.used_default && unit.configured == Decision::Warn)
    );
    Ok(())
}

#[test]
fn a_downgrade_and_a_missing_scope_match() -> Result<(), Box<dyn std::error::Error>> {
    let key = npm("widget");
    let mut old = empty_document();
    let mut new = empty_document();
    put(&mut old, key.clone(), "2.0.0", DependencyRole::Direct);
    put(&mut new, key.clone(), "1.0.0", DependencyRole::Direct);
    let text = rule(
        "    when:\n      change: [version-changed]\n      direction: [downgrade]\n    decision: deny\n",
    );
    let verdict = judge(&text, &old, &new)?;
    let unit = unit_kind(&verdict, ChangeKind::VersionChanged)?;
    assert_eq!(unit.configured, Decision::Deny);
    match &diff(&old, &new).changes.first().ok_or("change")?.detail {
        bomward_core::diff::ChangeDetail::VersionChanged { direction, .. } => {
            assert_eq!(*direction, Direction::Downgrade);
        }
        other => return Err(format!("{other:?}").into()),
    }

    let scope = rule(
        "    when:\n      change: [license-changed]\n      scope: [required]\n    decision: warn\n",
    );
    let mut old_license = empty_document();
    let mut new_license = empty_document();
    let scoped = npm("scoped");
    put(
        &mut old_license,
        scoped.clone(),
        "1.0.0",
        DependencyRole::Direct,
    );
    put(
        &mut new_license,
        scoped.clone(),
        "1.0.0",
        DependencyRole::Direct,
    );
    attrs(&mut old_license, &scoped, "1.0.0")?
        .licenses
        .insert("MIT".to_owned());
    attrs(&mut new_license, &scoped, "1.0.0")?
        .licenses
        .insert("Apache-2.0".to_owned());
    let scoped_verdict = judge(&scope, &old_license, &new_license)?;
    assert_eq!(
        unit_kind(&scoped_verdict, ChangeKind::LicenseChanged)?.configured,
        Decision::Warn
    );
    Ok(())
}

#[test]
fn one_hundred_thousand_changes_against_one_hundred_rules() -> Result<(), String> {
    let old = empty_document();
    let mut new = empty_document();
    for index in 0..100_000 {
        put(
            &mut new,
            npm(&format!("pkg{index}")),
            "1.0.0",
            DependencyRole::Direct,
        );
    }
    let mut text = String::from("version: 1\nmode: enforce\ndefault: allow\nrules:\n");
    for index in 0..100 {
        let _ = write!(
            text,
            "  - id: r{index:03}\n    when:\n      change: [added]\n      role: [direct]\n    decision: warn\n"
        );
    }
    let policy = load(&text).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let verdict = policy::evaluate(&policy, &diff(&old, &new));
    let elapsed = started.elapsed();
    assert_eq!(verdict.units.len(), 100_000);
    assert_eq!(verdict.counts.effective.warn, 100_000);
    eprintln!("load 100000 changes x 100 rules: {elapsed:?}");
    Ok(())
}
