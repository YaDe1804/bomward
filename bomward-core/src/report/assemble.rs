// SPDX-License-Identifier: AGPL-3.0-only

//! Build one view of a diff and an optional verdict.
//!
//! The three formats render this view. They do not walk the diff again.

use std::collections::BTreeMap;

use crate::diff::{Change, ChangeCounts, ChangeKind, Diff};
use crate::model::{ComponentKey, DependencyRole, VersionId, Warning, WarningKind};
use crate::policy::{Decision, DocumentCondition, Mode, Subject, Unit, Verdict};

use super::explain::{
    change_phrases, condition_phrases, human_change_phrases, human_root_phrases, root_phrases,
};
use super::json::{
    ChangeCountsJson, CountsJson, DecisionCountsJson, DetailJson, GateJson, JsonReport, ToolJson,
    UnitJson, UnitParts, WarningsJson, change_detail, decision_id, outcome_id, raw_message,
    root_json, warning_json,
};
use super::text::{Identity, PackageLabel, Phrase, counted, identity, package_label, role_id};

/// How many packages a human warning line names.
pub(crate) const WARNING_PACKAGES_LISTED: usize = 8;

/// The exact report-only sentence required by ADR 0007.
pub(crate) const REPORT_ONLY_NOTICE: &str = "No policy was applied.";

pub(crate) struct Prepared {
    pub(crate) lines: Vec<Line>,
    pub(crate) groups: Vec<Group>,
    pub(crate) notes: Vec<Vec<Phrase>>,
    pub(crate) json: JsonReport,
}

pub(crate) struct Group {
    /// `Denied`, `Warnings`, `Allowed`, or `Changes`.
    pub(crate) title: &'static str,
    pub(crate) entries: Vec<Entry>,
}

pub(crate) struct Entry {
    pub(crate) heading: Vec<Phrase>,
    pub(crate) phrases: Vec<Phrase>,
    pub(crate) rules: Vec<Phrase>,
}

pub(crate) enum Line {
    Text(String),
}

pub(crate) fn prepare(diff: &Diff, verdict: Option<&Verdict>, version: &'static str) -> Prepared {
    let weakened = verdict.map_or(0, weakened_denials);
    let exit_code = verdict.map_or(0, Verdict::exit_code);
    let mode = verdict.map(|verdict| verdict.mode);
    let entries = match verdict {
        Some(verdict) => judged_entries(diff, verdict),
        None => unjudged_entries(diff),
    };
    let units = entries.iter().map(|built| built.unit.clone()).collect();
    let groups = groups_of(&entries, verdict.is_some());
    let notes = notes_of(diff);
    let lines = header_lines(diff, verdict, weakened, exit_code, version);
    let decisions = verdict.map_or(
        DecisionCountsJson {
            deny: 0,
            warn: 0,
            allow: 0,
        },
        |verdict| DecisionCountsJson {
            deny: verdict.counts.effective.deny,
            warn: verdict.counts.effective.warn,
            allow: verdict.counts.effective.allow,
        },
    );
    Prepared {
        lines,
        groups,
        notes,
        json: JsonReport {
            report_version: super::json::REPORT_VERSION,
            tool: ToolJson {
                name: "bomward",
                version,
            },
            policy_applied: verdict.is_some(),
            notice: verdict.is_none().then_some(REPORT_ONLY_NOTICE),
            gate: GateJson {
                mode: mode_id(mode),
                configured: verdict.map(|verdict| outcome_id(verdict.configured)),
                effective: verdict.map(|verdict| outcome_id(verdict.effective)),
                weakened_denials: weakened,
                exit_code,
            },
            counts: CountsJson {
                decisions,
                changes: ChangeCountsJson::from(&diff.summary.counts),
            },
            units,
            root: diff.root.as_ref().map(root_json),
            roles_comparable: diff.roles_comparable,
            warnings: WarningsJson {
                old: diff.warnings.old.iter().map(warning_json).collect(),
                new: diff.warnings.new.iter().map(warning_json).collect(),
            },
        },
    }
}

