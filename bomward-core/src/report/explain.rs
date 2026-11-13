// SPDX-License-Identifier: AGPL-3.0-only

//! One sentence per change kind and per document condition.
//!
//! Terminal, Markdown, and JSON all use these phrases.

use std::collections::BTreeSet;

use crate::diff::{
    AttributeFlag, BoundedNodes, Change, ChangeDetail, Direction, HashConflict, RootChange,
    RootSnapshot,
};
use crate::model::{ContentHash, DependencyRole, VersionAttributes, WarningKind};

use super::text::{Phrase, direction_text, package_label, role_id, value_list};

/// What the human report may leave out. JSON keeps the full sentence.
#[derive(Clone, Copy)]
struct Prose {
    provenance: bool,
    hash_contents: bool,
}

const FULL: Prose = Prose {
    provenance: true,
    hash_contents: true,
};

/// Sentence for one dependency change. The package label is not repeated.
pub(crate) fn change_phrases(change: &Change) -> Vec<Phrase> {
    change_phrases_for(change, FULL)
}

/// The same sentence for Markdown and the terminal.
///
/// Hash contents stay in JSON. Provenance is kept only for a transitive add,
/// because a direct package would only repeat the project component.
pub(crate) fn human_change_phrases(change: &Change) -> Vec<Phrase> {
    let provenance = matches!(change.detail, ChangeDetail::Added { .. })
        && change.new_role == Some(DependencyRole::Transitive);
    change_phrases_for(
        change,
        Prose {
            provenance,
            hash_contents: false,
        },
    )
}

fn change_phrases_for(change: &Change, prose: Prose) -> Vec<Phrase> {
    match &change.detail {
        ChangeDetail::Added {
            attributes,
            introduced_by,
            path_from_root,
        } => added_phrases(attributes, introduced_by, path_from_root.as_ref(), prose),
        ChangeDetail::Removed { attributes } => removed_phrases(attributes, prose.hash_contents),
        ChangeDetail::VersionChanged {
            direction,
            old_attributes,
            new_attributes,
            flags,
        } => version_phrases(
            change,
            *direction,
            old_attributes,
            new_attributes,
            flags,
            prose.hash_contents,
        ),
        ChangeDetail::SetChanged { added, removed } => {
            set_phrases(change.kind.id(), added, removed)
        }
        ChangeDetail::HashChanged { algorithms } => {
            hash_phrases(&change.version.version, algorithms, prose.hash_contents)
        }
        ChangeDetail::HashCoverageChanged {
            added,
            removed,
            all_dropped,
        } => coverage_phrases(&change.version.version, added, removed, *all_dropped),
        ChangeDetail::RoleChanged => role_phrases(change.old_role, change.new_role),
    }
}

/// Sentence for the project component.
pub(crate) fn root_phrases(root: &RootChange) -> Vec<Phrase> {
    root_phrases_for(root, true)
}

/// Project-component sentence for Markdown and the terminal.
pub(crate) fn human_root_phrases(root: &RootChange) -> Vec<Phrase> {
    root_phrases_for(root, false)
}

fn root_phrases_for(root: &RootChange, hash_contents: bool) -> Vec<Phrase> {
    match (&root.old, &root.new) {
        (Some(old), Some(new)) => {
            let mut phrases = vec![Phrase::Text(
                "The project component changed from ".to_owned(),
            )];
            phrases.extend(snapshot_value(old));
            phrases.push(Phrase::Text(" to ".to_owned()));
            phrases.extend(snapshot_value(new));
            phrases.push(Phrase::Text(".".to_owned()));
            phrases.extend(attribute_delta(
                &old.attributes,
                &new.attributes,
                hash_contents,
            ));
            phrases
        }
        (None, Some(new)) => {
            let mut phrases = vec![Phrase::Text("The project component is ".to_owned())];
            phrases.extend(snapshot_value(new));
            phrases.push(Phrase::Text(".".to_owned()));
            phrases
        }
        (Some(old), None) => {
            let mut phrases = vec![Phrase::Text("The project component ".to_owned())];
            phrases.extend(snapshot_value(old));
            phrases.push(Phrase::Text(" was removed.".to_owned()));
            phrases
        }
        (None, None) => vec![Phrase::Text("The project component changed.".to_owned())],
    }
}

/// Sentence for a document condition that is not the project component.
pub(crate) fn condition_phrases(condition: &str, warning: Option<WarningKind>) -> Vec<Phrase> {
    if let Some(kind) = warning {
        return vec![
            Phrase::Text("The new document reports ".to_owned()),
            Phrase::Text(kind.id().to_owned()),
            Phrase::Text(". ".to_owned()),
            Phrase::Text(kind.message().to_owned()),
            Phrase::Text(".".to_owned()),
        ];
    }
    vec![
        Phrase::Text("Document condition ".to_owned()),
        Phrase::Text(condition.to_owned()),
        Phrase::Text(".".to_owned()),
    ]
}

