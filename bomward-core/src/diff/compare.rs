// SPDX-License-Identifier: AGPL-3.0-only

//! Linear merge of the two component maps.
//!
//! Keys and versions are already ordered. Each step advances the side that
//! is behind, so the walk is proportional to the two maps. Pairing into
//! `version-changed` happens only when both sides have exactly one
//! dependency version and those ids differ.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Component, ComponentKey, ContentHash, Document, Version, VersionId};

use super::direction::direction;
use super::provenance::{self, Provenance};
use super::{AttributeFlag, Change, ChangeDetail, ChangeKind, HashConflict, RootSnapshot};

pub(super) fn root_snapshot(document: &Document) -> Option<RootSnapshot> {
    provenance::find_root(document)
}

struct Walk<'a> {
    old_root: Option<&'a RootSnapshot>,
    new_root: Option<&'a RootSnapshot>,
    provenance: &'a Provenance,
    roles_comparable: bool,
}

pub(super) fn changes(old: &Document, new: &Document, roles_comparable: bool) -> Vec<Change> {
    let old_root = root_snapshot(old);
    let new_root = root_snapshot(new);
    let provenance = Provenance::from_document(new, new_root.as_ref());
    let walk = Walk {
        old_root: old_root.as_ref(),
        new_root: new_root.as_ref(),
        provenance: &provenance,
        roles_comparable,
    };
    let mut changes = Vec::new();
    let mut old_keys = old.components.iter().peekable();
    let mut new_keys = new.components.iter().peekable();
    loop {
        let old_key = old_keys.peek().map(|(key, _)| *key);
        let new_key = new_keys.peek().map(|(key, _)| *key);
        match (old_key, new_key) {
            (None, None) => break,
            (Some(_), None) => {
                if let Some((key, component)) = old_keys.next() {
                    push_removed(&mut changes, key, component, old_root.as_ref());
                }
            }
            (None, Some(_)) => {
                if let Some((key, component)) = new_keys.next() {
                    push_added(&mut changes, key, component, new_root.as_ref(), &provenance);
                }
            }
            (Some(old_only), Some(new_only)) => match old_only.cmp(new_only) {
                Ordering::Less => {
                    if let Some((key, component)) = old_keys.next() {
                        push_removed(&mut changes, key, component, old_root.as_ref());
                    }
                }
                Ordering::Greater => {
                    if let Some((key, component)) = new_keys.next() {
                        push_added(&mut changes, key, component, new_root.as_ref(), &provenance);
                    }
                }
                Ordering::Equal => {
                    if let (Some((key, old_component)), Some((_, new_component))) =
                        (old_keys.next(), new_keys.next())
                    {
                        compare_key(&mut changes, key, old_component, new_component, &walk);
                    }
                }
            },
        }
    }
    changes.sort_by(|left, right| {
        (&left.key, &left.version, left.kind).cmp(&(&right.key, &right.version, right.kind))
    });
    changes
}

fn push_removed(
    changes: &mut Vec<Change>,
    key: &ComponentKey,
    component: &Component,
    root: Option<&RootSnapshot>,
) {
    for (version, stored) in dependency_versions(key, component, root) {
        changes.push(removed_change(key, version, stored));
    }
}

fn push_added(
    changes: &mut Vec<Change>,
    key: &ComponentKey,
    component: &Component,
    root: Option<&RootSnapshot>,
    provenance: &Provenance,
) {
    for (version, stored) in dependency_versions(key, component, root) {
        changes.push(added_change(key, version, stored, provenance));
    }
}

