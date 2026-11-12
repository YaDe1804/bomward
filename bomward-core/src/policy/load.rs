// SPDX-License-Identifier: AGPL-3.0-only

//! Read a version-1 policy.
//!
//! Aliases are rejected (`max_alias_expansions` is 0). Duplicate keys are
//! errors. Unknown fields are errors. The file is not a SBOM: these are
//! mistakes in configuration, and the load fails instead of warning.

use std::collections::{BTreeSet, HashSet};

use serde::Deserialize;

use noyalib::{ParserConfig, from_str_with_config};

use crate::diff::{ChangeKind, Direction};
use crate::model::WarningKind;

use super::error::{PolicyError, sanitize};
use super::license::canonical_license;
use super::limits::PolicyLimits;
use super::pattern::{Pattern, PatternError};
use super::schema::{
    ChangeFilter, ChangeRule, CoverageKind, Decision, DocumentCondition, MissingAttr, Mode,
    RoleMatch, Rule, RuleBody, ScopeMatch, UnknownRole,
};

/// A policy that has already passed validation.
#[derive(Clone, Debug)]
pub struct Policy {
    mode: Mode,
    default_decision: Decision,
    unknown_role: UnknownRole,
    rules: Vec<Rule>,
}

impl Policy {
    /// `mode` from the file.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// `default` from the file.
    #[must_use]
    pub fn default_decision(&self) -> Decision {
        self.default_decision
    }

    /// `description` of a rule, when the file has one.
    #[must_use]
    pub fn description(&self, id: &str) -> Option<&str> {
        self.rules
            .iter()
            .find(|rule| rule.id == id)
            .and_then(|rule| rule.description.as_deref())
    }

    pub(crate) fn unknown_role(&self) -> UnknownRole {
        self.unknown_role
    }