struct Built {
    rank: u8,
    order: usize,
    group_title: &'static str,
    entry: Entry,
    unit: UnitJson,
}

struct Slot {
    rank: u8,
    order: usize,
    group_title: &'static str,
}

fn judged_entries(diff: &Diff, verdict: &Verdict) -> Vec<Built> {
    let changes = change_index(diff);
    let mut built = Vec::new();
    for (order, unit) in verdict.units.iter().enumerate() {
        built.push(judged_entry(diff, &changes, unit, order));
    }
    built.sort_by_key(|entry| (entry.rank, entry.order));
    built
}

fn change_index(diff: &Diff) -> BTreeMap<(&ComponentKey, &VersionId, ChangeKind), &Change> {
    let mut index = BTreeMap::new();
    for change in &diff.changes {
        index
            .entry((&change.key, &change.version, change.kind))
            .or_insert(change);
    }
    index
}

fn judged_entry<'a>(
    diff: &'a Diff,
    changes: &BTreeMap<(&'a ComponentKey, &'a VersionId, ChangeKind), &'a Change>,
    unit: &'a Unit,
    order: usize,
) -> Built {
    let rank = decision_rank(unit.effective);
    let group_title = match unit.effective {
        Decision::Deny => "Denied",
        Decision::Warn => "Warnings",
        Decision::Allow => "Allowed",
    };
    match &unit.subject {
        Subject::Change { key, version, kind } => {
            let change = changes.get(&(key, version, *kind)).copied();
            change_built(
                change,
                key,
                version,
                *kind,
                Some(unit),
                Slot {
                    rank,
                    order,
                    group_title,
                },
            )
        }
        Subject::Document { condition } => document_built(
            diff,
            *condition,
            Some(unit),
            Slot {
                rank,
                order,
                group_title,
            },
        ),
    }
}

fn unjudged_entries(diff: &Diff) -> Vec<Built> {
    let mut built = Vec::new();
    if diff.root.is_some() {
        built.push(document_built(
            diff,
            DocumentCondition::RootChanged,
            None,
            Slot {
                rank: 3,
                order: 0,
                group_title: "Changes",
            },
        ));
    }
    for (index, change) in diff.changes.iter().enumerate() {
        built.push(change_built(
            Some(change),
            &change.key,
            &change.version,
            change.kind,
            None,
            Slot {
                rank: 3,
                order: index + 1,
                group_title: "Changes",
            },
        ));
    }
    built
}

fn change_built(
    change: Option<&Change>,
    key: &ComponentKey,
    version: &VersionId,
    kind: ChangeKind,
    unit: Option<&Unit>,
    slot: Slot,
) -> Built {
    let label = package_label(key, version);
    let id = identity(key, Some(version));
    let role = change.map(reviewed_role);
    let spoken = change.map_or_else(
        || vec![Phrase::Text("Change is missing from the diff.".to_owned())],
        change_phrases,
    );
    let phrases = change.map_or_else(|| spoken.clone(), human_change_phrases);
    let detail = change.map_or(DetailJson::Condition, change_detail);
    finish(Finish {
        label: &label,
        kind: Some(kind.id()),
        role,
        phrases,
        spoken,
        id: &id,
        json_kind: Some(kind.id().to_owned()),
        condition: None,
        detail,
        unit,
        slot,
        subject: "change",
    })
}

