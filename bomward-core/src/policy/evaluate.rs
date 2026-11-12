// SPDX-License-Identifier: AGPL-3.0-only

//! Judge a diff against a loaded policy.
//!
//! Evaluation cannot fail and does no I/O. Rule order is not an input:
//! the strictest matching decision wins, and every matching id is kept,
//! sorted. [`Verdict::to_canonical_json`] is a test encoding. It is not a
//! public contract.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::diff::{
    AttributeFlag, Change, ChangeDetail, ChangeKind, Diff, effective_scopes, hash_coverage,
};
use crate::model::{ComponentKey, DependencyRole, VersionAttributes, VersionId, Warning};
use crate::purl::canonical_purl;

use super::license::analyze_license;
use super::load::Policy;
use super::pattern::Pattern;
use super::schema::{
    ChangeFilter, ChangeRule, CoverageKind, Decision, DocumentCondition, LicenseStatus,
    MissingAttr, Mode, RoleMatch, Rule, RuleBody, ScopeMatch, UnknownRole,
};

/// Overall result of a verdict.
///
/// `allow` on every unit is `pass`. Otherwise the strictest decision is
/// `warn` or `deny`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Every unit is `allow`, or there are no units.
    Pass,
    /// The strictest effective decision is `warn`.
    Warn,
    /// The strictest effective decision is `deny`.
    Deny,
}

/// One judged change or document condition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Unit {
    /// The change or the document condition.
    pub subject: Subject,
    /// Decision before `mode` is applied.
    pub configured: Decision,
    /// Decision after `mode` is applied.
    pub effective: Decision,
    /// Every matching rule id, sorted. Empty when `used_default` is set.
    pub rules: Vec<String>,
    /// True when no rule matched and `default` was used.
    pub used_default: bool,
}

/// How many units carry each decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnitCounts {
    /// Counts of the configured decisions.
    pub configured: DecisionCounts,
    /// Counts of the effective decisions.
    pub effective: DecisionCounts,
}

/// `allow`, `warn`, and `deny` totals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DecisionCounts {
    /// Units whose decision is `allow`.
    pub allow: u64,
    /// Units whose decision is `warn`.
    pub warn: u64,
    /// Units whose decision is `deny`.
    pub deny: u64,
}

impl UnitCounts {
    fn from_units(units: &[Unit]) -> Self {
        Self {
            configured: DecisionCounts::from_decisions(units.iter().map(|unit| unit.configured)),
            effective: DecisionCounts::from_decisions(units.iter().map(|unit| unit.effective)),
        }
    }
}

impl DecisionCounts {
    fn from_decisions(decisions: impl Iterator<Item = Decision>) -> Self {
        let mut counts = Self {
            allow: 0,
            warn: 0,
            deny: 0,
        };
        for decision in decisions {
            match decision {
                Decision::Allow => counts.allow = counts.allow.saturating_add(1),
                Decision::Warn => counts.warn = counts.warn.saturating_add(1),
                Decision::Deny => counts.deny = counts.deny.saturating_add(1),
            }
        }
        counts
    }
}

/// The policy's judgment of one diff.
///
/// [`Verdict::to_canonical_json`] is deterministic and it is not a public
/// contract. Field names may change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Verdict {
    /// `mode` from the policy file.
    pub mode: Mode,
    /// Strictest configured decision, as [`Outcome`].
    pub configured: Outcome,
    /// Strictest effective decision, as [`Outcome`].
    pub effective: Outcome,
    /// Document conditions, then changes in diff order.
    pub units: Vec<Unit>,
    /// Totals for configured and effective decisions.
    pub counts: UnitCounts,
    /// Warnings from the old document. They are not units in v0.1.
    pub old_warnings: Vec<Warning>,
}

impl Verdict {
    /// Exit code for this verdict.
    ///
    /// `1` when the effective result is `deny`, and `0` otherwise. A
    /// `warn` mode that turned `deny` into `warn` returns `0`. The CLI
    /// applies this in a later work package.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.effective {
            Outcome::Deny => 1,
            Outcome::Pass | Outcome::Warn => 0,
        }
    }

    /// JSON encoding for golden tests.
    ///
    /// The same verdict always produces the same bytes. The shape is
    /// internal. See the module docs.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Judge `diff` with `policy`.