fn compare_key(
    changes: &mut Vec<Change>,
    key: &ComponentKey,
    old: &Component,
    new: &Component,
    walk: &Walk<'_>,
) {
    let old_versions = dependency_versions(key, old, walk.old_root);
    let new_versions = dependency_versions(key, new, walk.new_root);
    if let Some(change) =
        paired_version_change(key, &old_versions, &new_versions, walk.roles_comparable)
    {
        changes.push(change);
        return;
    }
    let mut old_versions = old_versions.into_iter().peekable();
    let mut new_versions = new_versions.into_iter().peekable();
    loop {
        let old_id = old_versions.peek().map(|(version, _)| *version);
        let new_id = new_versions.peek().map(|(version, _)| *version);
        match (old_id, new_id) {
            (None, None) => break,
            (Some(_), None) => {
                if let Some((version, stored)) = old_versions.next() {
                    changes.push(removed_change(key, version, stored));
                }
            }
            (None, Some(_)) => {
                if let Some((version, stored)) = new_versions.next() {
                    changes.push(added_change(key, version, stored, walk.provenance));
                }
            }
            (Some(old_only), Some(new_only)) => match old_only.cmp(new_only) {
                Ordering::Less => {
                    if let Some((version, stored)) = old_versions.next() {
                        changes.push(removed_change(key, version, stored));
                    }
                }
                Ordering::Greater => {
                    if let Some((version, stored)) = new_versions.next() {
                        changes.push(added_change(key, version, stored, walk.provenance));
                    }
                }
                Ordering::Equal => {
                    if let (Some((version, old_stored)), Some((_, new_stored))) =
                        (old_versions.next(), new_versions.next())
                    {
                        push_attribute_changes(
                            changes,
                            key,
                            version,
                            old_stored,
                            new_stored,
                            walk.roles_comparable,
                        );
                    }
                }
            },
        }
    }
}

fn paired_version_change(
    key: &ComponentKey,
    old_versions: &[(&VersionId, &Version)],
    new_versions: &[(&VersionId, &Version)],
    roles_comparable: bool,
) -> Option<Change> {
    if old_versions.len() != 1 || new_versions.len() != 1 {
        return None;
    }
    let (old_id, old_version) = old_versions.first().copied()?;
    let (new_id, new_version) = new_versions.first().copied()?;
    if old_id == new_id {
        return None;
    }
    Some(version_change(
        key,
        old_id,
        old_version,
        new_id,
        new_version,
        roles_comparable,
    ))
}

fn dependency_versions<'a>(
    key: &ComponentKey,
    component: &'a Component,
    root: Option<&RootSnapshot>,
) -> Vec<(&'a VersionId, &'a Version)> {
    component
        .versions
        .iter()
        .filter(|(version, _)| !is_root_version(root, key, version))
        .collect()
}

fn is_root_version(root: Option<&RootSnapshot>, key: &ComponentKey, version: &VersionId) -> bool {
    root.is_some_and(|snapshot| &snapshot.key == key && &snapshot.version == version)
}

fn removed_change(key: &ComponentKey, version: &VersionId, stored: &Version) -> Change {
    Change {
        key: key.clone(),
        kind: ChangeKind::Removed,
        version: version.clone(),
        old_version: Some(version.clone()),
        new_version: None,
        old_role: Some(stored.role),
        new_role: None,
        judged: stored.attributes.clone(),
        detail: ChangeDetail::Removed {
            attributes: Box::new(stored.attributes.clone()),
        },
    }
}

fn added_change(
    key: &ComponentKey,
    version: &VersionId,
    stored: &Version,
    provenance: &Provenance,
) -> Change {
    let node = crate::model::GraphNode {
        key: key.clone(),
        version: version.clone(),
    };
    Change {
        key: key.clone(),
        kind: ChangeKind::Added,
        version: version.clone(),
        old_version: None,
        new_version: Some(version.clone()),
        old_role: None,
        new_role: Some(stored.role),
        judged: stored.attributes.clone(),
        detail: ChangeDetail::Added {
            attributes: Box::new(stored.attributes.clone()),
            introduced_by: provenance.introduced_by(&node),
            path_from_root: provenance.path_from_root(&node),
        },
    }
}