fn document_built(
    diff: &Diff,
    condition: DocumentCondition,
    unit: Option<&Unit>,
    slot: Slot,
) -> Built {
    let spoken = if condition == DocumentCondition::RootChanged {
        diff.root.as_ref().map_or_else(
            || vec![Phrase::Text("The project component changed.".to_owned())],
            root_phrases,
        )
    } else {
        condition_phrases(condition.id(), warning_kind(condition))
    };
    let phrases = if condition == DocumentCondition::RootChanged {
        diff.root
            .as_ref()
            .map_or_else(|| spoken.clone(), human_root_phrases)
    } else {
        spoken.clone()
    };
    let detail = if condition == DocumentCondition::RootChanged {
        DetailJson::RootChanged
    } else {
        DetailJson::Condition
    };
    let label = PackageLabel {
        ecosystem: None,
        name: "project component".to_owned(),
        version: String::new(),
        qualifiers: Vec::new(),
        subpath: None,
    };
    let id = Identity {
        purl: None,
        ecosystem: None,
        namespace: None,
        name: None,
        group: None,
        qualifiers: Vec::new(),
        subpath: None,
        version: None,
        epoch: None,
    };
    finish(Finish {
        label: &label,
        kind: Some(condition.id()),
        role: None,
        phrases,
        spoken,
        id: &id,
        json_kind: None,
        condition: Some(condition.id().to_owned()),
        detail,
        unit,
        slot,
        subject: "document",
    })
}

struct Finish<'a> {
    label: &'a PackageLabel,
    kind: Option<&'a str>,
    role: Option<DependencyRole>,
    phrases: Vec<Phrase>,
    spoken: Vec<Phrase>,
    id: &'a Identity,
    json_kind: Option<String>,
    condition: Option<String>,
    detail: DetailJson,
    unit: Option<&'a Unit>,
    slot: Slot,
    subject: &'static str,
}

fn finish(input: Finish<'_>) -> Built {
    let Finish {
        label,
        kind,
        role,
        phrases,
        spoken,
        id,
        json_kind,
        condition,
        detail,
        unit,
        slot,
        subject,
    } = input;
    let heading = heading_phrases(unit.map(|unit| unit.effective), label, kind, role, subject);
    let rules = unit.map_or_else(Vec::new, |unit| {
        rules_phrases(&unit.rules, unit.used_default)
    });
    let configured = unit.map(|unit| decision_id(unit.configured));
    let effective = unit.map(|unit| decision_id(unit.effective));
    let rules_vec = unit.map_or_else(Vec::new, |unit| unit.rules.clone());
    let used_default = unit.is_some_and(|unit| unit.used_default);
    let message = raw_message(&spoken);
    Built {
        rank: slot.rank,
        order: slot.order,
        group_title: slot.group_title,
        entry: Entry {
            heading,
            phrases,
            rules,
        },
        unit: UnitJson::from_parts(UnitParts {
            subject,
            identity: Some(id),
            kind: json_kind,
            condition,
            role: role.map(role_id),
            configured,
            effective,
            rules: rules_vec,
            used_default,
            message,
            detail,
        }),
    }
}

fn heading_phrases(
    decision: Option<Decision>,
    label: &PackageLabel,
    kind: Option<&str>,
    role: Option<DependencyRole>,
    subject: &str,
) -> Vec<Phrase> {
    let mut phrases = Vec::new();
    if let Some(decision) = decision {
        phrases.push(Phrase::Text(
            super::json::decision_label(decision).to_owned(),
        ));
        phrases.push(Phrase::Text(" ".to_owned()));
    }
    if subject == "document" && kind == Some("root-changed") {
        phrases.push(Phrase::Text("project component".to_owned()));
    } else if subject != "document" {
        if let Some(ecosystem) = &label.ecosystem {
            phrases.push(Phrase::Value(ecosystem.clone()));
            phrases.push(Phrase::Text(" ".to_owned()));
        }
        phrases.push(Phrase::Value(label.name.clone()));
        phrases.push(Phrase::Text("@".to_owned()));
        phrases.push(Phrase::Value(label.version.clone()));
        for (qualifier, value) in &label.qualifiers {
            phrases.push(Phrase::Text(" ".to_owned()));
            phrases.push(Phrase::Value(qualifier.clone()));
            phrases.push(Phrase::Text("=".to_owned()));
            phrases.push(Phrase::Value(value.clone()));
        }
        if let Some(subpath) = &label.subpath {
            phrases.push(Phrase::Text("#".to_owned()));
            phrases.push(Phrase::Value(subpath.clone()));
        }
    }
    if let Some(kind) = kind {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.push(Phrase::Text(kind.to_owned()));
    }
    if let Some(role) = role {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.push(Phrase::Text(role_id(role).to_owned()));
    }
    phrases
}