    pub(crate) fn rules(&self) -> &[Rule] {
        &self.rules
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    version: u64,
    mode: Mode,
    #[serde(rename = "default")]
    default_decision: Decision,
    #[serde(default)]
    unknown_role: Option<UnknownRole>,
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    #[serde(default)]
    description: Option<String>,
    when: RawWhen,
    #[serde(default)]
    unless: Option<Vec<String>>,
    decision: Decision,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWhen {
    #[serde(default)]
    change: Option<Vec<String>>,
    #[serde(default)]
    direction: Option<Vec<String>>,
    #[serde(default)]
    role: Option<Vec<String>>,
    #[serde(default)]
    license: Option<Vec<String>>,
    #[serde(default)]
    missing: Option<Vec<String>>,
    #[serde(default)]
    coverage: Option<Vec<String>>,
    #[serde(default)]
    scope: Option<Vec<String>>,
    #[serde(default)]
    package: Option<Vec<String>>,
    #[serde(default)]
    document: Option<Vec<String>>,
}

pub(crate) fn parse(text: &str, limits: &PolicyLimits) -> Result<Policy, PolicyError> {
    let config = parser_config(limits);
    let raw: RawPolicy = from_str_with_config(text, &config).map_err(|err| yaml_error(&err))?;
    compile(raw, limits)
}

fn parser_config(limits: &PolicyLimits) -> ParserConfig {
    let document = usize::try_from(limits.max_file_bytes).unwrap_or(usize::MAX);
    // The parser's own sequence cap must sit above our rule and list limits,
    // so a file that is one past the limit still parses and the error can
    // name that limit and the count. The file-size cap bounds the input.
    ParserConfig::strict()
        .max_alias_expansions(0)
        .max_document_length(document)
        .max_sequence_length(1_000_000)
}

fn yaml_error(err: &noyalib::Error) -> PolicyError {
    let location = err.location();
    PolicyError::Invalid {
        line: location.as_ref().map(noyalib::Location::line),
        column: location.as_ref().map(noyalib::Location::column),
        message: sanitize(&err.to_string()),
    }
}

fn compile(raw: RawPolicy, limits: &PolicyLimits) -> Result<Policy, PolicyError> {
    if raw.version != 1 {
        return Err(invalid(format!(
            "policy field version: expected 1, found {}",
            raw.version
        )));
    }
    if raw.rules.len() > limits.max_rules {
        return Err(PolicyError::TooManyRules {
            found: raw.rules.len(),
            limit: limits.max_rules,
        });
    }
    let mut seen = HashSet::new();
    let mut rules = Vec::with_capacity(raw.rules.len());
    for rule in raw.rules {
        rules.push(compile_rule(rule, limits, &mut seen)?);
    }
    Ok(Policy {
        mode: raw.mode,
        default_decision: raw.default_decision,
        unknown_role: raw.unknown_role.unwrap_or_default(),
        rules,
    })
}

fn compile_rule(
    raw: RawRule,
    limits: &PolicyLimits,
    seen: &mut HashSet<String>,
) -> Result<Rule, PolicyError> {
    if !valid_id(&raw.id) {
        return Err(invalid(format!(
            "policy rule id {} does not match [a-z0-9][a-z0-9-]{{0,63}}",
            sanitize(&raw.id)
        )));
    }
    if !seen.insert(raw.id.clone()) {
        return Err(invalid(format!("policy rule {}: duplicate id", raw.id)));
    }
    if let Some(description) = &raw.description {
        let found = description.chars().count();
        if found > limits.max_description_chars {
            return Err(PolicyError::DescriptionTooLong {
                rule: raw.id,
                found,
                limit: limits.max_description_chars,
            });
        }
    }
    let body = compile_body(&raw, limits)?;
    Ok(Rule {
        id: raw.id,
        description: raw.description,
        decision: raw.decision,
        body,
    })
}

fn compile_body(raw: &RawRule, limits: &PolicyLimits) -> Result<RuleBody, PolicyError> {
    let when = &raw.when;
    let has_document = when.document.is_some();
    let has_change = when.change.is_some()
        || when.direction.is_some()
        || when.role.is_some()
        || when.license.is_some()
        || when.missing.is_some()
        || when.coverage.is_some()
        || when.scope.is_some()
        || when.package.is_some();
    if has_document && has_change {
        return Err(rule_message(
            &raw.id,
            "when",
            "change matchers and document are both set",
        ));
    }
    if !has_document && !has_change {
        return Err(rule_message(&raw.id, "when", "no matcher is set"));
    }
    if has_document {
        if raw.unless.is_some() {
            return Err(rule_message(
                &raw.id,
                "unless",
                "unless is only valid on a change rule",
            ));
        }
        let Some(document) = required_list(&raw.id, "document", when.document.as_ref(), limits)?
        else {
            return Err(rule_message(&raw.id, "when", "no matcher is set"));
        };
        return Ok(RuleBody::Document(compile_document(&raw.id, document)?));
    }
    Ok(RuleBody::Change(Box::new(compile_change(raw, limits)?)))
}

fn compile_change(raw: &RawRule, limits: &PolicyLimits) -> Result<ChangeRule, PolicyError> {
    let when = &raw.when;
    let changes = compile_changes(&raw.id, when.change.as_ref(), limits)?;
    let directions = optional_mapped(
        &raw.id,
        "direction",
        when.direction.as_ref(),
        limits,
        parse_direction,
    )?;
    if directions.is_some() && !filter_contains(&changes, ChangeKind::VersionChanged) {
        return Err(rule_message(
            &raw.id,
            "direction",
            "direction requires change to list version-changed",
        ));
    }
    let coverage = optional_mapped(
        &raw.id,
        "coverage",
        when.coverage.as_ref(),
        limits,
        parse_coverage,
    )?;
    if coverage.is_some() && !filter_contains(&changes, ChangeKind::HashCoverageChanged) {
        return Err(rule_message(
            &raw.id,
            "coverage",
            "coverage requires change to list hash-coverage-changed",
        ));
    }
    Ok(ChangeRule {
        changes,
        roles: optional_mapped(&raw.id, "role", when.role.as_ref(), limits, parse_role)?,
        directions,
        licenses: compile_licenses(&raw.id, when.license.as_ref(), limits)?,
        missing: optional_mapped(
            &raw.id,
            "missing",
            when.missing.as_ref(),
            limits,
            parse_missing,
        )?,
        coverage,
        scopes: optional_mapped(&raw.id, "scope", when.scope.as_ref(), limits, parse_scope)?,
        packages: compile_patterns(&raw.id, "package", when.package.as_ref(), limits)?,
        unless: compile_patterns(&raw.id, "unless", raw.unless.as_ref(), limits)?
            .unwrap_or_default(),
    })
}

fn filter_contains(filter: &ChangeFilter, kind: ChangeKind) -> bool {
    match filter {
        ChangeFilter::Any => false,
        ChangeFilter::Kinds(kinds) => kinds.contains(&kind),
    }
}

fn compile_changes(
    rule: &str,
    values: Option<&Vec<String>>,
    limits: &PolicyLimits,
) -> Result<ChangeFilter, PolicyError> {
    let Some(values) = required_list(rule, "change", values, limits)? else {
        return Ok(ChangeFilter::Any);
    };
    if values.len() == 1 && values.first() == Some(&"any".to_owned()) {
        return Ok(ChangeFilter::Any);
    }
    if values.iter().any(|value| value == "any") {
        return Err(rule_message(
            rule,
            "change",
            "any cannot be combined with another value",
        ));
    }
    let mut kinds = BTreeSet::new();
    for value in values {
        let Some(kind) = change_kind(value) else {
            return Err(unknown_value(rule, "change", value));
        };
        kinds.insert(kind);
    }
    Ok(ChangeFilter::Kinds(kinds))
}

fn compile_licenses(
    rule: &str,
    values: Option<&Vec<String>>,
    limits: &PolicyLimits,
) -> Result<Option<Vec<&'static str>>, PolicyError> {
    let Some(values) = required_list(rule, "license", values, limits)? else {
        return Ok(None);
    };
    let mut licenses = Vec::with_capacity(values.len());
    for value in values {
        let Some(canonical) = canonical_license(value) else {
            return Err(unknown_value(rule, "license", value));
        };
        licenses.push(canonical);
    }
    Ok(Some(licenses))
}

fn compile_patterns(
    rule: &str,
    field: &'static str,
    values: Option<&Vec<String>>,
    limits: &PolicyLimits,
) -> Result<Option<Vec<Pattern>>, PolicyError> {
    let Some(values) = required_list(rule, field, values, limits)? else {
        return Ok(None);
    };
    let mut patterns = Vec::with_capacity(values.len());
    for value in values {
        let found = value.chars().count();
        if found > limits.max_pattern_chars {
            return Err(PolicyError::PatternTooLong {
                found,
                limit: limits.max_pattern_chars,
            });
        }
        match Pattern::compile(value) {
            Ok(pattern) => patterns.push(pattern),
            Err(PatternError::Prefix) => {
                return Err(rule_message(
                    rule,
                    field,
                    &format!("pattern {} must start with pkg: or be **", sanitize(value)),
                ));
            }
        }
    }
    Ok(Some(patterns))
}

fn compile_document(
    rule: &str,
    values: &[String],
) -> Result<BTreeSet<DocumentCondition>, PolicyError> {
    let mut conditions = BTreeSet::new();
    for value in values {
        let Some(condition) = document_condition(value) else {
            return Err(unknown_value(rule, "document", value));
        };
        conditions.insert(condition);
    }
    Ok(conditions)
}

fn required_list<'a>(
    rule: &str,
    field: &'static str,
    values: Option<&'a Vec<String>>,
    limits: &PolicyLimits,
) -> Result<Option<&'a [String]>, PolicyError> {
    let Some(values) = values else {
        return Ok(None);
    };
    if values.is_empty() {
        return Err(rule_message(rule, field, "the list is empty"));
    }
    if values.len() > limits.max_list_entries {
        return Err(PolicyError::ListTooLong {
            rule: rule.to_owned(),
            field,
            found: values.len(),
            limit: limits.max_list_entries,
        });
    }
    Ok(Some(values))
}