fn added_phrases(
    attributes: &VersionAttributes,
    introduced_by: &BoundedNodes,
    path_from_root: Option<&BoundedNodes>,
    prose: Prose,
) -> Vec<Phrase> {
    let mut phrases = vec![Phrase::Text("Added. ".to_owned())];
    if prose.provenance {
        phrases.extend(nodes_phrases(
            "Introduced by ",
            introduced_by,
            "Nothing in the graph depends on it",
        ));
        phrases.push(Phrase::Text(". ".to_owned()));
        match path_from_root {
            Some(path) => {
                phrases.extend(nodes_phrases(
                    "Path from the root: ",
                    path,
                    "No path from the root",
                ));
                phrases.push(Phrase::Text(". ".to_owned()));
            }
            None => phrases.push(Phrase::Text("No path from the root. ".to_owned())),
        }
    }
    phrases.extend(attribute_inventory(attributes, prose.hash_contents));
    phrases
}

fn removed_phrases(attributes: &VersionAttributes, hash_contents: bool) -> Vec<Phrase> {
    let mut phrases = vec![Phrase::Text("Removed. ".to_owned())];
    phrases.extend(attribute_inventory(attributes, hash_contents));
    phrases
}

fn version_phrases(
    change: &Change,
    direction: Direction,
    old_attributes: &VersionAttributes,
    new_attributes: &VersionAttributes,
    flags: &BTreeSet<AttributeFlag>,
    hash_contents: bool,
) -> Vec<Phrase> {
    let mut phrases = vec![Phrase::Text("Version changed from ".to_owned())];
    phrases.push(Phrase::Value(display_version(
        change.old_version.as_ref().unwrap_or(&change.version),
    )));
    phrases.push(Phrase::Text(" to ".to_owned()));
    phrases.push(Phrase::Value(display_version(
        change.new_version.as_ref().unwrap_or(&change.version),
    )));
    phrases.push(Phrase::Text(format!(" ({}).", direction_text(direction))));
    if flags.is_empty() {
        return phrases;
    }
    phrases.push(Phrase::Text(" ".to_owned()));
    let mut flagged = false;
    for flag in flags {
        if flagged {
            phrases.push(Phrase::Text(" ".to_owned()));
        }
        flagged = true;
        phrases.extend(flag_phrases(
            *flag,
            old_attributes,
            new_attributes,
            change.old_role,
            change.new_role,
            hash_contents,
        ));
    }
    phrases
}

fn flag_phrases(
    flag: AttributeFlag,
    old: &VersionAttributes,
    new: &VersionAttributes,
    old_role: Option<DependencyRole>,
    new_role: Option<DependencyRole>,
    hash_contents: bool,
) -> Vec<Phrase> {
    match flag {
        AttributeFlag::LicenseChanged => set_clause("Licenses", &old.licenses, &new.licenses),
        AttributeFlag::SupplierChanged => set_clause("Suppliers", &old.suppliers, &new.suppliers),
        AttributeFlag::ScopeChanged => set_clause("Scopes", &old.scopes, &new.scopes),
        AttributeFlag::HashChanged => hash_flag_phrases(&old.hashes, &new.hashes, hash_contents),
        AttributeFlag::HashCoverageChanged => {
            let (added, removed) = algorithm_delta(&old.hashes, &new.hashes);
            let all_dropped = new.hashes.is_empty() && !old.hashes.is_empty();
            coverage_phrases("", &added, &removed, all_dropped)
        }
        AttributeFlag::RoleChanged => role_phrases(old_role, new_role),
    }
}

fn set_phrases(kind: &str, added: &BTreeSet<String>, removed: &BTreeSet<String>) -> Vec<Phrase> {
    let title = match kind {
        "license-changed" => "Licenses",
        "supplier-changed" => "Suppliers",
        "scope-changed" => "Scopes",
        _ => "Values",
    };
    let mut phrases = value_list(&format!("{title} added: "), &sorted(added));
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list("Removed: ", &sorted(removed)));
    phrases.push(Phrase::Text(".".to_owned()));
    phrases
}

fn set_clause(title: &str, old: &BTreeSet<String>, new: &BTreeSet<String>) -> Vec<Phrase> {
    let added = new.difference(old).cloned().collect::<Vec<_>>();
    let removed = old.difference(new).cloned().collect::<Vec<_>>();
    let mut phrases = value_list(&format!("{title} added: "), &added);
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list("Removed: ", &removed));
    phrases.push(Phrase::Text(".".to_owned()));
    phrases
}