fn rules_phrases(rules: &[String], used_default: bool) -> Vec<Phrase> {
    if used_default {
        return vec![Phrase::Text("Decision from default.".to_owned())];
    }
    let mut phrases = vec![Phrase::Text("Rules: ".to_owned())];
    for (index, rule) in rules.iter().enumerate() {
        if index > 0 {
            phrases.push(Phrase::Text(", ".to_owned()));
        }
        phrases.push(Phrase::Value(rule.clone()));
    }
    phrases.push(Phrase::Text(".".to_owned()));
    phrases
}

fn groups_of(entries: &[Built], judged: bool) -> Vec<Group> {
    let titles: &[&str] = if judged {
        &["Denied", "Warnings", "Allowed"]
    } else {
        &["Changes"]
    };
    titles
        .iter()
        .filter_map(|title| {
            let group: Vec<Entry> = entries
                .iter()
                .filter(|entry| entry.group_title == *title)
                .map(|entry| Entry {
                    heading: entry.entry.heading.clone(),
                    phrases: entry.entry.phrases.clone(),
                    rules: entry.entry.rules.clone(),
                })
                .collect();
            if group.is_empty() {
                None
            } else {
                Some(Group {
                    title,
                    entries: group,
                })
            }
        })
        .collect()
}

fn header_lines(
    diff: &Diff,
    verdict: Option<&Verdict>,
    weakened: u64,
    exit_code: u8,
    version: &str,
) -> Vec<Line> {
    let mut lines = vec![Line::Text(format!("bomward {version}"))];
    match verdict {
        None => lines.push(Line::Text(REPORT_ONLY_NOTICE.to_owned())),
        Some(verdict) => {
            lines.push(Line::Text(format!(
                "Result: {}",
                super::json::outcome_label(verdict.effective)
            )));
            if weakened > 0 {
                lines.push(Line::Text(weakened_sentence(weakened)));
            }
        }
    }
    lines.push(Line::Text(format!("Exit code: {exit_code}")));
    lines.push(Line::Text(format!(
        "Mode: {}",
        mode_id(verdict.map(|verdict| verdict.mode))
    )));
    if let Some(verdict) = verdict {
        lines.push(Line::Text(format!(
            "Decisions: {}, {}, {}",
            counted(verdict.counts.effective.deny, "denial", "denials"),
            counted(verdict.counts.effective.warn, "warning", "warnings"),
            counted(verdict.counts.effective.allow, "allowed", "allowed"),
        )));
    }
    lines.push(Line::Text(change_line(&diff.summary.counts)));
    lines
}

fn change_line(counts: &ChangeCounts) -> String {
    let parts = [
        (counts.added, "added"),
        (counts.removed, "removed"),
        (counts.version_changed, "version-changed"),
        (counts.license_changed, "license-changed"),
        (counts.supplier_changed, "supplier-changed"),
        (counts.hash_changed, "hash-changed"),
        (counts.hash_coverage_changed, "hash-coverage-changed"),
        (counts.scope_changed, "scope-changed"),
        (counts.role_changed, "role-changed"),
    ];
    let shown: Vec<String> = parts
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| format!("{count} {name}"))
        .collect();
    if shown.is_empty() {
        "No dependency changes.".to_owned()
    } else {
        format!("Changes: {}", shown.join(", "))
    }
}

fn notes_of(diff: &Diff) -> Vec<Vec<Phrase>> {
    let mut notes = Vec::new();
    if !diff.roles_comparable {
        notes.push(vec![Phrase::Text(
            "Roles were not compared because at least one document has no dependency graph."
                .to_owned(),
        )]);
    }
    if let Some(note) = warning_note("new", &diff.warnings.new) {
        notes.push(note);
    }
    if let Some(note) = warning_note("old", &diff.warnings.old) {
        notes.push(note);
    }
    notes
}