fn optional_mapped<T: Ord>(
    rule: &str,
    field: &'static str,
    values: Option<&Vec<String>>,
    limits: &PolicyLimits,
    parse: fn(&str) -> Option<T>,
) -> Result<Option<BTreeSet<T>>, PolicyError> {
    let Some(values) = required_list(rule, field, values, limits)? else {
        return Ok(None);
    };
    let mut mapped = BTreeSet::new();
    for value in values {
        let Some(item) = parse(value) else {
            return Err(unknown_value(rule, field, value));
        };
        mapped.insert(item);
    }
    Ok(Some(mapped))
}

fn change_kind(value: &str) -> Option<ChangeKind> {
    Some(match value {
        "added" => ChangeKind::Added,
        "removed" => ChangeKind::Removed,
        "version-changed" => ChangeKind::VersionChanged,
        "license-changed" => ChangeKind::LicenseChanged,
        "supplier-changed" => ChangeKind::SupplierChanged,
        "hash-changed" => ChangeKind::HashChanged,
        "hash-coverage-changed" => ChangeKind::HashCoverageChanged,
        "scope-changed" => ChangeKind::ScopeChanged,
        "role-changed" => ChangeKind::RoleChanged,
        _ => return None,
    })
}

fn parse_direction(value: &str) -> Option<Direction> {
    Some(match value {
        "upgrade" => Direction::Upgrade,
        "downgrade" => Direction::Downgrade,
        "unknown" => Direction::Unknown,
        _ => return None,
    })
}