///
/// Every dependency change is one unit, in diff order. Document conditions
/// come first: `root-changed`, then `graph-unavailable`, then warning ids
/// from the new document in the order of ADR 0010. Warnings from the old
/// document are copied onto the verdict and are not units.
#[must_use]
pub fn evaluate(policy: &Policy, diff: &Diff) -> Verdict {
    let mut units = Vec::new();
    for condition in document_conditions(diff) {
        units.push(judge_document(policy, condition));
    }
    for change in &diff.changes {
        units.push(judge_change(policy, change));
    }
    let counts = UnitCounts::from_units(&units);
    Verdict {
        mode: policy.mode(),
        configured: fold(units.iter().map(|unit| unit.configured)),
        effective: fold(units.iter().map(|unit| unit.effective)),
        units,
        counts,
        old_warnings: diff.warnings.old.clone(),
    }
}

fn document_conditions(diff: &Diff) -> Vec<DocumentCondition> {
    let mut conditions = Vec::new();
    if diff.root.is_some() {
        conditions.push(DocumentCondition::RootChanged);
    }
    if !diff.roles_comparable {
        conditions.push(DocumentCondition::GraphUnavailable);
    }
    let mut kinds = BTreeSet::new();
    for warning in &diff.warnings.new {
        kinds.insert(warning.kind);
    }
    for kind in kinds {
        conditions.push(DocumentCondition::from_warning(kind));
    }
    conditions
}

fn judge_document(policy: &Policy, condition: DocumentCondition) -> Unit {
    let (configured, rules, used_default) = decide(policy, |rule| match &rule.body {
        RuleBody::Document(conditions) => conditions.contains(&condition),
        RuleBody::Change(_) => false,
    });
    finish(
        policy.mode(),
        Subject::Document { condition },
        configured,
        rules,
        used_default,
    )
}

fn judge_change(policy: &Policy, change: &Change) -> Unit {
    let licenses = license_view(change);
    let (configured, rules, used_default) = decide(policy, |rule| match &rule.body {
        RuleBody::Change(body) => change_matches(body, change, policy.unknown_role(), &licenses),
        RuleBody::Document(_) => false,
    });
    finish(
        policy.mode(),
        Subject::Change {
            key: change.key.clone(),
            version: change.version.clone(),
            kind: change.kind,
        },
        configured,
        rules,
        used_default,
    )
}

fn decide(
    policy: &Policy,
    mut matches: impl FnMut(&Rule) -> bool,
) -> (Decision, Vec<String>, bool) {
    let mut rules = BTreeSet::new();
    let mut strictest: Option<Decision> = None;
    for rule in policy.rules() {
        if !matches(rule) {
            continue;
        }
        rules.insert(rule.id.clone());
        strictest = Some(match strictest {
            Some(current) => current.stricter(rule.decision),
            None => rule.decision,
        });
    }
    match strictest {
        Some(decision) => (decision, rules.into_iter().collect(), false),
        None => (policy.default_decision(), Vec::new(), true),
    }
}

fn finish(
    mode: Mode,
    subject: Subject,
    configured: Decision,
    rules: Vec<String>,
    used_default: bool,
) -> Unit {
    Unit {
        subject,
        configured,
        effective: configured.effective(mode),
        rules,
        used_default,
    }
}

fn fold(decisions: impl Iterator<Item = Decision>) -> Outcome {
    let worst = decisions.fold(Decision::Allow, Decision::stricter);
    match worst {
        Decision::Allow => Outcome::Pass,
        Decision::Warn => Outcome::Warn,
        Decision::Deny => Outcome::Deny,
    }
}