fn version_change(
    key: &ComponentKey,
    old_id: &VersionId,
    old_version: &Version,
    new_id: &VersionId,
    new_version: &Version,
    roles_comparable: bool,
) -> Change {
    Change {
        key: key.clone(),
        kind: ChangeKind::VersionChanged,
        version: new_id.clone(),
        old_version: Some(old_id.clone()),
        new_version: Some(new_id.clone()),
        old_role: Some(old_version.role),
        new_role: Some(new_version.role),
        judged: new_version.attributes.clone(),
        detail: ChangeDetail::VersionChanged {
            direction: direction(key, old_id, new_id),
            old_attributes: Box::new(old_version.attributes.clone()),
            new_attributes: Box::new(new_version.attributes.clone()),
            flags: flags_between(old_version, new_version, roles_comparable),
        },
    }
}

fn push_attribute_changes(
    changes: &mut Vec<Change>,
    key: &ComponentKey,
    version: &VersionId,
    old: &Version,
    new: &Version,
    roles_comparable: bool,
) {
    let shared = Shared {
        key,
        version,
        old,
        new,
    };
    push_set(
        changes,
        &shared,
        ChangeKind::LicenseChanged,
        &old.attributes.licenses,
        &new.attributes.licenses,
    );
    push_set(
        changes,
        &shared,
        ChangeKind::SupplierChanged,
        &old.attributes.suppliers,
        &new.attributes.suppliers,
    );
    push_hashes(changes, &shared);
    let old_scopes = scopes_for_compare(&old.attributes.scopes);
    let new_scopes = scopes_for_compare(&new.attributes.scopes);
    push_set(
        changes,
        &shared,
        ChangeKind::ScopeChanged,
        &old_scopes,
        &new_scopes,
    );
    if roles_comparable && old.role != new.role {
        changes.push(shared.change(ChangeKind::RoleChanged, ChangeDetail::RoleChanged));
    }
}

/// `CycloneDX` says a missing `scope` means `required`.
///
/// The model keeps the empty set, so a report can still show that the
/// field was absent. Only the comparison fills it in.
pub(super) fn scopes_for_compare(scopes: &BTreeSet<String>) -> BTreeSet<String> {
    if scopes.is_empty() {
        BTreeSet::from([String::from("required")])
    } else {
        scopes.clone()
    }
}

struct Shared<'a> {
    key: &'a ComponentKey,
    version: &'a VersionId,
    old: &'a Version,
    new: &'a Version,
}

fn push_set(
    changes: &mut Vec<Change>,
    shared: &Shared<'_>,
    kind: ChangeKind,
    old_values: &BTreeSet<String>,
    new_values: &BTreeSet<String>,
) {
    let (added, removed) = set_delta(old_values, new_values);
    if added.is_empty() && removed.is_empty() {
        return;
    }
    changes.push(shared.change(kind, ChangeDetail::SetChanged { added, removed }));
}

fn push_hashes(changes: &mut Vec<Change>, shared: &Shared<'_>) {
    let delta = hash_delta(&shared.old.attributes.hashes, &shared.new.attributes.hashes);
    if !delta.conflicts.is_empty() {
        changes.push(shared.change(
            ChangeKind::HashChanged,
            ChangeDetail::HashChanged {
                algorithms: delta.conflicts,
            },
        ));
    }
    if !delta.added.is_empty() || !delta.removed.is_empty() {
        changes.push(shared.change(
            ChangeKind::HashCoverageChanged,
            ChangeDetail::HashCoverageChanged {
                added: delta.added,
                removed: delta.removed,
                all_dropped: delta.all_dropped,
            },
        ));
    }
}

impl Shared<'_> {
    fn change(&self, kind: ChangeKind, detail: ChangeDetail) -> Change {
        Change {
            key: self.key.clone(),
            kind,
            version: self.version.clone(),
            old_version: Some(self.version.clone()),
            new_version: Some(self.version.clone()),
            old_role: Some(self.old.role),
            new_role: Some(self.new.role),
            judged: self.new.attributes.clone(),
            detail,
        }
    }
}

