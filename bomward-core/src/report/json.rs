// SPDX-License-Identifier: AGPL-3.0-only

//! JSON report. Field order is the order of these structs.
//!
//! Values are the original SBOM strings. This format does not encode
//! controls a second time and it does not cut long values.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::diff::{
    AttributeFlag, BoundedNodes, Change, ChangeCounts, ChangeDetail, Direction, HashConflict,
    RootChange, RootSnapshot,
};
use crate::model::{VersionAttributes, Warning};
use crate::policy::{Decision, Outcome};

use super::text::{Identity, Phrase, identity, role_id};

/// `report_version` of this document.
pub const REPORT_VERSION: u32 = 1;

#[derive(Clone, Serialize)]
pub(crate) struct JsonReport {
    pub(crate) report_version: u32,
    pub(crate) tool: ToolJson,
    pub(crate) policy_applied: bool,
    pub(crate) notice: Option<&'static str>,
    pub(crate) gate: GateJson,
    pub(crate) counts: CountsJson,
    pub(crate) units: Vec<UnitJson>,
    pub(crate) root: Option<RootJson>,
    pub(crate) roles_comparable: bool,
    pub(crate) warnings: WarningsJson,
}

#[derive(Clone, Serialize)]
pub(crate) struct ToolJson {
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct GateJson {
    pub(crate) mode: &'static str,
    pub(crate) configured: Option<&'static str>,
    pub(crate) effective: Option<&'static str>,
    pub(crate) weakened_denials: u64,
    pub(crate) exit_code: u8,
}

#[derive(Clone, Serialize)]
pub(crate) struct CountsJson {
    pub(crate) decisions: DecisionCountsJson,
    pub(crate) changes: ChangeCountsJson,
}

#[derive(Clone, Serialize)]
pub(crate) struct DecisionCountsJson {
    pub(crate) deny: u64,
    pub(crate) warn: u64,
    pub(crate) allow: u64,
}

#[derive(Clone, Serialize)]
pub(crate) struct ChangeCountsJson {
    pub(crate) added: u64,
    pub(crate) removed: u64,
    #[serde(rename = "version-changed")]
    pub(crate) version_changed: u64,
    #[serde(rename = "license-changed")]
    pub(crate) license_changed: u64,
    #[serde(rename = "supplier-changed")]
    pub(crate) supplier_changed: u64,
    #[serde(rename = "hash-changed")]
    pub(crate) hash_changed: u64,
    #[serde(rename = "hash-coverage-changed")]
    pub(crate) hash_coverage_changed: u64,
    #[serde(rename = "scope-changed")]
    pub(crate) scope_changed: u64,
    #[serde(rename = "role-changed")]
    pub(crate) role_changed: u64,
}

impl From<&ChangeCounts> for ChangeCountsJson {
    fn from(counts: &ChangeCounts) -> Self {
        Self {
            added: counts.added,
            removed: counts.removed,
            version_changed: counts.version_changed,
            license_changed: counts.license_changed,
            supplier_changed: counts.supplier_changed,
            hash_changed: counts.hash_changed,
            hash_coverage_changed: counts.hash_coverage_changed,
            scope_changed: counts.scope_changed,
            role_changed: counts.role_changed,
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct UnitJson {
    pub(crate) subject: &'static str,
    pub(crate) purl: Option<String>,
    pub(crate) ecosystem: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) qualifiers: BTreeMap<String, String>,
    pub(crate) subpath: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) epoch: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) condition: Option<String>,
    pub(crate) role: Option<&'static str>,
    pub(crate) configured: Option<&'static str>,
    pub(crate) effective: Option<&'static str>,
    pub(crate) rules: Vec<String>,
    pub(crate) used_default: bool,
    pub(crate) message: String,
    pub(crate) detail: DetailJson,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum DetailJson {
    Added {
        introduced_by: NodesJson,
        path_from_root: Option<NodesJson>,
        attributes: AttributesJson,
    },
    Removed {
        attributes: AttributesJson,
    },
    VersionChanged {
        direction: &'static str,
        flags: Vec<&'static str>,
        old_version: Option<String>,
        old_epoch: Option<String>,
        new_version: Option<String>,
        new_epoch: Option<String>,
        old_attributes: AttributesJson,
        new_attributes: AttributesJson,
    },
    LicenseChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
    SupplierChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
    ScopeChanged {
        added: Vec<String>,
        removed: Vec<String>,
    },
    HashChanged {
        algorithms: Vec<HashConflictJson>,
    },
    HashCoverageChanged {
        added: Vec<String>,
        removed: Vec<String>,
        all_dropped: bool,
    },
    RoleChanged {
        old_role: Option<&'static str>,
        new_role: Option<&'static str>,
    },
    RootChanged,
    Condition,
}

#[derive(Clone, Serialize)]
pub(crate) struct NodesJson {
    pub(crate) shown: Vec<NodeJson>,
    pub(crate) total: usize,
}

#[derive(Clone, Serialize)]
pub(crate) struct NodeJson {
    pub(crate) purl: Option<String>,
    pub(crate) ecosystem: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) epoch: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct AttributesJson {
    pub(crate) licenses: Vec<String>,
    pub(crate) hashes: Vec<HashJson>,
    pub(crate) suppliers: Vec<String>,
    pub(crate) scopes: Vec<String>,
    pub(crate) types: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct HashJson {
    pub(crate) alg: String,
    pub(crate) content: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct HashConflictJson {
    pub(crate) algorithm: String,
    pub(crate) old_contents: Vec<String>,
    pub(crate) new_contents: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct RootJson {
    pub(crate) old: Option<SnapshotJson>,
    pub(crate) new: Option<SnapshotJson>,
}

#[derive(Clone, Serialize)]
pub(crate) struct SnapshotJson {
    pub(crate) purl: Option<String>,
    pub(crate) ecosystem: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) version: String,
    pub(crate) epoch: Option<String>,
    pub(crate) role: &'static str,
    pub(crate) attributes: AttributesJson,
}

#[derive(Clone, Serialize)]
pub(crate) struct WarningsJson {
    pub(crate) old: Vec<WarningJson>,
    pub(crate) new: Vec<WarningJson>,
}

#[derive(Clone, Serialize)]
pub(crate) struct WarningJson {
    pub(crate) kind: &'static str,
    pub(crate) purl: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) message: &'static str,
}

pub(crate) fn raw_message(phrases: &[Phrase]) -> String {
    let mut out = String::new();
    for phrase in phrases {
        match phrase {
            Phrase::Text(text) | Phrase::Value(text) => out.push_str(text),
        }
    }
    out
}

pub(crate) fn outcome_id(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Warn => "warn",
        Outcome::Deny => "deny",
    }
}

pub(crate) fn decision_id(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Warn => "warn",
        Decision::Deny => "deny",
    }
}

pub(crate) fn decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "Allow",
        Decision::Warn => "Warning",
        Decision::Deny => "Deny",
    }
}