fn change_matches(
    rule: &ChangeRule,
    change: &Change,
    unknown_role: UnknownRole,
    licenses: &LicenseView,
) -> bool {
    if !kind_matches(&rule.changes, change) {
        return false;
    }
    if let Some(directions) = &rule.directions {
        let ChangeDetail::VersionChanged { direction, .. } = &change.detail else {
            return false;
        };
        if !directions.contains(direction) {
            return false;
        }
    }
    if let Some(roles) = &rule.roles
        && !role_matches(roles, reviewed_role(change), unknown_role)
    {
        return false;
    }
    if let Some(wanted) = &rule.licenses
        && !license_matches(wanted, licenses)
    {
        return false;
    }
    if let Some(status) = &rule.license_status
        && !status_matches(status, licenses, change)
    {
        return false;
    }
    if let Some(missing) = &rule.missing
        && !missing
            .iter()
            .any(|attr| attribute_missing(*attr, &change.judged))
    {
        return false;
    }
    if let Some(coverage) = &rule.coverage
        && !coverage_matches(coverage, change)
    {
        return false;
    }
    if let Some(scopes) = &rule.scopes
        && !scope_matches(scopes, &change.judged)
    {
        return false;
    }
    if let Some(packages) = &rule.packages
        && !packages
            .iter()
            .any(|pattern| package_matches(pattern, &change.key))
    {
        return false;
    }
    !rule
        .unless
        .iter()
        .any(|pattern| package_matches(pattern, &change.key))
}

fn kind_matches(filter: &ChangeFilter, change: &Change) -> bool {
    match filter {
        ChangeFilter::Any => true,
        ChangeFilter::Kinds(kinds) => {
            kinds.contains(&change.kind)
                || flagged_kinds(change)
                    .iter()
                    .any(|kind| kinds.contains(kind))
        }
    }
}

fn flagged_kinds(change: &Change) -> BTreeSet<ChangeKind> {
    let ChangeDetail::VersionChanged { flags, .. } = &change.detail else {
        return BTreeSet::new();
    };
    flags.iter().map(|flag| flag_kind(*flag)).collect()
}

