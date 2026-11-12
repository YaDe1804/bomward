// SPDX-License-Identifier: AGPL-3.0-only

//! Compare two normalized documents.
//!
//! [`diff`] joins on component identity ([ADR 0005](https://github.com/YaDe1804/bomward/blob/main/docs/adr/0005-component-identity.md))
//! and never reads `bom-ref`. The change kinds and the pairing rule are
//! [ADR 0011](https://github.com/YaDe1804/bomward/blob/main/docs/adr/0011-change-model.md).
//! Direction is [ADR 0012](https://github.com/YaDe1804/bomward/blob/main/docs/adr/0012-version-ordering.md).
//!
//! The function cannot fail, does not panic, and does no I/O. Strings from
//! the documents are copied as data. Escaping belongs to the report.
//!
//! Memory stays within the two documents plus [`INTRODUCED_BY_LIMIT`] and
//! [`PATH_FROM_ROOT_LIMIT`]. The walk is a linear merge of the ordered maps.
//! Graph walks keep a visited set. Nothing in the result grows with input
//! order, and an ignored JSON field never reaches this module.
//!
//! [`Diff::to_canonical_json`] is a test encoding. It is **not** a public
//! contract: field names and key order may change. It is not a `CycloneDX`
//! document and it is not the report.
//!
//! # Examples
//!
//! ```
//! use bomward_core::diff::diff;
//! use bomward_core::Document;
//!
//! let document = Document {
//!     components: Default::default(),
//!     dependencies: Default::default(),
//!     warnings: Vec::new(),
//! };
//! let result = diff(&document, &document);
//! assert!(result.changes.is_empty());
//! ```

mod compare;
mod direction;
mod provenance;

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::model::{
    ComponentKey, DependencyRole, Document, GraphNode, VersionAttributes, VersionId, Warning,
    WarningKind,
};

/// Most nodes stored on `introduced_by`.
///
/// The entry also stores the real total, so a longer list is not dropped
/// silently. 32 is the default component-nesting depth.
pub const INTRODUCED_BY_LIMIT: usize = 32;

/// Most nodes stored on `path_from_root`, counting the root and the target.
///
/// The entry also stores the real length of the path.
pub const PATH_FROM_ROOT_LIMIT: usize = 32;

/// Compare `old` with `new`.
///
/// The same pair always produces the same [`Diff`]. `diff(document, document)`
/// has no changes and a zero summary. Warnings are still copied.
#[must_use]
pub fn diff(old: &Document, new: &Document) -> Diff {
    let roles_comparable = has_graph(old) && has_graph(new);
    let changes = compare::changes(old, new, roles_comparable);
    let summary = summarize(&changes);
    let root = root_change(old, new);
    Diff {
        changes,
        root,
        summary,
        roles_comparable,
        warnings: WarningSides {
            old: old.warnings.clone(),
            new: new.warnings.clone(),
        },
    }
}

/// One review, ready for policy and the report.
///
/// Changes are sorted by component key, then by version, then by
/// [`ChangeKind`]. That order does not depend on the input files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diff {
    /// Dependency changes. A root version is not in this list.
    pub changes: Vec<Change>,
    /// Old and new `metadata.component` when those snapshots differ.
    pub root: Option<RootChange>,
    /// Counts of [`Self::changes`]. The root entry is not counted.
    pub summary: Summary,
    /// Whether both documents have a graph, so roles may be compared.
    ///
    /// A document has a graph when it has a root and a `dependencies`
    /// field. Ingest records the opposite with [`crate::WarningKind::RoleUnknown`].
    /// When this is false, the diff has no `role-changed` entry and no
    /// role flag. `old_role` and `new_role` are still copied.
    pub roles_comparable: bool,
    /// Warnings copied from each document.
    pub warnings: WarningSides,
}

impl Diff {
    /// JSON encoding of this diff for golden tests.
    ///
    /// The same diff always produces the same bytes. The shape is internal.
    /// See the module docs.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// No dependency changes and no root change.
    ///
    /// Warnings can still be present. They are not changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.root.is_none()
    }
}

/// Warnings from the two documents, unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WarningSides {
    /// Warnings from the old document, in that document's order.
    pub old: Vec<Warning>,
    /// Warnings from the new document, in that document's order.
    pub new: Vec<Warning>,
}

/// The project component, when the two snapshots differ.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RootChange {
    /// Root in the old document, if it had one.
    pub old: Option<RootSnapshot>,
    /// Root in the new document, if it has one.
    pub new: Option<RootSnapshot>,
}