fn hash_phrases(version: &str, algorithms: &[HashConflict], hash_contents: bool) -> Vec<Phrase> {
    let mut phrases = vec![
        Phrase::Text("Same version, different content. Version ".to_owned()),
        Phrase::Value(version.to_owned()),
        Phrase::Text(". ".to_owned()),
    ];
    for (index, conflict) in algorithms.iter().enumerate() {
        if index > 0 {
            phrases.push(Phrase::Text(" ".to_owned()));
        }
        phrases.push(Phrase::Text("Algorithm ".to_owned()));
        phrases.push(Phrase::Value(conflict.algorithm.clone()));
        if hash_contents {
            phrases.push(Phrase::Text(" changed from ".to_owned()));
            phrases.extend(value_list("", &sorted(&conflict.old_contents)));
            phrases.push(Phrase::Text(" to ".to_owned()));
            phrases.extend(value_list("", &sorted(&conflict.new_contents)));
            phrases.push(Phrase::Text(".".to_owned()));
        } else {
            phrases.push(Phrase::Text(" changed.".to_owned()));
        }
    }
    phrases
}

fn hash_flag_phrases(
    old: &BTreeSet<ContentHash>,
    new: &BTreeSet<ContentHash>,
    hash_contents: bool,
) -> Vec<Phrase> {
    let mut phrases = vec![Phrase::Text("Hash content changed.".to_owned())];
    for conflict in hash_conflicts(old, new) {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.push(Phrase::Value(conflict.algorithm));
        if hash_contents {
            phrases.push(Phrase::Text(" from ".to_owned()));
            phrases.extend(value_list(
                "",
                &conflict.old_contents.into_iter().collect::<Vec<_>>(),
            ));
            phrases.push(Phrase::Text(" to ".to_owned()));
            phrases.extend(value_list(
                "",
                &conflict.new_contents.into_iter().collect::<Vec<_>>(),
            ));
            phrases.push(Phrase::Text(".".to_owned()));
        } else {
            phrases.push(Phrase::Text(".".to_owned()));
        }
    }
    phrases
}

fn coverage_phrases(
    version: &str,
    added: &BTreeSet<String>,
    removed: &BTreeSet<String>,
    all_dropped: bool,
) -> Vec<Phrase> {
    let mut phrases = Vec::new();
    if !version.is_empty() {
        phrases.push(Phrase::Text(
            "Hash coverage changed for the same version ".to_owned(),
        ));
        phrases.push(Phrase::Value(version.to_owned()));
        phrases.push(Phrase::Text(". ".to_owned()));
    }
    phrases.extend(value_list(
        "Algorithms added: ",
        &added.iter().cloned().collect::<Vec<_>>(),
    ));
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list(
        "Algorithms removed: ",
        &removed.iter().cloned().collect::<Vec<_>>(),
    ));
    phrases.push(Phrase::Text(".".to_owned()));
    if all_dropped {
        phrases.push(Phrase::Text(" Every hash was dropped.".to_owned()));
    }
    phrases
}

fn role_phrases(old: Option<DependencyRole>, new: Option<DependencyRole>) -> Vec<Phrase> {
    vec![
        Phrase::Text("Role changed from ".to_owned()),
        Phrase::Text(old.map_or("unknown", role_id).to_owned()),
        Phrase::Text(" to ".to_owned()),
        Phrase::Text(new.map_or("unknown", role_id).to_owned()),
        Phrase::Text(".".to_owned()),
    ]
}

fn attribute_inventory(attributes: &VersionAttributes, hash_contents: bool) -> Vec<Phrase> {
    let mut phrases = value_list(
        "Licenses: ",
        &attributes.licenses.iter().cloned().collect::<Vec<_>>(),
    );
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list(
        "Suppliers: ",
        &attributes.suppliers.iter().cloned().collect::<Vec<_>>(),
    ));
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list(
        "Scopes: ",
        &attributes.scopes.iter().cloned().collect::<Vec<_>>(),
    ));
    phrases.push(Phrase::Text(". ".to_owned()));
    phrases.extend(value_list(
        "Hash algorithms: ",
        &algorithm_names(&attributes.hashes),
    ));
    if hash_contents {
        phrases.push(Phrase::Text(". ".to_owned()));
        phrases.extend(value_list(
            "Hash contents: ",
            &attributes
                .hashes
                .iter()
                .map(|hash| hash.content.clone())
                .collect::<Vec<_>>(),
        ));
        phrases.push(Phrase::Text(".".to_owned()));
    } else {
        phrases.push(Phrase::Text(".".to_owned()));
    }
    phrases
}