fn flags_between(old: &Version, new: &Version, roles_comparable: bool) -> BTreeSet<AttributeFlag> {
    let mut flags = BTreeSet::new();
    if old.attributes.licenses != new.attributes.licenses {
        flags.insert(AttributeFlag::LicenseChanged);
    }
    if old.attributes.suppliers != new.attributes.suppliers {
        flags.insert(AttributeFlag::SupplierChanged);
    }
    let hashes = hash_delta(&old.attributes.hashes, &new.attributes.hashes);
    if !hashes.conflicts.is_empty() {
        flags.insert(AttributeFlag::HashChanged);
    }
    if !hashes.added.is_empty() || !hashes.removed.is_empty() {
        flags.insert(AttributeFlag::HashCoverageChanged);
    }
    if scopes_for_compare(&old.attributes.scopes) != scopes_for_compare(&new.attributes.scopes) {
        flags.insert(AttributeFlag::ScopeChanged);
    }
    if roles_comparable && old.role != new.role {
        flags.insert(AttributeFlag::RoleChanged);
    }
    flags
}

fn set_delta(
    old: &BTreeSet<String>,
    new: &BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let added = new.difference(old).cloned().collect();
    let removed = old.difference(new).cloned().collect();
    (added, removed)
}

pub(super) struct HashDelta {
    conflicts: Vec<HashConflict>,
    pub(super) added: BTreeSet<String>,
    pub(super) removed: BTreeSet<String>,
    pub(super) all_dropped: bool,
}

pub(super) fn hash_delta(old: &BTreeSet<ContentHash>, new: &BTreeSet<ContentHash>) -> HashDelta {
    let old_groups = group_hashes(old);
    let new_groups = group_hashes(new);
    let mut conflicts = Vec::new();
    let mut added = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let mut old_algs = old_groups.iter().peekable();
    let mut new_algs = new_groups.iter().peekable();
    loop {
        let old_alg = old_algs.peek().map(|(alg, _)| *alg);
        let new_alg = new_algs.peek().map(|(alg, _)| *alg);
        match (old_alg, new_alg) {
            (None, None) => break,
            (Some(_), None) => {
                if let Some((alg, _)) = old_algs.next() {
                    removed.insert(alg.clone());
                }
            }
            (None, Some(_)) => {
                if let Some((alg, _)) = new_algs.next() {
                    added.insert(alg.clone());
                }
            }
            (Some(old_only), Some(new_only)) => match old_only.cmp(new_only) {
                Ordering::Less => {
                    if let Some((alg, _)) = old_algs.next() {
                        removed.insert(alg.clone());
                    }
                }
                Ordering::Greater => {
                    if let Some((alg, _)) = new_algs.next() {
                        added.insert(alg.clone());
                    }
                }
                Ordering::Equal => {
                    if let (Some((alg, old_group)), Some((_, new_group))) =
                        (old_algs.next(), new_algs.next())
                        && old_group.normalized != new_group.normalized
                    {
                        conflicts.push(HashConflict {
                            algorithm: alg.clone(),
                            old_contents: old_group.original.clone(),
                            new_contents: new_group.original.clone(),
                        });
                    }
                }
            },
        }
    }
    HashDelta {
        conflicts,
        added,
        removed,
        all_dropped: !old.is_empty() && new.is_empty(),
    }
}

struct HashGroup {
    original: BTreeSet<String>,
    normalized: BTreeSet<String>,
}

fn group_hashes(hashes: &BTreeSet<ContentHash>) -> BTreeMap<String, HashGroup> {
    let mut groups = BTreeMap::new();
    for hash in hashes {
        let group = groups.entry(hash.alg.clone()).or_insert_with(|| HashGroup {
            original: BTreeSet::new(),
            normalized: BTreeSet::new(),
        });
        group.normalized.insert(hash.content.to_ascii_lowercase());
        group.original.insert(hash.content.clone());
    }
    groups
}