/// One `metadata.component` version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RootSnapshot {
    /// Component key of the root version.
    pub key: ComponentKey,
    /// Version id of the root version.
    pub version: VersionId,
    /// Role. This is [`DependencyRole::Root`] for the snapshot that was chosen.
    pub role: DependencyRole,
    /// Attributes of that version.
    pub attributes: VersionAttributes,
}

/// One dependency change.
///
/// `version` is the version the list is sorted by: the new version for
/// `added` and `version-changed`, the old version for `removed`, and the
/// shared version otherwise.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Change {
    /// Component the change belongs to.
    pub key: ComponentKey,
    /// Change kind.
    pub kind: ChangeKind,
    /// Version used for sorting. See the struct docs.
    pub version: VersionId,
    /// Version in the old document, when this change has one.
    pub old_version: Option<VersionId>,
    /// Version in the new document, when this change has one.
    pub new_version: Option<VersionId>,
    /// Role of the old version. Absent for `added`.
    pub old_role: Option<DependencyRole>,
    /// Role of the new version. Absent for `removed`.
    pub new_role: Option<DependencyRole>,
    /// Values the report needs for this kind.
    pub detail: ChangeDetail,
}

/// Change kinds from ADR 0011, in sort order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// A version exists only in the new document.
    Added,
    /// A version exists only in the old document.
    Removed,
    /// Exactly one version on each side, and they differ.
    VersionChanged,
    /// The license set differs for a version in both documents.
    LicenseChanged,
    /// The supplier set differs for a version in both documents.
    SupplierChanged,
    /// A shared algorithm has different content.
    HashChanged,
    /// An algorithm was added or removed, with no shared contradiction.
    HashCoverageChanged,
    /// The `CycloneDX` `scope` set differs.
    ScopeChanged,
    /// The graph role differs.
    RoleChanged,
}

impl ChangeKind {
    /// Stable identifier. This is the policy contract.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::VersionChanged => "version-changed",
            Self::LicenseChanged => "license-changed",
            Self::SupplierChanged => "supplier-changed",
            Self::HashChanged => "hash-changed",
            Self::HashCoverageChanged => "hash-coverage-changed",
            Self::ScopeChanged => "scope-changed",
            Self::RoleChanged => "role-changed",
        }
    }
}

/// Values carried with a [`Change`].
///
/// The surrounding [`Change::kind`] says which variant is present.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeDetail {
    /// A version that exists only in the new document.
    Added {
        /// Attributes of the new version.
        attributes: Box<VersionAttributes>,
        /// Nodes in the new document that directly depend on this version.
        ///
        /// At most [`INTRODUCED_BY_LIMIT`] nodes, in [`GraphNode`] order.
        /// [`BoundedNodes::total`] is the full count.
        introduced_by: BoundedNodes,
        /// One shortest path from the root to this version.
        ///
        /// Absent when the version is not reachable or the new document has
        /// no root. At most [`PATH_FROM_ROOT_LIMIT`] nodes. The total is the
        /// full path length, including the root and this version.
        path_from_root: Option<BoundedNodes>,
    },
    /// A version that exists only in the old document.
    Removed {
        /// Attributes of the old version.
        attributes: Box<VersionAttributes>,
    },
    /// One version replaced by one other version of the same key.
    ///
    /// Attribute and role differences are flags, not extra changes.
    VersionChanged {
        /// `upgrade`, `downgrade`, or `unknown`. Never a guess.
        direction: Direction,
        /// Attributes of the old version.
        old_attributes: Box<VersionAttributes>,
        /// Attributes of the new version.
        new_attributes: Box<VersionAttributes>,
        /// Differences a policy can match without a second row.
        flags: BTreeSet<AttributeFlag>,
    },
    /// Strings added to and removed from a set.
    ///
    /// Used for licenses, suppliers, and scopes.
    SetChanged {
        /// Strings present only in the new set.
        added: BTreeSet<String>,
        /// Strings present only in the old set.
        removed: BTreeSet<String>,
    },
    /// Shared algorithms whose content differs.
    HashChanged {
        /// One entry per algorithm, in algorithm-name order.
        algorithms: Vec<HashConflict>,
    },
    /// Algorithms that appeared or disappeared.
    HashCoverageChanged {
        /// Algorithm names present only in the new document.
        added: BTreeSet<String>,
        /// Algorithm names present only in the old document.
        removed: BTreeSet<String>,
        /// True when the new version has no hashes and the old version had some.
        all_dropped: bool,
    },
    /// The role changed. The roles themselves are on the [`Change`].
    RoleChanged,
}