pub(crate) fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "Pass",
        Outcome::Warn => "Warning",
        Outcome::Deny => "Deny",
    }
}

pub(crate) fn change_detail(change: &Change) -> DetailJson {
    match &change.detail {
        ChangeDetail::Added {
            attributes,
            introduced_by,
            path_from_root,
        } => DetailJson::Added {
            introduced_by: nodes_json(introduced_by),
            path_from_root: path_from_root.as_ref().map(nodes_json),
            attributes: attributes_json(attributes),
        },
        ChangeDetail::Removed { attributes } => DetailJson::Removed {
            attributes: attributes_json(attributes),
        },
        ChangeDetail::VersionChanged {
            direction,
            old_attributes,
            new_attributes,
            flags,
        } => DetailJson::VersionChanged {
            direction: direction_id(*direction),
            flags: flags.iter().map(|flag| flag_id(*flag)).collect(),
            old_version: change
                .old_version
                .as_ref()
                .map(|version| version.version.clone()),
            old_epoch: change
                .old_version
                .as_ref()
                .and_then(|version| version.epoch.clone()),
            new_version: change
                .new_version
                .as_ref()
                .map(|version| version.version.clone()),
            new_epoch: change
                .new_version
                .as_ref()
                .and_then(|version| version.epoch.clone()),
            old_attributes: attributes_json(old_attributes),
            new_attributes: attributes_json(new_attributes),
        },
        ChangeDetail::SetChanged { added, removed } => match change.kind.id() {
            "supplier-changed" => DetailJson::SupplierChanged {
                added: added.iter().cloned().collect(),
                removed: removed.iter().cloned().collect(),
            },
            "scope-changed" => DetailJson::ScopeChanged {
                added: added.iter().cloned().collect(),
                removed: removed.iter().cloned().collect(),
            },
            _ => DetailJson::LicenseChanged {
                added: added.iter().cloned().collect(),
                removed: removed.iter().cloned().collect(),
            },
        },
        ChangeDetail::HashChanged { algorithms } => DetailJson::HashChanged {
            algorithms: algorithms.iter().map(conflict_json).collect(),
        },
        ChangeDetail::HashCoverageChanged {
            added,
            removed,
            all_dropped,
        } => DetailJson::HashCoverageChanged {
            added: added.iter().cloned().collect(),
            removed: removed.iter().cloned().collect(),
            all_dropped: *all_dropped,
        },
        ChangeDetail::RoleChanged => DetailJson::RoleChanged {
            old_role: change.old_role.map(role_id),
            new_role: change.new_role.map(role_id),
        },
    }
}