fn flag_kind(flag: AttributeFlag) -> ChangeKind {
    match flag {
        AttributeFlag::LicenseChanged => ChangeKind::LicenseChanged,
        AttributeFlag::SupplierChanged => ChangeKind::SupplierChanged,
        AttributeFlag::HashChanged => ChangeKind::HashChanged,
        AttributeFlag::HashCoverageChanged => ChangeKind::HashCoverageChanged,
        AttributeFlag::ScopeChanged => ChangeKind::ScopeChanged,
        AttributeFlag::RoleChanged => ChangeKind::RoleChanged,
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

fn role_matches(
    wanted: &BTreeSet<RoleMatch>,
    role: DependencyRole,
    unknown_role: UnknownRole,
) -> bool {
    let stand_in = match unknown_role {
        UnknownRole::Direct => RoleMatch::Direct,
        UnknownRole::Transitive => RoleMatch::Transitive,
    };
    match role {
        DependencyRole::Direct => wanted.contains(&RoleMatch::Direct),
        DependencyRole::Transitive => wanted.contains(&RoleMatch::Transitive),
        DependencyRole::Unknown => {
            wanted.contains(&RoleMatch::Unknown) || wanted.contains(&stand_in)
        }
        DependencyRole::Root => false,
    }
}

struct LicenseView {
    ids: BTreeSet<&'static str>,
    unrecognized: bool,
}

fn license_view(change: &Change) -> LicenseView {
    let mut ids = BTreeSet::new();
    let mut unrecognized = false;
    for text in considered_licenses(change) {
        let facts = analyze_license(text);
        if !facts.recognized {
            unrecognized = true;
        }
        ids.extend(facts.ids);
    }
    LicenseView { ids, unrecognized }
}

fn license_matches(wanted: &[&str], licenses: &LicenseView) -> bool {
    wanted.iter().any(|id| licenses.ids.contains(id))
}

fn status_matches(
    wanted: &BTreeSet<LicenseStatus>,
    licenses: &LicenseView,
    change: &Change,
) -> bool {
    wanted.contains(&LicenseStatus::Unrecognized)
        && licenses.unrecognized
        && license_status_applies(change)
}

fn license_status_applies(change: &Change) -> bool {
    match change.kind {
        ChangeKind::Added | ChangeKind::LicenseChanged => true,
        ChangeKind::VersionChanged => matches!(
            &change.detail,
            ChangeDetail::VersionChanged { flags, .. }
                if flags.contains(&AttributeFlag::LicenseChanged)
        ),
        ChangeKind::Removed
        | ChangeKind::SupplierChanged
        | ChangeKind::HashChanged
        | ChangeKind::HashCoverageChanged
        | ChangeKind::ScopeChanged
        | ChangeKind::RoleChanged => false,
    }
}

fn considered_licenses(change: &Change) -> Vec<&str> {
    if license_delta_only(change) {
        added_licenses(change)
    } else {
        change.judged.licenses.iter().map(String::as_str).collect()
    }
}

fn license_delta_only(change: &Change) -> bool {
    match change.kind {
        ChangeKind::LicenseChanged => true,
        ChangeKind::VersionChanged => match &change.detail {
            ChangeDetail::VersionChanged { flags, .. } => {
                flags.contains(&AttributeFlag::LicenseChanged)
            }
            _ => false,
        },
        _ => false,
    }
}

fn added_licenses(change: &Change) -> Vec<&str> {
    match (&change.kind, &change.detail) {
        (ChangeKind::LicenseChanged, ChangeDetail::SetChanged { added, .. }) => {
            added.iter().map(String::as_str).collect()
        }
        (
            ChangeKind::VersionChanged,
            ChangeDetail::VersionChanged {
                old_attributes,
                new_attributes,
                ..
            },
        ) => new_attributes
            .licenses
            .difference(&old_attributes.licenses)
            .map(String::as_str)
            .collect(),
        _ => Vec::new(),
    }
}

fn attribute_missing(attr: MissingAttr, judged: &VersionAttributes) -> bool {
    match attr {
        MissingAttr::License => judged.licenses.is_empty(),
        MissingAttr::Hash => judged.hashes.is_empty(),
        MissingAttr::Supplier => judged.suppliers.is_empty(),
    }
}

fn coverage_matches(wanted: &BTreeSet<CoverageKind>, change: &Change) -> bool {
    let facts = coverage_facts(change);
    wanted.iter().any(|kind| match kind {
        CoverageKind::AlgorithmAdded => facts.added,
        CoverageKind::AlgorithmRemoved => facts.removed,
        CoverageKind::AllDropped => facts.all_dropped,
    })
}

struct CoverageFacts {
    added: bool,
    removed: bool,
    all_dropped: bool,
}

fn coverage_facts(change: &Change) -> CoverageFacts {
    match &change.detail {
        ChangeDetail::HashCoverageChanged {
            added,
            removed,
            all_dropped,
        } => CoverageFacts {
            added: !added.is_empty(),
            removed: !removed.is_empty(),
            all_dropped: *all_dropped,
        },
        ChangeDetail::VersionChanged {
            old_attributes,
            new_attributes,
            ..
        } => {
            let coverage = hash_coverage(&old_attributes.hashes, &new_attributes.hashes);
            CoverageFacts {
                added: !coverage.added.is_empty(),
                removed: !coverage.removed.is_empty(),
                all_dropped: coverage.all_dropped,
            }
        }
        _ => CoverageFacts {
            added: false,
            removed: false,
            all_dropped: false,
        },
    }
}

fn scope_matches(wanted: &BTreeSet<ScopeMatch>, judged: &VersionAttributes) -> bool {
    let effective = effective_scopes(&judged.scopes);
    wanted
        .iter()
        .any(|scope| effective.contains(scope_word(*scope)))
}

fn scope_word(scope: ScopeMatch) -> &'static str {
    match scope {
        ScopeMatch::Required => "required",
        ScopeMatch::Optional => "optional",
        ScopeMatch::Excluded => "excluded",
    }
}

fn package_matches(pattern: &Pattern, key: &ComponentKey) -> bool {
    match canonical_purl(key) {
        Some(purl) => pattern.matches(&purl),
        None => pattern.universal,
    }
}

/// What one verdict unit refers to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Subject {
    /// One dependency change.
    Change {
        /// Component key.
        key: ComponentKey,
        /// Version used to sort the change.
        version: VersionId,
        /// Change kind.
        kind: ChangeKind,
    },
    /// One document condition.
    Document {
        /// `root-changed`, `graph-unavailable`, or a warning id.
        condition: DocumentCondition,
    },
}
