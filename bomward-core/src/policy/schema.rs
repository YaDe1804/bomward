// SPDX-License-Identifier: AGPL-3.0-only

//! Compiled policy. The YAML shape is private to the loader.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::diff::{ChangeKind, Direction};
use crate::model::WarningKind;

use super::pattern::Pattern;

/// `mode` from the policy file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A configured `deny` is reported as `warn`. The exit code stays 0.
    Warn,
    /// A configured `deny` stays `deny`. The exit code is 1.
    Enforce,
}

/// One rule's decision, and the decision on one verdict unit.
///
/// `deny` is stricter than `warn`, which is stricter than `allow`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The unit may pass.
    Allow,
    /// The unit is reported and does not fail the run.
    Warn,
    /// The unit fails the run when the mode is `enforce`.
    Deny,
}

impl Decision {
    pub(crate) fn stricter(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::Warn => 1,
            Self::Deny => 2,
        }
    }

    /// The mode's effective decision.
    ///
    /// `warn` turns `deny` into `warn`. `enforce` keeps the decision.
    pub(crate) fn effective(self, mode: Mode) -> Self {
        match (mode, self) {
            (Mode::Warn, Self::Deny) => Self::Warn,
            _ => self,
        }
    }
}

/// How an `unknown` role is read. The default is [`UnknownRole::Direct`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UnknownRole {
    #[default]
    Direct,
    Transitive,
}

/// One compiled rule.
#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub(crate) id: String,
    pub(crate) description: Option<String>,
    pub(crate) decision: Decision,
    pub(crate) body: RuleBody,
}

/// Change matchers, or document conditions. Never both.
#[derive(Clone, Debug)]
pub(crate) enum RuleBody {
    /// Matchers combined with AND. Lists inside a matcher are OR.
    Change(Box<ChangeRule>),
    /// Document conditions. The list is OR.
    Document(BTreeSet<DocumentMatch>),
}

/// A change rule after validation.
#[derive(Clone, Debug)]
pub(crate) struct ChangeRule {
    pub(crate) changes: ChangeFilter,
    pub(crate) roles: Option<BTreeSet<RoleMatch>>,
    pub(crate) directions: Option<BTreeSet<Direction>>,
    pub(crate) licenses: Option<Vec<&'static str>>,
    pub(crate) license_status: Option<BTreeSet<LicenseStatus>>,
    pub(crate) missing: Option<BTreeSet<MissingAttr>>,
    pub(crate) coverage: Option<BTreeSet<CoverageKind>>,
    pub(crate) scopes: Option<BTreeSet<ScopeMatch>>,
    pub(crate) packages: Option<Vec<Pattern>>,
    pub(crate) unless: Vec<Pattern>,
}

/// Which change kinds a rule names.
#[derive(Clone, Debug)]
pub(crate) enum ChangeFilter {
    /// `change: [any]`, or a change rule that does not list `change`.
    Any,
    /// The listed kinds. Flag kinds also match a version update that carries the flag.
    Kinds(BTreeSet<ChangeKind>),
}

/// A role a rule may name. `root` is not a value here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum RoleMatch {
    Direct,
    Transitive,
    Unknown,
}

/// `license_status` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum LicenseStatus {
    /// The string is neither an SPDX id nor a valid SPDX expression.
    Unrecognized,
}

/// `missing` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum MissingAttr {
    License,
    Hash,
    Supplier,
}

/// `coverage` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum CoverageKind {
    AlgorithmAdded,
    AlgorithmRemoved,
    AllDropped,
}

/// `scope` values. An empty scope set on the component is `required`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ScopeMatch {
    Required,
    Optional,
    Excluded,
}

/// One document condition a rule can name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentCondition {
    /// `metadata.component` differs.
    RootChanged,
    /// At least one document has no dependency graph.
    GraphUnavailable,
    /// A warning id from the new document.
    InvalidPurl,
    /// `orphan-ref`.
    OrphanRef,
    /// `duplicate-bom-ref`.
    DuplicateBomRef,
    /// `role-unknown`.
    RoleUnknown,
    /// `conflicting-attributes`.
    ConflictingAttributes,
    /// `version-mismatch`.
    VersionMismatch,
    /// `duplicate-key`.
    DuplicateKey,
    /// `unreachable`.
    Unreachable,
    /// `no-components`.
    NoComponents,
    /// `unknown-hash-algorithm`.
    UnknownHashAlgorithm,
}

impl DocumentCondition {
    /// Policy id. Warning variants use the id from ADR 0010.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::RootChanged => "root-changed",
            Self::GraphUnavailable => "graph-unavailable",
            Self::InvalidPurl => WarningKind::InvalidPurl.id(),
            Self::OrphanRef => WarningKind::OrphanRef.id(),
            Self::DuplicateBomRef => WarningKind::DuplicateBomRef.id(),
            Self::RoleUnknown => WarningKind::RoleUnknown.id(),
            Self::ConflictingAttributes => WarningKind::ConflictingAttributes.id(),
            Self::VersionMismatch => WarningKind::VersionMismatch.id(),
            Self::DuplicateKey => WarningKind::DuplicateKey.id(),
            Self::Unreachable => WarningKind::Unreachable.id(),
            Self::NoComponents => WarningKind::NoComponents.id(),
            Self::UnknownHashAlgorithm => WarningKind::UnknownHashAlgorithm.id(),
        }
    }

    pub(crate) const fn from_warning(kind: WarningKind) -> Self {
        match kind {
            WarningKind::InvalidPurl => Self::InvalidPurl,
            WarningKind::OrphanRef => Self::OrphanRef,
            WarningKind::DuplicateBomRef => Self::DuplicateBomRef,
            WarningKind::RoleUnknown => Self::RoleUnknown,
            WarningKind::ConflictingAttributes => Self::ConflictingAttributes,
            WarningKind::VersionMismatch => Self::VersionMismatch,
            WarningKind::DuplicateKey => Self::DuplicateKey,
            WarningKind::Unreachable => Self::Unreachable,
            WarningKind::NoComponents => Self::NoComponents,
            WarningKind::UnknownHashAlgorithm => Self::UnknownHashAlgorithm,
        }
    }
}

/// Document condition stored on a rule. Same ids as [`DocumentCondition`].
pub(crate) type DocumentMatch = DocumentCondition;