pub(crate) struct UnitParts<'a> {
    pub(crate) subject: &'static str,
    pub(crate) identity: Option<&'a Identity>,
    pub(crate) kind: Option<String>,
    pub(crate) condition: Option<String>,
    pub(crate) role: Option<&'static str>,
    pub(crate) configured: Option<&'static str>,
    pub(crate) effective: Option<&'static str>,
    pub(crate) rules: Vec<String>,
    pub(crate) used_default: bool,
    pub(crate) message: String,
    pub(crate) detail: DetailJson,
}

impl UnitJson {
    pub(crate) fn from_parts(parts: UnitParts<'_>) -> Self {
        let id = parts.identity;
        Self {
            subject: parts.subject,
            purl: id.and_then(|id| id.purl.clone()),
            ecosystem: id.and_then(|id| id.ecosystem.clone()),
            namespace: id.and_then(|id| id.namespace.clone()),
            name: id.and_then(|id| id.name.clone()),
            group: id.and_then(|id| id.group.clone()),
            qualifiers: id.map_or_else(BTreeMap::new, |id| id.qualifiers.iter().cloned().collect()),
            subpath: id.and_then(|id| id.subpath.clone()),
            version: id.and_then(|id| id.version.clone()),
            epoch: id.and_then(|id| id.epoch.clone()),
            kind: parts.kind,
            condition: parts.condition,
            role: parts.role,
            configured: parts.configured,
            effective: parts.effective,
            rules: parts.rules,
            used_default: parts.used_default,
            message: parts.message,
            detail: parts.detail,
        }
    }
}

pub(crate) fn root_json(root: &RootChange) -> RootJson {
    RootJson {
        old: root.old.as_ref().map(snapshot_json),
        new: root.new.as_ref().map(snapshot_json),
    }
}

pub(crate) fn warning_json(warning: &Warning) -> WarningJson {
    let id = warning.key.as_ref().map(|key| identity(key, None));
    WarningJson {
        kind: warning.kind.id(),
        purl: id.as_ref().and_then(|id| id.purl.clone()),
        group: id.as_ref().and_then(|id| id.group.clone()),
        name: id.as_ref().and_then(|id| id.name.clone()),
        message: warning.kind.message(),
    }
}

fn snapshot_json(snapshot: &RootSnapshot) -> SnapshotJson {
    let id = identity(&snapshot.key, Some(&snapshot.version));
    SnapshotJson {
        purl: id.purl,
        ecosystem: id.ecosystem,
        namespace: id.namespace,
        name: id.name,
        group: id.group,
        version: snapshot.version.version.clone(),
        epoch: snapshot.version.epoch.clone(),
        role: role_id(snapshot.role),
        attributes: attributes_json(&snapshot.attributes),
    }
}

fn nodes_json(nodes: &BoundedNodes) -> NodesJson {
    NodesJson {
        shown: nodes
            .nodes
            .iter()
            .map(|node| {
                let id = identity(&node.key, Some(&node.version));
                NodeJson {
                    purl: id.purl,
                    ecosystem: id.ecosystem,
                    namespace: id.namespace,
                    name: id.name,
                    group: id.group,
                    version: id.version,
                    epoch: id.epoch,
                }
            })
            .collect(),
        total: nodes.total,
    }
}

fn attributes_json(attributes: &VersionAttributes) -> AttributesJson {
    AttributesJson {
        licenses: attributes.licenses.iter().cloned().collect(),
        hashes: attributes
            .hashes
            .iter()
            .map(|hash| HashJson {
                alg: hash.alg.clone(),
                content: hash.content.clone(),
            })
            .collect(),
        suppliers: attributes.suppliers.iter().cloned().collect(),
        scopes: attributes.scopes.iter().cloned().collect(),
        types: attributes.types.iter().cloned().collect(),
    }
}

fn conflict_json(conflict: &HashConflict) -> HashConflictJson {
    HashConflictJson {
        algorithm: conflict.algorithm.clone(),
        old_contents: conflict.old_contents.iter().cloned().collect(),
        new_contents: conflict.new_contents.iter().cloned().collect(),
    }
}

fn direction_id(direction: Direction) -> &'static str {
    match direction {
        Direction::Upgrade => "upgrade",
        Direction::Downgrade => "downgrade",
        Direction::Unknown => "unknown",
    }
}

fn flag_id(flag: AttributeFlag) -> &'static str {
    match flag {
        AttributeFlag::LicenseChanged => "license-changed",
        AttributeFlag::SupplierChanged => "supplier-changed",
        AttributeFlag::HashChanged => "hash-changed",
        AttributeFlag::HashCoverageChanged => "hash-coverage-changed",
        AttributeFlag::ScopeChanged => "scope-changed",
        AttributeFlag::RoleChanged => "role-changed",
    }
}