fn attribute_delta(
    old: &VersionAttributes,
    new: &VersionAttributes,
    hash_contents: bool,
) -> Vec<Phrase> {
    let mut phrases = Vec::new();
    if old.licenses != new.licenses {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.extend(set_clause("Licenses", &old.licenses, &new.licenses));
    }
    if old.suppliers != new.suppliers {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.extend(set_clause("Suppliers", &old.suppliers, &new.suppliers));
    }
    if old.scopes != new.scopes {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.extend(set_clause("Scopes", &old.scopes, &new.scopes));
    }
    if old.hashes != new.hashes {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.extend(hash_flag_phrases(&old.hashes, &new.hashes, hash_contents));
    }
    phrases
}

fn snapshot_value(snapshot: &RootSnapshot) -> Vec<Phrase> {
    let mut phrases = Vec::new();
    push_label(
        &mut phrases,
        package_label(&snapshot.key, &snapshot.version),
    );
    phrases
}

fn nodes_phrases(prefix: &str, nodes: &BoundedNodes, empty: &str) -> Vec<Phrase> {
    if nodes.total == 0 || nodes.nodes.is_empty() {
        return vec![Phrase::Text(empty.to_owned())];
    }
    let mut phrases = vec![Phrase::Text(prefix.to_owned())];
    for (index, node) in nodes.nodes.iter().enumerate() {
        if index > 0 {
            phrases.push(Phrase::Text(", ".to_owned()));
        }
        push_label(&mut phrases, package_label(&node.key, &node.version));
    }
    if nodes.nodes.len() < nodes.total {
        phrases.push(Phrase::Text(format!(
            " ({shown} of {total})",
            shown = nodes.nodes.len(),
            total = nodes.total
        )));
    }
    phrases
}

fn push_label(phrases: &mut Vec<Phrase>, label: super::text::PackageLabel) {
    if let Some(ecosystem) = label.ecosystem {
        phrases.push(Phrase::Value(ecosystem));
        phrases.push(Phrase::Text(" ".to_owned()));
    }
    phrases.push(Phrase::Value(label.name));
    phrases.push(Phrase::Text("@".to_owned()));
    phrases.push(Phrase::Value(label.version));
    for (qualifier, value) in label.qualifiers {
        phrases.push(Phrase::Text(" ".to_owned()));
        phrases.push(Phrase::Value(qualifier));
        phrases.push(Phrase::Text("=".to_owned()));
        phrases.push(Phrase::Value(value));
    }
    if let Some(subpath) = label.subpath {
        phrases.push(Phrase::Text("#".to_owned()));
        phrases.push(Phrase::Value(subpath));
    }
}

fn display_version(version: &crate::model::VersionId) -> String {
    match &version.epoch {
        Some(epoch) => format!("{epoch}:{}", version.version),
        None => version.version.clone(),
    }
}

fn sorted(values: &BTreeSet<String>) -> Vec<String> {
    values.iter().cloned().collect()
}

fn algorithm_names(hashes: &BTreeSet<ContentHash>) -> Vec<String> {
    hashes.iter().map(|hash| hash.alg.clone()).collect()
}

fn algorithm_delta(
    old: &BTreeSet<ContentHash>,
    new: &BTreeSet<ContentHash>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let old_names: BTreeSet<_> = old.iter().map(|hash| hash.alg.clone()).collect();
    let new_names: BTreeSet<_> = new.iter().map(|hash| hash.alg.clone()).collect();
    (
        new_names.difference(&old_names).cloned().collect(),
        old_names.difference(&new_names).cloned().collect(),
    )
}

fn hash_conflicts(old: &BTreeSet<ContentHash>, new: &BTreeSet<ContentHash>) -> Vec<HashConflict> {
    let names: BTreeSet<_> = old
        .iter()
        .map(|hash| hash.alg.as_str())
        .filter(|name| new.iter().any(|hash| hash.alg == *name))
        .map(str::to_owned)
        .collect();
    let mut conflicts = Vec::new();
    for name in names {
        let old_contents: BTreeSet<_> = old
            .iter()
            .filter(|hash| hash.alg == name)
            .map(|hash| hash.content.clone())
            .collect();
        let new_contents: BTreeSet<_> = new
            .iter()
            .filter(|hash| hash.alg == name)
            .map(|hash| hash.content.clone())
            .collect();
        if old_contents != new_contents {
            conflicts.push(HashConflict {
                algorithm: name,
                old_contents,
                new_contents,
            });
        }
    }
    conflicts
}