fn warning_note(side: &str, warnings: &[Warning]) -> Option<Vec<Phrase>> {
    if warnings.is_empty() {
        return None;
    }
    let mut grouped: BTreeMap<WarningKind, Vec<&Warning>> = BTreeMap::new();
    for warning in warnings {
        grouped.entry(warning.kind).or_default().push(warning);
    }
    let mut phrases = vec![Phrase::Text(format!("Warnings in the {side} document: "))];
    for (index, (kind, group)) in grouped.iter().enumerate() {
        if index > 0 {
            phrases.push(Phrase::Text(", ".to_owned()));
        }
        let count = u64::try_from(group.len()).unwrap_or(u64::MAX);
        phrases.push(Phrase::Text(counted(
            count,
            &format!("{id} warning", id = kind.id()),
            &format!("{id} warnings", id = kind.id()),
        )));
        phrases.push(Phrase::Text(" (".to_owned()));
        let shown = group.len().min(WARNING_PACKAGES_LISTED);
        for (package_index, warning) in group.iter().take(shown).enumerate() {
            if package_index > 0 {
                phrases.push(Phrase::Text(", ".to_owned()));
            }
            match warning.key.as_ref() {
                Some(key) => {
                    let id = identity(key, None);
                    if let Some(ecosystem) = id.ecosystem {
                        phrases.push(Phrase::Value(ecosystem));
                        phrases.push(Phrase::Text(" ".to_owned()));
                    }
                    if let Some(name) = id.name {
                        phrases.push(Phrase::Value(name));
                    }
                }
                None => phrases.push(Phrase::Text("no package".to_owned())),
            }
        }
        if group.len() > shown {
            let more = group.len() - shown;
            phrases.push(Phrase::Text(format!(
                ", and {}",
                counted(u64::try_from(more).unwrap_or(u64::MAX), "more", "more")
            )));
        }
        phrases.push(Phrase::Text(")".to_owned()));
    }
    phrases.push(Phrase::Text(".".to_owned()));
    Some(phrases)
}

fn weakened_denials(verdict: &Verdict) -> u64 {
    verdict
        .units
        .iter()
        .filter(|unit| unit.configured == Decision::Deny && unit.effective == Decision::Warn)
        .count()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn weakened_sentence(count: u64) -> String {
    if count == 1 {
        "1 denial reported as a warning because the policy runs in warn mode".to_owned()
    } else {
        format!("{count} denials reported as warnings because the policy runs in warn mode")
    }
}

fn decision_rank(decision: Decision) -> u8 {
    match decision {
        Decision::Deny => 0,
        Decision::Warn => 1,
        Decision::Allow => 2,
    }
}

fn reviewed_role(change: &Change) -> DependencyRole {
    let role = if change.kind == ChangeKind::Removed {
        change.old_role
    } else {
        change.new_role
    };
    role.unwrap_or(DependencyRole::Unknown)
}

fn mode_id(mode: Option<Mode>) -> &'static str {
    match mode {
        None => "report-only",
        Some(Mode::Warn) => "warn",
        Some(Mode::Enforce) => "enforce",
    }
}

fn warning_kind(condition: DocumentCondition) -> Option<WarningKind> {
    match condition {
        DocumentCondition::InvalidPurl => Some(WarningKind::InvalidPurl),
        DocumentCondition::OrphanRef => Some(WarningKind::OrphanRef),
        DocumentCondition::DuplicateBomRef => Some(WarningKind::DuplicateBomRef),
        DocumentCondition::RoleUnknown => Some(WarningKind::RoleUnknown),
        DocumentCondition::ConflictingAttributes => Some(WarningKind::ConflictingAttributes),
        DocumentCondition::VersionMismatch => Some(WarningKind::VersionMismatch),
        DocumentCondition::DuplicateKey => Some(WarningKind::DuplicateKey),
        DocumentCondition::Unreachable => Some(WarningKind::Unreachable),
        DocumentCondition::NoComponents => Some(WarningKind::NoComponents),
        DocumentCondition::UnknownHashAlgorithm => Some(WarningKind::UnknownHashAlgorithm),
        DocumentCondition::RootChanged | DocumentCondition::GraphUnavailable => None,
    }
}