/// A flag on [`ChangeDetail::VersionChanged`].
///
/// The names match the change kinds a staying version would have produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributeFlag {
    /// The license set differs.
    LicenseChanged,
    /// The supplier set differs.
    SupplierChanged,
    /// A shared algorithm has different content.
    HashChanged,
    /// An algorithm was added or removed.
    HashCoverageChanged,
    /// The `scope` set differs.
    ScopeChanged,
    /// The graph role differs.
    RoleChanged,
}

/// Direction of a [`ChangeKind::VersionChanged`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// The new version is newer by the rule in ADR 0012.
    Upgrade,
    /// The new version is older by the rule in ADR 0012.
    Downgrade,
    /// No order applies, or precedence is equal.
    Unknown,
}

/// One algorithm whose content is not the same on both sides.
///
/// Content strings are copied as they appear. Comparison still ignores
/// ASCII case, so a case-only difference does not produce this entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HashConflict {
    /// Algorithm name, compared exactly.
    pub algorithm: String,
    /// Contents in the old document.
    pub old_contents: BTreeSet<String>,
    /// Contents in the new document.
    pub new_contents: BTreeSet<String>,
}

/// A list capped at a named limit, plus the real total.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BoundedNodes {
    /// The first `limit` nodes. Shorter when the total is smaller.
    pub nodes: Vec<GraphNode>,
    /// How many nodes exist before the cap.
    pub total: usize,
}

/// Counts of each change kind.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ChangeCounts {
    /// [`ChangeKind::Added`].
    pub added: u64,
    /// [`ChangeKind::Removed`].
    pub removed: u64,
    /// [`ChangeKind::VersionChanged`].
    pub version_changed: u64,
    /// [`ChangeKind::LicenseChanged`].
    pub license_changed: u64,
    /// [`ChangeKind::SupplierChanged`].
    pub supplier_changed: u64,
    /// [`ChangeKind::HashChanged`].
    pub hash_changed: u64,
    /// [`ChangeKind::HashCoverageChanged`].
    pub hash_coverage_changed: u64,
    /// [`ChangeKind::ScopeChanged`].
    pub scope_changed: u64,
    /// [`ChangeKind::RoleChanged`].
    pub role_changed: u64,
}

impl ChangeCounts {
    fn add(&mut self, kind: ChangeKind) {
        let slot = match kind {
            ChangeKind::Added => &mut self.added,
            ChangeKind::Removed => &mut self.removed,
            ChangeKind::VersionChanged => &mut self.version_changed,
            ChangeKind::LicenseChanged => &mut self.license_changed,
            ChangeKind::SupplierChanged => &mut self.supplier_changed,
            ChangeKind::HashChanged => &mut self.hash_changed,
            ChangeKind::HashCoverageChanged => &mut self.hash_coverage_changed,
            ChangeKind::ScopeChanged => &mut self.scope_changed,
            ChangeKind::RoleChanged => &mut self.role_changed,
        };
        *slot = slot.saturating_add(1);
    }
}

/// How many changes of each kind, and the same counts split by policy role.
///
/// The policy role is the new role, except for `removed`, which uses the
/// old role. A root entry is not included.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Summary {
    /// One count per kind, across every role.
    pub counts: ChangeCounts,
    /// The same counts for each role that has at least one change.
    ///
    /// Sorted by [`DependencyRole`].
    pub by_role: Vec<RoleCount>,
}

/// Counts for one policy role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoleCount {
    /// The role the summary used for these changes.
    pub role: DependencyRole,
    /// Counts of changes that use this role.
    pub counts: ChangeCounts,
}

fn has_graph(document: &Document) -> bool {
    document
        .warnings
        .iter()
        .all(|warning| warning.kind != WarningKind::RoleUnknown)
}

fn summarize(changes: &[Change]) -> Summary {
    let mut counts = ChangeCounts::default();
    let mut by_role: BTreeMap<DependencyRole, ChangeCounts> = BTreeMap::new();
    for change in changes {
        counts.add(change.kind);
        if let Some(role) = policy_role(change) {
            by_role.entry(role).or_default().add(change.kind);
        }
    }
    Summary {
        counts,
        by_role: by_role
            .into_iter()
            .map(|(role, counts)| RoleCount { role, counts })
            .collect(),
    }
}

fn policy_role(change: &Change) -> Option<DependencyRole> {
    if change.kind == ChangeKind::Removed {
        change.old_role
    } else {
        change.new_role
    }
}

fn root_change(old: &Document, new: &Document) -> Option<RootChange> {
    let old_root = compare::root_snapshot(old);
    let new_root = compare::root_snapshot(new);
    if old_root == new_root {
        None
    } else {
        Some(RootChange {
            old: old_root,
            new: new_root,
        })
    }
}