fn parse_role(value: &str) -> Option<RoleMatch> {
    Some(match value {
        "direct" => RoleMatch::Direct,
        "transitive" => RoleMatch::Transitive,
        "unknown" => RoleMatch::Unknown,
        _ => return None,
    })
}

fn parse_missing(value: &str) -> Option<MissingAttr> {
    Some(match value {
        "license" => MissingAttr::License,
        "hash" => MissingAttr::Hash,
        "supplier" => MissingAttr::Supplier,
        _ => return None,
    })
}

fn parse_coverage(value: &str) -> Option<CoverageKind> {
    Some(match value {
        "algorithm-added" => CoverageKind::AlgorithmAdded,
        "algorithm-removed" => CoverageKind::AlgorithmRemoved,
        "all-dropped" => CoverageKind::AllDropped,
        _ => return None,
    })
}

fn parse_scope(value: &str) -> Option<ScopeMatch> {
    Some(match value {
        "required" => ScopeMatch::Required,
        "optional" => ScopeMatch::Optional,
        "excluded" => ScopeMatch::Excluded,
        _ => return None,
    })
}

fn document_condition(value: &str) -> Option<DocumentCondition> {
    Some(match value {
        "root-changed" => DocumentCondition::RootChanged,
        "graph-unavailable" => DocumentCondition::GraphUnavailable,
        "invalid-purl" => DocumentCondition::from_warning(WarningKind::InvalidPurl),
        "orphan-ref" => DocumentCondition::from_warning(WarningKind::OrphanRef),
        "duplicate-bom-ref" => DocumentCondition::from_warning(WarningKind::DuplicateBomRef),
        "role-unknown" => DocumentCondition::from_warning(WarningKind::RoleUnknown),
        "conflicting-attributes" => {
            DocumentCondition::from_warning(WarningKind::ConflictingAttributes)
        }
        "version-mismatch" => DocumentCondition::from_warning(WarningKind::VersionMismatch),
        "duplicate-key" => DocumentCondition::from_warning(WarningKind::DuplicateKey),
        "unreachable" => DocumentCondition::from_warning(WarningKind::Unreachable),
        "no-components" => DocumentCondition::from_warning(WarningKind::NoComponents),
        "unknown-hash-algorithm" => {
            DocumentCondition::from_warning(WarningKind::UnknownHashAlgorithm)
        }
        _ => return None,
    })
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let mut len = 1usize;
    for ch in chars {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' {
            return false;
        }
        len += 1;
        if len > 64 {
            return false;
        }
    }
    true
}

fn invalid(message: String) -> PolicyError {
    PolicyError::Invalid {
        line: None,
        column: None,
        message,
    }
}

fn rule_message(rule: &str, field: &str, detail: &str) -> PolicyError {
    invalid(format!("policy rule {rule} field {field}: {detail}"))
}

fn unknown_value(rule: &str, field: &str, value: &str) -> PolicyError {
    rule_message(rule, field, &format!("unknown value {}", sanitize(value)))
}
